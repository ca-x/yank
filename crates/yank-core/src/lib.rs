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
        let now = now_ts();
        let description = summarize_text(&text);
        let formats = vec![ClipFormat::text(&text)];
        let content_hash = content_hash(&formats);

        Self {
            id: Uuid::new_v4().to_string(),
            device_id: device_id.into(),
            description,
            primary_text: Some(text),
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
        if let Some(existing) =
            self.find_active_by_hash(&content_hash(&[ClipFormat::text(text)]))?
        {
            return Ok(existing);
        }
        self.save_clip(&Clip::from_text(device_id, text))
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
    fn deduplicates_active_text_by_hash() {
        let store = Store::open_memory().unwrap();
        let first = store.save_text_clip("device-a", "same text").unwrap();
        let second = store.save_text_clip("device-a", "same text").unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(store.list_clips(20).unwrap().len(), 1);
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
