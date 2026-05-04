use anyhow::Result;
use arboard::Clipboard;
use makepad_widgets::*;
use yank_client::{
    paths,
    sync::{SyncClient, SyncConfig},
};
use yank_core::{
    Settings, Store, Theme,
    i18n::{self, I18nBundle},
};

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
                        draw_bg: { color: #f6f1e8 }
                        width: Fill,
                        height: Fill,
                    }

                    dark_surface = <View> {
                        show_bg: true,
                        draw_bg: { color: #111716 }
                        width: Fill,
                        height: Fill,
                    }

                    content = <View> {
                        flow: Down,
                        width: Fill,
                        height: Fill,
                        spacing: 14,
                        padding: { left: 26, right: 26, top: 24, bottom: 24 }

                        title = <H1> { text: "" }
                        status = <TextBox> { text: "" }

                        toolbar = <View> {
                            flow: RightWrap,
                            height: Fit,
                            width: Fill,
                            spacing: 8,
                            capture_button = <Button> { text: "" }
                            copy_button = <Button> { text: "" }
                            sync_button = <Button> { text: "" }
                            theme_button = <Button> { text: "" }
                            language_button = <Button> { text: "" }
                        }

                        settings_title = <H2> { text: "" }
                        local_status = <TextBox> { text: "" }

                        config = <View> {
                            flow: Down,
                            width: Fill,
                            height: Fit,
                            spacing: 8,
                            server_label = <Label> { text: "" }
                            server_input = <TextInput> { width: Fill, empty_text: "" }
                            token_label = <Label> { text: "" }
                            token_input = <TextInput> { width: Fill, empty_text: "", is_password: true }
                            save_settings_button = <Button> { text: "" }
                        }

                        history_title = <H2> { text: "" }
                        clip_count = <Label> { text: "" }
                        clip_0 = <TextBox> { text: "" }
                        clip_1 = <TextBox> { text: "" }
                        clip_2 = <TextBox> { text: "" }
                        clip_3 = <TextBox> { text: "" }
                        clip_4 = <TextBox> { text: "" }
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
        if self.button(id!(capture_button)).clicked(actions) {
            self.capture_clipboard(cx);
        }
        if self.button(id!(copy_button)).clicked(actions) {
            self.copy_latest(cx);
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
        if self.button(id!(save_settings_button)).clicked(actions) {
            self.save_connection_settings(cx);
        }
    }
}

impl App {
    fn initialize(&mut self, cx: &mut Cx) {
        self.initialized = true;
        match ClientState::load() {
            Ok(state) => {
                self.state = Some(state);
                self.apply_i18n(cx);
                self.refresh_history(cx);
                self.set_status(cx, "app.status_local_ready");
            }
            Err(error) => {
                self.state = Some(ClientState::fallback(error.to_string()));
                self.apply_i18n(cx);
                self.refresh_history(cx);
            }
        }
    }

    fn button(&self, id: &[LiveId]) -> ButtonRef {
        self.ui.widget(id).as_button()
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

    fn template(&self, key: &str, name: &str, value: impl ToString) -> String {
        self.text(key).replace(name, &value.to_string())
    }

    fn apply_i18n(&mut self, cx: &mut Cx) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let messages = &state.messages;

        self.widget(id!(title))
            .set_text(cx, messages.text("app.title"));
        self.widget(id!(capture_button))
            .set_text(cx, messages.text("app.capture"));
        self.widget(id!(copy_button))
            .set_text(cx, messages.text("app.copy"));
        self.widget(id!(sync_button))
            .set_text(cx, messages.text("app.sync_now"));
        self.widget(id!(theme_button)).set_text(
            cx,
            match state.settings.theme {
                Theme::Light => messages.text("app.dark"),
                Theme::Dark => messages.text("app.light"),
            },
        );
        self.widget(id!(language_button))
            .set_text(cx, messages.text("app.lang_toggle"));
        self.widget(id!(settings_title))
            .set_text(cx, messages.text("app.settings"));
        self.widget(id!(local_status))
            .set_text(cx, messages.text("app.local_status"));
        self.widget(id!(server_label))
            .set_text(cx, messages.text("app.server"));
        self.widget(id!(token_label))
            .set_text(cx, messages.text("app.token"));
        self.widget(id!(save_settings_button))
            .set_text(cx, messages.text("app.save_settings"));
        self.widget(id!(history_title))
            .set_text(cx, messages.text("app.latest"));

        self.widget(id!(server_input))
            .set_text(cx, state.settings.server_url.as_deref().unwrap_or(""));
        self.widget(id!(token_input))
            .set_text(cx, state.settings.token.as_deref().unwrap_or(""));
        self.ui
            .widget(id!(server_input))
            .as_text_input()
            .set_empty_text(cx, messages.text("app.server_placeholder").to_owned());
        self.ui
            .widget(id!(token_input))
            .as_text_input()
            .set_empty_text(cx, messages.text("app.token_placeholder").to_owned());

        let light = state.settings.theme == Theme::Light;
        self.widget(id!(light_surface)).set_visible(cx, light);
        self.widget(id!(dark_surface)).set_visible(cx, !light);
        self.ui.redraw(cx);
    }

    fn capture_clipboard(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.capture_clipboard());
        match result {
            Some(Ok(())) => {
                self.set_status(cx, "app.status_capture_saved");
                self.refresh_history(cx);
            }
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
        }
    }

    fn copy_latest(&mut self, cx: &mut Cx) {
        let result = self.with_state_mut(|state| state.copy_latest());
        match result {
            Some(Ok(true)) => self.set_status(cx, "app.status_copied"),
            Some(Ok(false)) => self.set_status(cx, "app.status_no_clip"),
            Some(Err(error)) => self.set_status_text(cx, &error.to_string()),
            None => self.set_status(cx, "app.status_clipboard_unavailable"),
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

    fn toggle_theme(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.settings.theme = state.settings.theme.toggle();
            let _ = state.persist_settings();
        }
        self.apply_i18n(cx);
    }

    fn toggle_language(&mut self, cx: &mut Cx) {
        if let Some(state) = &mut self.state {
            state.settings.language = state.settings.language.toggle();
            state.messages = i18n::bundle(state.settings.language);
            let _ = state.persist_settings();
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
        self.set_status(cx, "app.status_settings_saved");
    }

    fn refresh_history(&mut self, cx: &mut Cx) {
        let Some(state) = self.state.as_ref() else {
            return;
        };

        let clips = state.store.list_clips(5).unwrap_or_default();
        let count = state
            .store
            .stats()
            .map(|stats| stats.clip_count)
            .unwrap_or(0);
        self.widget(id!(clip_count))
            .set_text(cx, &self.template("app.history_count", "{count}", count));

        for index in 0..5 {
            let label = match index {
                0 => id!(clip_0),
                1 => id!(clip_1),
                2 => id!(clip_2),
                3 => id!(clip_3),
                _ => id!(clip_4),
            };
            let text = clips
                .get(index)
                .and_then(|clip| clip.primary_text.as_deref())
                .map(yank_core::summarize_text)
                .unwrap_or_else(|| {
                    if index == 0 {
                        self.text("app.empty")
                    } else {
                        String::new()
                    }
                });
            self.widget(label).set_text(cx, &text);
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
        let messages = i18n::bundle(settings.language);
        let clipboard = Clipboard::new().ok();
        Ok(Self {
            store,
            settings,
            messages,
            clipboard,
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
        }
    }

    fn capture_clipboard(&mut self) -> Result<()> {
        let Some(clipboard) = &mut self.clipboard else {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
        };
        let text = clipboard.get_text()?;
        if text.trim().is_empty() {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_empty"));
        }
        let clip = self.store.save_text_clip(&self.settings.device_id, &text)?;
        if let Some(sync) = self.sync_client() {
            let _ = sync.push_clip(clip)?;
        }
        Ok(())
    }

    fn copy_latest(&mut self) -> Result<bool> {
        let Some(clipboard) = &mut self.clipboard else {
            anyhow::bail!("{}", self.messages.text("app.status_clipboard_unavailable"));
        };
        let Some(clip) = self.store.list_clips(1)?.into_iter().next() else {
            return Ok(false);
        };
        let Some(text) = clip.primary_text else {
            return Ok(false);
        };
        clipboard.set_text(text)?;
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

fn blank_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

app_main!(App);

fn main() {
    app_main();
}
