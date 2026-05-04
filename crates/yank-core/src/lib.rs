use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

pub const APP_NAME: &str = "yank";
pub const DEFAULT_SERVER_BIND: &str = "127.0.0.1:7219";

pub mod i18n;

#[derive(Debug, Error)]
pub enum YankError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid language: {0}")]
    InvalidLanguage(String),
    #[error("invalid theme: {0}")]
    InvalidTheme(String),
}

pub type Result<T> = std::result::Result<T, YankError>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "en" | "en-US" | "english" => Ok(Self::En),
            "zh" | "zh-CN" | "cn" | "chinese" => Ok(Self::Zh),
            other => Err(YankError::InvalidLanguage(other.to_owned())),
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::En => Self::Zh,
            Self::Zh => Self::En,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            other => Err(YankError::InvalidTheme(other.to_owned())),
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub theme: Theme,
    pub device_id: String,
    pub server_url: Option<String>,
    pub token: Option<String>,
    pub sync_enabled: bool,
    pub capture_enabled: bool,
    pub capture_text_enabled: bool,
    pub capture_html_enabled: bool,
    pub capture_image_enabled: bool,
    pub capture_files_enabled: bool,
    pub capture_interval_ms: u64,
    pub max_history: u32,
    pub duplicate_moves_to_top: bool,
    pub start_on_login: bool,
    pub show_tray_icon: bool,
    pub show_in_taskbar: bool,
    pub quick_paste_position: String,
    pub quick_paste_find_as_you_type: bool,
    pub quick_paste_show_hotkey_text: bool,
    pub quick_paste_show_leading_whitespace: bool,
    pub quick_paste_show_thumbnails: bool,
    pub quick_paste_draw_rtf: bool,
    pub quick_paste_ensure_visible: bool,
    pub quick_paste_show_groups_in_main: bool,
    pub quick_paste_prompt_delete: bool,
    pub quick_paste_always_show_scrollbar: bool,
    pub quick_paste_show_pasted_indicator: bool,
    pub quick_paste_elevated_paste: bool,
    pub quick_paste_lines_per_row: u32,
    pub quick_paste_transparency_percent: u32,
    pub hotkey_show_history: String,
    pub hotkey_search: String,
    pub hotkey_copy_selected: String,
    pub hotkey_delete_selected: String,
    pub hotkey_toggle_pin: String,
    pub hotkey_edit_selected: String,
    pub hotkey_capture_now: String,
    pub hotkey_sync_now: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: Theme::default(),
            device_id: Uuid::new_v4().to_string(),
            server_url: None,
            token: None,
            sync_enabled: false,
            capture_enabled: true,
            capture_text_enabled: true,
            capture_html_enabled: true,
            capture_image_enabled: true,
            capture_files_enabled: true,
            capture_interval_ms: 1000,
            max_history: 500,
            duplicate_moves_to_top: true,
            start_on_login: false,
            show_tray_icon: true,
            show_in_taskbar: true,
            quick_paste_position: "cursor".to_owned(),
            quick_paste_find_as_you_type: true,
            quick_paste_show_hotkey_text: true,
            quick_paste_show_leading_whitespace: false,
            quick_paste_show_thumbnails: true,
            quick_paste_draw_rtf: true,
            quick_paste_ensure_visible: true,
            quick_paste_show_groups_in_main: true,
            quick_paste_prompt_delete: true,
            quick_paste_always_show_scrollbar: false,
            quick_paste_show_pasted_indicator: true,
            quick_paste_elevated_paste: false,
            quick_paste_lines_per_row: 1,
            quick_paste_transparency_percent: 0,
            hotkey_show_history: "Ctrl+Backtick".to_owned(),
            hotkey_search: "Ctrl+F".to_owned(),
            hotkey_copy_selected: "Enter".to_owned(),
            hotkey_delete_selected: "Delete".to_owned(),
            hotkey_toggle_pin: "Ctrl+P".to_owned(),
            hotkey_edit_selected: "Ctrl+E".to_owned(),
            hotkey_capture_now: "Ctrl+Shift+C".to_owned(),
            hotkey_sync_now: "Ctrl+Shift+S".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClipFormat {
    pub format: String,
    pub mime: Option<String>,
    pub data: Vec<u8>,
}

