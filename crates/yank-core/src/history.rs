use crate::{Clip, Result, Store, now_ts, write_transaction};
use chrono::{Datelike, Local, TimeZone};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRange {
    Today,
    Day1,
    Day2,
    #[default]
    Day3,
    Day4,
    Day5,
    Week,
    Month,
    All,
}

impl HistoryRange {
    pub fn cutoff(self) -> i64 {
        let now = Local::now();
        let date = now.date_naive();
        let boundary = match self {
            Self::All => return i64::MIN,
            Self::Day1 => return now_ts().saturating_sub(86400),
            Self::Day2 => return now_ts().saturating_sub(2 * 86400),
            Self::Day3 => return now_ts().saturating_sub(3 * 86400),
            Self::Day4 => return now_ts().saturating_sub(4 * 86400),
            Self::Day5 => return now_ts().saturating_sub(5 * 86400),
            Self::Today => date,
            Self::Week => {
                date - chrono::Days::new(u64::from(date.weekday().num_days_from_monday()))
            }
            Self::Month => date.with_day(1).unwrap_or(date),
        };
        Local
            .from_local_datetime(&boundary.and_hms_opt(0, 0, 0).expect("midnight"))
            .earliest()
            .map_or_else(
                || now.timestamp().saturating_sub(86400),
                |time| time.timestamp(),
            )
    }
}

#[derive(Clone, Debug)]
pub struct HistoryCursor {
    pub pinned: bool,
    pub recency: i64,
    pub id: String,
}

pub struct HistoryPage {
    pub clips: Vec<Clip>,
    pub cursor: Option<HistoryCursor>,
}

pub struct HistoryWindow {
    pub clips: Vec<Clip>,
    pub has_more: bool,
    pub has_earlier: bool,
}

