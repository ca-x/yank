//! Local-first clipboard records and transactional replication.
use prost::Message;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, time::Duration};
use thiserror::Error;
use uuid::Uuid;

pub const APP_NAME: &str = "yank";
pub mod history;
pub mod icons;
pub const MAX_WIRE_BYTES: usize = 16 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}
pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Language {
    En,
    Zh,
}
pub mod i18n;
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

    pub fn rtf(value: &str) -> Self {
        Self {
            format: "text/rtf;charset=utf-8".to_owned(),
            mime: Some("text/rtf".to_owned()),
            data: value.as_bytes().to_vec(),
        }
    }

    pub fn color(value: &str) -> Self {
        Self {
            format: "application/x-yank-color".to_owned(),
            mime: Some("text/plain".to_owned()),
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

    pub fn is_rtf(&self) -> bool {
        self.mime.as_deref() == Some("text/rtf") || self.format.starts_with("text/rtf")
    }

    pub fn is_color(&self) -> bool {
        self.format == "application/x-yank-color"
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

    pub fn rtf_value(&self) -> Option<&str> {
        if self.is_rtf() {
            std::str::from_utf8(&self.data).ok()
        } else {
            None
        }
    }

    pub fn color_value(&self) -> Option<&str> {
        if self.is_color() {
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

/// A version vector provides causal ordering without wall-clock assumptions.
pub type Clock = BTreeMap<String, u64>;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub device_id: String,
    pub description: String,
    pub text: String,
    pub kind: String,
    pub collection: String,
    pub formats: Vec<ClipFormat>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
    pub pinned: bool,
    pub clock: Clock,
    #[serde(default)]
    pub source_app: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub write_order: u64,
    #[serde(default)]
    pub write_actor: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) blob_refs: Vec<String>,
}
impl Clip {
    pub fn text(device: &str, text: &str) -> Self {
        Self::from_formats(device, text, vec![ClipFormat::text(text)])
    }
    pub fn from_formats(device: &str, text: &str, formats: Vec<ClipFormat>) -> Self {
        let kind = if formats.iter().any(|f| {
            f.image_rgba_dimensions().is_some()
                || f.mime.as_deref().is_some_and(|m| m.starts_with("image/"))
        }) {
            "image"
        } else if formats.iter().any(ClipFormat::is_file_list) {
            "files"
        } else if text.starts_with("https://") || text.starts_with("http://") {
            "link"
        } else if is_color(text) {
            "color"
        } else if text.lines().count() > 1
            && [
                "fn ",
                "function ",
                "const ",
                "let ",
                "import ",
                "def ",
                "SELECT ",
                "public ",
            ]
            .iter()
            .any(|s| text.contains(s))
        {
            "code"
        } else {
            "text"
        };
        Self {
            id: new_id(),
            device_id: device.into(),
            description: text
                .lines()
                .next()
                .unwrap_or(kind)
                .chars()
                .take(100)
                .collect(),
            text: text.into(),
            kind: kind.into(),
            collection: String::new(),
            formats,
            created_at: now_ts(),
            updated_at: now_ts(),
            deleted: false,
            pinned: false,
            clock: Clock::new(),
            source_app: String::new(),
            version_id: new_id(),
            write_order: 0,
            write_actor: device.into(),
            blob_refs: Vec::new(),
        }
    }
    pub fn content_hash(&self) -> String {
        hex::encode(Sha256::digest(
            serde_json::to_vec(&self.formats).unwrap_or_default(),
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub clips: Vec<Clip>,
    pub cursor: i64,
    pub epoch: String,
    pub more: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mutation {
    pub operation: String,
    pub clip: Clip,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    pub operation: String,
    pub clip: Clip,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub clips: u64,
    pub pinned: u64,
    pub deleted: u64,
    pub pending: u64,
    pub conflicts: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerAddress {
    pub relay_urls: Vec<String>,
    pub direct_addresses: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub code: String,
    pub address: PeerAddress,
}
/// Cleanup preferences belong to this replica; deletion mutations use normal sync.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    #[default]
    Total,
    PerType,
}

pub const CLIP_KINDS: [&str; 6] = ["text", "link", "image", "files", "code", "color"];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupPolicy {
    pub enabled: bool,
    pub mode: CleanupMode,
    /// Limits count live, unpinned clips; favorites never consume this budget.
    pub max_items: u32,
    pub per_type: BTreeMap<String, u32>,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: CleanupMode::Total,
            max_items: 1000,
            per_type: CLIP_KINDS
                .into_iter()
                .map(|kind| (kind.into(), 200))
                .collect(),
        }
    }
}

impl CleanupPolicy {
    pub fn validate(&self) -> Result<()> {
        if !(1..=1_000_000).contains(&self.max_items)
            || self.per_type.iter().any(|(kind, count)| {
                !CLIP_KINDS.contains(&kind.as_str()) || !(1..=1_000_000).contains(count)
            })
            || (self.mode == CleanupMode::PerType
                && CLIP_KINDS
                    .iter()
                    .any(|kind| !self.per_type.contains_key(*kind)))
        {
            return Err(Error::Invalid(
                "Cleanup limits must be between 1 and 1000000 for every selected type".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Ask,
    Minimize,
    Quit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowStyle {
    #[default]
    System,
    TrafficLights,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub auto_update: bool,
    pub launch_at_login: bool,
    pub close_behavior: CloseBehavior,
    pub window_style: WindowStyle,
    pub history_range: history::HistoryRange,
    pub retention_days: u32,
    pub cleanup: CleanupPolicy,
    pub backup_daily: bool,
    pub excluded_apps: String,
    pub panel_position: String,
    pub panel_size: u32,
    pub silent_start: bool,
    pub hotkey_show: String,
    pub hotkey_next: String,
    pub hotkey_pause: String,
    pub hotkey_float: String,
    pub capture_protection: bool,
    pub apply_remote: bool,
    pub device_name: String,
    pub ditto_enabled: bool,
    pub ditto_listen: String,
    pub ditto_password: String,
    pub ditto_remote: String,
    pub capture: bool,
    pub upload: bool,
    pub server_url: String,
    pub username: String,
    pub account_id: String,
    pub relay_url: String,
    pub token: String,
    pub mode: String,
    pub theme: String,
    pub language: String,
    pub exclude_pattern: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            auto_update: false,
            launch_at_login: false,
            close_behavior: CloseBehavior::Ask,
            window_style: WindowStyle::System,
            history_range: history::HistoryRange::default(),
            retention_days: 0,
            cleanup: CleanupPolicy::default(),
            backup_daily: true,
            excluded_apps: String::new(),
            panel_position: "center".into(),
            panel_size: 3,
            silent_start: false,
            hotkey_show: "mod+shift+v".into(),
            hotkey_next: "mod+alt+n".into(),
            hotkey_pause: "mod+alt+p".into(),
            hotkey_float: "mod+shift+f".into(),
            capture_protection: true,
            apply_remote: true,
            device_name: "My device".into(),
            ditto_enabled: false,
            ditto_listen: "127.0.0.1:23443".into(),
            ditto_password: String::new(),
            ditto_remote: String::new(),
            capture: true,
            upload: false,
            server_url: String::new(),
            username: String::new(),
            account_id: String::new(),
            relay_url: String::new(),
            token: String::new(),
            mode: "relay".into(),
            theme: "system".into(),
            language: "zh".into(),
            exclude_pattern: String::new(),
        }
    }
}

/// One connection per worker; never hold a transaction across network I/O.
pub struct Store {
    conn: Connection,
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(3))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS blobs(hash TEXT PRIMARY KEY,data BLOB NOT NULL);
          CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS clips(id TEXT PRIMARY KEY,body TEXT NOT NULL,deleted INTEGER NOT NULL,pinned INTEGER NOT NULL,kind TEXT NOT NULL,collection TEXT NOT NULL,search TEXT NOT NULL,updated INTEGER NOT NULL,hash TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS changes(seq INTEGER PRIMARY KEY AUTOINCREMENT,clip_id TEXT NOT NULL,body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS outbox(clip_id TEXT PRIMARY KEY,operation TEXT UNIQUE NOT NULL,body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,ack TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS conflicts(id TEXT PRIMARY KEY,clip_id TEXT NOT NULL,body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS peers(id TEXT PRIMARY KEY,body TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS clips_search ON clips(deleted,pinned DESC,updated DESC,id);
          CREATE INDEX IF NOT EXISTS clips_hash ON clips(hash);
          CREATE INDEX IF NOT EXISTS clips_cleanup ON clips(deleted,pinned,kind,updated DESC,id DESC);")?;
        conn.execute(
            "INSERT OR IGNORE INTO metadata VALUES('epoch',?1)",
            [new_id()],
        )?;
        history::init(&conn)?;
        let store = Self { conn };
        if store.value("default_views_initialized")?.is_none() {
            let tx = write_transaction(&store.conn)?;
            for (id, name, filter, icon) in [
                ("images", "图片", "image", "image"),
                ("links", "链接", "link", "link"),
                ("code", "代码", "code", "code"),
                ("files", "文件", "files", "file"),
                ("colors", "颜色", "color", "palette"),
            ] {
                store.save_view(SavedView {
                    id: format!("default-{id}"),
                    name: name.into(),
                    query: String::new(),
                    filter: filter.into(),
                    days: 0,
                    icon: icon.into(),
                })?;
            }
            store.set_value("default_views_initialized", "1")?;
            tx.commit()?;
        }
        Ok(store)
    }
    pub fn get(&self, id: &str) -> Result<Option<Clip>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT body FROM clips WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        raw.map(|s| self.load(&s)).transpose()
    }
    pub fn list(&self, query: &str, filter: &str, limit: u32, offset: u32) -> Result<Vec<Clip>> {
        self.list_with_age(query, filter, 0, limit, offset)
    }
    pub fn list_with_age(
        &self,
        query: &str,
        filter: &str,
        days: u32,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Clip>> {
        let cutoff = if days == 0 {
            i64::MIN
        } else {
            now_ts() - i64::from(days) * 86400
        };
        let mut stmt = self.conn.prepare("SELECT body FROM clips WHERE deleted=0 AND updated>=?5 AND instr(lower(search),lower(?1))>0 AND (?2='all' OR (?2='pinned' AND pinned=1) OR kind=?2 OR collection=?2) ORDER BY pinned DESC,updated DESC,id DESC LIMIT ?3 OFFSET ?4")?;
        let rows = stmt.query_map(
            params![query, filter, limit.min(500), offset, cutoff],
            |r| r.get::<_, String>(0),
        )?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    fn pack(&self, clip: &Clip) -> Result<String> {
        let mut stored = clip.clone();
        stored.blob_refs.clear();
        for format in &mut stored.formats {
            let hash = hex::encode(Sha256::digest(&format.data));
            self.conn.execute(
                "INSERT OR IGNORE INTO blobs VALUES(?1,?2)",
                params![hash, format.data],
            )?;
            stored.blob_refs.push(hash);
            format.data.clear();
        }
        Ok(serde_json::to_string(&stored)?)
    }
    fn expand(&self, mut clip: Clip) -> Result<Clip> {
        if !clip.blob_refs.is_empty() {
            if clip.blob_refs.len() != clip.formats.len() {
                return Err(Error::Invalid("Invalid format references".into()));
            }
            for (format, hash) in clip.formats.iter_mut().zip(&clip.blob_refs) {
                format.data =
                    self.conn
                        .query_row("SELECT data FROM blobs WHERE hash=?1", [hash], |r| r.get(0))?;
            }
            clip.blob_refs.clear();
        }
        Ok(clip)
    }
    fn load(&self, raw: &str) -> Result<Clip> {
        self.expand(serde_json::from_str(raw)?)
    }
    fn write(&self, clip: &Clip) -> Result<()> {
        let body = self.pack(clip)?;
        self.conn.execute("INSERT INTO clips(id,body,deleted,pinned,kind,collection,search,updated,hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET body=excluded.body,deleted=excluded.deleted,pinned=excluded.pinned,kind=excluded.kind,collection=excluded.collection,search=excluded.search,updated=excluded.updated,hash=excluded.hash", params![clip.id,body,clip.deleted,clip.pinned,clip.kind,clip.collection,format!("{} {} {}",clip.description,clip.text,clip.collection),clip.updated_at,clip.content_hash()])?;
        let rank = self.next_history_rank()?;
        self.conn.execute(
            "UPDATE clips SET recency=?2,activity_at=MAX(activity_at,updated) WHERE id=?1",
            params![clip.id, rank],
        )?;
        self.conn.execute(
            "INSERT INTO changes(clip_id,body) VALUES(?1,?2)",
            params![clip.id, body],
        )?;
        Ok(())
    }
    pub fn save_local(&self, clip: Clip, device: &str) -> Result<Clip> {
        let tx = write_transaction(&self.conn)?;
        let clip = self.save_local_record(clip, device)?;
        tx.commit()?;
        Ok(clip)
    }
    fn save_local_record(&self, mut clip: Clip, device: &str) -> Result<Clip> {
        let previous = self.get(&clip.id)?;
        if let Some(previous) = &previous {
            let observed = previous.clock.get(device).copied().unwrap_or(0);
            let counter = clip.clock.entry(device.into()).or_default();
            *counter = (*counter).max(observed);
        }
        let counter = clip.clock.entry(device.into()).or_default();
        *counter = counter
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Version counter exhausted".into()))?;
        clip.version_id = new_id();
        clip.write_order = self
            .value("lamport")?
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            .max(clip.write_order)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Write order exhausted".into()))?;
        clip.write_actor = device.into();
        clip.device_id = device.into();
        clip.updated_at = now_ts();
        validate(&clip)?;
        let clip = self.merge(&clip)?;
        self.conn.execute("INSERT INTO outbox VALUES(?1,?2,?3) ON CONFLICT(clip_id) DO UPDATE SET operation=excluded.operation,body=excluded.body",params![clip.id,new_id(),self.pack(&clip)?])?;
        Ok(clip)
    }
    pub fn capture(&self, clip: Clip, device: &str) -> Result<Clip> {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT body FROM clips WHERE hash=?1 AND deleted=0 LIMIT 1",
                [clip.content_hash()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(raw) = existing {
            return self.load(&raw);
        }
        self.save_local(clip, device)
    }
    /// Authorize recalling a file list observed through this machine's OS clipboard.
    /// This local capability is never replicated or included in backups.
    pub fn authorize_local_files(&self, clip: &Clip) -> Result<()> {
        if clip.formats.iter().any(ClipFormat::is_file_list) {
            self.set_value(&format!("local.files.{}", clip.id), &clip.content_hash())?;
        }
        Ok(())
    }
    pub fn can_copy_local_files(&self, clip: &Clip) -> Result<bool> {
        Ok(self.value(&format!("local.files.{}", clip.id))?.as_deref()
            == Some(clip.content_hash().as_str()))
    }
    pub fn mutate(&self, id: &str, device: &str, apply: impl FnOnce(&mut Clip)) -> Result<Clip> {
        let tx = write_transaction(&self.conn)?;
        let mut clip = self
            .get(id)?
            .ok_or_else(|| Error::Invalid("Clip no longer exists".into()))?;
        apply(&mut clip);
        let clip = self.save_local_record(clip, device)?;
        tx.commit()?;
        Ok(clip)
    }
    /// Merge a remote record. Concurrent content is retained for explicit recovery.
    fn merge(&self, remote: &Clip) -> Result<Clip> {
        validate(remote)?;
        let observed = self
            .value("lamport")?
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if remote.write_order > observed {
            self.set_value("lamport", &remote.write_order.to_string())?;
        }
        let Some(local) = self.get(&remote.id)? else {
            self.write(remote)?;
            return Ok(remote.clone());
        };
        if local == *remote {
            return Ok(local);
        }
        let remote_newer = dominates(&remote.clock, &local.clock);
        let local_newer = dominates(&local.clock, &remote.clock);
        if local_newer && !remote_newer {
            return Ok(local);
        }
        if remote_newer && !local_newer {
            self.write(remote)?;
            return Ok(remote.clone());
        }
        // Stable choice makes every replica converge; both payloads remain recoverable.
        let l = resolution_key(&local)?;
        let r = resolution_key(remote)?;
        let (mut winner, loser) = if l >= r {
            (local, remote.clone())
        } else {
            (remote.clone(), local)
        };
        let raw = self.pack(&loser)?;
        let key = hex::encode(Sha256::digest(raw.as_bytes()));
        self.conn.execute(
            "INSERT OR IGNORE INTO conflicts VALUES(?1,?2,?3)",
            params![key, loser.id, raw],
        )?;
        merge_clock(&mut winner.clock, &loser.clock);
        self.write(&winner)?;
        Ok(winner)
    }
    pub fn receive(&self, clips: &[Clip]) -> Result<()> {
        let tx = write_transaction(&self.conn)?;
        for clip in clips {
            self.merge(clip)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn accept(&self, mutation: &Mutation) -> Result<Ack> {
        let tx = write_transaction(&self.conn)?;
        let fingerprint = hex::encode(Sha256::digest(serde_json::to_vec(mutation)?));
        let previous: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT fingerprint,ack FROM operations WHERE id=?1",
                [&mutation.operation],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((expected, ack)) = previous {
            if expected != fingerprint {
                return Err(Error::Invalid(
                    "Operation ID reused with different data".into(),
                ));
            }
            let mut ack: Ack = serde_json::from_str(&ack)?;
            ack.clip = self.expand(ack.clip)?;
            return Ok(ack);
        }
        let ack = Ack {
            operation: mutation.operation.clone(),
            clip: self.merge(&mutation.clip)?,
        };
        self.conn.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                mutation.operation,
                fingerprint,
                serde_json::to_string(&Ack {
                    operation: ack.operation.clone(),
                    clip: serde_json::from_str(&self.pack(&ack.clip)?)?
                })?
            ],
        )?;
        tx.commit()?;
        Ok(ack)
    }
    pub fn pending(&self) -> Result<Vec<Mutation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT operation,body FROM outbox ORDER BY rowid LIMIT 50")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.map(|row| {
            let (operation, body) = row?;
            Ok(Mutation {
                operation,
                clip: self.load(&body)?,
            })
        })
        .collect()
    }
    pub fn acknowledge(&self, ack: &Ack) -> Result<()> {
        let tx = write_transaction(&self.conn)?;
        self.merge(&ack.clip)?;
        self.conn
            .execute("DELETE FROM outbox WHERE operation=?1", [&ack.operation])?;
        tx.commit()?;
        Ok(())
    }
    pub fn page(&self, since: i64, limit: u32) -> Result<Page> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq,body FROM changes WHERE seq>?1 ORDER BY seq LIMIT ?2")?;
        let rows = stmt.query_map(params![since, limit.clamp(1, 100)], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut page = Page {
            cursor: since,
            epoch: self.value("epoch")?.unwrap_or_default(),
            ..Page::default()
        };
        let mut bytes = 1024;
        for row in rows {
            let (seq, raw) = row?;
            let clip: Clip = self.load(&raw)?;
            let size = clip.to_proto().encoded_len() + 32;
            if bytes + size > MAX_WIRE_BYTES - 1024 {
                break;
            }
            bytes += size;
            page.cursor = seq;
            page.clips.push(clip);
        }
        page.more = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM changes WHERE seq>?1)",
            [page.cursor],
            |r| r.get(0),
        )?;
        Ok(page)
    }
    pub fn apply_page(&self, key: &str, page: &Page) -> Result<()> {
        let tx = write_transaction(&self.conn)?;
        for clip in &page.clips {
            self.merge(clip)?;
        }
        self.set_value(key, &format!("{}:{}", page.epoch, page.cursor))?;
        tx.commit()?;
        Ok(())
    }
    pub fn cursor(&self, key: &str, epoch: &str) -> Result<i64> {
        Ok(self
            .value(key)?
            .and_then(|v| {
                v.strip_prefix(&format!("{epoch}:"))
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(0))
    }
    pub fn value(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn set_value(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO metadata VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
        Ok(())
    }
    pub fn preferences(&self) -> Result<Preferences> {
        self.value("preferences")?
            .map(|s| serde_json::from_str(&s).map_err(Error::from))
            .unwrap_or_else(|| Ok(Preferences::default()))
    }
    pub fn save_preferences(&self, prefs: &Preferences) -> Result<()> {
        prefs.cleanup.validate()?;
        self.set_value("preferences", &serde_json::to_string(prefs)?)
    }
    pub fn peers(&self) -> Result<Vec<Peer>> {
        let mut stmt = self.conn.prepare("SELECT body FROM peers ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn trust(&self, peer: &Peer) -> Result<()> {
        self.conn.execute(
            "INSERT INTO peers VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
            params![peer.id, serde_json::to_string(peer)?],
        )?;
        Ok(())
    }
    pub fn untrust(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM peers WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn stats(&self) -> Result<Stats> {
        Ok(Stats {
            clips: self
                .conn
                .query_row("SELECT count(*) FROM clips WHERE deleted=0", [], |r| {
                    r.get(0)
                })?,
            pinned: self.conn.query_row(
                "SELECT count(*) FROM clips WHERE pinned=1 AND deleted=0",
                [],
                |r| r.get(0),
            )?,
            deleted: self.conn.query_row(
                "SELECT count(*) FROM clips WHERE deleted=1",
                [],
                |r| r.get(0),
            )?,
            pending: self
                .conn
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?,
            conflicts: self
                .conn
                .query_row("SELECT count(*) FROM conflicts", [], |r| r.get(0))?,
        })
    }
    pub fn conflicts(&self) -> Result<Vec<(String, Clip)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,body FROM conflicts ORDER BY rowid DESC LIMIT 100")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.map(|r| {
            let (id, body) = r?;
            Ok((id, self.load(&body)?))
        })
        .collect()
    }
    pub fn restore_conflict(&self, conflict: &str, device: &str) -> Result<()> {
        let tx = write_transaction(&self.conn)?;
        let raw: String =
            self.conn
                .query_row("SELECT body FROM conflicts WHERE id=?1", [conflict], |r| {
                    r.get(0)
                })?;
        let mut clip = self.load(&raw)?;
        if let Some(current) = self.get(&clip.id)? {
            merge_clock(&mut clip.clock, &current.clock);
        }
        clip.deleted = false;
        self.save_local_record(clip, device)?;
        self.conn
            .execute("DELETE FROM conflicts WHERE id=?1", [conflict])?;
        tx.commit()?;
        Ok(())
    }
    pub fn export(&self) -> Result<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT body FROM clips WHERE deleted=0 ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let clips: Result<Vec<Clip>> = rows.map(|r| self.load(&r?)).collect();
        Ok(serde_json::to_string_pretty(&Backup {
            version: 1,
            clips: clips?,
            views: self.views()?,
        })?)
    }
    pub fn import(&self, json: &str, device: &str) -> Result<usize> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Backup(Backup),
            Clips(Vec<Clip>),
        }
        let (clips, views) = match serde_json::from_str::<Input>(json)? {
            Input::Backup(backup) => {
                if backup.version != 1 {
                    return Err(Error::Invalid("Unsupported backup version".into()));
                }
                (backup.clips, backup.views)
            }
            Input::Clips(clips) => (clips, Vec::new()),
        };
        for clip in &clips {
            validate(clip)?;
            if !clip.blob_refs.is_empty() {
                return Err(Error::Invalid("Backup contains unresolved blobs".into()));
            }
        }
        let count = clips.len();
        let tx = write_transaction(&self.conn)?;
        for mut clip in clips {
            clip.id = new_id();
            clip.clock.clear();
            self.save_local_record(clip, device)?;
        }
        for view in views {
            self.save_view(view)?;
        }
        tx.commit()?;
        Ok(count)
    }
}
fn dominates(a: &Clock, b: &Clock) -> bool {
    b.iter()
        .all(|(key, value)| a.get(key).copied().unwrap_or(0) >= *value)
}
fn merge_clock(a: &mut Clock, b: &Clock) {
    for (key, value) in b {
        let entry = a.entry(key.clone()).or_default();
        *entry = (*entry).max(*value);
    }
}
fn validate(clip: &Clip) -> Result<()> {
    if clip.id.is_empty()
        || clip.id.len() > 128
        || clip.clock.len() > 256
        || clip.to_proto().encoded_len() > MAX_WIRE_BYTES / 2
    {
        return Err(Error::Invalid(
            "Invalid or oversized clipboard record".into(),
        ));
    }
    Ok(())
}

pub mod wire;
pub use wire::{Wire, decode, encode};
pub use yank_proto::v1 as protocol;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub id: String,
    pub name: String,
    pub query: String,
    pub filter: String,
    pub days: u32,
    pub icon: String,
}
impl Store {
    pub fn views(&self) -> Result<Vec<SavedView>> {
        self.value("saved_views")?
            .map(|s| serde_json::from_str(&s).map_err(Error::from))
            .unwrap_or_else(|| Ok(Vec::new()))
    }
    pub fn save_view(&self, view: SavedView) -> Result<()> {
        if view.id.is_empty()
            || view.id.len() > 128
            || view.name.trim().is_empty()
            || view.name.len() > 200
            || view.query.len() > 4096
        {
            return Err(Error::Invalid("Invalid smart list".into()));
        }
        let mut views = self.views()?;
        views.retain(|v| v.id != view.id);
        views.push(view);
        if views.len() > 100 {
            return Err(Error::Invalid("At most 100 smart lists".into()));
        }
        self.set_value("saved_views", &serde_json::to_string(&views)?)
    }
    pub fn remove_view(&self, id: &str) -> Result<()> {
        let mut views = self.views()?;
        views.retain(|v| v.id != id);
        self.set_value("saved_views", &serde_json::to_string(&views)?)
    }
    pub fn collections(&self) -> Result<Vec<String>> {
        let mut stmt=self.conn.prepare("SELECT DISTINCT collection FROM clips WHERE deleted=0 AND collection!='' ORDER BY collection")?;
        Ok(stmt
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?)
    }
    /// Select victims and append their tombstones/outbox mutations under one write lock.
    /// A bounded batch keeps capture and incoming synchronization responsive.
    pub fn cleanup(&self, policy: &CleanupPolicy, days: u32, device: &str) -> Result<usize> {
        policy.validate()?;
        if !policy.enabled {
            return Ok(0);
        }
        self.cleanup_batch(Some(policy), days, device)
    }

    /// Legacy age-only maintenance, retained for callers that explicitly request expiry.
    pub fn expire(&self, days: u32, device: &str) -> Result<usize> {
        if days == 0 {
            return Ok(0);
        }
        self.cleanup_batch(None, days, device)
    }

    fn cleanup_batch(
        &self,
        policy: Option<&CleanupPolicy>,
        days: u32,
        device: &str,
    ) -> Result<usize> {
        let tx = write_transaction(&self.conn)?;
        let mut ids = std::collections::BTreeSet::new();
        if days > 0 {
            let cutoff = now_ts().saturating_sub(i64::from(days) * 86400);
            let mut stmt = self.conn.prepare(
                "SELECT id FROM clips WHERE deleted=0 AND pinned=0 AND updated<?1 ORDER BY updated,id LIMIT 200",
            )?;
            ids.extend(
                stmt.query_map([cutoff], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            );
        }
        if let Some(policy) = policy {
            match policy.mode {
                CleanupMode::Total => {
                    let mut stmt = self.conn.prepare(
                        "SELECT id FROM clips WHERE deleted=0 AND pinned=0 ORDER BY updated DESC,id DESC LIMIT 200 OFFSET ?1",
                    )?;
                    for id in stmt.query_map([policy.max_items], |row| row.get::<_, String>(0))? {
                        if ids.len() == 200 {
                            break;
                        }
                        ids.insert(id?);
                    }
                }
                CleanupMode::PerType => {
                    let mut stmt = self.conn.prepare(
                        "SELECT id FROM clips WHERE deleted=0 AND pinned=0 AND kind=?1 ORDER BY updated DESC,id DESC LIMIT 200 OFFSET ?2",
                    )?;
                    for (kind, count) in &policy.per_type {
                        for id in
                            stmt.query_map(params![kind, count], |row| row.get::<_, String>(0))?
                        {
                            if ids.len() == 200 {
                                break;
                            }
                            ids.insert(id?);
                        }
                        if ids.len() == 200 {
                            break;
                        }
                    }
                }
            }
        }
        for id in &ids {
            let mut clip = self
                .get(id)?
                .ok_or_else(|| Error::Invalid("Cleanup candidate disappeared".into()))?;
            clip.deleted = true;
            self.save_local_record(clip, device)?;
        }
        tx.commit()?;
        Ok(ids.len())
    }
}
fn is_color(text: &str) -> bool {
    let s = text.trim();
    s.strip_prefix('#').is_some_and(|v| {
        [3, 4, 6, 8].contains(&v.len()) && v.chars().all(|c| c.is_ascii_hexdigit())
    })
}
/// Sensitive OS clipboard formats are always excluded before persistence or sync.
pub fn confidential_formats(formats: &[String]) -> bool {
    formats.iter().any(|format| {
        [
            "org.nspasteboard.ConcealedType",
            "org.nspasteboard.TransientType",
            "org.nspasteboard.AutoGeneratedType",
            "ExcludeClipboardContentFromMonitorProcessing",
            "Password",
            "application/x-keepassxc-password",
        ]
        .iter()
        .any(|s| format.eq_ignore_ascii_case(s))
    })
}

impl Preferences {
    pub fn detach_account(&mut self) {
        self.token.clear();
        self.account_id.clear();
        self.username.clear();
        self.upload = false;
    }
}

fn resolution_key(clip: &Clip) -> Result<(u64, String, String, String)> {
    let mut canonical = clip.clone();
    canonical.clock.clear();
    canonical.blob_refs.clear();
    Ok((
        canonical.write_order,
        canonical.write_actor.clone(),
        canonical.version_id.clone(),
        hex::encode(Sha256::digest(encode(&canonical)?)),
    ))
}

#[derive(Serialize, Deserialize)]
struct Backup {
    version: u32,
    clips: Vec<Clip>,
    #[serde(default)]
    views: Vec<SavedView>,
}

fn write_transaction(conn: &Connection) -> Result<rusqlite::Transaction<'_>> {
    Ok(rusqlite::Transaction::new_unchecked(
        conn,
        rusqlite::TransactionBehavior::Immediate,
    )?)
}
