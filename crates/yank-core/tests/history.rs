use yank_core::{Clip, CloseBehavior, Preferences, Store, WindowStyle, history::HistoryRange};

#[test]
fn old_preferences_default_to_recent_history_and_first_close_prompt() {
    let preferences: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(preferences.history_range, HistoryRange::Day3);
    assert_eq!(preferences.close_behavior, CloseBehavior::Ask);
    assert_eq!(preferences.window_style, WindowStyle::System);
    assert!(!preferences.launch_at_login);
}

#[test]
fn desktop_and_history_preferences_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite3");
    let preferences = Preferences {
        close_behavior: CloseBehavior::Minimize,
        window_style: WindowStyle::TrafficLights,
        history_range: HistoryRange::Month,
        launch_at_login: true,
        ..Default::default()
    };
    Store::open(&path)
        .unwrap()
        .save_preferences(&preferences)
        .unwrap();
    assert_eq!(
        Store::open(&path).unwrap().preferences().unwrap(),
        preferences
    );
}

#[test]
fn reuse_promotes_locally_without_sync_mutations_or_content_changes() {
    let store = Store::memory().unwrap();
    let original = store
        .capture(Clip::text("device", "first"), "device")
        .unwrap();
    let newer = store
        .capture(Clip::text("device", "second"), "device")
        .unwrap();
    let before = store.stats().unwrap();
    let page_before = store.page(0, 100).unwrap();
    assert_eq!(
        store
            .history_page("", "all", i64::MIN, None, 100)
            .unwrap()
            .clips[0]
            .id,
        newer.id
    );
    assert!(store.mark_used(&original.id).unwrap());
    assert_eq!(
        store
            .history_page("", "all", i64::MIN, None, 100)
            .unwrap()
            .clips[0]
            .id,
        original.id
    );
    assert_eq!(store.get(&original.id).unwrap().unwrap(), original);
    assert_eq!(store.stats().unwrap().pending, before.pending);
    assert_eq!(store.page(0, 100).unwrap().cursor, page_before.cursor);
    assert!(!store.mark_used("missing").unwrap());
    store
        .mutate(&original.id, "device", |clip| clip.deleted = true)
        .unwrap();
    assert!(!store.mark_used(&original.id).unwrap());
}

#[test]
fn window_reads_a_bounded_slice_and_detects_more_without_returning_sentinel() {
    let store = Store::memory().unwrap();
    for index in 0..205 {
        store
            .capture(Clip::text("device", &format!("record {index}")), "device")
            .unwrap();
    }
    let first = store.history_window("", "all", i64::MIN, 100).unwrap();
    assert_eq!(first.clips.len(), 100);
    assert!(first.has_more);
    assert!(!first.has_earlier);
    let expanded = store.history_window("", "all", i64::MIN, 300).unwrap();
    assert_eq!(expanded.clips.len(), 205);
    assert!(!expanded.has_more);
    assert_eq!(first.clips, expanded.clips[..100]);
}

#[test]
fn keyset_pages_preserve_pins_types_and_stable_order() {
    let store = Store::memory().unwrap();
    for index in 0..215 {
        let mut clip = Clip::text("device", &format!("record {index}"));
        clip.pinned = index % 11 == 0;
        store.capture(clip, "device").unwrap();
    }
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = store
            .history_page("", "text", i64::MIN, cursor.as_ref(), 37)
            .unwrap();
        if page.clips.is_empty() {
            break;
        }
        cursor = page.cursor;
        records.extend(page.clips);
    }
    assert_eq!(records.len(), 215);
    let unique: std::collections::BTreeSet<_> = records.iter().map(|clip| &clip.id).collect();
    assert_eq!(unique.len(), 215);
    assert!(
        records
            .windows(2)
            .all(|pair| pair[0].pinned || !pair[1].pinned)
    );
    assert!(
        store
            .history_page("record 21", "all", i64::MIN, None, 100)
            .unwrap()
            .clips
            .iter()
            .all(|clip| clip.text.contains("record 21"))
    );
    assert!(
        store
            .history_page("", "image", i64::MIN, None, 100)
            .unwrap()
            .clips
            .is_empty()
    );
}

#[test]
fn recent_range_can_expand_without_deleting_old_history() {
    let store = Store::memory().unwrap();
    let mut old = Clip::text("device", "old note");
    old.updated_at = 1;
    old.created_at = 1;
    old.clock.insert("device".into(), 1);
    old.write_order = 1;
    store.receive(std::slice::from_ref(&old)).unwrap();
    let recent = store
        .capture(Clip::text("device", "recent note"), "device")
        .unwrap();
    let cutoff = HistoryRange::Day3.cutoff();
    let page = store.history_page("", "all", cutoff, None, 100).unwrap();
    assert_eq!(
        page.clips.iter().map(|clip| &clip.id).collect::<Vec<_>>(),
        vec![&recent.id]
    );
    assert!(store.history_has_earlier("", "all", cutoff).unwrap());
    assert_eq!(
        store
            .history_page("", "all", i64::MIN, None, 100)
            .unwrap()
            .clips
            .len(),
        2
    );
    assert!(store.mark_used(&old.id).unwrap());
    assert_eq!(
        store
            .history_page("", "all", cutoff, None, 100)
            .unwrap()
            .clips[0]
            .id,
        old.id
    );
}

#[test]
fn upgrading_an_old_database_backfills_history_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE clips(id TEXT PRIMARY KEY,body TEXT NOT NULL,deleted INTEGER NOT NULL,pinned INTEGER NOT NULL,kind TEXT NOT NULL,collection TEXT NOT NULL,search TEXT NOT NULL,updated INTEGER NOT NULL,hash TEXT NOT NULL)").unwrap();
    let clip = Clip::text("device", "legacy note");
    connection
        .execute(
            "INSERT INTO clips VALUES(?1,?2,0,0,'text','','legacy note',?3,'legacy')",
            rusqlite::params![
                clip.id,
                serde_json::to_string(&clip).unwrap(),
                clip.updated_at
            ],
        )
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .history_page("", "all", i64::MIN, None, 100)
            .unwrap()
            .clips[0]
            .id,
        clip.id
    );
    assert!(store.mark_used(&clip.id).unwrap());
    drop(store);
    assert_eq!(
        Store::open(&path)
            .unwrap()
            .history_page("", "all", i64::MIN, None, 100)
            .unwrap()
            .clips
            .len(),
        1
    );
}