impl ClipFormat {
    pub fn text(value: &str) -> Self {
        Self {
            format: "text/plain;charset=utf-8".to_owned(),
            mime: Some("text/plain".to_owned()),
            data: value.as_bytes().to_vec(),
        }
    }

    pub fn html(value: &str) -> Self {
        Self {
            format: "text/html;charset=utf-8".to_owned(),
            mime: Some("text/html".to_owned()),
            data: value.as_bytes().to_vec(),
        }
    }

    pub fn image_rgba(width: usize, height: usize, bytes: Vec<u8>) -> Self {
        Self {
            format: format!("image/rgba8;width={width};height={height}"),
            mime: Some("image/x-rgba8".to_owned()),
            data: bytes,
        }
    }

    pub fn file_list(paths: &[String]) -> Self {
        Self {
            format: "application/x-yank-file-list+json".to_owned(),
            mime: Some("application/json".to_owned()),
            data: serde_json::to_vec(paths).expect("file list paths should serialize"),
        }
    }

    pub fn is_text(&self) -> bool {
        self.mime.as_deref() == Some("text/plain") || self.format.starts_with("text/plain")
    }

    pub fn is_html(&self) -> bool {
        self.mime.as_deref() == Some("text/html") || self.format.starts_with("text/html")
    }

    pub fn text_value(&self) -> Option<&str> {
        if self.is_text() {
            std::str::from_utf8(&self.data).ok()
        } else {
            None
        }
    }

    pub fn html_value(&self) -> Option<&str> {
        if self.is_html() {
            std::str::from_utf8(&self.data).ok()
        } else {
            None
        }
    }

    pub fn image_rgba_dimensions(&self) -> Option<(usize, usize)> {
        if !self.format.starts_with("image/rgba8;") {
            return None;
        }

        let mut width = None;
        let mut height = None;
        for part in self.format.split(';').skip(1) {
            if let Some(value) = part.strip_prefix("width=") {
                width = value.parse().ok();
            } else if let Some(value) = part.strip_prefix("height=") {
                height = value.parse().ok();
            }
        }
        Some((width?, height?))
    }

    pub fn is_file_list(&self) -> bool {
        self.format == "application/x-yank-file-list+json"
    }

