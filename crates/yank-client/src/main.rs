use anyhow::Result;
use arboard::{Clipboard, Error as ClipboardError, ImageData};
use chrono::{DateTime, Local};
use makepad_widgets::makepad_draw::text::{font::FontId, fonts::Fonts, loader::FontDefinition};
use makepad_widgets::*;
use std::{
    borrow::Cow,
    cell::RefCell,
    env, fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::Receiver,
};
use yank_client::{
    paths,
    sync::{SyncClient, SyncConfig},
};
use yank_core::{
    Clip, ClipFormat, Settings, Store, Theme, content_hash,
    i18n::{self, I18nBundle},
};

#[cfg(target_os = "linux")]
use ksni::blocking::TrayMethods as _;
#[cfg(target_os = "linux")]
use std::sync::mpsc;

const HISTORY_ROWS: usize = 20;
const HISTORY_PAGE_STEP: usize = 10;
const MIN_CAPTURE_INTERVAL_MS: u64 = 250;

include!(concat!(env!("OUT_DIR"), "/embedded_makepad_fonts.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureFormatKind {
    Text,
    Html,
    Image,
    Files,
}

script_mod! {
    use mod.prelude.widgets.*

    let AppCard = RoundedView{
        width: Fill
        height: Fit
        flow: Down
        spacing: theme.space_2
        padding: theme.mspace_3{left: theme.space_3, right: theme.space_3, top: theme.space_2, bottom: theme.space_2}
        new_batch: true
        draw_bg.color: theme.color_bg_container
        draw_bg.border_radius: 4.0
        draw_bg.border_size: 1.0
        draw_bg.border_color: theme.color_bg_highlight
    }

    let PanelCard = RoundedView{
        width: Fill
        height: Fill
        flow: Down
        spacing: theme.space_2
        padding: theme.mspace_3{left: theme.space_3, right: theme.space_3, top: theme.space_2, bottom: theme.space_2}
        new_batch: true
        draw_bg.color: theme.color_bg_container
        draw_bg.border_radius: 4.0
        draw_bg.border_size: 1.0
        draw_bg.border_color: theme.color_bg_highlight
    }

    let FieldRow = View{
        width: Fill
        height: Fit
        flow: Right
        spacing: theme.space_2
        align: Align{y: 0.5}
    }

    let FieldGroup = View{
        width: Fill
        height: Fit
        flow: Down
        spacing: theme.space_1
    }

    let MutedLabel = Label{
        width: Fill
        height: Fit
        draw_text.color: theme.color_label_inner_inactive
        draw_text.text_style.font_size: theme.font_size_p
    }

    let MetaLabel = Label{
        width: Fit
        height: Fit
        draw_text.color: theme.color_label_inner_inactive
        draw_text.text_style.font_size: theme.font_size_code
    }

    let SectionTitle = Label{
        width: Fill
        height: Fit
        draw_text.color: theme.color_label_inner
        draw_text.text_style: theme.font_bold{font_size: theme.font_size_p}
    }

    let DenseButton = ButtonFlat{
        height: 30
        margin: 0.
        padding: theme.mspace_2{left: theme.space_2, right: theme.space_2}
        draw_bg +: {
            border_radius: 3.0
            border_size: 1.0
        }
        draw_text +: {
            text_style +: {font_size: theme.font_size_p}
        }
    }

    let MenuButton = ButtonFlat{
        width: Fit
        height: 28
        margin: 0.
        padding: theme.mspace_2{left: theme.space_2, right: theme.space_2}
        draw_bg +: {
            border_radius: 2.0
            border_size: 1.0
        }
        draw_text +: {
            text_style +: {font_size: theme.font_size_p}
        }
    }

    let TabButton = ButtonFlat{
        width: 122
        height: 30
        margin: 0.
        padding: theme.mspace_2{left: theme.space_2, right: theme.space_2}
        draw_bg +: {
            border_radius: 2.0
            border_size: 1.0
        }
        draw_text +: {
            text_style +: {font_size: theme.font_size_p}
        }
    }

    let ActionButton = Button{
        height: 32
        margin: 0.
        padding: theme.mspace_2{left: theme.space_3, right: theme.space_3}
        draw_bg +: {
            border_radius: 3.0
        }
        draw_text +: {
            text_style +: {font_size: theme.font_size_p}
        }
    }

    let HistoryRow = ButtonFlat{
        width: Fill
        height: 30
        margin: 0.
        padding: theme.mspace_2{left: theme.space_2, right: theme.space_2}
        align: Align{x: 0.0 y: 0.5}
        label_walk: Walk{width: Fill, height: Fit}
        draw_bg +: {
            border_radius: 0.0
            border_size: 1.0
            color: theme.color_bg_even
            color_hover: theme.color_bg_highlight_inline
            color_focus: theme.color_bg_highlight
            color_down: theme.color_inset_focus
            border_color: theme.color_bg_highlight
            border_color_hover: theme.color_bevel_hover
            border_color_focus: theme.color_highlight
        }
        draw_text +: {
            text_style: theme.font_regular{font_size: theme.font_size_p}
        }
    }

    let PreviewSurface = RoundedView{
        width: Fill
        height: Fill
        flow: Down
        padding: theme.mspace_3{left: theme.space_3, right: theme.space_3, top: theme.space_3, bottom: theme.space_3}
        new_batch: true
        draw_bg.color: theme.color_inset
        draw_bg.border_radius: 4.0
        draw_bg.border_size: 1.0
        draw_bg.border_color: theme.color_bg_highlight
    }

    let InlinePanel = RoundedView{
        width: Fill
        height: Fit
        flow: Down
        spacing: theme.space_1
        padding: theme.mspace_3{left: theme.space_2, right: theme.space_2, top: theme.space_2, bottom: theme.space_2}
        new_batch: true
        draw_bg.color: theme.color_inset
        draw_bg.border_radius: 3.0
        draw_bg.border_size: 1.0
        draw_bg.border_color: theme.color_bg_highlight
    }

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                pass.clear_color: theme.color_bg_app
                window.title: "yank"
                window.inner_size: vec2(460, 590)
                body +: {
                    width: Fill
                    height: Fill
                    flow: Down
                    spacing: 0

                    app_header := SolidView{
                        width: Fill
                        height: Fit
                        flow: Right
                        spacing: theme.space_2
                        padding: theme.mspace_3{left: theme.space_2, right: theme.space_2, top: theme.space_1, bottom: theme.space_1}
                        align: Align{y: 0.5}
                        draw_bg.color: theme.color_app_caption_bar

                        header_text := View{
                            width: Fill
                            height: Fit
                            flow: Down
                            spacing: 0.
                            title := Label{
                                width: Fill
                                text: ""
                                draw_text.color: theme.color_label_inner
                                draw_text.text_style: theme.font_bold{font_size: theme.font_size_p}
                            }
                            subtitle := Label{
                                width: Fill
                                text: ""
                                draw_text.color: theme.color_label_inner_inactive
                                draw_text.text_style.font_size: theme.font_size_p
                            }
                        }

                        status_shell := RoundedView{
                            width: 188
                            height: Fit
                            padding: theme.mspace_2{left: theme.space_2, right: theme.space_2, top: theme.space_1, bottom: theme.space_1}
                            new_batch: true
                            draw_bg.color: theme.color_bg_highlight_inline
                            draw_bg.border_radius: 2.0
                            status := TextBox{
                                width: Fill
                                height: Fit
                                text: ""
                                draw_text.color: theme.color_label_inner
                                draw_text.text_style.font_size: theme.font_size_code
                            }
                        }

                        settings_button := DenseButton{text: ""}
                    }

                    main_page := View{
                        width: Fill
                        height: Fill
                        flow: Down
                        spacing: theme.space_1
                        padding: theme.mspace_3{left: theme.space_1, right: theme.space_1, top: theme.space_1, bottom: theme.space_1}

                        quick_paste_shell := PanelCard{
                            width: Fill
                            height: Fill
                            spacing: theme.space_1
                            padding: theme.mspace_3{left: theme.space_1, right: theme.space_1, top: theme.space_1, bottom: theme.space_1}
                            history_header := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_2
                                align: Align{y: 0.5}
                                history_title := SectionTitle{text: ""}
                                clip_count := MetaLabel{text: ""}
                            }
                            group_bar := View{
                                visible: false
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                filter_all_button := MenuButton{text: ""}
                                filter_pinned_button := MenuButton{text: ""}
                                filter_text_button := MenuButton{text: ""}
                                filter_image_button := MenuButton{text: ""}
                                filter_files_button := MenuButton{text: ""}
                            }
                            group_panel := InlinePanel{
                                visible: false
                                group_filter_title := SectionTitle{text: ""}
                                group_filter_row_a := View{
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    spacing: theme.space_1
                                    group_history_button := MenuButton{text: ""}
                                    group_pinned_button := MenuButton{text: ""}
                                    group_text_button := MenuButton{text: ""}
                                }
                                group_filter_row_b := View{
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    spacing: theme.space_1
                                    group_image_button := MenuButton{text: ""}
                                    group_files_button := MenuButton{text: ""}
                                }
                            }
                            rows := ScrollYView{
                                width: Fill
                                height: Fill
                                flow: Down
                                spacing: 0.
                                row_0 := HistoryRow{text: ""}
                                row_1 := HistoryRow{text: ""}
                                row_2 := HistoryRow{text: ""}
                                row_3 := HistoryRow{text: ""}
                                row_4 := HistoryRow{text: ""}
                                row_5 := HistoryRow{text: ""}
                                row_6 := HistoryRow{text: ""}
                                row_7 := HistoryRow{text: ""}
                                row_8 := HistoryRow{text: ""}
                                row_9 := HistoryRow{text: ""}
                                row_10 := HistoryRow{text: ""}
                                row_11 := HistoryRow{text: ""}
                                row_12 := HistoryRow{text: ""}
                                row_13 := HistoryRow{text: ""}
                                row_14 := HistoryRow{text: ""}
                                row_15 := HistoryRow{text: ""}
                                row_16 := HistoryRow{text: ""}
                                row_17 := HistoryRow{text: ""}
                                row_18 := HistoryRow{text: ""}
                                row_19 := HistoryRow{text: ""}
                            }

                            editor_panel := View{
                                width: Fill
                                height: 126
                                visible: false
                                flow: Right
                                spacing: theme.space_1
                                edit_group := View{
                                    width: Fill
                                    height: Fill
                                    flow: Down
                                    spacing: theme.space_1
                                    selected_title := SectionTitle{text: ""}
                                    selected_meta := MutedLabel{text: ""}
                                    edit_label := SectionTitle{text: ""}
                                    edit_input := TextInput{
                                        width: Fill
                                        height: Fill
                                        is_multiline: true
                                        empty_text: ""
                                    }
                                }
                                detail_actions := View{
                                    width: Fit
                                    height: Fill
                                    flow: Down
                                    spacing: theme.space_1
                                    copy_selected_button := ActionButton{text: ""}
                                    save_edit_button := DenseButton{text: ""}
                                    pin_button := DenseButton{text: ""}
                                    delete_button := DenseButton{text: ""}
                                }
                            }
                        }

                        menu_panel := AppCard{
                            visible: false
                            flow: Down
                            spacing: theme.space_1
                            menu_row_a := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                menu_copy_button := MenuButton{text: ""}
                                menu_paste_plain_button := MenuButton{text: ""}
                                menu_edit_button := MenuButton{text: ""}
                                menu_delete_button := MenuButton{text: ""}
                            }
                            menu_row_b := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                menu_pin_button := MenuButton{text: ""}
                                menu_capture_button := MenuButton{text: ""}
                                menu_capture_toggle_button := MenuButton{text: ""}
                                menu_sync_button := MenuButton{text: ""}
                            }
                            menu_row_c := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                menu_refresh_button := MenuButton{text: ""}
                                menu_options_button := MenuButton{text: ""}
                                menu_exit_button := MenuButton{text: ""}
                            }
                            menu_row_d := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                menu_groups_button := MenuButton{text: ""}
                                menu_position_button := MenuButton{text: ""}
                                menu_lines_button := MenuButton{text: ""}
                                menu_transparency_button := MenuButton{text: ""}
                            }
                            menu_row_e := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                menu_upper_button := MenuButton{text: ""}
                                menu_lower_button := MenuButton{text: ""}
                                menu_trim_button := MenuButton{text: ""}
                                menu_no_lf_button := MenuButton{text: ""}
                                menu_camel_button := MenuButton{text: ""}
                            }
                        }

                        search_card := View{
                            width: Fill
                            height: 40
                            flow: Right
                            spacing: theme.space_1
                            padding: theme.mspace_3{left: theme.space_1, right: theme.space_1, top: theme.space_1, bottom: theme.space_1}
                            align: Align{y: 0.5}
                            groups_button := MenuButton{text: ""}
                            search_input := TextInput{width: Fill height: 32 empty_text: ""}
                            clear_search_button := DenseButton{text: ""}
                            system_menu_button := MenuButton{text: ""}
                        }
                    }

                    settings_page := ScrollYView{
                        width: Fill
                        height: Fill
                        visible: false
                        flow: Down
                        spacing: theme.space_2
                        padding: theme.mspace_3{left: theme.space_2, right: theme.space_2, top: theme.space_2, bottom: theme.space_2}

                        settings_header := AppCard{
                            width: Fill
                            flow: Right
                            spacing: theme.space_2
                            align: Align{y: 0.5}
                            settings_title_group := View{
                                width: Fill
                                height: Fit
                                flow: Down
                                spacing: theme.space_1
                                settings_title := SectionTitle{text: ""}
                                settings_subtitle := MutedLabel{text: ""}
                            }
                            back_to_main_button := DenseButton{text: ""}
                        }

                        settings_tabs := AppCard{
                            width: Fill
                            flow: Down
                            spacing: theme.space_1
                            tab_row_1 := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                settings_general_tab := TabButton{text: ""}
                                settings_types_tab := TabButton{text: ""}
                                settings_keyboard_tab := TabButton{text: ""}
                            }
                            tab_row_2 := View{
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: theme.space_1
                                settings_quick_paste_tab := TabButton{text: ""}
                                settings_sync_tab := TabButton{text: ""}
                                settings_about_tab := TabButton{text: ""}
                            }
                        }

                        appearance_settings := AppCard{
                            width: Fill
                            appearance_title := SectionTitle{text: ""}
                            FieldRow{
                                language_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                language_button := DenseButton{text: ""}
                                theme_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                theme_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                start_on_login_button := DenseButton{text: ""}
                                show_tray_button := DenseButton{text: ""}
                                show_taskbar_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                popup_position_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                popup_position_button := DenseButton{text: ""}
                            }
                        }

                        behavior_settings := AppCard{
                            width: Fill
                            behavior_title := SectionTitle{text: ""}
                            local_status := TextBox{width: Fill height: Fit text: ""}
                            FieldRow{
                                device_id_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                device_id_value := TextInput{width: Fill height: 34 empty_text: "" is_read_only: true}
                                copy_device_id_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                duplicate_policy_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                duplicate_policy_button := DenseButton{text: ""}
                            }
                            FieldGroup{
                                capture_formats_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                FieldRow{
                                    capture_text_button := DenseButton{text: ""}
                                    capture_html_button := DenseButton{text: ""}
                                    capture_image_button := DenseButton{text: ""}
                                    capture_files_button := DenseButton{text: ""}
                                }
                            }
                            FieldRow{
                                max_history_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                max_history_input := TextInput{width: 120 height: 34 empty_text: ""}
                                save_behavior_button := ActionButton{text: ""}
                            }
                            FieldRow{
                                capture_interval_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                capture_interval_input := TextInput{width: 120 height: 34 empty_text: ""}
                            }
                        }

                        quick_paste_settings := AppCard{
                            width: Fill
                            quick_paste_title := SectionTitle{text: ""}
                            FieldRow{
                                show_hotkey_text_button := DenseButton{text: ""}
                                show_leading_ws_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                find_as_you_type_button := DenseButton{text: ""}
                                show_thumbnails_button := DenseButton{text: ""}
                                draw_rtf_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                ensure_visible_button := DenseButton{text: ""}
                                show_groups_main_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                prompt_delete_button := DenseButton{text: ""}
                                always_show_scrollbar_button := DenseButton{text: ""}
                                show_pasted_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                elevated_paste_button := DenseButton{text: ""}
                            }
                            FieldRow{
                                lines_per_row_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                lines_per_row_input := TextInput{width: 90 height: 34 empty_text: ""}
                                save_quick_paste_button := ActionButton{text: ""}
                            }
                            FieldRow{
                                transparency_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                transparency_input := TextInput{width: 90 height: 34 empty_text: ""}
                            }
                        }

                        sync_settings := AppCard{
                            width: Fill
                            sync_settings_title := SectionTitle{text: ""}
                            FieldGroup{
                                server_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                server_input := TextInput{width: Fill height: 34 empty_text: ""}
                            }
                            FieldGroup{
                                token_label := Label{text: "" draw_text.color: theme.color_label_inner}
                                token_input := TextInput{width: Fill height: 34 empty_text: "" is_password: true}
                            }
                            save_settings_button := ActionButton{width: Fit text: ""}
                        }

                        hotkeys_settings := AppCard{
                            width: Fill
                            hotkeys_title := SectionTitle{text: ""}
                            hotkeys_status := MutedLabel{text: ""}
                            hotkey_grid := View{
                                width: Fill
                                height: Fit
                                flow: Down
                                spacing: theme.space_2
                                FieldRow{hotkey_show_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_show_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_search_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_search_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_copy_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_copy_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_delete_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_delete_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_pin_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_pin_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_edit_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_edit_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_capture_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_capture_input := TextInput{width: Fill height: 34 empty_text: ""}}
                                FieldRow{hotkey_sync_label := Label{width: 180 text: "" draw_text.color: theme.color_label_inner} hotkey_sync_input := TextInput{width: Fill height: 34 empty_text: ""}}
                            }
                            save_hotkeys_button := ActionButton{width: Fit text: ""}
                        }

                        about_settings := AppCard{
                            width: Fill
                            about_title := SectionTitle{text: ""}
                            about_text := MutedLabel{text: ""}
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    state: Option<ClientState>,
    #[rust]
    active_page: ClientPage,
    #[rust]
    active_settings_tab: SettingsTab,
    #[rust]
    clip_filter: ClipFilter,
    #[rust]
    menu_visible: bool,
    #[rust]
    group_panel_visible: bool,
    #[rust]
    editor_visible: bool,
    #[rust]
    pending_delete_id: Option<String>,
    #[rust]
    initialized: bool,
    #[rust]
    poll_timer: Timer,
    #[rust]
    tray_timer: Timer,
    #[rust]
    tray_rx: Option<Receiver<TrayCommand>>,
    #[cfg(target_os = "linux")]
    #[rust]
    tray_handle: Option<ksni::blocking::Handle<YankTray>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ClientPage {
    #[default]
    Main,
    Settings,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SettingsTab {
    #[default]
    General,
    Types,
    Keyboard,
    QuickPaste,
    Sync,
    About,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ClipFilter {
    #[default]
    All,
    Pinned,
    Text,
    Images,
    Files,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuickPastePosition {
    Cursor,
    Caret,
    Previous,
}

impl QuickPastePosition {
    fn parse(value: &str) -> Self {
        match value {
            "caret" => Self::Caret,
            "previous" => Self::Previous,
            _ => Self::Cursor,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Caret => "caret",
            Self::Previous => "previous",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Cursor => Self::Caret,
            Self::Caret => Self::Previous,
            Self::Previous => Self::Cursor,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextTransform {
    Upper,
    Lower,
    Trim,
    RemoveLineFeeds,
    CamelCase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayCommand {
    Open,
    Settings,
    CaptureNow,
    SyncNow,
    ToggleCapture,
    Exit,
}

#[cfg(target_os = "linux")]
struct TrayLabels {
    title: String,
    open: String,
    options: String,
    capture: String,
    sync: String,
    pause: String,
    resume: String,
    exit: String,
}

#[cfg(target_os = "linux")]
impl TrayLabels {
    fn from_messages(messages: &I18nBundle) -> Self {
        Self {
            title: messages.text("app.title").to_owned(),
            open: messages.text("app.tray_open").to_owned(),
            options: messages.text("app.tray_options").to_owned(),
            capture: messages.text("app.tray_capture").to_owned(),
            sync: messages.text("app.tray_sync").to_owned(),
            pause: messages.text("app.tray_pause").to_owned(),
            resume: messages.text("app.tray_resume").to_owned(),
            exit: messages.text("app.tray_exit").to_owned(),
        }
    }
}

#[cfg(target_os = "linux")]
struct YankTray {
    sender: mpsc::Sender<TrayCommand>,
    capture_enabled: bool,
    labels: TrayLabels,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for YankTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "yank".to_owned()
    }

    fn title(&self) -> String {
        self.labels.title.clone()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![tray_icon_pixmap(32), tray_icon_pixmap(64)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.labels.title.clone(),
            description: self.labels.title.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayCommand::Open);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, StandardItem};

        let open = self.sender.clone();
        let settings = self.sender.clone();
        let capture = self.sender.clone();
        let sync = self.sender.clone();
        let toggle = self.sender.clone();
        let exit = self.sender.clone();

        vec![
            StandardItem {
                label: self.labels.open.clone(),
                icon_name: "window-new".to_owned(),
                activate: Box::new(move |_| {
                    let _ = open.send(TrayCommand::Open);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.labels.options.clone(),
                icon_name: "preferences-system".to_owned(),
                activate: Box::new(move |_| {
                    let _ = settings.send(TrayCommand::Settings);
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: self.labels.capture.clone(),
                icon_name: "document-save".to_owned(),
                activate: Box::new(move |_| {
                    let _ = capture.send(TrayCommand::CaptureNow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.labels.sync.clone(),
                icon_name: "view-refresh".to_owned(),
                activate: Box::new(move |_| {
                    let _ = sync.send(TrayCommand::SyncNow);
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: if self.capture_enabled {
                    self.labels.pause.clone()
                } else {
                    self.labels.resume.clone()
                },
                checked: self.capture_enabled,
                activate: Box::new(move |tray: &mut YankTray| {
                    tray.capture_enabled = !tray.capture_enabled;
                    let _ = toggle.send(TrayCommand::ToggleCapture);
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: self.labels.exit.clone(),
                icon_name: "application-exit".to_owned(),
                activate: Box::new(move |_| {
                    let _ = exit.send(TrayCommand::Exit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[cfg(target_os = "linux")]
fn tray_icon_pixmap(size: i32) -> ksni::Icon {
    let size = size.max(16);
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let offset = ((y * size + x) * 4) as usize;
            let border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
            let sheet = x >= size / 5 && x <= size * 4 / 5 && y >= size / 6 && y <= size * 5 / 6;
            let clip = x >= size / 3 && x <= size * 2 / 3 && y >= size / 10 && y <= size / 4;
            let line = sheet && y % 7 == 0 && x > size / 3 && x < size * 2 / 3;

            let (a, r, g, b) = if border {
                (255, 45, 45, 55)
            } else if clip {
                (255, 76, 110, 245)
            } else if line {
                (255, 70, 76, 88)
            } else if sheet {
                (255, 238, 240, 245)
            } else {
                (0, 0, 0, 0)
            };
            data[offset] = a;
            data[offset + 1] = r;
            data[offset + 2] = g;
            data[offset + 3] = b;
        }
    }
    ksni::Icon {
        width: size,
        height: size,
        data,
    }
}

fn startup_theme() -> Theme {
    paths::database_path()
        .ok()
        .and_then(|path| Store::open(path).ok())
        .and_then(|store| store.settings().ok())
        .map(|settings| settings.theme)
        .unwrap_or_default()
}

fn register_makepad_widgets(vm: &mut ScriptVm, theme: Theme) {
    makepad_widgets::theme_mod(vm);
    apply_makepad_theme_to_vm(vm, theme);
    makepad_widgets::widgets_mod(vm);
}

fn apply_makepad_theme_to_vm(vm: &mut ScriptVm, theme: Theme) {
    match theme {
        Theme::Light => {
            script_eval!(vm, {
                mod.theme = mod.themes.light
            });
        }
        Theme::Dark => {
            script_eval!(vm, {
                mod.theme = mod.themes.dark
            });
        }
    }
}

fn apply_makepad_theme_to_cx(cx: &mut Cx, theme: Theme) {
    match theme {
        Theme::Light => {
            script_eval!(cx, {
                mod.theme = mod.themes.light
                mod.prelude.widgets_internal.theme = mod.themes.light
                mod.prelude.widgets.theme = mod.themes.light
            });
        }
        Theme::Dark => {
            script_eval!(cx, {
                mod.theme = mod.themes.dark
                mod.prelude.widgets_internal.theme = mod.themes.dark
                mod.prelude.widgets.theme = mod.themes.dark
            });
        }
    }
    cx.request_script_reapply();
    cx.redraw_all();
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        register_makepad_widgets(vm, startup_theme());
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if !self.initialized {
            self.initialize(cx);
        }

        self.drain_tray_commands(cx);

        if self.handle_type_to_search(cx, event) {
            return;
        }

        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some(query) = self.text_input(cx, ids!(search_input)).changed(actions) {
            if let Some(state) = &mut self.state {
                state.query = query;
            }
            self.refresh_history(cx);
        }

        if self
            .text_input(cx, ids!(search_input))
            .returned(actions)
            .is_some()
        {
            self.copy_selected(cx);
        }

        if self
            .text_input(cx, ids!(edit_input))
            .returned(actions)
            .is_some()
        {
            self.save_selected_edit(cx);
        }

        for index in 0..HISTORY_ROWS {
            if self.button(cx, row_id(index)).clicked(actions) {
                self.select_clip_by_index(cx, index);
            }
        }

        if self.button(cx, ids!(clear_search_button)).clicked(actions) {
            self.clear_search(cx);
        }
        if self.button(cx, ids!(groups_button)).clicked(actions) {
            self.toggle_group_panel(cx);
        }
        if self.button(cx, ids!(system_menu_button)).clicked(actions) {
            self.toggle_menu_panel(cx);
        }
        if self.button(cx, ids!(filter_all_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::All);
        }
        if self.button(cx, ids!(filter_pinned_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Pinned);
        }
        if self.button(cx, ids!(filter_text_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Text);
        }
        if self.button(cx, ids!(filter_image_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Images);
        }
        if self.button(cx, ids!(filter_files_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Files);
        }
        if self.button(cx, ids!(group_history_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::All);
        }
        if self.button(cx, ids!(group_pinned_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Pinned);
        }
        if self.button(cx, ids!(group_text_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Text);
        }
        if self.button(cx, ids!(group_image_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Images);
        }
        if self.button(cx, ids!(group_files_button)).clicked(actions) {
            self.set_clip_filter(cx, ClipFilter::Files);
        }
        if self
            .button(cx, ids!(copy_device_id_button))
            .clicked(actions)
        {
            self.copy_device_id(cx);
        }
        if self
            .button(cx, ids!(duplicate_policy_button))
            .clicked(actions)
        {
            self.toggle_duplicate_policy(cx);
        }
        if self.button(cx, ids!(capture_text_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Text);
        }
        if self.button(cx, ids!(capture_html_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Html);
        }
        if self.button(cx, ids!(capture_image_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Image);
        }
        if self.button(cx, ids!(capture_files_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Files);
        }
        if self
            .button(cx, ids!(menu_capture_toggle_button))
            .clicked(actions)
        {
            self.toggle_capture(cx);
        }
        if self.button(cx, ids!(menu_capture_button)).clicked(actions) {
            self.capture_clipboard(cx);
        }
        if self.button(cx, ids!(copy_selected_button)).clicked(actions)
            || self.button(cx, ids!(menu_copy_button)).clicked(actions)
        {
            self.copy_selected(cx);
        }
        if self
            .button(cx, ids!(menu_paste_plain_button))
            .clicked(actions)
        {
            self.copy_selected_plain_text(cx);
        }
        if self.button(cx, ids!(menu_edit_button)).clicked(actions) {
            self.show_editor(cx);
        }
        if self.button(cx, ids!(menu_refresh_button)).clicked(actions) {
            self.refresh_history(cx);
            self.set_status(cx, "app.status_refreshed");
        }
        if self.button(cx, ids!(menu_exit_button)).clicked(actions) {
            std::process::exit(0);
        }
        if self.button(cx, ids!(save_edit_button)).clicked(actions) {
            self.save_selected_edit(cx);
        }
        if self.button(cx, ids!(pin_button)).clicked(actions)
            || self.button(cx, ids!(menu_pin_button)).clicked(actions)
        {
            self.toggle_selected_pin(cx);
        }
        if self.button(cx, ids!(delete_button)).clicked(actions)
            || self.button(cx, ids!(menu_delete_button)).clicked(actions)
        {
            self.delete_selected(cx);
        }
        if self.button(cx, ids!(menu_sync_button)).clicked(actions) {
            self.sync_now(cx);
        }
        if self.button(cx, ids!(settings_button)).clicked(actions) {
            self.show_settings_page(cx);
        }
        if self.button(cx, ids!(menu_options_button)).clicked(actions) {
            self.show_settings_page(cx);
        }
        if self.button(cx, ids!(menu_groups_button)).clicked(actions) {
            self.toggle_group_panel(cx);
        }
        if self.button(cx, ids!(menu_position_button)).clicked(actions) {
            self.cycle_popup_position(cx);
        }
        if self.button(cx, ids!(menu_lines_button)).clicked(actions) {
            self.cycle_lines_per_row(cx);
        }
        if self
            .button(cx, ids!(menu_transparency_button))
            .clicked(actions)
        {
            self.cycle_transparency(cx);
        }
        if self.button(cx, ids!(menu_upper_button)).clicked(actions) {
            self.copy_selected_transformed(cx, TextTransform::Upper);
        }
        if self.button(cx, ids!(menu_lower_button)).clicked(actions) {
            self.copy_selected_transformed(cx, TextTransform::Lower);
        }
        if self.button(cx, ids!(menu_trim_button)).clicked(actions) {
            self.copy_selected_transformed(cx, TextTransform::Trim);
        }
        if self.button(cx, ids!(menu_no_lf_button)).clicked(actions) {
            self.copy_selected_transformed(cx, TextTransform::RemoveLineFeeds);
        }
        if self.button(cx, ids!(menu_camel_button)).clicked(actions) {
            self.copy_selected_transformed(cx, TextTransform::CamelCase);
        }
        if self.button(cx, ids!(back_to_main_button)).clicked(actions) {
            self.show_main_page(cx);
        }
        if self.button(cx, ids!(settings_general_tab)).clicked(actions) {
            self.show_settings_tab(cx, SettingsTab::General);
        }
        if self.button(cx, ids!(settings_types_tab)).clicked(actions) {
            self.show_settings_tab(cx, SettingsTab::Types);
        }
        if self
            .button(cx, ids!(settings_keyboard_tab))
            .clicked(actions)
        {
            self.show_settings_tab(cx, SettingsTab::Keyboard);
        }
        if self
            .button(cx, ids!(settings_quick_paste_tab))
            .clicked(actions)
        {
            self.show_settings_tab(cx, SettingsTab::QuickPaste);
        }
        if self.button(cx, ids!(settings_sync_tab)).clicked(actions) {
            self.show_settings_tab(cx, SettingsTab::Sync);
        }
        if self.button(cx, ids!(settings_about_tab)).clicked(actions) {
            self.show_settings_tab(cx, SettingsTab::About);
        }
        if self.button(cx, ids!(theme_button)).clicked(actions) {
            self.toggle_theme(cx);
        }
        if self.button(cx, ids!(language_button)).clicked(actions) {
            self.toggle_language(cx);
        }
        if self
            .button(cx, ids!(start_on_login_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.start_on_login);
        }
        if self.button(cx, ids!(show_tray_button)).clicked(actions) {
            self.toggle_bool_setting(cx, |settings| &mut settings.show_tray_icon);
            self.apply_tray_visibility();
        }
        if self.button(cx, ids!(show_taskbar_button)).clicked(actions) {
            self.toggle_bool_setting(cx, |settings| &mut settings.show_in_taskbar);
        }
        if self
            .button(cx, ids!(popup_position_button))
            .clicked(actions)
        {
            self.cycle_popup_position(cx);
        }
        if self
            .button(cx, ids!(find_as_you_type_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_find_as_you_type);
        }
        if self
            .button(cx, ids!(show_hotkey_text_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_show_hotkey_text);
        }
        if self
            .button(cx, ids!(show_leading_ws_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| {
                &mut settings.quick_paste_show_leading_whitespace
            });
        }
        if self
            .button(cx, ids!(show_thumbnails_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_show_thumbnails);
        }
        if self.button(cx, ids!(draw_rtf_button)).clicked(actions) {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_draw_rtf);
        }
        if self.button(cx, ids!(prompt_delete_button)).clicked(actions) {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_prompt_delete);
        }
        if self
            .button(cx, ids!(ensure_visible_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_ensure_visible);
        }
        if self
            .button(cx, ids!(show_groups_main_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_show_groups_in_main);
        }
        if self
            .button(cx, ids!(always_show_scrollbar_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| {
                &mut settings.quick_paste_always_show_scrollbar
            });
        }
        if self.button(cx, ids!(show_pasted_button)).clicked(actions) {
            self.toggle_bool_setting(cx, |settings| {
                &mut settings.quick_paste_show_pasted_indicator
            });
        }
        if self
            .button(cx, ids!(elevated_paste_button))
            .clicked(actions)
        {
            self.toggle_bool_setting(cx, |settings| &mut settings.quick_paste_elevated_paste);
        }
        if self.button(cx, ids!(save_behavior_button)).clicked(actions) {
            self.save_behavior_settings(cx);
        }
        if self
            .button(cx, ids!(save_quick_paste_button))
            .clicked(actions)
        {
            self.save_quick_paste_settings(cx);
        }
        if self.button(cx, ids!(save_settings_button)).clicked(actions) {
            self.save_connection_settings(cx);
        }
        if self.button(cx, ids!(save_hotkeys_button)).clicked(actions) {
            self.save_hotkey_settings(cx);
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.poll_timer.is_timer(event).is_some() {
            self.poll_clipboard(cx);
        }
        if self.tray_timer.is_timer(event).is_some() {
            self.drain_tray_commands(cx);
        }
    }

    fn handle_key_down(&mut self, cx: &mut Cx, event: &KeyEvent) {
        if self.active_page == ClientPage::Settings && event.key_code == KeyCode::Escape {
            self.show_main_page(cx);
            return;
        }

        if self.active_page == ClientPage::Settings {
            if self.shortcut_matches(|settings| &settings.hotkey_show_history, event)
                || self.shortcut_matches(|settings| &settings.hotkey_search, event)
            {
                self.show_main_page(cx);
            }
            return;
        }

        if self.handle_ditto_style_key(cx, event) {
            return;
        }

        if self.search_navigation_key(cx, event) {
            match event.key_code {
                KeyCode::ArrowUp => {
                    self.select_relative_clip(cx, -1);
                    return;
                }
                KeyCode::ArrowDown => {
                    self.select_relative_clip(cx, 1);
                    return;
                }
                KeyCode::PageUp => {
                    self.select_relative_clip(cx, -(HISTORY_PAGE_STEP as isize));
                    return;
                }
                KeyCode::PageDown => {
                    self.select_relative_clip(cx, HISTORY_PAGE_STEP as isize);
                    return;
                }
                _ => {}
            }
        }

        if self.shortcut_matches(|settings| &settings.hotkey_show_history, event)
            || self.shortcut_matches(|settings| &settings.hotkey_search, event)
        {
            self.show_main_page(cx);
            self.refresh_history(cx);
            self.widget(cx, ids!(search_input)).set_key_focus(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_copy_selected, event) {
            if self.detail_editor_has_focus(cx) {
                return;
            }
            self.copy_selected(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_delete_selected, event) {
            if self.text_entry_has_focus(cx) {
                return;
            }
            self.delete_selected(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_toggle_pin, event) {
            if self.text_entry_has_focus(cx) {
                return;
            }
            self.toggle_selected_pin(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_edit_selected, event) {
            self.widget(cx, ids!(edit_input)).set_key_focus(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_capture_now, event) {
            self.capture_clipboard(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_sync_now, event) {
            self.sync_now(cx);
            return;
        }

        if event.modifiers.is_primary()
            && let Some(index) = paste_position_key_index(event.key_code)
        {
            self.select_clip_by_index(cx, index);
            if event.modifiers.shift {
                self.copy_selected_plain_text(cx);
            } else {
                self.copy_selected(cx);
            }
            return;
        }

        if self.active_page == ClientPage::Main
            && self.history_list_has_focus(cx)
            && let Some(index) = number_key_index(event.key_code)
        {
            self.select_clip_by_index(cx, index);
            self.copy_selected(cx);
        }
    }
}

impl App {
    fn initialize(&mut self, cx: &mut Cx) {
        self.initialized = true;
        match ClientState::load() {
            Ok(state) => {
                self.state = Some(state);
                self.start_tray();
                self.restart_tray_timer(cx);
                self.restart_poll_timer(cx);
                self.apply_i18n(cx);
                self.refresh_history(cx);
                self.set_status(cx, "app.status_local_ready");
                self.apply_page_visibility(cx);
            }
            Err(error) => {
                self.state = Some(ClientState::fallback(error.to_string()));
                self.start_tray();
                self.restart_tray_timer(cx);
                self.restart_poll_timer(cx);
                self.apply_i18n(cx);
                self.refresh_history(cx);
                self.set_status(cx, "app.status_startup_fallback");
                self.apply_page_visibility(cx);
            }
        }
    }

    fn button(&self, cx: &Cx, id: &[LiveId]) -> ButtonRef {
        self.ui.button(cx, id)
    }

    fn text_input(&self, cx: &Cx, id: &[LiveId]) -> TextInputRef {
        self.ui.text_input(cx, id)
    }

    fn widget(&self, cx: &Cx, id: &[LiveId]) -> WidgetRef {
        self.ui.widget(cx, id)
    }

    fn handle_type_to_search(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Event::TextInput(input) = event else {
            return false;
        };
        if self.active_page != ClientPage::Main
            || input.input.is_empty()
            || self.text_entry_has_focus(cx)
        {
            return false;
        }
        if !self
            .state
            .as_ref()
            .map(|state| state.settings.quick_paste_find_as_you_type)
            .unwrap_or(true)
        {
            return false;
        }
        if input.input.chars().any(char::is_control) {
            return false;
        }

        let mut query = None;
        if let Some(state) = &mut self.state {
            state.query.push_str(&input.input);
            query = Some(state.query.clone());
        }
        if let Some(query) = query {
            self.widget(cx, ids!(search_input)).set_text(cx, &query);
        }
        self.widget(cx, ids!(search_input)).set_key_focus(cx);
        self.refresh_history(cx);
        true
    }

    fn handle_ditto_style_key(&mut self, cx: &mut Cx, event: &KeyEvent) -> bool {
        let search_focus = self.search_has_focus(cx);
        let edit_focus = self.detail_editor_has_focus(cx);

        if edit_focus {
            if event.modifiers.is_primary() && event.key_code == KeyCode::ReturnKey {
                self.save_selected_edit(cx);
                return true;
            }
            if event.key_code == KeyCode::Escape {
                self.widget(cx, ids!(search_input)).set_key_focus(cx);
                return true;
            }
            return false;
        }

        match event.key_code {
            KeyCode::Escape => {
                if self.query_is_empty() {
                    self.set_status(cx, "app.status_ready");
                } else {
                    self.clear_search(cx);
                    self.set_status(cx, "app.status_filter_cleared");
                }
                self.widget(cx, ids!(search_input)).set_key_focus(cx);
                true
            }
            KeyCode::ReturnKey if event.modifiers.shift => {
                self.copy_selected_plain_text(cx);
                true
            }
            KeyCode::ReturnKey if !event.modifiers.is_primary() && !event.modifiers.alt => {
                self.copy_selected(cx);
                true
            }
            KeyCode::Delete if !search_focus => {
                self.delete_selected(cx);
                true
            }
            KeyCode::F5 => {
                self.refresh_history(cx);
                self.set_status(cx, "app.status_refreshed");
                true
            }
            KeyCode::Home if !search_focus => {
                self.select_first_clip(cx);
                true
            }
            KeyCode::End if !search_focus => {
                self.select_last_clip(cx);
                true
            }
            KeyCode::Backspace if !search_focus && !self.query_is_empty() => {
                self.clear_search(cx);
                self.set_status(cx, "app.status_filter_cleared");
                true
            }
            KeyCode::KeyC if event.modifiers.is_primary() && !search_focus => {
                self.copy_selected(cx);
                true
            }
            KeyCode::KeyF if event.modifiers.is_primary() => {
                self.widget(cx, ids!(search_input)).set_key_focus(cx);
                true
            }
            _ => false,
        }
    }

    fn search_has_focus(&self, cx: &Cx) -> bool {
        self.widget(cx, ids!(search_input)).key_focus(cx)
    }

    fn detail_editor_has_focus(&self, cx: &Cx) -> bool {
        self.widget(cx, ids!(edit_input)).key_focus(cx)
    }

    fn text_entry_has_focus(&self, cx: &Cx) -> bool {
        [
            ids!(search_input),
            ids!(edit_input),
            ids!(device_id_value),
            ids!(max_history_input),
            ids!(capture_interval_input),
            ids!(lines_per_row_input),
            ids!(transparency_input),
            ids!(server_input),
            ids!(token_input),
            ids!(hotkey_show_input),
            ids!(hotkey_search_input),
            ids!(hotkey_copy_input),
            ids!(hotkey_delete_input),
            ids!(hotkey_pin_input),
            ids!(hotkey_edit_input),
            ids!(hotkey_capture_input),
            ids!(hotkey_sync_input),
        ]
        .into_iter()
        .any(|id| self.widget(cx, id).key_focus(cx))
    }

    fn history_list_has_focus(&self, cx: &Cx) -> bool {
        (0..HISTORY_ROWS).any(|index| self.widget(cx, row_id(index)).key_focus(cx))
    }

    fn search_navigation_key(&self, cx: &Cx, event: &KeyEvent) -> bool {
        if self.detail_editor_has_focus(cx) {
            return false;
        }

        matches!(
            event.key_code,
            KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::PageUp | KeyCode::PageDown
        )
    }

    fn query_is_empty(&self) -> bool {
        self.state
            .as_ref()
            .map(|state| state.query.is_empty())
            .unwrap_or(true)
    }

    fn show_main_page(&mut self, cx: &mut Cx) {
        self.active_page = ClientPage::Main;
        self.menu_visible = false;
        self.group_panel_visible = false;
        self.apply_page_visibility(cx);
        self.widget(cx, ids!(search_input)).set_key_focus(cx);
    }

    fn show_settings_page(&mut self, cx: &mut Cx) {
        self.active_page = ClientPage::Settings;
        self.menu_visible = false;
        self.group_panel_visible = false;
        self.apply_page_visibility(cx);
        self.apply_settings_tab_visibility(cx);
    }

    fn show_settings_tab(&mut self, cx: &mut Cx, tab: SettingsTab) {
        self.active_settings_tab = tab;
        self.apply_settings_tab_visibility(cx);
        if let Some(state) = self.state.as_ref() {
            self.apply_settings_tab_labels(cx, &state.messages);
        }
    }

    fn apply_page_visibility(&mut self, cx: &mut Cx) {
        let settings_visible = self.active_page == ClientPage::Settings;
        self.widget(cx, ids!(main_page))
            .set_visible(cx, !settings_visible);
        self.widget(cx, ids!(settings_page))
            .set_visible(cx, settings_visible);
        self.widget(cx, ids!(menu_panel))
            .set_visible(cx, self.menu_visible && !settings_visible);
        self.widget(cx, ids!(group_panel))
            .set_visible(cx, self.group_panel_visible && !settings_visible);
        self.widget(cx, ids!(editor_panel))
            .set_visible(cx, self.editor_visible && !settings_visible);
        self.ui.redraw(cx);
    }

    fn apply_settings_tab_visibility(&mut self, cx: &mut Cx) {
        let tab = self.active_settings_tab;
        self.widget(cx, ids!(appearance_settings))
            .set_visible(cx, matches!(tab, SettingsTab::General));
        self.widget(cx, ids!(behavior_settings))
            .set_visible(cx, matches!(tab, SettingsTab::Types));
        self.widget(cx, ids!(hotkeys_settings))
            .set_visible(cx, matches!(tab, SettingsTab::Keyboard));
        self.widget(cx, ids!(quick_paste_settings))
            .set_visible(cx, matches!(tab, SettingsTab::QuickPaste));
        self.widget(cx, ids!(sync_settings))
            .set_visible(cx, matches!(tab, SettingsTab::Sync));
        self.widget(cx, ids!(about_settings))
            .set_visible(cx, matches!(tab, SettingsTab::About));
        self.ui.redraw(cx);
    }

    fn toggle_menu_panel(&mut self, cx: &mut Cx) {
        self.menu_visible = !self.menu_visible;
        if self.menu_visible {
            self.group_panel_visible = false;
        }
        self.apply_page_visibility(cx);
    }

    fn toggle_group_panel(&mut self, cx: &mut Cx) {
        self.group_panel_visible = !self.group_panel_visible;
        if self.group_panel_visible {
            self.menu_visible = false;
        }
        self.apply_page_visibility(cx);
    }

    fn show_editor(&mut self, cx: &mut Cx) {
        self.editor_visible = true;
        self.apply_page_visibility(cx);
        self.widget(cx, ids!(edit_input)).set_key_focus(cx);
    }

    fn text(&self, key: &str) -> String {
        self.state
            .as_ref()
            .map(|state| state.messages.text(key).to_owned())
            .unwrap_or_else(|| key.to_owned())
    }

    fn template(&self, key: &str, values: &[(&str, String)]) -> String {
        let mut text = self.text(key);
        for (name, value) in values {
            text = text.replace(name, value);
        }
        text
    }

    fn apply_i18n(&mut self, cx: &mut Cx) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let messages = &state.messages;

        for (id, key) in [
            (ids!(title), "app.title"),
            (ids!(subtitle), "app.subtitle"),
            (ids!(settings_button), "app.settings"),
            (ids!(language_button), "app.lang_toggle"),
            (ids!(clear_search_button), "app.clear_search"),
            (ids!(history_title), "app.latest"),
            (ids!(selected_title), "app.no_selection"),
            (ids!(edit_label), "app.edit_text"),
            (ids!(copy_selected_button), "app.copy_selected"),
            (ids!(save_edit_button), "app.save_edit"),
            (ids!(delete_button), "app.delete"),
            (ids!(settings_title), "app.settings"),
            (ids!(settings_subtitle), "app.settings_subtitle"),
            (ids!(back_to_main_button), "app.back_to_history"),
            (ids!(appearance_title), "app.appearance"),
            (ids!(behavior_title), "app.capture_history"),
            (ids!(language_label), "app.language"),
            (ids!(theme_label), "app.theme"),
            (ids!(device_id_label), "app.device_id"),
            (ids!(copy_device_id_button), "app.copy_device_id"),
            (ids!(duplicate_policy_label), "app.duplicate_policy"),
            (ids!(capture_formats_label), "app.capture_formats"),
            (ids!(max_history_label), "app.max_history"),
            (ids!(capture_interval_label), "app.capture_interval"),
            (ids!(save_behavior_button), "app.save_behavior"),
            (ids!(sync_settings_title), "app.sync_settings"),
            (ids!(server_label), "app.server"),
            (ids!(token_label), "app.token"),
            (ids!(save_settings_button), "app.save_settings"),
            (ids!(hotkeys_title), "app.hotkeys"),
            (ids!(hotkeys_status), "app.hotkeys_status"),
            (ids!(hotkey_show_label), "app.hotkey_show"),
            (ids!(hotkey_search_label), "app.hotkey_search"),
            (ids!(hotkey_copy_label), "app.hotkey_copy"),
            (ids!(hotkey_delete_label), "app.hotkey_delete"),
            (ids!(hotkey_pin_label), "app.hotkey_pin"),
            (ids!(hotkey_edit_label), "app.hotkey_edit"),
            (ids!(hotkey_capture_label), "app.hotkey_capture"),
            (ids!(hotkey_sync_label), "app.hotkey_sync"),
            (ids!(save_hotkeys_button), "app.save_hotkeys"),
            (ids!(filter_all_button), "app.filter_all"),
            (ids!(filter_pinned_button), "app.filter_pinned"),
            (ids!(filter_text_button), "app.filter_text"),
            (ids!(filter_image_button), "app.filter_images"),
            (ids!(filter_files_button), "app.filter_files"),
            (ids!(group_filter_title), "app.group_filter_title"),
            (ids!(group_history_button), "app.filter_all"),
            (ids!(group_pinned_button), "app.filter_pinned"),
            (ids!(group_text_button), "app.filter_text"),
            (ids!(group_image_button), "app.filter_images"),
            (ids!(group_files_button), "app.filter_files"),
            (ids!(system_menu_button), "app.menu"),
            (ids!(menu_copy_button), "app.copy_selected"),
            (ids!(menu_paste_plain_button), "app.paste_plain"),
            (ids!(menu_edit_button), "app.edit"),
            (ids!(menu_refresh_button), "app.refresh"),
            (ids!(menu_delete_button), "app.delete"),
            (ids!(menu_capture_button), "app.capture"),
            (ids!(menu_sync_button), "app.sync_now"),
            (ids!(menu_options_button), "app.settings"),
            (ids!(menu_exit_button), "app.tray_exit"),
            (ids!(menu_groups_button), "app.groups"),
            (ids!(settings_general_tab), "app.settings_general"),
            (ids!(settings_types_tab), "app.settings_types"),
            (ids!(settings_keyboard_tab), "app.settings_keyboard"),
            (ids!(settings_quick_paste_tab), "app.settings_quick_paste"),
            (ids!(settings_sync_tab), "app.sync_settings"),
            (ids!(settings_about_tab), "app.settings_about"),
            (ids!(popup_position_label), "app.popup_position"),
            (ids!(quick_paste_title), "app.quick_paste_options"),
            (ids!(lines_per_row_label), "app.lines_per_row"),
            (ids!(transparency_label), "app.transparency"),
            (ids!(save_quick_paste_button), "app.save_quick_paste"),
            (ids!(menu_upper_button), "app.special_upper"),
            (ids!(menu_lower_button), "app.special_lower"),
            (ids!(menu_trim_button), "app.special_trim"),
            (ids!(menu_no_lf_button), "app.special_no_lf"),
            (ids!(menu_camel_button), "app.special_camel"),
            (ids!(about_title), "app.settings_about"),
        ] {
            self.widget(cx, id).set_text(cx, messages.text(key));
        }

        self.apply_filter_labels(cx, messages);
        self.apply_settings_tab_labels(cx, messages);
        self.widget(cx, ids!(about_text))
            .set_text(cx, messages.text("app.about_text"));
        self.widget(cx, ids!(groups_button))
            .set_text(cx, &self.clip_filter_label(messages));
        self.widget(cx, ids!(menu_position_button)).set_text(
            cx,
            &self.template(
                "app.quick_menu_position",
                &[(
                    "{position}",
                    popup_position_label(messages, &state.settings.quick_paste_position).to_owned(),
                )],
            ),
        );
        self.widget(cx, ids!(menu_lines_button)).set_text(
            cx,
            &self.template(
                "app.quick_menu_lines",
                &[(
                    "{lines}",
                    state.settings.quick_paste_lines_per_row.to_string(),
                )],
            ),
        );
        self.widget(cx, ids!(menu_transparency_button)).set_text(
            cx,
            &self.template(
                "app.quick_menu_transparency",
                &[(
                    "{percent}",
                    state.settings.quick_paste_transparency_percent.to_string(),
                )],
            ),
        );

        self.widget(cx, ids!(theme_button)).set_text(
            cx,
            match state.settings.theme {
                Theme::Light => messages.text("app.dark"),
                Theme::Dark => messages.text("app.light"),
            },
        );
        self.widget(cx, ids!(menu_capture_toggle_button)).set_text(
            cx,
            if state.settings.capture_enabled {
                messages.text("app.capture_on")
            } else {
                messages.text("app.capture_off")
            },
        );
        self.widget(cx, ids!(menu_pin_button)).set_text(
            cx,
            if state
                .selected_clip()
                .map(|clip| clip.pinned)
                .unwrap_or(false)
            {
                messages.text("app.unpin")
            } else {
                messages.text("app.pin")
            },
        );
        self.widget(cx, ids!(start_on_login_button)).set_text(
            cx,
            if state.settings.start_on_login {
                messages.text("app.start_on_login_on")
            } else {
                messages.text("app.start_on_login_off")
            },
        );
        self.widget(cx, ids!(show_tray_button)).set_text(
            cx,
            if state.settings.show_tray_icon {
                messages.text("app.show_tray_on")
            } else {
                messages.text("app.show_tray_off")
            },
        );
        self.widget(cx, ids!(show_taskbar_button)).set_text(
            cx,
            if state.settings.show_in_taskbar {
                messages.text("app.show_taskbar_on")
            } else {
                messages.text("app.show_taskbar_off")
            },
        );
        self.widget(cx, ids!(popup_position_button)).set_text(
            cx,
            popup_position_label(messages, &state.settings.quick_paste_position),
        );
        self.widget(cx, ids!(find_as_you_type_button)).set_text(
            cx,
            if state.settings.quick_paste_find_as_you_type {
                messages.text("app.find_as_type_on")
            } else {
                messages.text("app.find_as_type_off")
            },
        );
        self.widget(cx, ids!(show_hotkey_text_button)).set_text(
            cx,
            if state.settings.quick_paste_show_hotkey_text {
                messages.text("app.hotkey_text_on")
            } else {
                messages.text("app.hotkey_text_off")
            },
        );
        self.widget(cx, ids!(show_leading_ws_button)).set_text(
            cx,
            if state.settings.quick_paste_show_leading_whitespace {
                messages.text("app.leading_ws_on")
            } else {
                messages.text("app.leading_ws_off")
            },
        );
        self.widget(cx, ids!(show_thumbnails_button)).set_text(
            cx,
            if state.settings.quick_paste_show_thumbnails {
                messages.text("app.thumbnails_on")
            } else {
                messages.text("app.thumbnails_off")
            },
        );
        self.widget(cx, ids!(draw_rtf_button)).set_text(
            cx,
            if state.settings.quick_paste_draw_rtf {
                messages.text("app.draw_rtf_on")
            } else {
                messages.text("app.draw_rtf_off")
            },
        );
        self.widget(cx, ids!(prompt_delete_button)).set_text(
            cx,
            if state.settings.quick_paste_prompt_delete {
                messages.text("app.prompt_delete_on")
            } else {
                messages.text("app.prompt_delete_off")
            },
        );
        self.widget(cx, ids!(ensure_visible_button)).set_text(
            cx,
            if state.settings.quick_paste_ensure_visible {
                messages.text("app.ensure_visible_on")
            } else {
                messages.text("app.ensure_visible_off")
            },
        );
        self.widget(cx, ids!(show_groups_main_button)).set_text(
            cx,
            if state.settings.quick_paste_show_groups_in_main {
                messages.text("app.groups_main_on")
            } else {
                messages.text("app.groups_main_off")
            },
        );
        self.widget(cx, ids!(always_show_scrollbar_button))
            .set_text(
                cx,
                if state.settings.quick_paste_always_show_scrollbar {
                    messages.text("app.scrollbar_on")
                } else {
                    messages.text("app.scrollbar_off")
                },
            );
        self.widget(cx, ids!(show_pasted_button)).set_text(
            cx,
            if state.settings.quick_paste_show_pasted_indicator {
                messages.text("app.pasted_indicator_on")
            } else {
                messages.text("app.pasted_indicator_off")
            },
        );
        self.widget(cx, ids!(elevated_paste_button)).set_text(
            cx,
            if state.settings.quick_paste_elevated_paste {
                messages.text("app.elevated_paste_on")
            } else {
                messages.text("app.elevated_paste_off")
            },
        );
        self.widget(cx, ids!(duplicate_policy_button)).set_text(
            cx,
            if state.settings.duplicate_moves_to_top {
                messages.text("app.duplicate_move_top")
            } else {
                messages.text("app.duplicate_keep_existing")
            },
        );
        for (id, enabled, on_key, off_key) in [
            (
                ids!(capture_text_button),
                state.settings.capture_text_enabled,
                "app.capture_text_on",
                "app.capture_text_off",
            ),
            (
                ids!(capture_html_button),
                state.settings.capture_html_enabled,
                "app.capture_html_on",
                "app.capture_html_off",
            ),
            (
                ids!(capture_image_button),
                state.settings.capture_image_enabled,
                "app.capture_image_on",
                "app.capture_image_off",
            ),
            (
                ids!(capture_files_button),
                state.settings.capture_files_enabled,
                "app.capture_files_on",
                "app.capture_files_off",
            ),
        ] {
            self.widget(cx, id).set_text(
                cx,
                if enabled {
                    messages.text(on_key)
                } else {
                    messages.text(off_key)
                },
            );
        }

        self.text_input(cx, ids!(search_input))
            .set_empty_text(cx, messages.text("app.search_placeholder").to_owned());
        self.text_input(cx, ids!(edit_input))
            .set_empty_text(cx, messages.text("app.edit_placeholder").to_owned());
        self.text_input(cx, ids!(server_input))
            .set_empty_text(cx, messages.text("app.server_placeholder").to_owned());
        self.text_input(cx, ids!(token_input))
            .set_empty_text(cx, messages.text("app.token_placeholder").to_owned());
        self.text_input(cx, ids!(max_history_input))
            .set_empty_text(cx, messages.text("app.max_history_placeholder").to_owned());
        self.text_input(cx, ids!(capture_interval_input))
            .set_empty_text(
                cx,
                messages.text("app.capture_interval_placeholder").to_owned(),
            );

        self.widget(cx, ids!(server_input))
            .set_text(cx, state.settings.server_url.as_deref().unwrap_or(""));
        self.widget(cx, ids!(token_input))
            .set_text(cx, state.settings.token.as_deref().unwrap_or(""));
        self.widget(cx, ids!(device_id_value))
            .set_text(cx, &state.settings.device_id);
        self.widget(cx, ids!(max_history_input))
            .set_text(cx, &state.settings.max_history.to_string());
        self.widget(cx, ids!(capture_interval_input))
            .set_text(cx, &state.settings.capture_interval_ms.to_string());
        self.widget(cx, ids!(lines_per_row_input))
            .set_text(cx, &state.settings.quick_paste_lines_per_row.to_string());
        self.widget(cx, ids!(transparency_input)).set_text(
            cx,
            &state.settings.quick_paste_transparency_percent.to_string(),
        );
        self.widget(cx, ids!(hotkey_show_input))
            .set_text(cx, &state.settings.hotkey_show_history);
        self.widget(cx, ids!(hotkey_search_input))
            .set_text(cx, &state.settings.hotkey_search);
        self.widget(cx, ids!(hotkey_copy_input))
            .set_text(cx, &state.settings.hotkey_copy_selected);
        self.widget(cx, ids!(hotkey_delete_input))
            .set_text(cx, &state.settings.hotkey_delete_selected);
        self.widget(cx, ids!(hotkey_pin_input))
            .set_text(cx, &state.settings.hotkey_toggle_pin);
        self.widget(cx, ids!(hotkey_edit_input))
            .set_text(cx, &state.settings.hotkey_edit_selected);
        self.widget(cx, ids!(hotkey_capture_input))
            .set_text(cx, &state.settings.hotkey_capture_now);
        self.widget(cx, ids!(hotkey_sync_input))
            .set_text(cx, &state.settings.hotkey_sync_now);

        self.refresh_local_status(cx);
        self.refresh_detail(cx);
        self.apply_page_visibility(cx);
        self.apply_settings_tab_visibility(cx);
        self.sync_tray_state();
        self.ui.redraw(cx);
    }

    fn apply_filter_labels(&self, cx: &mut Cx, messages: &I18nBundle) {
        for (id, filter, key) in [
            (ids!(filter_all_button), ClipFilter::All, "app.filter_all"),
            (
                ids!(filter_pinned_button),
                ClipFilter::Pinned,
                "app.filter_pinned",
            ),
            (
                ids!(filter_text_button),
                ClipFilter::Text,
                "app.filter_text",
            ),
            (
                ids!(filter_image_button),
                ClipFilter::Images,
                "app.filter_images",
            ),
            (
                ids!(filter_files_button),
                ClipFilter::Files,
                "app.filter_files",
            ),
        ] {
            let text = if self.clip_filter == filter {
                messages
                    .text("app.active_choice")
                    .replace("{label}", messages.text(key))
            } else {
                messages.text(key).to_owned()
            };
            self.widget(cx, id).set_text(cx, &text);
        }
    }

    fn apply_settings_tab_labels(&self, cx: &mut Cx, messages: &I18nBundle) {
        for (id, tab, key) in [
            (
                ids!(settings_general_tab),
                SettingsTab::General,
                "app.settings_general",
            ),
            (
                ids!(settings_types_tab),
                SettingsTab::Types,
                "app.settings_types",
            ),
            (
                ids!(settings_keyboard_tab),
                SettingsTab::Keyboard,
                "app.settings_keyboard",
            ),
            (
                ids!(settings_quick_paste_tab),
                SettingsTab::QuickPaste,
                "app.settings_quick_paste",
            ),
            (
                ids!(settings_sync_tab),
                SettingsTab::Sync,
                "app.sync_settings",
            ),
            (
                ids!(settings_about_tab),
                SettingsTab::About,
                "app.settings_about",
            ),
        ] {
            let text = if self.active_settings_tab == tab {
                messages
                    .text("app.active_choice")
                    .replace("{label}", messages.text(key))
            } else {
                messages.text(key).to_owned()
            };
            self.widget(cx, id).set_text(cx, &text);
        }
    }

    fn refresh_local_status(&mut self, cx: &mut Cx) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let sync_status = if state.settings.sync_enabled {
            self.text("app.sync_enabled")
        } else {
            self.text("app.local_only")
        };
        let capture_status = if state.settings.capture_enabled {
            self.text("app.capture_enabled")
        } else {
            self.text("app.capture_paused")
        };
        let duplicate_status = if state.settings.duplicate_moves_to_top {
            self.text("app.duplicate_move_top")
        } else {
            self.text("app.duplicate_keep_existing")
        };
        let formats = enabled_capture_format_names(&state.messages, &state.settings);
        let status = self.template(
            "app.local_status",
            &[
                ("{sync}", sync_status),
                ("{capture}", capture_status),
                ("{duplicate}", duplicate_status),
                ("{formats}", formats),
                ("{interval}", state.settings.capture_interval_ms.to_string()),
                ("{max}", state.settings.max_history.to_string()),
            ],
        );
        self.widget(cx, ids!(local_status)).set_text(cx, &status);
    }

    fn refresh_history(&mut self, cx: &mut Cx) {
        let (clips, selected_id, count) = {
            let Some(state) = self.state.as_mut() else {
                return;
            };
            let limit = state.settings.max_history.max(HISTORY_ROWS as u32);
            let clips = state
                .store
                .search_clips(&state.query, limit)
                .unwrap_or_default()
                .into_iter()
                .filter(|clip| clip_matches_filter(clip, self.clip_filter))
                .take(HISTORY_ROWS)
                .collect::<Vec<_>>();
            let count = state
                .store
                .stats()
                .map(|stats| stats.clip_count)
                .unwrap_or(0);
            if !clips
                .iter()
                .any(|clip| Some(&clip.id) == state.selected_id.as_ref())
            {
                state.selected_id = clips.first().map(|clip| clip.id.clone());
            }
            state.history = clips.clone();
            (clips, state.selected_id.clone(), count)
        };

        self.widget(cx, ids!(clip_count)).set_text(
            cx,
            &self.template("app.history_count", &[("{count}", count.to_string())]),
        );

        for index in 0..HISTORY_ROWS {
            let id = row_id(index);
            if let Some(clip) = clips.get(index) {
                let selected = selected_id.as_deref() == Some(clip.id.as_str());
                let row_text = self.row_text(index, clip, selected);
                self.widget(cx, id).set_visible(cx, true);
                self.widget(cx, id).set_text(cx, &row_text);
            } else {
                self.widget(cx, id).set_visible(cx, index == 0);
                let empty_text = if index == 0 {
                    self.text("app.empty")
                } else {
                    String::new()
                };
                self.widget(cx, id).set_text(cx, &empty_text);
            }
        }

        self.refresh_detail(cx);
    }

    fn row_text(&self, index: usize, clip: &Clip, selected: bool) -> String {
        let show_leading_whitespace = self
            .state
            .as_ref()
            .map(|state| state.settings.quick_paste_show_leading_whitespace)
            .unwrap_or(false);
        let text = clip.primary_text.as_deref().map_or_else(
            || clip.description.clone(),
            |text| summarize_row_text(text, show_leading_whitespace),
        );
        let pin = if clip.pinned {
            self.text("app.pinned_marker")
        } else {
            String::new()
        };
        let types = self.row_type_text(clip);
        let source = clip.source_app.as_deref().unwrap_or("yank");
        let updated = format_timestamp(clip.updated_at);
        let key = if selected {
            "app.row_selected_template"
        } else {
            "app.row_template"
        };
        self.template(
            key,
            &[
                ("{index}", self.row_hotkey_text(index)),
                ("{pin}", pin),
                ("{types}", types),
                ("{source}", source.to_owned()),
                ("{updated}", updated),
                ("{text}", text),
            ],
        )
    }

    fn row_hotkey_text(&self, index: usize) -> String {
        if self
            .state
            .as_ref()
            .map(|state| state.settings.quick_paste_show_hotkey_text)
            .unwrap_or(true)
        {
            (index + 1).to_string()
        } else {
            String::new()
        }
    }

    fn row_type_text(&self, clip: &Clip) -> String {
        let Some(state) = self.state.as_ref() else {
            return String::new();
        };
        let types = clip_format_names(&state.messages, clip)
            .join(state.messages.text("app.list_separator"));
        if types.is_empty() {
            String::new()
        } else {
            self.template("app.row_type_prefix", &[("{types}", types)])
        }
    }

    fn refresh_detail(&mut self, cx: &mut Cx) {
        let selected = self
            .state
            .as_ref()
            .and_then(|state| state.selected_clip().cloned());

        if let Some(clip) = selected {
            let position = self
                .state
                .as_ref()
                .and_then(|state| state.selected_position())
                .map(|index| index + 1)
                .unwrap_or(0);
            self.widget(cx, ids!(selected_title)).set_text(
                cx,
                &self.template("app.selected_title", &[("{index}", position.to_string())]),
            );
            let pin = if clip.pinned {
                self.text("app.pinned")
            } else {
                self.text("app.not_pinned")
            };
            let types = self
                .state
                .as_ref()
                .map(|state| {
                    clip_format_names(&state.messages, &clip)
                        .join(state.messages.text("app.list_separator"))
                })
                .unwrap_or_default();
            self.widget(cx, ids!(selected_meta)).set_text(
                cx,
                &self.template(
                    "app.selected_meta",
                    &[
                        ("{id}", short_id(&clip.id)),
                        ("{formats}", clip.formats.len().to_string()),
                        ("{types}", types),
                        ("{updated}", format_timestamp(clip.updated_at)),
                        ("{pin}", pin),
                    ],
                ),
            );
            let editable_text = editable_text(&clip).unwrap_or_default();
            self.widget(cx, ids!(edit_input))
                .set_text(cx, editable_text);
            let pin_label = if clip.pinned {
                self.text("app.unpin")
            } else {
                self.text("app.pin")
            };
            self.widget(cx, ids!(pin_button)).set_text(cx, &pin_label);
            self.widget(cx, ids!(menu_pin_button))
                .set_text(cx, &pin_label);
        } else {
            self.widget(cx, ids!(selected_title))
                .set_text(cx, &self.text("app.no_selection"));
            self.widget(cx, ids!(selected_meta)).set_text(cx, "");
            self.widget(cx, ids!(edit_input)).set_text(cx, "");
            self.widget(cx, ids!(pin_button))
                .set_text(cx, &self.text("app.pin"));
            self.widget(cx, ids!(menu_pin_button))
                .set_text(cx, &self.text("app.pin"));
        }
    }

    fn clear_search(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.query.clear();
        }
        self.widget(cx, ids!(search_input)).set_text(cx, "");
        self.refresh_history(cx);
        self.set_status(cx, "app.status_ready");
    }

    fn set_clip_filter(&mut self, cx: &mut Cx, filter: ClipFilter) {
        self.clip_filter = filter;
        self.group_panel_visible = false;
        if let Some(state) = self.state.as_ref() {
            self.widget(cx, ids!(groups_button))
                .set_text(cx, &self.clip_filter_label(&state.messages));
            self.apply_filter_labels(cx, &state.messages);
        }
        self.refresh_history(cx);
    }

    fn clip_filter_label(&self, messages: &I18nBundle) -> String {
        let key = match self.clip_filter {
            ClipFilter::All => "app.filter_all",
            ClipFilter::Pinned => "app.filter_pinned",
            ClipFilter::Text => "app.filter_text",
            ClipFilter::Images => "app.filter_images",
            ClipFilter::Files => "app.filter_files",
        };
        format!("{}: {}", messages.text("app.groups"), messages.text(key))
    }

    fn copy_device_id(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.copy_device_id());
        match result {
            Some(Ok(())) => self.set_status(cx, "app.status_device_id_copied"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
        }
    }

    fn select_clip_by_index(&mut self, cx: &mut Cx, index: usize) {
        if let Some(state) = &mut self.state
            && let Some(clip) = state.history.get(index)
        {
            state.selected_id = Some(clip.id.clone());
            self.pending_delete_id = None;
            self.refresh_history(cx);
            self.widget(cx, row_id(index)).set_key_focus(cx);
        }
    }

    fn select_first_clip(&mut self, cx: &mut Cx) {
        self.select_clip_by_index(cx, 0);
    }

    fn select_last_clip(&mut self, cx: &mut Cx) {
        let Some(last_index) = self
            .state
            .as_ref()
            .and_then(|state| state.history.len().checked_sub(1))
        else {
            return;
        };
        self.select_clip_by_index(cx, last_index);
    }

    fn select_relative_clip(&mut self, cx: &mut Cx, delta: isize) {
        let Some(state) = &mut self.state else {
            return;
        };
        if state.history.is_empty() {
            return;
        }

        let current = state.selected_position().unwrap_or(0);
        let last = state.history.len().saturating_sub(1);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(last)
        };

        if let Some(clip) = state.history.get(next) {
            state.selected_id = Some(clip.id.clone());
            self.refresh_history(cx);
            self.widget(cx, row_id(next)).set_key_focus(cx);
        }
    }

    fn capture_clipboard(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.capture_clipboard(true));
        match result {
            Some(Ok(CaptureOutcome::Saved)) => {
                self.set_status(cx, "app.status_capture_saved");
                self.refresh_history(cx);
            }
            Some(Ok(CaptureOutcome::Unchanged)) => {
                self.set_status(cx, "app.status_capture_unchanged")
            }
            Some(Ok(CaptureOutcome::Empty)) => self.set_status(cx, "app.status_clipboard_empty"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
        }
    }

    fn poll_clipboard(&mut self, cx: &mut Cx) {
        let enabled = self
            .state
            .as_ref()
            .map(|state| state.settings.capture_enabled)
            .unwrap_or(false);
        if !enabled {
            return;
        }

        let result = self.with_state_mut(|state| state.capture_clipboard(false));
        match result {
            Some(Ok(CaptureOutcome::Saved)) => {
                self.set_status(cx, "app.status_auto_capture_saved");
                self.refresh_history(cx);
            }
            Some(Ok(CaptureOutcome::Unchanged | CaptureOutcome::Empty)) => {}
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => {}
        }
    }

    fn copy_selected(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.copy_selected());
        match result {
            Some(Ok(true)) => self.set_status(cx, "app.status_copied_selected"),
            Some(Ok(false)) => self.set_status(cx, "app.status_no_clip"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
        }
    }

    fn copy_selected_plain_text(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.copy_selected_plain_text());
        match result {
            Some(Ok(true)) => self.set_status(cx, "app.status_copied_plain_text"),
            Some(Ok(false)) => self.set_status(cx, "app.status_plain_text_unavailable"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
        }
    }

    fn copy_selected_transformed(&mut self, cx: &mut Cx, transform: TextTransform) {
        let result = self.with_state_mut(|state| state.copy_selected_transformed(transform));
        match result {
            Some(Ok(true)) => self.set_status(cx, "app.status_copied_transformed"),
            Some(Ok(false)) => self.set_status(cx, "app.status_plain_text_unavailable"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
        }
    }

    fn save_selected_edit(&mut self, cx: &mut Cx) {
        let Some(selected) = self.state.as_ref().and_then(|state| state.selected_clip()) else {
            self.set_status(cx, "app.status_no_selection");
            return;
        };
        if editable_text(selected).is_none() {
            self.set_status(cx, "app.status_edit_text_only");
            return;
        }

        let text = self.widget(cx, ids!(edit_input)).text();
        if text.trim().is_empty() {
            self.set_status(cx, "app.status_clipboard_empty");
            return;
        }

        let result = self.with_state_mut(|state| state.update_selected_text(&text));
        match result {
            Some(Ok(true)) => {
                self.set_status(cx, "app.status_edit_saved");
                self.refresh_history(cx);
            }
            Some(Ok(false)) => self.set_status(cx, "app.status_no_selection"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_no_selection"),
        }
    }

    fn toggle_selected_pin(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.toggle_selected_pin());
        match result {
            Some(Ok(true)) => {
                self.set_status(cx, "app.status_pin_updated");
                self.refresh_history(cx);
            }
            Some(Ok(false)) => self.set_status(cx, "app.status_no_selection"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_no_selection"),
        }
    }

    fn delete_selected(&mut self, cx: &mut Cx) {
        let selected_id = self
            .state
            .as_ref()
            .and_then(|state| state.selected_id.clone());
        let prompt_delete = self
            .state
            .as_ref()
            .map(|state| state.settings.quick_paste_prompt_delete)
            .unwrap_or(false);
        if prompt_delete
            && selected_id.is_some()
            && self.pending_delete_id.as_ref() != selected_id.as_ref()
        {
            self.pending_delete_id = selected_id;
            self.set_status(cx, "app.status_confirm_delete");
            return;
        }

        let result = self.with_state_mut(|state| state.delete_selected());
        match result {
            Some(Ok(true)) => {
                self.pending_delete_id = None;
                self.set_status(cx, "app.status_deleted");
                self.refresh_history(cx);
            }
            Some(Ok(false)) => self.set_status(cx, "app.status_no_selection"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_no_selection"),
        }
    }

    fn sync_now(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.sync_now());
        match result {
            Some(Ok(true)) => {
                self.set_status(cx, "app.status_sync_complete");
                self.refresh_history(cx);
            }
            Some(Ok(false)) => self.set_status(cx, "app.status_sync_skipped"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_sync_skipped"),
        }
    }

    fn toggle_capture(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.settings.capture_enabled = !state.settings.capture_enabled;
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.restart_poll_timer(cx);
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn toggle_duplicate_policy(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.settings.duplicate_moves_to_top = !state.settings.duplicate_moves_to_top;
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn toggle_capture_format(&mut self, cx: &mut Cx, kind: CaptureFormatKind) {
        if let Some(state) = &mut self.state {
            match kind {
                CaptureFormatKind::Text => {
                    state.settings.capture_text_enabled = !state.settings.capture_text_enabled
                }
                CaptureFormatKind::Html => {
                    state.settings.capture_html_enabled = !state.settings.capture_html_enabled
                }
                CaptureFormatKind::Image => {
                    state.settings.capture_image_enabled = !state.settings.capture_image_enabled
                }
                CaptureFormatKind::Files => {
                    state.settings.capture_files_enabled = !state.settings.capture_files_enabled
                }
            }
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn toggle_theme(&mut self, cx: &mut Cx) {
        let mut next_theme = None;
        if let Some(state) = &mut self.state {
            state.settings.theme = state.settings.theme.toggle();
            next_theme = Some(state.settings.theme);
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        if let Some(theme) = next_theme {
            apply_makepad_theme_to_cx(cx, theme);
        }
        self.apply_i18n(cx);
        self.refresh_history(cx);
    }

    fn toggle_language(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.settings.language = state.settings.language.toggle();
            state.messages = i18n::bundle(state.settings.language);
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.refresh_history(cx);
        self.set_status(cx, "app.status_ready");
    }

    fn save_connection_settings(&mut self, cx: &mut Cx) {
        let server_url = self.widget(cx, ids!(server_input)).text();
        let token = self.widget(cx, ids!(token_input)).text();
        if let Some(state) = &mut self.state {
            state.settings.server_url = blank_to_none(server_url);
            state.settings.token = blank_to_none(token);
            state.settings.sync_enabled =
                state.settings.server_url.is_some() && state.settings.token.is_some();
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.refresh_local_status(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn save_behavior_settings(&mut self, cx: &mut Cx) {
        let max_history = match parse_u32_setting(&self.widget(cx, ids!(max_history_input)).text())
        {
            Some(value) if value > 0 => value,
            _ => {
                self.set_status(cx, "app.status_invalid_number");
                return;
            }
        };
        let capture_interval_ms =
            match parse_u64_setting(&self.widget(cx, ids!(capture_interval_input)).text()) {
                Some(value) if value >= MIN_CAPTURE_INTERVAL_MS => value,
                _ => {
                    self.set_status(cx, "app.status_invalid_number");
                    return;
                }
            };

        if let Some(state) = &mut self.state {
            state.settings.max_history = max_history;
            state.settings.capture_interval_ms = capture_interval_ms;
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
            if let Err(error) = state.store.enforce_max_history(max_history) {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.restart_poll_timer(cx);
        self.refresh_local_status(cx);
        self.refresh_history(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn save_quick_paste_settings(&mut self, cx: &mut Cx) {
        let lines_per_row =
            match parse_u32_setting(&self.widget(cx, ids!(lines_per_row_input)).text()) {
                Some(value) if (1..=5).contains(&value) => value,
                _ => {
                    self.set_status(cx, "app.status_invalid_number");
                    return;
                }
            };
        let transparency_percent =
            match parse_u32_setting(&self.widget(cx, ids!(transparency_input)).text()) {
                Some(value) if value <= 90 => value,
                _ => {
                    self.set_status(cx, "app.status_invalid_number");
                    return;
                }
            };

        if let Some(state) = &mut self.state {
            state.settings.quick_paste_lines_per_row = lines_per_row;
            state.settings.quick_paste_transparency_percent = transparency_percent;
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn toggle_bool_setting(&mut self, cx: &mut Cx, field: impl FnOnce(&mut Settings) -> &mut bool) {
        if let Some(state) = &mut self.state {
            let value = field(&mut state.settings);
            *value = !*value;
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn cycle_popup_position(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            let next = QuickPastePosition::parse(&state.settings.quick_paste_position).next();
            state.settings.quick_paste_position = next.as_str().to_owned();
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn cycle_lines_per_row(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.settings.quick_paste_lines_per_row =
                if state.settings.quick_paste_lines_per_row >= 5 {
                    1
                } else {
                    state.settings.quick_paste_lines_per_row + 1
                };
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn cycle_transparency(&mut self, cx: &mut Cx) {
        const STEPS: &[u32] = &[0, 5, 10, 15, 20, 25, 30, 35, 40];
        if let Some(state) = &mut self.state {
            let current = state.settings.quick_paste_transparency_percent;
            let next_index = STEPS
                .iter()
                .position(|value| *value == current)
                .map(|index| (index + 1) % STEPS.len())
                .unwrap_or(0);
            state.settings.quick_paste_transparency_percent = STEPS[next_index];
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
        self.set_status(cx, "app.status_settings_saved");
    }

    fn save_hotkey_settings(&mut self, cx: &mut Cx) {
        let hotkeys = HotkeySettingsInput {
            show_history: self.widget(cx, ids!(hotkey_show_input)).text(),
            search: self.widget(cx, ids!(hotkey_search_input)).text(),
            copy_selected: self.widget(cx, ids!(hotkey_copy_input)).text(),
            delete_selected: self.widget(cx, ids!(hotkey_delete_input)).text(),
            toggle_pin: self.widget(cx, ids!(hotkey_pin_input)).text(),
            edit_selected: self.widget(cx, ids!(hotkey_edit_input)).text(),
            capture_now: self.widget(cx, ids!(hotkey_capture_input)).text(),
            sync_now: self.widget(cx, ids!(hotkey_sync_input)).text(),
        };
        if let Some(invalid) = hotkeys.invalid_shortcut() {
            self.set_status_text(
                cx,
                &self.template("app.status_hotkey_invalid", &[("{value}", invalid)]),
            );
            return;
        }

        if let Some(state) = &mut self.state {
            state.settings.hotkey_show_history = hotkeys.show_history;
            state.settings.hotkey_search = hotkeys.search;
            state.settings.hotkey_copy_selected = hotkeys.copy_selected;
            state.settings.hotkey_delete_selected = hotkeys.delete_selected;
            state.settings.hotkey_toggle_pin = hotkeys.toggle_pin;
            state.settings.hotkey_edit_selected = hotkeys.edit_selected;
            state.settings.hotkey_capture_now = hotkeys.capture_now;
            state.settings.hotkey_sync_now = hotkeys.sync_now;
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.set_status(cx, "app.status_settings_saved");
    }

    fn shortcut_matches(
        &self,
        shortcut: impl FnOnce(&Settings) -> &String,
        event: &KeyEvent,
    ) -> bool {
        self.state
            .as_ref()
            .and_then(|state| Shortcut::parse(shortcut(&state.settings)))
            .map(|shortcut| shortcut.matches(event))
            .unwrap_or(false)
    }

    fn restart_poll_timer(&mut self, cx: &mut Cx) {
        if !self.poll_timer.is_empty() {
            cx.stop_timer(self.poll_timer);
            self.poll_timer = Timer::empty();
        }
        if let Some(state) = &self.state
            && state.settings.capture_enabled
        {
            let interval = state
                .settings
                .capture_interval_ms
                .max(MIN_CAPTURE_INTERVAL_MS) as f64
                / 1000.0;
            self.poll_timer = cx.start_interval(interval);
        }
    }

    fn restart_tray_timer(&mut self, cx: &mut Cx) {
        if !self.tray_timer.is_empty() {
            cx.stop_timer(self.tray_timer);
            self.tray_timer = Timer::empty();
        }
        self.tray_timer = cx.start_interval(0.25);
    }

    fn drain_tray_commands(&mut self, cx: &mut Cx) {
        let mut commands = Vec::new();
        if let Some(rx) = &self.tray_rx {
            while let Ok(command) = rx.try_recv() {
                commands.push(command);
            }
        }

        for command in commands {
            match command {
                TrayCommand::Open => {
                    self.show_main_page(cx);
                    self.refresh_history(cx);
                }
                TrayCommand::Settings => self.show_settings_page(cx),
                TrayCommand::CaptureNow => self.capture_clipboard(cx),
                TrayCommand::SyncNow => self.sync_now(cx),
                TrayCommand::ToggleCapture => self.toggle_capture(cx),
                TrayCommand::Exit => std::process::exit(0),
            }
        }
    }

    fn start_tray(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let Some(state) = self.state.as_ref() else {
                return;
            };
            if !state.settings.show_tray_icon || self.tray_handle.is_some() {
                return;
            }

            let (tx, rx) = mpsc::channel();
            let tray = YankTray {
                sender: tx,
                capture_enabled: state.settings.capture_enabled,
                labels: TrayLabels::from_messages(&state.messages),
            };
            match tray.assume_sni_available(true).spawn() {
                Ok(handle) => {
                    self.tray_rx = Some(rx);
                    self.tray_handle = Some(handle);
                }
                Err(error) => {
                    eprintln!("system tray unavailable: {error}");
                }
            }
        }
    }

    fn sync_tray_state(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if let (Some(handle), Some(state)) = (self.tray_handle.as_ref(), self.state.as_ref()) {
                let capture_enabled = state.settings.capture_enabled;
                let labels = TrayLabels::from_messages(&state.messages);
                let _ = handle.update(|tray| {
                    tray.capture_enabled = capture_enabled;
                    tray.labels = labels;
                });
            }
        }
    }

    fn apply_tray_visibility(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let enabled = self
                .state
                .as_ref()
                .map(|state| state.settings.show_tray_icon)
                .unwrap_or(false);
            if enabled {
                self.start_tray();
            } else if let Some(handle) = self.tray_handle.take() {
                handle.shutdown().wait();
                self.tray_rx = None;
            }
        }
    }

    fn set_status(&mut self, cx: &mut Cx, key: &str) {
        let text = self.text(key);
        self.set_status_text(cx, &text);
    }

    fn set_status_text(&mut self, cx: &mut Cx, text: &str) {
        self.widget(cx, ids!(status)).set_text(cx, text);
    }

    fn with_state_mut<T>(&mut self, action: impl FnOnce(&mut ClientState) -> T) -> Option<T> {
        self.state.as_mut().map(action)
    }
}

struct ClientState {
    store: Store,
    settings: Settings,
    messages: I18nBundle,
    clipboard: Option<Clipboard>,
    history: Vec<Clip>,
    selected_id: Option<String>,
    query: String,
    last_clipboard_hash: Option<String>,
}

impl ClientState {
    fn load() -> Result<Self> {
        let store = Store::open(paths::database_path()?)?;
        let mut settings = store.settings()?;
        if let Ok(server_url) = std::env::var("YANK_SERVER_URL") {
            settings.server_url = blank_to_none(server_url);
        }
        if let Ok(token) = std::env::var("YANK_TOKEN") {
            settings.token = blank_to_none(token);
        }
        settings.sync_enabled = settings.server_url.is_some() && settings.token.is_some();
        store.save_settings(&settings)?;
        let _ = store.enforce_max_history(settings.max_history);
        let messages = i18n::bundle(settings.language);
        let clipboard = Clipboard::new().ok();
        Ok(Self {
            store,
            settings,
            messages,
            clipboard,
            history: Vec::new(),
            selected_id: None,
            query: String::new(),
            last_clipboard_hash: None,
        })
    }

    fn fallback(message: String) -> Self {
        let store = Store::open_memory().expect("in-memory store should initialize");
        let settings = Settings::default();
        let messages = i18n::bundle(settings.language);
        let _ = store.set_setting("startup_error", &message);
        Self {
            store,
            settings,
            messages,
            clipboard: Clipboard::new().ok(),
            history: Vec::new(),
            selected_id: None,
            query: String::new(),
            last_clipboard_hash: None,
        }
    }

    fn selected_clip(&self) -> Option<&Clip> {
        let selected_id = self.selected_id.as_deref()?;
        self.history.iter().find(|clip| clip.id == selected_id)
    }

    fn selected_position(&self) -> Option<usize> {
        let selected_id = self.selected_id.as_deref()?;
        self.history.iter().position(|clip| clip.id == selected_id)
    }

    fn capture_clipboard(&mut self, force: bool) -> Result<CaptureOutcome> {
        let settings = self.settings.clone();
        let snapshot = {
            let Some(clipboard) = &mut self.clipboard else {
                anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
            };
            read_clipboard_snapshot(clipboard, &settings)?
        };
        let Some(snapshot) = snapshot else {
            return Ok(CaptureOutcome::Empty);
        };

        let hash = content_hash(&snapshot.formats);
        if !force && self.last_clipboard_hash.as_deref() == Some(hash.as_str()) {
            return Ok(CaptureOutcome::Unchanged);
        }

        let clip = if self.settings.duplicate_moves_to_top {
            self.store.save_clip_deduplicated(
                &Clip::from_formats(
                    &self.settings.device_id,
                    snapshot.description,
                    snapshot.primary_text,
                    snapshot.formats,
                ),
                true,
            )?
        } else if let Some(existing) = self.store.find_active_by_content_hash(&hash)? {
            existing
        } else {
            self.store.save_clip(&Clip::from_formats(
                &self.settings.device_id,
                snapshot.description,
                snapshot.primary_text,
                snapshot.formats,
            ))?
        };

        self.last_clipboard_hash = Some(hash);
        self.store.enforce_max_history(self.settings.max_history)?;
        if let Some(sync) = self.sync_client() {
            let _ = sync.push_clip(clip)?;
        }
        Ok(CaptureOutcome::Saved)
    }

    fn copy_selected(&mut self) -> Result<bool> {
        let Some(clip) = self.selected_clip().cloned() else {
            return Ok(false);
        };
        let Some(clipboard) = &mut self.clipboard else {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
        };
        if restore_clip_to_clipboard(clipboard, &clip)? {
            self.last_clipboard_hash = Some(clip.content_hash);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn copy_selected_plain_text(&mut self) -> Result<bool> {
        let Some(text) = self.selected_clip().and_then(plain_text_payload) else {
            return Ok(false);
        };
        let Some(clipboard) = &mut self.clipboard else {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
        };
        clipboard.set_text(text.clone())?;
        self.last_clipboard_hash = Some(content_hash(&[ClipFormat::text(&text)]));
        Ok(true)
    }

    fn copy_selected_transformed(&mut self, transform: TextTransform) -> Result<bool> {
        let Some(text) = self.selected_clip().and_then(plain_text_payload) else {
            return Ok(false);
        };
        let Some(clipboard) = &mut self.clipboard else {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
        };
        let transformed = transform_text(&text, transform);
        clipboard.set_text(transformed.clone())?;
        self.last_clipboard_hash = Some(content_hash(&[ClipFormat::text(&transformed)]));
        Ok(true)
    }

    fn copy_device_id(&mut self) -> Result<()> {
        let Some(clipboard) = &mut self.clipboard else {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
        };
        clipboard.set_text(self.settings.device_id.clone())?;
        Ok(())
    }

    fn update_selected_text(&mut self, text: &str) -> Result<bool> {
        let Some(id) = self.selected_id.clone() else {
            return Ok(false);
        };
        let Some(clip) = self.store.update_clip_text(&id, text)? else {
            return Ok(false);
        };
        if let Some(sync) = self.sync_client() {
            let _ = sync.push_clip(clip)?;
        }
        Ok(true)
    }

    fn toggle_selected_pin(&mut self) -> Result<bool> {
        let Some(id) = self.selected_id.clone() else {
            return Ok(false);
        };
        let Some(clip) = self.store.toggle_clip_pinned(&id)? else {
            return Ok(false);
        };
        if let Some(sync) = self.sync_client() {
            let _ = sync.push_clip(clip)?;
        }
        Ok(true)
    }

    fn delete_selected(&mut self) -> Result<bool> {
        let Some(id) = self.selected_id.clone() else {
            return Ok(false);
        };
        if !self.store.delete_clip(&id)? {
            return Ok(false);
        }
        if let Some(sync) = self.sync_client() {
            let _ = sync.delete_clip(&id);
        }
        self.selected_id = None;
        Ok(true)
    }

    fn sync_now(&mut self) -> Result<bool> {
        let Some(sync) = self.sync_client() else {
            return Ok(false);
        };
        let response = sync.pull_since(0)?;
        for clip in response.clips {
            self.store.save_clip(&clip)?;
        }
        self.store.enforce_max_history(self.settings.max_history)?;
        Ok(true)
    }

    fn persist_settings(&self) -> yank_core::Result<()> {
        self.store.save_settings(&self.settings)
    }

    fn sync_client(&self) -> Option<SyncClient> {
        let config = SyncConfig::new(
            self.settings.server_url.clone().unwrap_or_default(),
            self.settings.token.clone().unwrap_or_default(),
        )?;
        Some(SyncClient::new(config))
    }
}

#[derive(Debug)]
struct ClipboardSnapshot {
    description: String,
    primary_text: Option<String>,
    formats: Vec<ClipFormat>,
}

fn read_clipboard_snapshot(
    clipboard: &mut Clipboard,
    settings: &Settings,
) -> Result<Option<ClipboardSnapshot>> {
    let mut formats = Vec::new();
    let mut primary_text = None;
    let mut description = None;

    if settings.capture_files_enabled
        && let Some(paths) = read_optional_clipboard(clipboard.get().file_list())?
    {
        let paths = paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| !path.trim().is_empty())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            let searchable = paths.join("\n");
            primary_text.get_or_insert(searchable);
            description.get_or_insert_with(|| summarize_paths(&paths));
            formats.push(ClipFormat::file_list(&paths));
        }
    }

    if settings.capture_image_enabled
        && let Some(image) = read_optional_clipboard(clipboard.get_image())?
    {
        let bytes = image.bytes.into_owned();
        let expected_len = image.width.saturating_mul(image.height).saturating_mul(4);
        if expected_len == bytes.len() {
            description.get_or_insert_with(|| format!("{}x{}", image.width, image.height));
            formats.push(ClipFormat::image_rgba(image.width, image.height, bytes));
        }
    }

    if settings.capture_html_enabled
        && let Some(html) = read_optional_clipboard(clipboard.get().html())?
        && !html.trim().is_empty()
    {
        let searchable = html_to_text(&html);
        let searchable = if searchable.is_empty() {
            html.clone()
        } else {
            searchable
        };
        primary_text.get_or_insert(searchable.clone());
        description.get_or_insert_with(|| yank_core::summarize_text(&searchable));
        formats.push(ClipFormat::html(&html));
    }

    if settings.capture_text_enabled
        && let Some(text) = read_optional_clipboard(clipboard.get_text())?
        && !text.trim().is_empty()
    {
        primary_text.get_or_insert(text.clone());
        description.get_or_insert_with(|| yank_core::summarize_text(&text));
        formats.push(ClipFormat::text(&text));
    }

    if formats.is_empty() {
        return Ok(None);
    }

    Ok(Some(ClipboardSnapshot {
        description: description.unwrap_or_else(|| {
            primary_text
                .as_deref()
                .map(yank_core::summarize_text)
                .unwrap_or_default()
        }),
        primary_text,
        formats,
    }))
}

fn read_optional_clipboard<T>(result: std::result::Result<T, ClipboardError>) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(ClipboardError::ContentNotAvailable | ClipboardError::ConversionFailure) => Ok(None),
        Err(error) => Err(anyhow::anyhow!("{error}")),
    }
}

fn restore_clip_to_clipboard(clipboard: &mut Clipboard, clip: &Clip) -> Result<bool> {
    if let Some(paths) = clip.formats.iter().find_map(ClipFormat::file_list_paths)
        && !paths.is_empty()
    {
        let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        clipboard.set().file_list(&paths)?;
        return Ok(true);
    }

    if let Some((width, height, bytes)) = clip.formats.iter().find_map(image_rgba_payload) {
        clipboard.set_image(ImageData {
            width,
            height,
            bytes: Cow::Owned(bytes),
        })?;
        return Ok(true);
    }

    if let Some(html) = clip.formats.iter().find_map(ClipFormat::html_value) {
        let alt_text = clip
            .formats
            .iter()
            .find_map(ClipFormat::text_value)
            .or(clip.primary_text.as_deref());
        clipboard
            .set()
            .html(html.to_owned(), alt_text.map(str::to_owned))?;
        return Ok(true);
    }

    if let Some(text) = clip
        .formats
        .iter()
        .find_map(ClipFormat::text_value)
        .or(clip.primary_text.as_deref())
    {
        clipboard.set_text(text.to_owned())?;
        return Ok(true);
    }

    Ok(false)
}

fn image_rgba_payload(format: &ClipFormat) -> Option<(usize, usize, Vec<u8>)> {
    let (width, height) = format.image_rgba_dimensions()?;
    let expected_len = width.checked_mul(height)?.checked_mul(4)?;
    if format.data.len() == expected_len {
        Some((width, height, format.data.clone()))
    } else {
        None
    }
}

fn editable_text(clip: &Clip) -> Option<&str> {
    clip.formats.iter().find_map(ClipFormat::text_value)
}

fn plain_text_payload(clip: &Clip) -> Option<String> {
    if let Some(text) = editable_text(clip).filter(|text| !text.trim().is_empty()) {
        return Some(text.to_owned());
    }
    if let Some(text) = clip
        .primary_text
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        return Some(text.to_owned());
    }
    if let Some(text) = clip
        .formats
        .iter()
        .find_map(ClipFormat::html_value)
        .map(html_to_text)
        .filter(|text| !text.trim().is_empty())
    {
        return Some(text);
    }
    clip.formats
        .iter()
        .find_map(ClipFormat::file_list_paths)
        .filter(|paths| !paths.is_empty())
        .map(|paths| paths.join("\n"))
}

fn transform_text(text: &str, transform: TextTransform) -> String {
    match transform {
        TextTransform::Upper => text.to_uppercase(),
        TextTransform::Lower => text.to_lowercase(),
        TextTransform::Trim => text.trim().to_owned(),
        TextTransform::RemoveLineFeeds => text
            .lines()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        TextTransform::CamelCase => {
            let mut output = String::new();
            for (index, word) in text
                .split(|ch: char| !ch.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .enumerate()
            {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    if index == 0 {
                        output.push_str(&first.to_lowercase().collect::<String>());
                    } else {
                        output.push_str(&first.to_uppercase().collect::<String>());
                    }
                    output.push_str(&chars.as_str().to_lowercase());
                }
            }
            output
        }
    }
}

fn enabled_capture_format_names(messages: &I18nBundle, settings: &Settings) -> String {
    let mut names = Vec::new();
    if settings.capture_text_enabled {
        names.push(messages.text("app.format_text"));
    }
    if settings.capture_html_enabled {
        names.push(messages.text("app.format_html"));
    }
    if settings.capture_image_enabled {
        names.push(messages.text("app.format_image"));
    }
    if settings.capture_files_enabled {
        names.push(messages.text("app.format_files"));
    }
    if names.is_empty() {
        messages.text("app.capture_none").to_owned()
    } else {
        names.join(messages.text("app.list_separator"))
    }
}

fn clip_format_names<'a>(messages: &'a I18nBundle, clip: &Clip) -> Vec<&'a str> {
    let mut names = Vec::new();
    if clip.formats.iter().any(ClipFormat::is_text) {
        names.push(messages.text("app.format_text"));
    }
    if clip.formats.iter().any(ClipFormat::is_html) {
        names.push(messages.text("app.format_html"));
    }
    if clip
        .formats
        .iter()
        .any(|format| format.image_rgba_dimensions().is_some())
    {
        names.push(messages.text("app.format_image"));
    }
    if clip.formats.iter().any(ClipFormat::is_file_list) {
        names.push(messages.text("app.format_files"));
    }
    names
}

fn clip_matches_filter(clip: &Clip, filter: ClipFilter) -> bool {
    match filter {
        ClipFilter::All => true,
        ClipFilter::Pinned => clip.pinned,
        ClipFilter::Text => clip.formats.iter().any(ClipFormat::is_text),
        ClipFilter::Images => clip
            .formats
            .iter()
            .any(|format| format.image_rgba_dimensions().is_some()),
        ClipFilter::Files => clip.formats.iter().any(ClipFormat::is_file_list),
    }
}

fn popup_position_label<'a>(messages: &'a I18nBundle, value: &str) -> &'a str {
    match QuickPastePosition::parse(value) {
        QuickPastePosition::Cursor => messages.text("app.popup_cursor"),
        QuickPastePosition::Caret => messages.text("app.popup_caret"),
        QuickPastePosition::Previous => messages.text("app.popup_previous"),
    }
}

fn summarize_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
        })
        .take(4)
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_row_text(text: &str, show_leading_whitespace: bool) -> String {
    if show_leading_whitespace {
        let visible = text
            .chars()
            .map(|ch| match ch {
                ' ' => '.',
                '\t' => '>',
                '\r' | '\n' => ' ',
                other => other,
            })
            .take(160)
            .collect::<String>();
        if visible.trim().is_empty() {
            "(empty text)".to_owned()
        } else {
            visible
        }
    } else {
        yank_core::summarize_text(text)
    }
}

fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    let mut entity = String::new();
    let mut in_entity = false;

    for ch in html.chars() {
        match ch {
            '<' if !in_entity => in_tag = true,
            '>' if in_tag && !in_entity => {
                in_tag = false;
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            '&' if !in_tag => {
                in_entity = true;
                entity.clear();
            }
            ';' if in_entity => {
                in_entity = false;
                output.push_str(match entity.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    "nbsp" => " ",
                    _ => "",
                });
            }
            _ if in_entity => entity.push(ch),
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }

    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureOutcome {
    Saved,
    Unchanged,
    Empty,
}

struct HotkeySettingsInput {
    show_history: String,
    search: String,
    copy_selected: String,
    delete_selected: String,
    toggle_pin: String,
    edit_selected: String,
    capture_now: String,
    sync_now: String,
}

impl HotkeySettingsInput {
    fn invalid_shortcut(&self) -> Option<String> {
        for value in [
            &self.show_history,
            &self.search,
            &self.copy_selected,
            &self.delete_selected,
            &self.toggle_pin,
            &self.edit_selected,
            &self.capture_now,
            &self.sync_now,
        ] {
            if Shortcut::parse(value).is_none() {
                return Some(value.clone());
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Shortcut {
    primary: bool,
    shift: bool,
    alt: bool,
    key_code: KeyCode,
}

impl Shortcut {
    fn parse(value: &str) -> Option<Self> {
        let mut primary = false;
        let mut shift = false;
        let mut alt = false;
        let mut key_code = None;

        for token in value
            .split('+')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
        {
            let normalized = token.to_ascii_lowercase();
            match normalized.as_str() {
                "ctrl" | "control" | "cmd" | "command" | "primary" => primary = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                _ => key_code = parse_key_code(&normalized),
            }
        }

        Some(Self {
            primary,
            shift,
            alt,
            key_code: key_code?,
        })
    }

    fn matches(self, event: &KeyEvent) -> bool {
        self.key_code == event.key_code
            && self.primary == event.modifiers.is_primary()
            && self.shift == event.modifiers.shift
            && self.alt == event.modifiers.alt
    }
}

fn parse_key_code(value: &str) -> Option<KeyCode> {
    match value {
        "enter" | "return" => Some(KeyCode::ReturnKey),
        "delete" | "del" => Some(KeyCode::Delete),
        "escape" | "esc" => Some(KeyCode::Escape),
        "backspace" => Some(KeyCode::Backspace),
        "space" => Some(KeyCode::Space),
        "tab" => Some(KeyCode::Tab),
        "backtick" | "`" => Some(KeyCode::Backtick),
        "0" => Some(KeyCode::Key0),
        "1" => Some(KeyCode::Key1),
        "2" => Some(KeyCode::Key2),
        "3" => Some(KeyCode::Key3),
        "4" => Some(KeyCode::Key4),
        "5" => Some(KeyCode::Key5),
        "6" => Some(KeyCode::Key6),
        "7" => Some(KeyCode::Key7),
        "8" => Some(KeyCode::Key8),
        "9" => Some(KeyCode::Key9),
        "numpad0" | "num0" => Some(KeyCode::Numpad0),
        "numpad1" | "num1" => Some(KeyCode::Numpad1),
        "numpad2" | "num2" => Some(KeyCode::Numpad2),
        "numpad3" | "num3" => Some(KeyCode::Numpad3),
        "numpad4" | "num4" => Some(KeyCode::Numpad4),
        "numpad5" | "num5" => Some(KeyCode::Numpad5),
        "numpad6" | "num6" => Some(KeyCode::Numpad6),
        "numpad7" | "num7" => Some(KeyCode::Numpad7),
        "numpad8" | "num8" => Some(KeyCode::Numpad8),
        "numpad9" | "num9" => Some(KeyCode::Numpad9),
        "a" => Some(KeyCode::KeyA),
        "b" => Some(KeyCode::KeyB),
        "c" => Some(KeyCode::KeyC),
        "d" => Some(KeyCode::KeyD),
        "e" => Some(KeyCode::KeyE),
        "f" => Some(KeyCode::KeyF),
        "g" => Some(KeyCode::KeyG),
        "h" => Some(KeyCode::KeyH),
        "i" => Some(KeyCode::KeyI),
        "j" => Some(KeyCode::KeyJ),
        "k" => Some(KeyCode::KeyK),
        "l" => Some(KeyCode::KeyL),
        "m" => Some(KeyCode::KeyM),
        "n" => Some(KeyCode::KeyN),
        "o" => Some(KeyCode::KeyO),
        "p" => Some(KeyCode::KeyP),
        "q" => Some(KeyCode::KeyQ),
        "r" => Some(KeyCode::KeyR),
        "s" => Some(KeyCode::KeyS),
        "t" => Some(KeyCode::KeyT),
        "u" => Some(KeyCode::KeyU),
        "v" => Some(KeyCode::KeyV),
        "w" => Some(KeyCode::KeyW),
        "x" => Some(KeyCode::KeyX),
        "y" => Some(KeyCode::KeyY),
        "z" => Some(KeyCode::KeyZ),
        _ => None,
    }
}

fn number_key_index(key_code: KeyCode) -> Option<usize> {
    match key_code {
        KeyCode::Key1 | KeyCode::Numpad1 => Some(0),
        KeyCode::Key2 | KeyCode::Numpad2 => Some(1),
        KeyCode::Key3 | KeyCode::Numpad3 => Some(2),
        KeyCode::Key4 | KeyCode::Numpad4 => Some(3),
        KeyCode::Key5 | KeyCode::Numpad5 => Some(4),
        KeyCode::Key6 | KeyCode::Numpad6 => Some(5),
        KeyCode::Key7 | KeyCode::Numpad7 => Some(6),
        KeyCode::Key8 | KeyCode::Numpad8 => Some(7),
        KeyCode::Key9 | KeyCode::Numpad9 => Some(8),
        _ => None,
    }
}

fn paste_position_key_index(key_code: KeyCode) -> Option<usize> {
    match key_code {
        KeyCode::Key0 | KeyCode::Numpad0 => Some(9),
        _ => None,
    }
    .or_else(|| number_key_index(key_code))
}

fn row_id(index: usize) -> &'static [LiveId] {
    match index {
        0 => ids!(row_0),
        1 => ids!(row_1),
        2 => ids!(row_2),
        3 => ids!(row_3),
        4 => ids!(row_4),
        5 => ids!(row_5),
        6 => ids!(row_6),
        7 => ids!(row_7),
        8 => ids!(row_8),
        9 => ids!(row_9),
        10 => ids!(row_10),
        11 => ids!(row_11),
        12 => ids!(row_12),
        13 => ids!(row_13),
        14 => ids!(row_14),
        15 => ids!(row_15),
        16 => ids!(row_16),
        17 => ids!(row_17),
        18 => ids!(row_18),
        _ => ids!(row_19),
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn format_timestamp(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .map(|time| {
            time.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| timestamp.to_string())
}

fn parse_u32_setting(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn parse_u64_setting(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn blank_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn main() {
    Cx::init_log();
    if Cx::pre_start() {
        return;
    }

    let cx = Rc::new(RefCell::new(Cx::new(
        makepad_widgets::_app_main_event_closure!(App),
    )));
    let studio_http = makepad_widgets::resolve_studio_http();
    cx.borrow_mut().init_websockets(&studio_http);
    if std::env::args().any(|value| value == "--stdin-loop") {
        cx.borrow_mut().in_makepad_studio = true;
    }
    let makepad_package_root = prepare_embedded_makepad_package_root();
    cx.borrow_mut().package_root = Some(makepad_package_root.clone());
    cx.borrow_mut().init_cx_os();
    register_embedded_makepad_fonts(&mut cx.borrow_mut(), &makepad_package_root);
    Cx::event_loop(cx);
}

fn register_embedded_makepad_fonts(cx: &mut Cx, package_root: &str) {
    CxDraw::lazy_construct_fonts(cx);
    let fonts = cx.get_global::<Rc<RefCell<Fonts>>>().clone();
    let mut fonts = fonts.borrow_mut();

    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "IBMPlexSans-Text.ttf"),
        IBM_PLEX_SANS_TEXT,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "IBMPlexSans-SemiBold.ttf"),
        IBM_PLEX_SANS_SEMIBOLD,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "IBMPlexSans-Italic.ttf"),
        IBM_PLEX_SANS_ITALIC,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "IBMPlexSans-BoldItalic.ttf"),
        IBM_PLEX_SANS_BOLD_ITALIC,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "LiberationMono-Regular.ttf"),
        LIBERATION_MONO_REGULAR,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "fa-solid-900.ttf"),
        FONT_AWESOME_SOLID,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "LXGWWenKaiRegular.ttf"),
        LXGW_WENKAI_REGULAR,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "LXGWWenKaiBold.ttf"),
        LXGW_WENKAI_BOLD,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path(package_root, "NotoColorEmoji.ttf"),
        NOTO_COLOR_EMOJI,
        &[0.0],
    );
}

fn makepad_widget_font_path(package_root: &str, file_name: &str) -> String {
    makepad_font_crate_path(package_root, "makepad_widgets", file_name)
}

fn makepad_font_crate_path(package_root: &str, crate_name: &str, file_name: &str) -> String {
    format!("{package_root}/{crate_name}/resources/{file_name}")
}

fn register_font_variants(fonts: &mut Fonts, path: &str, data: &'static [u8], ascenders: &[f32]) {
    for ascender in ascenders {
        register_font(fonts, &[path.to_owned()], data.to_vec(), *ascender, 0.0);
    }
}

fn register_font(
    fonts: &mut Fonts,
    paths: &[String],
    data: Vec<u8>,
    ascender: f32,
    descender: f32,
) {
    let font_id = makepad_font_id(paths, ascender, descender);
    if fonts.is_font_known(font_id) {
        return;
    }

    fonts.define_font(
        font_id,
        FontDefinition {
            data: SharedBytes::from_vec(data),
            index: 0,
            ascender_fudge_in_ems: ascender,
            descender_fudge_in_ems: descender,
            weight: None,
            variations: Vec::new(),
        },
    );
}

fn makepad_font_id(paths: &[String], ascender: f32, descender: f32) -> FontId {
    let mut live_id = LiveId::seeded();
    for path in paths {
        live_id = live_id.str_append(path);
    }
    live_id = live_id
        .bytes_append(&ascender.to_be_bytes())
        .bytes_append(&descender.to_be_bytes());
    FontId::from(live_id.0)
}

struct EmbeddedMakepadResource {
    crate_name: &'static str,
    file_name: &'static str,
    data: &'static [u8],
}

const EMBEDDED_MAKEPAD_RESOURCES: &[EmbeddedMakepadResource] = &[
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "IBMPlexSans-Text.ttf",
        data: IBM_PLEX_SANS_TEXT,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "IBMPlexSans-SemiBold.ttf",
        data: IBM_PLEX_SANS_SEMIBOLD,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "IBMPlexSans-Italic.ttf",
        data: IBM_PLEX_SANS_ITALIC,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "IBMPlexSans-BoldItalic.ttf",
        data: IBM_PLEX_SANS_BOLD_ITALIC,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "LiberationMono-Regular.ttf",
        data: LIBERATION_MONO_REGULAR,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "NotoSans-Regular.ttf",
        data: NOTO_SANS_REGULAR,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "fa-solid-900.ttf",
        data: FONT_AWESOME_SOLID,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "LXGWWenKaiRegular.ttf",
        data: LXGW_WENKAI_REGULAR,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "LXGWWenKaiBold.ttf",
        data: LXGW_WENKAI_BOLD,
    },
    EmbeddedMakepadResource {
        crate_name: "makepad_widgets",
        file_name: "NotoColorEmoji.ttf",
        data: NOTO_COLOR_EMOJI,
    },
];

fn prepare_embedded_makepad_package_root() -> String {
    let root = env::var_os("YANK_MAKEPAD_RESOURCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::temp_dir()
                .join("yank")
                .join("makepad-resources")
                .join(env!("CARGO_PKG_VERSION"))
        });

    materialize_embedded_makepad_resources(&root).unwrap_or_else(|error| {
        panic!(
            "could not prepare embedded Makepad resources in {}: {error}",
            root.display()
        )
    });
    root.to_string_lossy().replace('\\', "/")
}

fn materialize_embedded_makepad_resources(root: &Path) -> Result<()> {
    for resource in EMBEDDED_MAKEPAD_RESOURCES {
        let path = root
            .join(resource.crate_name)
            .join("resources")
            .join(resource.file_name);
        write_embedded_makepad_resource(&path, resource.data)?;
    }
    Ok(())
}

fn write_embedded_makepad_resource(path: &Path, data: &[u8]) -> Result<()> {
    if fs::read(path).is_ok_and(|existing| existing == data) {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)?;
    Ok(())
}
