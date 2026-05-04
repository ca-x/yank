use anyhow::Result;
use arboard::{Clipboard, Error as ClipboardError, ImageData};
use makepad_widgets::makepad_draw::text::{font::FontId, fonts::Fonts, loader::FontDefinition};
use makepad_widgets::*;
use std::{
    borrow::Cow,
    cell::RefCell,
    env, fs,
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

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(1280, 860)
                body +: {
                    content := ScrollYView{
                        width: Fill
                        height: Fill
                        flow: Down
                        spacing: 14
                        padding: Inset{left: 24 right: 24 top: 22 bottom: 24}

                        title := H1{text: ""}
                        status := TextBox{width: Fill height: Fit text: ""}

                        toolbar := View{
                            width: Fill
                            height: Fit
                            flow: Right {wrap: true}
                            spacing: 8
                            capture_toggle_button := Button{text: ""}
                            capture_button := Button{text: ""}
                            sync_button := Button{text: ""}
                            theme_button := Button{text: ""}
                            language_button := Button{text: ""}
                        }

                        search_bar := View{
                            width: Fill
                            height: Fit
                            flow: Right
                            spacing: 8
                            search_label := Label{text: ""}
                            search_input := TextInput{width: Fill height: Fit empty_text: ""}
                            clear_search_button := Button{text: ""}
                        }

                        workspace := View{
                            width: Fill
                            height: Fit
                            flow: Right
                            spacing: 16

                            history_panel := View{
                                width: 460
                                height: Fit
                                flow: Down
                                spacing: 7
                                history_title := H2{text: ""}
                                clip_count := Label{text: ""}
                                row_0 := Button{width: Fill text: ""}
                                row_1 := Button{width: Fill text: ""}
                                row_2 := Button{width: Fill text: ""}
                                row_3 := Button{width: Fill text: ""}
                                row_4 := Button{width: Fill text: ""}
                                row_5 := Button{width: Fill text: ""}
                                row_6 := Button{width: Fill text: ""}
                                row_7 := Button{width: Fill text: ""}
                                row_8 := Button{width: Fill text: ""}
                                row_9 := Button{width: Fill text: ""}
                                row_10 := Button{width: Fill text: ""}
                                row_11 := Button{width: Fill text: ""}
                                row_12 := Button{width: Fill text: ""}
                                row_13 := Button{width: Fill text: ""}
                                row_14 := Button{width: Fill text: ""}
                                row_15 := Button{width: Fill text: ""}
                                row_16 := Button{width: Fill text: ""}
                                row_17 := Button{width: Fill text: ""}
                                row_18 := Button{width: Fill text: ""}
                                row_19 := Button{width: Fill text: ""}
                            }

                            detail_panel := View{
                                width: Fill
                                height: Fit
                                flow: Down
                                spacing: 10

                                selected_title := H2{text: ""}
                                selected_meta := Label{width: Fill text: ""}
                                preview := TextBox{width: Fill height: Fit text: ""}
                                edit_label := Label{text: ""}
                                edit_input := TextInput{
                                    width: Fill
                                    height: 116
                                    is_multiline: true
                                    empty_text: ""
                                }
                                detail_actions := View{
                                    width: Fill
                                    height: Fit
                                    flow: Right {wrap: true}
                                    spacing: 8
                                    copy_selected_button := Button{text: ""}
                                    save_edit_button := Button{text: ""}
                                    pin_button := Button{text: ""}
                                    delete_button := Button{text: ""}
                                }
                            }
                        }

                        settings_title := H2{text: ""}
                        local_status := TextBox{width: Fill height: Fit text: ""}

                        behavior_settings := View{
                            width: Fill
                            height: Fit
                            flow: Right {wrap: true}
                            spacing: 8
                            device_id_label := Label{text: ""}
                            device_id_value := TextInput{width: 300 height: Fit empty_text: "" is_read_only: true}
                            copy_device_id_button := Button{text: ""}
                            duplicate_policy_label := Label{text: ""}
                            duplicate_policy_button := Button{text: ""}
                            capture_formats_label := Label{text: ""}
                            capture_text_button := Button{text: ""}
                            capture_html_button := Button{text: ""}
                            capture_image_button := Button{text: ""}
                            capture_files_button := Button{text: ""}
                            max_history_label := Label{text: ""}
                            max_history_input := TextInput{width: 120 height: Fit empty_text: ""}
                            capture_interval_label := Label{text: ""}
                            capture_interval_input := TextInput{width: 120 height: Fit empty_text: ""}
                            save_behavior_button := Button{text: ""}
                        }

                        sync_settings := View{
                            width: Fill
                            height: Fit
                            flow: Down
                            spacing: 8
                            sync_settings_title := H2{text: ""}
                            server_label := Label{text: ""}
                            server_input := TextInput{width: Fill height: Fit empty_text: ""}
                            token_label := Label{text: ""}
                            token_input := TextInput{width: Fill height: Fit empty_text: "" is_password: true}
                            save_settings_button := Button{text: ""}
                        }

                        hotkeys_settings := View{
                            width: Fill
                            height: Fit
                            flow: Down
                            spacing: 8
                            hotkeys_title := H2{text: ""}
                            hotkeys_status := Label{text: ""}
                            hotkey_show_label := Label{text: ""}
                            hotkey_show_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_search_label := Label{text: ""}
                            hotkey_search_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_copy_label := Label{text: ""}
                            hotkey_copy_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_delete_label := Label{text: ""}
                            hotkey_delete_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_pin_label := Label{text: ""}
                            hotkey_pin_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_edit_label := Label{text: ""}
                            hotkey_edit_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_capture_label := Label{text: ""}
                            hotkey_capture_input := TextInput{width: Fill height: Fit empty_text: ""}
                            hotkey_sync_label := Label{text: ""}
                            hotkey_sync_input := TextInput{width: Fill height: Fit empty_text: ""}
                            save_hotkeys_button := Button{text: ""}
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
    initialized: bool,
    #[rust]
    poll_timer: Timer,
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
            .button(cx, ids!(capture_toggle_button))
            .clicked(actions)
        {
            self.toggle_capture(cx);
        }
        if self.button(cx, ids!(capture_button)).clicked(actions) {
            self.capture_clipboard(cx);
        }
        if self.button(cx, ids!(copy_selected_button)).clicked(actions) {
            self.copy_selected(cx);
        }
        if self.button(cx, ids!(save_edit_button)).clicked(actions) {
            self.save_selected_edit(cx);
        }
        if self.button(cx, ids!(pin_button)).clicked(actions) {
            self.toggle_selected_pin(cx);
        }
        if self.button(cx, ids!(delete_button)).clicked(actions) {
            self.delete_selected(cx);
        }
        if self.button(cx, ids!(sync_button)).clicked(actions) {
            self.sync_now(cx);
        }
        if self.button(cx, ids!(theme_button)).clicked(actions) {
            self.toggle_theme(cx);
        }
        if self.button(cx, ids!(language_button)).clicked(actions) {
            self.toggle_language(cx);
        }
        if self.button(cx, ids!(save_behavior_button)).clicked(actions) {
            self.save_behavior_settings(cx);
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
    }

    fn handle_key_down(&mut self, cx: &mut Cx, event: &KeyEvent) {
        if self.shortcut_matches(|settings| &settings.hotkey_show_history, event)
            || self.shortcut_matches(|settings| &settings.hotkey_search, event)
        {
            self.refresh_history(cx);
            self.widget(cx, ids!(search_input)).set_key_focus(cx);
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

    fn button(&self, cx: &Cx, id: &[LiveId]) -> ButtonRef {
        self.ui.button(cx, id)
    }

    fn text_input(&self, cx: &Cx, id: &[LiveId]) -> TextInputRef {
        self.ui.text_input(cx, id)
    }

    fn widget(&self, cx: &Cx, id: &[LiveId]) -> WidgetRef {
        self.ui.widget(cx, id)
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
            (ids!(capture_button), "app.capture"),
            (ids!(sync_button), "app.sync_now"),
            (ids!(language_button), "app.lang_toggle"),
            (ids!(search_label), "app.search"),
            (ids!(clear_search_button), "app.clear_search"),
            (ids!(history_title), "app.latest"),
            (ids!(selected_title), "app.no_selection"),
            (ids!(edit_label), "app.edit_text"),
            (ids!(copy_selected_button), "app.copy_selected"),
            (ids!(save_edit_button), "app.save_edit"),
            (ids!(delete_button), "app.delete"),
            (ids!(settings_title), "app.settings"),
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
        ] {
            self.widget(cx, id).set_text(cx, messages.text(key));
        }

        self.widget(cx, ids!(theme_button)).set_text(
            cx,
            match state.settings.theme {
                Theme::Light => messages.text("app.dark"),
                Theme::Dark => messages.text("app.light"),
            },
        );
        self.widget(cx, ids!(capture_toggle_button)).set_text(
            cx,
            if state.settings.capture_enabled {
                messages.text("app.capture_on")
            } else {
                messages.text("app.capture_off")
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
        self.widget(cx, ids!(local_status)).set_text(cx, &status);
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
                        ("{updated}", clip.updated_at.to_string()),
                        ("{pin}", pin),
                    ],
                ),
            );
            let preview = self.clip_preview(&clip);
            let editable_text = editable_text(&clip).unwrap_or_default();
            self.widget(cx, ids!(preview)).set_text(cx, &preview);
            self.widget(cx, ids!(edit_input))
                .set_text(cx, editable_text);
            let pin_label = if clip.pinned {
                self.text("app.unpin")
            } else {
                self.text("app.pin")
            };
            self.widget(cx, ids!(pin_button)).set_text(cx, &pin_label);
        } else {
            self.widget(cx, ids!(selected_title))
                .set_text(cx, &self.text("app.no_selection"));
            self.widget(cx, ids!(selected_meta)).set_text(cx, "");
            self.widget(cx, ids!(preview))
                .set_text(cx, &self.text("app.empty"));
            self.widget(cx, ids!(edit_input)).set_text(cx, "");
            self.widget(cx, ids!(pin_button))
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
        self.widget(cx, ids!(search_input)).set_text(cx, "");
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