    pub fn file_list_paths(&self) -> Option<Vec<String>> {
        if self.is_file_list() {
            serde_json::from_slice(&self.data).ok()
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub device_id: String,
    pub description: String,
    pub primary_text: Option<String>,
    pub formats: Vec<ClipFormat>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub content_hash: String,
    pub pinned: bool,
    pub source_app: Option<String>,
}

impl Clip {
    pub fn from_text(device_id: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self::from_formats(
            device_id,
            summarize_text(&text),
            Some(text.clone()),
            vec![ClipFormat::text(&text)],
        )
    }

    pub fn from_formats(
        device_id: impl Into<String>,
        description: impl Into<String>,
        primary_text: Option<String>,
        formats: Vec<ClipFormat>,
    ) -> Self {
        let now = now_ts();
        let content_hash = content_hash(&formats);

        Self {
            id: Uuid::new_v4().to_string(),
            device_id: device_id.into(),
            description: description.into(),
            primary_text,
            formats,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            content_hash,
            pinned: false,
            source_app: None,
        }
    }

    pub fn touch_for_remote(mut self) -> Self {
        if self.id.is_empty() {
            self.id = Uuid::new_v4().to_string();
        }
        if self.created_at <= 0 {
            self.created_at = now_ts();
        }
        if self.updated_at <= 0 {
            self.updated_at = self.created_at;
        }
        if self.content_hash.is_empty() {
            self.content_hash = content_hash(&self.formats);
        }
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushClipRequest {
    pub clip: Clip,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushClipResponse {
    pub clip: Clip,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullClipsResponse {
    pub clips: Vec<Clip>,
    pub server_time: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub name: &'static str,
    pub version: &'static str,
    pub server_time: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreStats {
    pub clip_count: i64,
    pub deleted_count: i64,
    pub device_count: i64,
    pub newest_clip_at: Option<i64>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    pub fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS clips (
                id TEXT PRIMARY KEY NOT NULL,
                device_id TEXT NOT NULL,
                description TEXT NOT NULL,
                primary_text TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                deleted_at INTEGER,
                content_hash TEXT NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0,
                source_app TEXT
            );

            CREATE TABLE IF NOT EXISTS clip_formats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id TEXT NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
                format TEXT NOT NULL,
                mime TEXT,
                data BLOB NOT NULL,
                UNIQUE(clip_id, format)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS clips_updated_at_idx ON clips(updated_at DESC);
            CREATE INDEX IF NOT EXISTS clips_hash_idx ON clips(content_hash);
            CREATE INDEX IF NOT EXISTS clips_device_idx ON clips(device_id);
            "#,
        )?;
        Ok(())
    }

    pub fn save_clip(&self, clip: &Clip) -> Result<Clip> {
        let clip = clip.clone().touch_for_remote();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            r#"
            INSERT INTO clips (
                id, device_id, description, primary_text, created_at, updated_at,
                deleted_at, content_hash, pinned, source_app
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                device_id = excluded.device_id,
                description = excluded.description,
                primary_text = excluded.primary_text,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at,
                content_hash = excluded.content_hash,
                pinned = excluded.pinned,
                source_app = excluded.source_app
            "#,
            params![
                clip.id,
                clip.device_id,
                clip.description,
                clip.primary_text,
                clip.created_at,
                clip.updated_at,
                clip.deleted_at,
                clip.content_hash,
                clip.pinned as i64,
                clip.source_app,
            ],
        )?;
        tx.execute(
            "DELETE FROM clip_formats WHERE clip_id = ?1",
            params![clip.id],
        )?;
        for format in &clip.formats {
            tx.execute(
                r#"
                INSERT INTO clip_formats (clip_id, format, mime, data)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![clip.id, format.format, format.mime, format.data],
            )?;
        }
        tx.execute(
            "INSERT INTO sync_events (clip_id, event_type, created_at) VALUES (?1, ?2, ?3)",
            params![clip.id, "upsert", now_ts()],
        )?;
        tx.commit()?;
        Ok(clip)
    }

    pub fn save_text_clip(&self, device_id: &str, text: &str) -> Result<Clip> {
        self.save_clip_deduplicated(&Clip::from_text(device_id, text), true)
    }

    pub fn save_clip_deduplicated(&self, clip: &Clip, move_to_top: bool) -> Result<Clip> {
        let clip = clip.clone().touch_for_remote();
        if let Some(mut existing) = self.find_active_by_hash(&clip.content_hash)? {
            if move_to_top {
                let updated_at = now_ts();
                self.conn.execute(
                    "UPDATE clips SET updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                    params![updated_at, existing.id],
                )?;
                self.conn.execute(
                    "INSERT INTO sync_events (clip_id, event_type, created_at) VALUES (?1, ?2, ?3)",
                    params![existing.id, "upsert", updated_at],
                )?;
                existing.updated_at = updated_at;
            }
            return Ok(existing);
        }
        self.save_clip(&clip)
    }

    pub fn list_clips(&self, limit: u32) -> Result<Vec<Clip>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, device_id, description, primary_text, created_at, updated_at,
                   deleted_at, content_hash, pinned, source_app
            FROM clips
            WHERE deleted_at IS NULL
            ORDER BY pinned DESC, updated_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| self.clip_from_row(row))?;
        let mut clips = Vec::new();
        for row in rows {
            clips.push(self.with_formats(row?)?);
        }
        Ok(clips)
    }

    pub fn search_clips(&self, query: &str, limit: u32) -> Result<Vec<Clip>> {
        let query = query.trim();
        if query.is_empty() {
            return self.list_clips(limit);
        }

        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, device_id, description, primary_text, created_at, updated_at,
                   deleted_at, content_hash, pinned, source_app
            FROM clips
            WHERE deleted_at IS NULL
              AND (description LIKE ?1 ESCAPE '\' OR primary_text LIKE ?1 ESCAPE '\')
            ORDER BY pinned DESC, updated_at DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![pattern, limit], |row| self.clip_from_row(row))?;
        let mut clips = Vec::new();
        for row in rows {
            clips.push(self.with_formats(row?)?);
        }
        Ok(clips)
    }

    pub fn list_clips_since(&self, since: i64, limit: u32) -> Result<Vec<Clip>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, device_id, description, primary_text, created_at, updated_at,
                   deleted_at, content_hash, pinned, source_app
            FROM clips
            WHERE updated_at > ?1
            ORDER BY updated_at ASC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![since, limit], |row| self.clip_from_row(row))?;
        let mut clips = Vec::new();
        for row in rows {
            clips.push(self.with_formats(row?)?);
        }
        Ok(clips)
    }

    pub fn get_clip(&self, id: &str) -> Result<Option<Clip>> {
        let clip = self
            .conn
            .query_row(
                r#"
                SELECT id, device_id, description, primary_text, created_at, updated_at,
                       deleted_at, content_hash, pinned, source_app
                FROM clips
                WHERE id = ?1
                "#,
                params![id],
                |row| self.clip_from_row(row),
            )
            .optional()?;
        clip.map(|clip| self.with_formats(clip)).transpose()
    }

    pub fn delete_clip(&self, id: &str) -> Result<bool> {
        let deleted_at = now_ts();
        let changed = self.conn.execute(
            "UPDATE clips SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![deleted_at, id],
        )?;
        if changed > 0 {
            self.conn.execute(
                "INSERT INTO sync_events (clip_id, event_type, created_at) VALUES (?1, ?2, ?3)",
                params![id, "delete", deleted_at],
            )?;
        }
        Ok(changed > 0)
    }

    pub fn set_clip_pinned(&self, id: &str, pinned: bool) -> Result<Option<Clip>> {
        let Some(mut clip) = self.get_clip(id)? else {
            return Ok(None);
        };
        if clip.deleted_at.is_some() {
            return Ok(None);
        }
        clip.pinned = pinned;
        clip.updated_at = now_ts();
        self.save_clip(&clip).map(Some)
    }

    pub fn toggle_clip_pinned(&self, id: &str) -> Result<Option<Clip>> {
        let Some(clip) = self.get_clip(id)? else {
            return Ok(None);
        };
        self.set_clip_pinned(id, !clip.pinned)
    }

    pub fn update_clip_text(&self, id: &str, text: &str) -> Result<Option<Clip>> {
        let Some(mut clip) = self.get_clip(id)? else {
            return Ok(None);
        };
        if clip.deleted_at.is_some() {
            return Ok(None);
        }

        clip.primary_text = Some(text.to_owned());
        clip.description = summarize_text(text);
        clip.formats = vec![ClipFormat::text(text)];
        clip.content_hash = content_hash(&clip.formats);
        clip.updated_at = now_ts();
        self.save_clip(&clip).map(Some)
    }

    pub fn find_active_by_content_hash(&self, hash: &str) -> Result<Option<Clip>> {
        self.find_active_by_hash(hash)
    }

    pub fn enforce_max_history(&self, max_history: u32) -> Result<usize> {
        if max_history == 0 {
            return Ok(0);
        }

        let ids = {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id
                FROM clips
                WHERE deleted_at IS NULL AND pinned = 0
                ORDER BY updated_at DESC
                LIMIT -1 OFFSET ?1
                "#,
            )?;
            let rows = stmt.query_map(params![max_history], |row| row.get::<_, String>(0))?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(row?);
            }
            ids
        };