pub(crate) fn init(connection: &Connection) -> Result<()> {
    let already_migrated = || {
        connection
            .query_row(
                "SELECT value FROM metadata WHERE key='history_schema'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|version| version.as_deref() == Some("2"))
    };
    if already_migrated()? {
        return Ok(());
    }
    let transaction = write_transaction(connection)?;
    // A concurrent opener may have rebuilt the indexes while we waited for the write lock.
    if already_migrated()? {
        return Ok(());
    }
    let columns = connection
        .prepare("PRAGMA table_info(clips)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "recency") {
        connection.execute_batch("ALTER TABLE clips ADD COLUMN recency INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE clips ADD COLUMN activity_at INTEGER NOT NULL DEFAULT 0;
            WITH ranked AS (SELECT id,ROW_NUMBER() OVER (ORDER BY updated,id) AS rank FROM clips)
            UPDATE clips SET recency=(SELECT rank FROM ranked WHERE ranked.id=clips.id),activity_at=updated;")?;
    }
    connection.execute_batch("INSERT OR IGNORE INTO metadata(key,value) SELECT 'history_clock',CAST(COALESCE(MAX(recency),0) AS TEXT) FROM clips;
        CREATE INDEX IF NOT EXISTS clips_history ON clips(deleted,pinned DESC,recency DESC,id DESC);
        CREATE INDEX IF NOT EXISTS clips_history_kind ON clips(deleted,kind,pinned DESC,recency DESC,id DESC);
        CREATE INDEX IF NOT EXISTS clips_history_time ON clips(deleted,activity_at);
        CREATE INDEX IF NOT EXISTS clips_history_board ON clips(collection,pinned DESC,recency DESC,id DESC)
            WHERE deleted=0 AND collection!='';
        DROP INDEX IF EXISTS clips_search;
        CREATE INDEX clips_search ON clips(deleted,pinned DESC,updated DESC,id DESC);")?;
    connection.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('history_schema','2')",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

// The list and the "load earlier" affordance must resolve reserved filter names identically.
fn history_filter(filter: &str) -> (&'static str, &'static str) {
    match filter {
        "all" => ("", "clips_history"),
        "pinned" => (" AND pinned=1", "clips_history"),
        kind if crate::CLIP_KINDS.contains(&kind) => (" AND kind=?2", "clips_history_kind"),
        "" => (" AND collection=?2", "clips_history"),
        _ => (
            " AND collection!='' AND collection=?2",
            "clips_history_board",
        ),
    }
}

pub(super) fn search_clause(query: &str) -> &'static str {
    if query.is_empty() {
        ""
    } else {
        " AND instr(lower(search),lower(?1))>0"
    }
}

impl Store {
    pub fn history_window(
        &self,
        query: &str,
        filter: &str,
        cutoff: i64,
        limit: u32,
    ) -> Result<HistoryWindow> {
        let transaction = self.conn.unchecked_transaction()?;
        let mut clips = Vec::new();
        let mut cursor = None;
        let target = limit.saturating_add(1);
        while clips.len() < target as usize {
            let requested = (target - clips.len() as u32).min(100);
            let page = self.history_page(query, filter, cutoff, cursor.as_ref(), requested)?;
            let exhausted = page.clips.len() < requested as usize;
            cursor = page.cursor;
            clips.extend(page.clips);
            if exhausted {
                break;
            }
        }
        let has_more = clips.len() > limit as usize;
        clips.truncate(limit as usize);
        let has_earlier = cutoff != i64::MIN && self.history_has_earlier(query, filter, cutoff)?;
        transaction.commit()?;
        Ok(HistoryWindow {
            clips,
            has_more,
            has_earlier,
        })
    }

    pub(crate) fn next_history_rank(&self) -> Result<i64> {
        Ok(self.conn.query_row("UPDATE metadata SET value=CAST(value AS INTEGER)+1 WHERE key='history_clock' RETURNING CAST(value AS INTEGER)", [], |row| row.get(0))?)
    }

    pub fn mark_used(&self, id: &str) -> Result<bool> {
        let transaction = write_transaction(&self.conn)?;
        let live = self
            .conn
            .query_row(
                "SELECT 1 FROM clips WHERE id=?1 AND deleted=0",
                [id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if live {
            let rank = self.next_history_rank()?;
            self.conn.execute("UPDATE clips SET recency=?2,activity_at=MAX(activity_at,?3) WHERE id=?1 AND deleted=0", params![id,rank,now_ts()])?;
        }
        transaction.commit()?;
        Ok(live)
    }

    pub fn history_page(
        &self,
        query: &str,
        filter: &str,
        cutoff: i64,
        cursor: Option<&HistoryCursor>,
        limit: u32,
    ) -> Result<HistoryPage> {
        let (constraint, index) = history_filter(filter);
        let search = search_clause(query);
        let seek = if cursor.is_some() {
            " AND (pinned,recency,id)<(?4,?5,?6)"
        } else {
            ""
        };
        let sql = format!(
            "SELECT body,pinned,recency,id FROM clips INDEXED BY {index} WHERE deleted=0 AND activity_at>=?3{search}{constraint}{seek} ORDER BY pinned DESC,recency DESC,id DESC LIMIT ?7"
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let rows = statement.query_map(
            params![
                query,
                filter,
                cutoff,
                cursor.map(|cursor| cursor.pinned),
                cursor.map(|cursor| cursor.recency),
                cursor.map(|cursor| cursor.id.as_str()),
                limit.clamp(1, 100)
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    HistoryCursor {
                        pinned: row.get(1)?,
                        recency: row.get(2)?,
                        id: row.get(3)?,
                    },
                ))
            },
        )?;
        let mut clips = Vec::new();
        let mut cursor = None;
        for row in rows {
            let (body, next) = row?;
            clips.push(serde_json::from_str(&body)?);
            cursor = Some(next);
        }
        Ok(HistoryPage { clips, cursor })
    }

    pub fn history_has_earlier(&self, query: &str, filter: &str, cutoff: i64) -> Result<bool> {
        let (constraint, _) = history_filter(filter);
        let search = search_clause(query);
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM clips WHERE deleted=0 AND activity_at<?3{constraint}{search})"
        );
        Ok(self
            .conn
            .prepare_cached(&sql)?
            .query_row(params![query, filter, cutoff], |row| row.get(0))?)
    }
}
