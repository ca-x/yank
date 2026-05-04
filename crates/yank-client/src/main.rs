use anyhow::Result;
use arboard::{Clipboard, Error as ClipboardError, ImageData};
use makepad_widgets::makepad_draw::text::{font::FontId, fonts::Fonts, loader::FontDefinition};
use makepad_widgets::*;
use std::{
    any::TypeId,
    borrow::Cow,
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};
use yank_client::{
    paths,
    sync::{SyncClient, SyncConfig},
};
use yank_core::{
    Clip, ClipFormat, Settings, Store, Theme, content_hash,
    i18n::{self, I18nBundle},
};

const HISTORY_ROWS: usize = 20;
const MIN_CAPTURE_INTERVAL_MS: u64 = 250;
const MAKEPAD_PACKAGE_ROOT: &str = "makepad";

include!(concat!(env!("OUT_DIR"), "/embedded_makepad_fonts.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureFormatKind {
    Text,
    Html,
    Image,
    Files,
}

live_design! {
    link widgets;
    use link::widgets::*;
    use link::theme::*;

    App = {{App}} {
        ui: <Root> {
            main_window = <Window> {
                body = <View> {
                    flow: Overlay,
                    width: Fill,
                    height: Fill,

                    light_surface = <View> {
                        show_bg: true,
                        draw_bg: { color: #f5f1e8 }
                        width: Fill,
                        height: Fill,
                    }

                    dark_surface = <View> {
                        show_bg: true,
                        draw_bg: { color: #111716 }
                        width: Fill,
                        height: Fill,
                    }

                    content = <ScrollYView> {
                        flow: Down,
                        width: Fill,
                        height: Fill,
                        spacing: 14,
                        padding: { left: 24, right: 24, top: 22, bottom: 24 }

                        title = <H1> { text: "" }
                        status = <TextBox> { text: "" }

                        toolbar = <View> {
                            flow: RightWrap,
                            height: Fit,
                            width: Fill,
                            spacing: 8,
                            capture_toggle_button = <Button> { text: "" }
                            capture_button = <Button> { text: "" }
                            sync_button = <Button> { text: "" }
                            theme_button = <Button> { text: "" }
                            language_button = <Button> { text: "" }
                        }

                        search_bar = <View> {
                            flow: RightWrap,
                            width: Fill,
                            height: Fit,
                            spacing: 8,
                            search_label = <Label> { text: "" }
                            search_input = <TextInput> { width: Fill, empty_text: "" }
                            clear_search_button = <Button> { text: "" }
                        }

                        workspace = <View> {
                            flow: RightWrap,
                            width: Fill,
                            height: Fit,
                            spacing: 16,

                            history_panel = <View> {
                                flow: Down,
                                width: 460,
                                height: Fit,
                                spacing: 7,
                                history_title = <H2> { text: "" }
                                clip_count = <Label> { text: "" }
                                row_0 = <Button> { width: Fill, text: "" }
                                row_1 = <Button> { width: Fill, text: "" }
                                row_2 = <Button> { width: Fill, text: "" }
                                row_3 = <Button> { width: Fill, text: "" }
                                row_4 = <Button> { width: Fill, text: "" }
                                row_5 = <Button> { width: Fill, text: "" }
                                row_6 = <Button> { width: Fill, text: "" }
                                row_7 = <Button> { width: Fill, text: "" }
                                row_8 = <Button> { width: Fill, text: "" }
                                row_9 = <Button> { width: Fill, text: "" }
                                row_10 = <Button> { width: Fill, text: "" }
                                row_11 = <Button> { width: Fill, text: "" }
                                row_12 = <Button> { width: Fill, text: "" }
                                row_13 = <Button> { width: Fill, text: "" }
                                row_14 = <Button> { width: Fill, text: "" }
                                row_15 = <Button> { width: Fill, text: "" }
                                row_16 = <Button> { width: Fill, text: "" }
                                row_17 = <Button> { width: Fill, text: "" }
                                row_18 = <Button> { width: Fill, text: "" }
                                row_19 = <Button> { width: Fill, text: "" }
                            }

                            detail_panel = <View> {
                                flow: Down,
                                width: Fill,
                                height: Fit,
                                spacing: 10,

                                selected_title = <H2> { text: "" }
                                selected_meta = <Label> { text: "" }
                                preview = <TextBox> { width: Fill, text: "" }
                                edit_label = <Label> { text: "" }
                                edit_input = <TextInput> {
                                    width: Fill,
                                    height: 116,
                                    empty_text: "",
                                    layout: { flow: RightWrap }
                                }
                                detail_actions = <View> {
                                    flow: RightWrap,
                                    width: Fill,
                                    height: Fit,
                                    spacing: 8,
                                    copy_selected_button = <Button> { text: "" }
                                    save_edit_button = <Button> { text: "" }
                                    pin_button = <Button> { text: "" }
                                    delete_button = <Button> { text: "" }
                                }
                            }
                        }

                        settings_title = <H2> { text: "" }
                        local_status = <TextBox> { text: "" }

                        behavior_settings = <View> {
                            flow: RightWrap,
                            width: Fill,
                            height: Fit,
                            spacing: 8,
                            device_id_label = <Label> { text: "" }
                            device_id_value = <TextInput> { width: 300, empty_text: "", is_read_only: true }
                            copy_device_id_button = <Button> { text: "" }
                            duplicate_policy_label = <Label> { text: "" }
                            duplicate_policy_button = <Button> { text: "" }
                            capture_formats_label = <Label> { text: "" }
                            capture_text_button = <Button> { text: "" }
                            capture_html_button = <Button> { text: "" }
                            capture_image_button = <Button> { text: "" }
                            capture_files_button = <Button> { text: "" }
                            max_history_label = <Label> { text: "" }
                            max_history_input = <TextInput> { width: 120, empty_text: "" }
                            capture_interval_label = <Label> { text: "" }
                            capture_interval_input = <TextInput> { width: 120, empty_text: "" }
                            save_behavior_button = <Button> { text: "" }
                        }

                        sync_settings = <View> {
                            flow: Down,
                            width: Fill,
                            height: Fit,
                            spacing: 8,
                            sync_settings_title = <H2> { text: "" }
                            server_label = <Label> { text: "" }
                            server_input = <TextInput> { width: Fill, empty_text: "" }
                            token_label = <Label> { text: "" }
                            token_input = <TextInput> { width: Fill, empty_text: "", is_password: true }
                            save_settings_button = <Button> { text: "" }
                        }

                        hotkeys_settings = <View> {
                            flow: Down,
                            width: Fill,
                            height: Fit,
                            spacing: 8,
                            hotkeys_title = <H2> { text: "" }
                            hotkeys_status = <Label> { text: "" }
                            hotkey_show_label = <Label> { text: "" }
                            hotkey_show_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_search_label = <Label> { text: "" }
                            hotkey_search_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_copy_label = <Label> { text: "" }
                            hotkey_copy_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_delete_label = <Label> { text: "" }
                            hotkey_delete_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_pin_label = <Label> { text: "" }
                            hotkey_pin_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_edit_label = <Label> { text: "" }
                            hotkey_edit_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_capture_label = <Label> { text: "" }
                            hotkey_capture_input = <TextInput> { width: Fill, empty_text: "" }
                            hotkey_sync_label = <Label> { text: "" }
                            hotkey_sync_input = <TextInput> { width: Fill, empty_text: "" }
                            save_hotkeys_button = <Button> { text: "" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Live, LiveHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    state: Option<ClientState>,
    #[rust]
    initialized: bool,
    #[rust]
    poll_timer: Timer,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
    }
}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if !self.initialized {
            self.initialize(cx);
        }

        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some(query) = self.text_input(id!(search_input)).changed(actions) {
            if let Some(state) = &mut self.state {
                state.query = query;
            }
            self.refresh_history(cx);
        }

        if self
            .text_input(id!(search_input))
            .returned(actions)
            .is_some()
        {
            self.copy_selected(cx);
        }

        if self.text_input(id!(edit_input)).returned(actions).is_some() {
            self.save_selected_edit(cx);
        }

        for index in 0..HISTORY_ROWS {
            if self.button(row_id(index)).clicked(actions) {
                self.select_clip_by_index(cx, index);
            }
        }

        if self.button(id!(clear_search_button)).clicked(actions) {
            self.clear_search(cx);
        }
        if self.button(id!(copy_device_id_button)).clicked(actions) {
            self.copy_device_id(cx);
        }
        if self.button(id!(duplicate_policy_button)).clicked(actions) {
            self.toggle_duplicate_policy(cx);
        }
        if self.button(id!(capture_text_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Text);
        }
        if self.button(id!(capture_html_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Html);
        }
        if self.button(id!(capture_image_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Image);
        }
        if self.button(id!(capture_files_button)).clicked(actions) {
            self.toggle_capture_format(cx, CaptureFormatKind::Files);
        }
        if self.button(id!(capture_toggle_button)).clicked(actions) {
            self.toggle_capture(cx);
        }
        if self.button(id!(capture_button)).clicked(actions) {
            self.capture_clipboard(cx);
        }
        if self.button(id!(copy_selected_button)).clicked(actions) {
            self.copy_selected(cx);
        }
        if self.button(id!(save_edit_button)).clicked(actions) {
            self.save_selected_edit(cx);
        }
        if self.button(id!(pin_button)).clicked(actions) {
            self.toggle_selected_pin(cx);
        }
        if self.button(id!(delete_button)).clicked(actions) {
            self.delete_selected(cx);
        }
        if self.button(id!(sync_button)).clicked(actions) {
            self.sync_now(cx);
        }
        if self.button(id!(theme_button)).clicked(actions) {
            self.toggle_theme(cx);
        }
        if self.button(id!(language_button)).clicked(actions) {
            self.toggle_language(cx);
        }
        if self.button(id!(save_behavior_button)).clicked(actions) {
            self.save_behavior_settings(cx);
        }
        if self.button(id!(save_settings_button)).clicked(actions) {
            self.save_connection_settings(cx);
        }
        if self.button(id!(save_hotkeys_button)).clicked(actions) {
            self.save_hotkey_settings(cx);
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.poll_timer.is_timer(event).is_some() {
            self.poll_clipboard(cx);
        }
    }

    fn handle_key_down(&mut self, cx: &mut Cx, event: &KeyEvent) {
        if self.shortcut_matches(|settings| &settings.hotkey_show_history, event)
            || self.shortcut_matches(|settings| &settings.hotkey_search, event)
        {
            self.refresh_history(cx);
            self.widget(id!(search_input)).set_key_focus(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_copy_selected, event) {
            self.copy_selected(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_delete_selected, event) {
            self.delete_selected(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_toggle_pin, event) {
            self.toggle_selected_pin(cx);
            return;
        }
        if self.shortcut_matches(|settings| &settings.hotkey_edit_selected, event) {
            self.widget(id!(edit_input)).set_key_focus(cx);
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
                self.restart_poll_timer(cx);
                self.apply_i18n(cx);
                self.refresh_history(cx);
                self.set_status(cx, "app.status_local_ready");
            }
            Err(error) => {
                self.state = Some(ClientState::fallback(error.to_string()));
                self.restart_poll_timer(cx);
                self.apply_i18n(cx);
                self.refresh_history(cx);
                self.set_status(cx, "app.status_startup_fallback");
            }
        }
    }

    fn button(&self, id: &[LiveId]) -> ButtonRef {
        self.ui.widget(id).as_button()
    }

    fn text_input(&self, id: &[LiveId]) -> TextInputRef {
        self.ui.widget(id).as_text_input()
    }

    fn widget(&self, id: &[LiveId]) -> WidgetRef {
        self.ui.widget(id)
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
            (id!(title), "app.title"),
            (id!(capture_button), "app.capture"),
            (id!(sync_button), "app.sync_now"),
            (id!(language_button), "app.lang_toggle"),
            (id!(search_label), "app.search"),
            (id!(clear_search_button), "app.clear_search"),
            (id!(history_title), "app.latest"),
            (id!(selected_title), "app.no_selection"),
            (id!(edit_label), "app.edit_text"),
            (id!(copy_selected_button), "app.copy_selected"),
            (id!(save_edit_button), "app.save_edit"),
            (id!(delete_button), "app.delete"),
            (id!(settings_title), "app.settings"),
            (id!(device_id_label), "app.device_id"),
            (id!(copy_device_id_button), "app.copy_device_id"),
            (id!(duplicate_policy_label), "app.duplicate_policy"),
            (id!(capture_formats_label), "app.capture_formats"),
            (id!(max_history_label), "app.max_history"),
            (id!(capture_interval_label), "app.capture_interval"),
            (id!(save_behavior_button), "app.save_behavior"),
            (id!(sync_settings_title), "app.sync_settings"),
            (id!(server_label), "app.server"),
            (id!(token_label), "app.token"),
            (id!(save_settings_button), "app.save_settings"),
            (id!(hotkeys_title), "app.hotkeys"),
            (id!(hotkeys_status), "app.hotkeys_status"),
            (id!(hotkey_show_label), "app.hotkey_show"),
            (id!(hotkey_search_label), "app.hotkey_search"),
            (id!(hotkey_copy_label), "app.hotkey_copy"),
            (id!(hotkey_delete_label), "app.hotkey_delete"),
            (id!(hotkey_pin_label), "app.hotkey_pin"),
            (id!(hotkey_edit_label), "app.hotkey_edit"),
            (id!(hotkey_capture_label), "app.hotkey_capture"),
            (id!(hotkey_sync_label), "app.hotkey_sync"),
            (id!(save_hotkeys_button), "app.save_hotkeys"),
        ] {
            self.widget(id).set_text(cx, messages.text(key));
        }

        self.widget(id!(theme_button)).set_text(
            cx,
            match state.settings.theme {
                Theme::Light => messages.text("app.dark"),
                Theme::Dark => messages.text("app.light"),
            },
        );
        self.widget(id!(capture_toggle_button)).set_text(
            cx,
            if state.settings.capture_enabled {
                messages.text("app.capture_on")
            } else {
                messages.text("app.capture_off")
            },
        );
        self.widget(id!(duplicate_policy_button)).set_text(
            cx,
            if state.settings.duplicate_moves_to_top {
                messages.text("app.duplicate_move_top")
            } else {
                messages.text("app.duplicate_keep_existing")
            },
        );
        for (id, enabled, on_key, off_key) in [
            (
                id!(capture_text_button),
                state.settings.capture_text_enabled,
                "app.capture_text_on",
                "app.capture_text_off",
            ),
            (
                id!(capture_html_button),
                state.settings.capture_html_enabled,
                "app.capture_html_on",
                "app.capture_html_off",
            ),
            (
                id!(capture_image_button),
                state.settings.capture_image_enabled,
                "app.capture_image_on",
                "app.capture_image_off",
            ),
            (
                id!(capture_files_button),
                state.settings.capture_files_enabled,
                "app.capture_files_on",
                "app.capture_files_off",
            ),
        ] {
            self.widget(id).set_text(
                cx,
                if enabled {
                    messages.text(on_key)
                } else {
                    messages.text(off_key)
                },
            );
        }

        self.text_input(id!(search_input))
            .set_empty_text(cx, messages.text("app.search_placeholder").to_owned());
        self.text_input(id!(edit_input))
            .set_empty_text(cx, messages.text("app.edit_placeholder").to_owned());
        self.text_input(id!(server_input))
            .set_empty_text(cx, messages.text("app.server_placeholder").to_owned());
        self.text_input(id!(token_input))
            .set_empty_text(cx, messages.text("app.token_placeholder").to_owned());
        self.text_input(id!(max_history_input))
            .set_empty_text(cx, messages.text("app.max_history_placeholder").to_owned());
        self.text_input(id!(capture_interval_input)).set_empty_text(
            cx,
            messages.text("app.capture_interval_placeholder").to_owned(),
        );

        self.widget(id!(server_input))
            .set_text(cx, state.settings.server_url.as_deref().unwrap_or(""));
        self.widget(id!(token_input))
            .set_text(cx, state.settings.token.as_deref().unwrap_or(""));
        self.widget(id!(device_id_value))
            .set_text(cx, &state.settings.device_id);
        self.widget(id!(max_history_input))
            .set_text(cx, &state.settings.max_history.to_string());
        self.widget(id!(capture_interval_input))
            .set_text(cx, &state.settings.capture_interval_ms.to_string());
        self.widget(id!(hotkey_show_input))
            .set_text(cx, &state.settings.hotkey_show_history);
        self.widget(id!(hotkey_search_input))
            .set_text(cx, &state.settings.hotkey_search);
        self.widget(id!(hotkey_copy_input))
            .set_text(cx, &state.settings.hotkey_copy_selected);
        self.widget(id!(hotkey_delete_input))
            .set_text(cx, &state.settings.hotkey_delete_selected);
        self.widget(id!(hotkey_pin_input))
            .set_text(cx, &state.settings.hotkey_toggle_pin);
        self.widget(id!(hotkey_edit_input))
            .set_text(cx, &state.settings.hotkey_edit_selected);
        self.widget(id!(hotkey_capture_input))
            .set_text(cx, &state.settings.hotkey_capture_now);
        self.widget(id!(hotkey_sync_input))
            .set_text(cx, &state.settings.hotkey_sync_now);

        let light = state.settings.theme == Theme::Light;
        self.widget(id!(light_surface)).set_visible(cx, light);
        self.widget(id!(dark_surface)).set_visible(cx, !light);
        self.refresh_local_status(cx);
        self.refresh_detail(cx);
        self.ui.redraw(cx);
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
        self.widget(id!(local_status)).set_text(cx, &status);
    }

    fn refresh_history(&mut self, cx: &mut Cx) {
        let (clips, selected_id, count) = {
            let Some(state) = self.state.as_mut() else {
                return;
            };
            let clips = state
                .store
                .search_clips(&state.query, HISTORY_ROWS as u32)
                .unwrap_or_default();
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

        self.widget(id!(clip_count)).set_text(
            cx,
            &self.template("app.history_count", &[("{count}", count.to_string())]),
        );

        for index in 0..HISTORY_ROWS {
            let id = row_id(index);
            if let Some(clip) = clips.get(index) {
                let selected = selected_id.as_deref() == Some(clip.id.as_str());
                let row_text = self.row_text(index, clip, selected);
                self.widget(id).set_visible(cx, true);
                self.widget(id).set_text(cx, &row_text);
            } else {
                self.widget(id).set_visible(cx, index == 0);
                let empty_text = if index == 0 {
                    self.text("app.empty")
                } else {
                    String::new()
                };
                self.widget(id).set_text(cx, &empty_text);
            }
        }

        self.refresh_detail(cx);
    }

    fn row_text(&self, index: usize, clip: &Clip, selected: bool) -> String {
        let text = clip
            .primary_text
            .as_deref()
            .map(yank_core::summarize_text)
            .unwrap_or_else(|| clip.description.clone());
        let pin = if clip.pinned {
            self.text("app.pinned_marker")
        } else {
            String::new()
        };
        let types = self.row_type_text(clip);
        let key = if selected {
            "app.row_selected_template"
        } else {
            "app.row_template"
        };
        self.template(
            key,
            &[
                ("{index}", (index + 1).to_string()),
                ("{pin}", pin),
                ("{types}", types),
                ("{text}", text),
            ],
        )
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
            self.widget(id!(selected_title)).set_text(
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
            self.widget(id!(selected_meta)).set_text(
                cx,
                &self.template(
                    "app.selected_meta",
                    &[
                        ("{id}", short_id(&clip.id)),
                        ("{formats}", clip.formats.len().to_string()),
                        ("{types}", types),
                        ("{updated}", clip.updated_at.to_string()),
                        ("{pin}", pin),
                    ],
                ),
            );
            let preview = self.clip_preview(&clip);
            let editable_text = editable_text(&clip).unwrap_or_default();
            self.widget(id!(preview)).set_text(cx, &preview);
            self.widget(id!(edit_input)).set_text(cx, editable_text);
            let pin_label = if clip.pinned {
                self.text("app.unpin")
            } else {
                self.text("app.pin")
            };
            self.widget(id!(pin_button)).set_text(cx, &pin_label);
        } else {
            self.widget(id!(selected_title))
                .set_text(cx, &self.text("app.no_selection"));
            self.widget(id!(selected_meta)).set_text(cx, "");
            self.widget(id!(preview))
                .set_text(cx, &self.text("app.empty"));
            self.widget(id!(edit_input)).set_text(cx, "");
            self.widget(id!(pin_button))
                .set_text(cx, &self.text("app.pin"));
        }
    }

    fn clip_preview(&self, clip: &Clip) -> String {
        if let Some(text) = editable_text(clip) {
            return text.to_owned();
        }
        if let Some(text) = clip.primary_text.as_deref().filter(|text| !text.is_empty()) {
            return text.to_owned();
        }
        if let Some((width, height)) = clip
            .formats
            .iter()
            .find_map(ClipFormat::image_rgba_dimensions)
        {
            return self.template(
                "app.preview_image",
                &[
                    ("{width}", width.to_string()),
                    ("{height}", height.to_string()),
                ],
            );
        }
        clip.formats
            .iter()
            .find_map(ClipFormat::html_value)
            .map(html_to_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| clip.description.clone())
    }

    fn clear_search(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.query.clear();
        }
        self.widget(id!(search_input)).set_text(cx, "");
        self.refresh_history(cx);
        self.set_status(cx, "app.status_ready");
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
            self.refresh_history(cx);
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

    fn save_selected_edit(&mut self, cx: &mut Cx) {
        let Some(selected) = self.state.as_ref().and_then(|state| state.selected_clip()) else {
            self.set_status(cx, "app.status_no_selection");
            return;
        };
        if editable_text(selected).is_none() {
            self.set_status(cx, "app.status_edit_text_only");
            return;
        }

        let text = self.widget(id!(edit_input)).text();
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
        let result = self.with_state_mut(|state| state.delete_selected());
        match result {
            Some(Ok(true)) => {
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
        if let Some(state) = &mut self.state {
            state.settings.theme = state.settings.theme.toggle();
            if let Err(error) = state.persist_settings() {
                self.set_status_text(cx, &error.to_string());
                return;
            }
        }
        self.apply_i18n(cx);
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
        let server_url = self.widget(id!(server_input)).text();
        let token = self.widget(id!(token_input)).text();
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
        let max_history = match parse_u32_setting(&self.widget(id!(max_history_input)).text()) {
            Some(value) if value > 0 => value,
            _ => {
                self.set_status(cx, "app.status_invalid_number");
                return;
            }
        };
        let capture_interval_ms =
            match parse_u64_setting(&self.widget(id!(capture_interval_input)).text()) {
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

    fn save_hotkey_settings(&mut self, cx: &mut Cx) {
        let hotkeys = HotkeySettingsInput {
            show_history: self.widget(id!(hotkey_show_input)).text(),
            search: self.widget(id!(hotkey_search_input)).text(),
            copy_selected: self.widget(id!(hotkey_copy_input)).text(),
            delete_selected: self.widget(id!(hotkey_delete_input)).text(),
            toggle_pin: self.widget(id!(hotkey_pin_input)).text(),
            edit_selected: self.widget(id!(hotkey_edit_input)).text(),
            capture_now: self.widget(id!(hotkey_capture_input)).text(),
            sync_now: self.widget(id!(hotkey_sync_input)).text(),
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

    fn set_status(&mut self, cx: &mut Cx, key: &str) {
        let text = self.text(key);
        self.set_status_text(cx, &text);
    }

    fn set_status_text(&mut self, cx: &mut Cx, text: &str) {
        self.widget(id!(status)).set_text(cx, text);
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
        KeyCode::Key1 => Some(0),
        KeyCode::Key2 => Some(1),
        KeyCode::Key3 => Some(2),
        KeyCode::Key4 => Some(3),
        KeyCode::Key5 => Some(4),
        KeyCode::Key6 => Some(5),
        KeyCode::Key7 => Some(6),
        KeyCode::Key8 => Some(7),
        KeyCode::Key9 => Some(8),
        _ => None,
    }
}

fn row_id(index: usize) -> &'static [LiveId] {
    match index {
        0 => id!(row_0),
        1 => id!(row_1),
        2 => id!(row_2),
        3 => id!(row_3),
        4 => id!(row_4),
        5 => id!(row_5),
        6 => id!(row_6),
        7 => id!(row_7),
        8 => id!(row_8),
        9 => id!(row_9),
        10 => id!(row_10),
        11 => id!(row_11),
        12 => id!(row_12),
        13 => id!(row_13),
        14 => id!(row_14),
        15 => id!(row_15),
        16 => id!(row_16),
        17 => id!(row_17),
        18 => id!(row_18),
        _ => id!(row_19),
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
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
    if Cx::pre_start() {
        return;
    }

    let app = Rc::new(RefCell::new(None));
    let cx = Rc::new(RefCell::new(Cx::new(Box::new(move |cx, event| {
        if let Event::Startup = event {
            *app.borrow_mut() = App::new_main(cx);
        }
        if let Event::LiveEdit = event {
            app.borrow_mut().update_main(cx);
        }
        if let Some(app) = &mut *app.borrow_mut() {
            <dyn AppMain>::handle_event(app, cx, event);
        }
    }))));

    register_empty_makepad_main_module(&mut cx.borrow_mut());
    cx.borrow_mut()
        .init_websockets(std::option_env!("MAKEPAD_STUDIO_HTTP").unwrap_or(""));
    if std::env::args().any(|value| value == "--stdin-loop") {
        cx.borrow_mut().in_makepad_studio = true;
    }
    live_design(&mut cx.borrow_mut());
    cx.borrow_mut().live_registry.borrow_mut().package_root = Some(MAKEPAD_PACKAGE_ROOT.to_owned());
    cx.borrow_mut().init_cx_os();
    App::register_main_module(&mut cx.borrow_mut());
    register_embedded_makepad_fonts(&mut cx.borrow_mut());
    Cx::event_loop(cx);
}

fn register_empty_makepad_main_module(cx: &mut Cx) {
    cx.live_registry.borrow_mut().main_module = Some(LiveTypeInfo {
        live_type: TypeId::of::<()>(),
        type_name: LiveId::from_str_with_lut("YankEmptyMain").expect("valid live id"),
        module_id: LiveModuleId::from_str("yank::empty").expect("valid live module id"),
        live_ignore: true,
        fields: Vec::new(),
    });
}

fn register_embedded_makepad_fonts(cx: &mut Cx) {
    CxDraw::lazy_construct_fonts(cx);
    let fonts = cx.get_global::<Rc<RefCell<Fonts>>>().clone();
    let mut fonts = fonts.borrow_mut();

    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("IBMPlexSans-Text.ttf"),
        IBM_PLEX_SANS_TEXT,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("IBMPlexSans-SemiBold.ttf"),
        IBM_PLEX_SANS_SEMIBOLD,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("IBMPlexSans-Italic.ttf"),
        IBM_PLEX_SANS_ITALIC,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("IBMPlexSans-BoldItalic.ttf"),
        IBM_PLEX_SANS_BOLD_ITALIC,
        &[-0.1, 0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("LiberationMono-Regular.ttf"),
        LIBERATION_MONO_REGULAR,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("fa-solid-900.ttf"),
        FONT_AWESOME_SOLID,
        &[0.0],
    );
    register_multipart_font_variants(
        &mut fonts,
        &[
            makepad_font_crate_path("makepad_fonts_chinese_regular", "LXGWWenKaiRegular.ttf"),
            makepad_font_crate_path("makepad_fonts_chinese_regular_2", "LXGWWenKaiRegular.ttf.2"),
        ],
        &[LXGW_WENKAI_REGULAR, LXGW_WENKAI_REGULAR_2],
        &[0.0],
    );
    register_multipart_font_variants(
        &mut fonts,
        &[
            makepad_font_crate_path("makepad_fonts_chinese_bold", "LXGWWenKaiBold.ttf"),
            makepad_font_crate_path("makepad_fonts_chinese_bold_2", "LXGWWenKaiBold.ttf.2"),
        ],
        &[LXGW_WENKAI_BOLD, LXGW_WENKAI_BOLD_2],
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_font_crate_path("makepad_fonts_emoji", "NotoColorEmoji.ttf"),
        NOTO_COLOR_EMOJI,
        &[0.0],
    );

    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("LXGWWenKaiRegular.ttf"),
        LXGW_WENKAI_REGULAR,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("LXGWWenKaiBold.ttf"),
        LXGW_WENKAI_BOLD,
        &[0.0],
    );
    register_font_variants(
        &mut fonts,
        &makepad_widget_font_path("NotoColorEmoji.ttf"),
        NOTO_COLOR_EMOJI,
        &[0.0],
    );
}

fn makepad_widget_font_path(file_name: &str) -> String {
    makepad_font_crate_path("makepad_widgets", file_name)
}

fn makepad_font_crate_path(crate_name: &str, file_name: &str) -> String {
    format!("{MAKEPAD_PACKAGE_ROOT}/{crate_name}/resources/{file_name}")
}

fn register_font_variants(fonts: &mut Fonts, path: &str, data: &'static [u8], ascenders: &[f32]) {
    for ascender in ascenders {
        register_font(fonts, &[path.to_owned()], data.to_vec(), *ascender, 0.0);
    }
}

fn register_multipart_font_variants(
    fonts: &mut Fonts,
    paths: &[String],
    parts: &[&'static [u8]],
    ascenders: &[f32],
) {
    let mut data = Vec::new();
    for part in parts {
        data.extend_from_slice(part);
    }

    for ascender in ascenders {
        register_font(fonts, paths, data.clone(), *ascender, 0.0);
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
            data: Rc::new(data),
            index: 0,
            ascender_fudge_in_ems: ascender,
            descender_fudge_in_ems: descender,
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