        for id in &ids {
            self.delete_clip(id)?;
        }

        Ok(ids.len())
    }

    pub fn stats(&self) -> Result<StoreStats> {
        let clip_count = self.conn.query_row(
            "SELECT COUNT(*) FROM clips WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        let deleted_count = self.conn.query_row(
            "SELECT COUNT(*) FROM clips WHERE deleted_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        let device_count =
            self.conn
                .query_row("SELECT COUNT(DISTINCT device_id) FROM clips", [], |row| {
                    row.get(0)
                })?;
        let newest_clip_at = self
            .conn
            .query_row(
                "SELECT MAX(updated_at) FROM clips WHERE deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        Ok(StoreStats {
            clip_count,
            deleted_count,
            device_count,
            newest_clip_at,
        })
    }

    pub fn settings(&self) -> Result<Settings> {
        let mut settings = Settings::default();
        if let Some(language) = self.get_setting("language")? {
            settings.language = Language::parse(&language)?;
        }
        if let Some(theme) = self.get_setting("theme")? {
            settings.theme = Theme::parse(&theme)?;
        }
        if let Some(device_id) = self.get_setting("device_id")? {
            settings.device_id = device_id;
        } else {
            self.set_setting("device_id", &settings.device_id)?;
        }
        settings.server_url = self.get_setting("server_url")?;
        settings.token = self.get_setting("token")?;
        settings.sync_enabled = self
            .get_setting("sync_enabled")?
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or(false);
        settings.capture_enabled =
            self.get_bool_setting("capture_enabled", settings.capture_enabled)?;
        settings.capture_text_enabled =
            self.get_bool_setting("capture_text_enabled", settings.capture_text_enabled)?;
        settings.capture_html_enabled =
            self.get_bool_setting("capture_html_enabled", settings.capture_html_enabled)?;
        settings.capture_image_enabled =
            self.get_bool_setting("capture_image_enabled", settings.capture_image_enabled)?;
        settings.capture_files_enabled =
            self.get_bool_setting("capture_files_enabled", settings.capture_files_enabled)?;
        settings.capture_interval_ms =
            self.get_u64_setting("capture_interval_ms", settings.capture_interval_ms)?;
        settings.max_history = self.get_u32_setting("max_history", settings.max_history)?;
        settings.duplicate_moves_to_top =
            self.get_bool_setting("duplicate_moves_to_top", settings.duplicate_moves_to_top)?;
        settings.start_on_login =
            self.get_bool_setting("start_on_login", settings.start_on_login)?;
        settings.show_tray_icon =
            self.get_bool_setting("show_tray_icon", settings.show_tray_icon)?;
        settings.show_in_taskbar =
            self.get_bool_setting("show_in_taskbar", settings.show_in_taskbar)?;
        settings.quick_paste_position =
            self.get_string_setting("quick_paste_position", settings.quick_paste_position)?;
        settings.quick_paste_find_as_you_type = self.get_bool_setting(
            "quick_paste_find_as_you_type",
            settings.quick_paste_find_as_you_type,
        )?;
        settings.quick_paste_show_hotkey_text = self.get_bool_setting(
            "quick_paste_show_hotkey_text",
            settings.quick_paste_show_hotkey_text,
        )?;
        settings.quick_paste_show_leading_whitespace = self.get_bool_setting(
            "quick_paste_show_leading_whitespace",
            settings.quick_paste_show_leading_whitespace,
        )?;
        settings.quick_paste_show_thumbnails = self.get_bool_setting(
            "quick_paste_show_thumbnails",
            settings.quick_paste_show_thumbnails,
        )?;
        settings.quick_paste_draw_rtf =
            self.get_bool_setting("quick_paste_draw_rtf", settings.quick_paste_draw_rtf)?;
        settings.quick_paste_ensure_visible = self.get_bool_setting(
            "quick_paste_ensure_visible",
            settings.quick_paste_ensure_visible,
        )?;
        settings.quick_paste_show_groups_in_main = self.get_bool_setting(
            "quick_paste_show_groups_in_main",
            settings.quick_paste_show_groups_in_main,
        )?;
        settings.quick_paste_prompt_delete = self.get_bool_setting(
            "quick_paste_prompt_delete",
            settings.quick_paste_prompt_delete,
        )?;
        settings.quick_paste_always_show_scrollbar = self.get_bool_setting(
            "quick_paste_always_show_scrollbar",
            settings.quick_paste_always_show_scrollbar,
        )?;
        settings.quick_paste_show_pasted_indicator = self.get_bool_setting(
            "quick_paste_show_pasted_indicator",
            settings.quick_paste_show_pasted_indicator,
        )?;
        settings.quick_paste_elevated_paste = self.get_bool_setting(
            "quick_paste_elevated_paste",
            settings.quick_paste_elevated_paste,
        )?;
        settings.quick_paste_lines_per_row = self.get_u32_setting(
            "quick_paste_lines_per_row",
            settings.quick_paste_lines_per_row,
        )?;
        settings.quick_paste_transparency_percent = self.get_u32_setting(
            "quick_paste_transparency_percent",
            settings.quick_paste_transparency_percent,
        )?;
        settings.hotkey_show_history =
            self.get_string_setting("hotkey_show_history", settings.hotkey_show_history)?;
        settings.hotkey_search =
            self.get_string_setting("hotkey_search", settings.hotkey_search)?;
        settings.hotkey_copy_selected =
            self.get_string_setting("hotkey_copy_selected", settings.hotkey_copy_selected)?;
        settings.hotkey_delete_selected =
            self.get_string_setting("hotkey_delete_selected", settings.hotkey_delete_selected)?;
        settings.hotkey_toggle_pin =
            self.get_string_setting("hotkey_toggle_pin", settings.hotkey_toggle_pin)?;
        settings.hotkey_edit_selected =
            self.get_string_setting("hotkey_edit_selected", settings.hotkey_edit_selected)?;
        settings.hotkey_capture_now =
            self.get_string_setting("hotkey_capture_now", settings.hotkey_capture_now)?;
        settings.hotkey_sync_now =
            self.get_string_setting("hotkey_sync_now", settings.hotkey_sync_now)?;
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.set_setting("language", settings.language.as_str())?;
        self.set_setting("theme", settings.theme.as_str())?;
        self.set_setting("device_id", &settings.device_id)?;
        self.set_optional_setting("server_url", settings.server_url.as_deref())?;
        self.set_optional_setting("token", settings.token.as_deref())?;
        self.set_setting(
            "sync_enabled",
            if settings.sync_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "capture_enabled",
            if settings.capture_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "capture_text_enabled",
            if settings.capture_text_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "capture_html_enabled",
            if settings.capture_html_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "capture_image_enabled",
            if settings.capture_image_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "capture_files_enabled",
            if settings.capture_files_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "capture_interval_ms",
            &settings.capture_interval_ms.to_string(),
        )?;
        self.set_setting("max_history", &settings.max_history.to_string())?;
        self.set_setting(
            "duplicate_moves_to_top",
            if settings.duplicate_moves_to_top {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "start_on_login",
            if settings.start_on_login {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "show_tray_icon",
            if settings.show_tray_icon {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "show_in_taskbar",
            if settings.show_in_taskbar {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting("quick_paste_position", &settings.quick_paste_position)?;
        self.set_setting(
            "quick_paste_find_as_you_type",
            if settings.quick_paste_find_as_you_type {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_show_hotkey_text",
            if settings.quick_paste_show_hotkey_text {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_show_leading_whitespace",
            if settings.quick_paste_show_leading_whitespace {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_show_thumbnails",
            if settings.quick_paste_show_thumbnails {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_draw_rtf",
            if settings.quick_paste_draw_rtf {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_ensure_visible",
            if settings.quick_paste_ensure_visible {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_show_groups_in_main",
            if settings.quick_paste_show_groups_in_main {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_prompt_delete",
            if settings.quick_paste_prompt_delete {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_always_show_scrollbar",
            if settings.quick_paste_always_show_scrollbar {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_show_pasted_indicator",
            if settings.quick_paste_show_pasted_indicator {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_elevated_paste",
            if settings.quick_paste_elevated_paste {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_setting(
            "quick_paste_lines_per_row",
            &settings.quick_paste_lines_per_row.to_string(),
        )?;
        self.set_setting(
            "quick_paste_transparency_percent",
            &settings.quick_paste_transparency_percent.to_string(),
        )?;
        self.set_setting("hotkey_show_history", &settings.hotkey_show_history)?;
        self.set_setting("hotkey_search", &settings.hotkey_search)?;
        self.set_setting("hotkey_copy_selected", &settings.hotkey_copy_selected)?;
        self.set_setting("hotkey_delete_selected", &settings.hotkey_delete_selected)?;
        self.set_setting("hotkey_toggle_pin", &settings.hotkey_toggle_pin)?;
        self.set_setting("hotkey_edit_selected", &settings.hotkey_edit_selected)?;
        self.set_setting("hotkey_capture_now", &settings.hotkey_capture_now)?;
        self.set_setting("hotkey_sync_now", &settings.hotkey_sync_now)?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_optional_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        if let Some(value) = value {
            self.set_setting(key, value)
        } else {
            self.conn
                .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            Ok(())
        }
    }

    fn get_string_setting(&self, key: &str, default: String) -> Result<String> {
        Ok(self.get_setting(key)?.unwrap_or(default))
    }

    fn get_bool_setting(&self, key: &str, default: bool) -> Result<bool> {
        Ok(self
            .get_setting(key)?
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or(default))
    }

    fn get_u32_setting(&self, key: &str, default: u32) -> Result<u32> {
        Ok(self
            .get_setting(key)?
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(default))
    }

    fn get_u64_setting(&self, key: &str, default: u64) -> Result<u64> {
        Ok(self
            .get_setting(key)?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(default))
    }

    fn find_active_by_hash(&self, hash: &str) -> Result<Option<Clip>> {
        let clip = self
            .conn
            .query_row(
                r#"
                SELECT id, device_id, description, primary_text, created_at, updated_at,
                       deleted_at, content_hash, pinned, source_app
                FROM clips
                WHERE content_hash = ?1 AND deleted_at IS NULL
                ORDER BY updated_at DESC
                LIMIT 1
                "#,
                params![hash],
                |row| self.clip_from_row(row),
            )
            .optional()?;
        clip.map(|clip| self.with_formats(clip)).transpose()
    }

    fn with_formats(&self, mut clip: Clip) -> Result<Clip> {
        let mut stmt = self.conn.prepare(
            "SELECT format, mime, data FROM clip_formats WHERE clip_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![clip.id], |row| {
            Ok(ClipFormat {
                format: row.get(0)?,
                mime: row.get(1)?,
                data: row.get(2)?,
            })
        })?;
        let mut formats = Vec::new();
        for row in rows {
            formats.push(row?);
        }
        clip.formats = formats;
        Ok(clip)
    }

    fn clip_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<Clip> {
        Ok(Clip {
            id: row.get(0)?,
            device_id: row.get(1)?,
            description: row.get(2)?,
            primary_text: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            deleted_at: row.get(6)?,
            content_hash: row.get(7)?,
            pinned: row.get::<_, i64>(8)? != 0,
            source_app: row.get(9)?,
            formats: Vec::new(),
        })
    }
}

pub fn now_ts() -> i64 {
    Utc::now().timestamp()
}

pub fn summarize_text(text: &str) -> String {
    let summary = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect::<String>();
    if summary.is_empty() {
        "(empty text)".to_owned()
    } else {
        summary
    }
}

pub fn content_hash(formats: &[ClipFormat]) -> String {
    let mut hasher = Sha256::new();
    for format in formats {
        hasher.update(format.format.as_bytes());
        hasher.update([0]);
        if let Some(mime) = &format.mime {
            hasher.update(mime.as_bytes());
        }
        hasher.update([0]);
        hasher.update(&format.data);
        hasher.update([0xff]);
    }
    hex::encode(hasher.finalize())
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_text_clip_with_format_payload() {
        let store = Store::open_memory().unwrap();
        let clip = store.save_text_clip("device-a", "hello from yank").unwrap();

        let clips = store.list_clips(20).unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].id, clip.id);
        assert_eq!(clips[0].primary_text.as_deref(), Some("hello from yank"));
        assert_eq!(clips[0].formats[0].format, "text/plain;charset=utf-8");
    }

    #[test]
    fn stores_multi_format_clip_payloads() {
        let store = Store::open_memory().unwrap();
        let paths = vec!["/tmp/a.txt".to_owned(), "/tmp/b.png".to_owned()];
        let clip = Clip::from_formats(
            "device-a",
            "2 files, image and html",
            Some(paths.join("\n")),
            vec![
                ClipFormat::file_list(&paths),
                ClipFormat::image_rgba(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]),
                ClipFormat::html("<b>hello</b>"),
            ],
        );

        let saved = store.save_clip_deduplicated(&clip, true).unwrap();
        let loaded = store.get_clip(&saved.id).unwrap().unwrap();

        assert!(loaded.formats[0].is_file_list());
        assert_eq!(loaded.formats[0].file_list_paths().unwrap(), paths);
        assert_eq!(loaded.formats[1].image_rgba_dimensions(), Some((2, 1)));
        assert!(loaded.formats[2].is_html());
        assert_eq!(loaded.formats[2].html_value(), Some("<b>hello</b>"));
    }

    #[test]
    fn deduplicates_active_text_by_hash() {
        let store = Store::open_memory().unwrap();
        let first = store.save_text_clip("device-a", "same text").unwrap();
        let second = store.save_text_clip("device-a", "same text").unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(store.list_clips(20).unwrap().len(), 1);
    }

    #[test]
    fn searches_updates_pins_and_prunes_history() {
        let store = Store::open_memory().unwrap();
        let keep = store.save_text_clip("device-a", "keep pinned").unwrap();
        let edit = store.save_text_clip("device-a", "edit this").unwrap();
        let old = store.save_text_clip("device-a", "old clip").unwrap();

        let pinned = store.set_clip_pinned(&keep.id, true).unwrap().unwrap();
        assert!(pinned.pinned);
        assert_eq!(store.search_clips("edit", 10).unwrap()[0].id, edit.id);

        let edited = store
            .update_clip_text(&edit.id, "edited text body")
            .unwrap()
            .unwrap();
        assert_eq!(edited.primary_text.as_deref(), Some("edited text body"));
        assert_eq!(edited.formats[0], ClipFormat::text("edited text body"));

        assert_eq!(store.enforce_max_history(1).unwrap(), 1);
        assert!(
            store
                .get_clip(&old.id)
                .unwrap()
                .unwrap()
                .deleted_at
                .is_some()
        );
        assert!(
            store
                .get_clip(&keep.id)
                .unwrap()
                .unwrap()
                .deleted_at
                .is_none()
        );
    }

    #[test]
    fn soft_delete_keeps_sync_visible() {
        let store = Store::open_memory().unwrap();
        let clip = store.save_text_clip("device-a", "delete me").unwrap();

        assert!(store.delete_clip(&clip.id).unwrap());
        assert!(store.list_clips(20).unwrap().is_empty());
        assert!(
            store.list_clips_since(0, 20).unwrap()[0]
                .deleted_at
                .is_some()
        );
    }

    #[test]
    fn persists_settings() {
        let store = Store::open_memory().unwrap();
        let mut settings = store.settings().unwrap();
        settings.language = Language::Zh;
        settings.theme = Theme::Dark;
        settings.server_url = Some("http://localhost:7219".to_owned());
        settings.token = Some("secret".to_owned());
        settings.sync_enabled = true;

        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings().unwrap(), settings);
    }
}
