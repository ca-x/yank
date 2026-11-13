use std::collections::BTreeMap;
use yank_core::{CleanupMode, CleanupPolicy, Clip, ClipFormat, Store};

fn policy(count: u32) -> CleanupPolicy {
    CleanupPolicy {
        enabled: true,
        max_items: count,
        ..CleanupPolicy::default()
    }
}

fn seeded(db: &Store, id: &str, kind: &str, updated: i64, pinned: bool) -> Clip {
    let mut clip = Clip::text("origin", id);
    clip.id = id.into();
    clip.kind = kind.into();
    clip.updated_at = updated;
    clip.created_at = updated;
    clip.pinned = pinned;
    clip.clock.insert("origin".into(), 1);
    clip.write_order = 1;
    db.receive(std::slice::from_ref(&clip)).unwrap();
    clip
}

#[test]
fn disabled_cleanup_preserves_records_even_with_age_limit() {
    let db = Store::memory().unwrap();
    seeded(&db, "old", "text", 1, false);
    assert_eq!(db.cleanup(&CleanupPolicy::default(), 1, "a").unwrap(), 0);
    assert_eq!(db.stats().unwrap().clips, 1);
    assert_eq!(db.stats().unwrap().pending, 0);
}

#[test]
fn total_limit_is_stable_and_excludes_favorites() {
    let db = Store::memory().unwrap();
    seeded(&db, "a", "text", 100, false);
    seeded(&db, "b", "image", 100, false);
    seeded(&db, "c", "text", 100, false);
    seeded(&db, "favorite", "image", 1, true);
    assert_eq!(db.cleanup(&policy(2), 0, "a").unwrap(), 1);
    assert!(db.get("a").unwrap().unwrap().deleted);
    assert!(!db.get("b").unwrap().unwrap().deleted);
    assert!(!db.get("c").unwrap().unwrap().deleted);
    assert!(!db.get("favorite").unwrap().unwrap().deleted);
    assert_eq!(db.cleanup(&policy(2), 0, "a").unwrap(), 0);
    assert_eq!(db.stats().unwrap().pending, 1);
}

#[test]
fn type_limits_are_independent_and_age_does_not_remove_pins() {
    let db = Store::memory().unwrap();
    for kind in yank_core::CLIP_KINDS {
        seeded(&db, &format!("{kind}-old"), kind, 10, false);
        seeded(
            &db,
            &format!("{kind}-new"),
            kind,
            yank_core::now_ts(),
            false,
        );
    }
    seeded(&db, "unknown", "future-type", 10, false);
    seeded(&db, "favorite", "text", 1, true);
    let mut p = policy(100);
    p.mode = CleanupMode::PerType;
    p.per_type = yank_core::CLIP_KINDS
        .into_iter()
        .map(|k| (k.into(), 1))
        .collect();
    assert_eq!(db.cleanup(&p, 0, "a").unwrap(), 6);
    assert!(!db.get("unknown").unwrap().unwrap().deleted);
    assert_eq!(db.cleanup(&p, 1, "a").unwrap(), 1);
    assert!(!db.get("favorite").unwrap().unwrap().deleted);
    assert_eq!(db.stats().unwrap().clips, 7);
}

#[test]
fn batches_are_bounded_and_repeat_until_the_limit_is_reached() {
    let db = Store::memory().unwrap();
    for n in 0..405 {
        seeded(&db, &format!("{n:04}"), "text", n, false);
    }
    assert_eq!(db.cleanup(&policy(1), 0, "a").unwrap(), 200);
    assert_eq!(db.cleanup(&policy(1), 0, "a").unwrap(), 200);
    assert_eq!(db.cleanup(&policy(1), 0, "a").unwrap(), 4);
    assert_eq!(db.cleanup(&policy(1), 0, "a").unwrap(), 0);
    assert_eq!(db.stats().unwrap().clips, 1);
    assert!(!db.get("0404").unwrap().unwrap().deleted);
}

#[test]
fn cleanup_rolls_back_all_tombstones_changes_and_outbox_on_failure() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("atomic.sqlite3");
    let db = Store::open(&path).unwrap();
    for id in ["a", "b", "c"] {
        seeded(&db, id, "text", 1, false);
    }
    let before = db.page(0, 100).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_cleanup BEFORE UPDATE ON clips WHEN NEW.id='b' AND NEW.deleted=1 BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(db.cleanup(&policy(1), 0, "a").is_err());
    assert_eq!(db.stats().unwrap().clips, 3);
    assert_eq!(db.stats().unwrap().pending, 0);
    assert_eq!(db.page(0, 100).unwrap(), before);
    assert_eq!(db.value("lamport").unwrap().as_deref(), Some("1"));
}

#[test]
fn old_preferences_default_to_disabled_and_invalid_limits_are_rejected() {
    let db = Store::memory().unwrap();
    db.set_value("preferences", r#"{"language":"en","retention_days":30}"#)
        .unwrap();
    let mut prefs = db.preferences().unwrap();
    assert!(!prefs.cleanup.enabled);
    assert_eq!(prefs.retention_days, 30);
    prefs.cleanup.max_items = 0;
    assert!(db.save_preferences(&prefs).is_err());
    prefs.cleanup = policy(3);
    prefs.cleanup.mode = CleanupMode::PerType;
    prefs.cleanup.per_type = BTreeMap::new();
    assert!(db.save_preferences(&prefs).is_err());
    prefs.cleanup = policy(3);
    prefs.cleanup.per_type.insert("unknown".into(), 2);
    assert!(db.save_preferences(&prefs).is_err());
    assert!(!db.preferences().unwrap().cleanup.enabled);
}

fn upload(client: &Store, server: &Store) {
    loop {
        let pending = client.pending().unwrap();
        if pending.is_empty() {
            break;
        }
        for mutation in pending {
            let wire = yank_core::encode(&mutation).unwrap();
            let received = yank_core::decode(&wire).unwrap();
            let ack = server.accept(&received).unwrap();
            client.acknowledge(&ack).unwrap();
            // A lost response can always be retried with the same operation ID.
            assert_eq!(server.accept(&received).unwrap(), ack);
        }
    }
}
fn pull(client: &Store, source: &Store, key: &str) {
    let mut since = 0;
    loop {
        let page = source.page(since, 7).unwrap();
        since = page.cursor;
        client.apply_page(key, &page).unwrap();
        if !page.more {
            break;
        }
    }
}

#[test]
fn offline_replay_and_old_upload_ack_cannot_undo_cleanup() {
    let a = Store::memory().unwrap();
    let b = Store::memory().unwrap();
    let server = Store::memory().unwrap();
    let mut original = Clip::text("a", "old");
    original.id = "a".into();
    let original = a.save_local(original, "a").unwrap();
    let old_op = a.pending().unwrap().remove(0);
    let delayed_ack = server.accept(&old_op).unwrap();
    b.receive(std::slice::from_ref(&original)).unwrap();
    let mut recent = Clip::text("a", "new");
    recent.id = "z".into();
    a.save_local(recent, "a").unwrap();
    assert_eq!(a.cleanup(&policy(1), 0, "a").unwrap(), 1);
    let deleted = a.get("a").unwrap().unwrap();
    let pending_before = a.stats().unwrap().pending;
    a.acknowledge(&delayed_ack).unwrap();
    assert_eq!(
        a.stats().unwrap().pending,
        pending_before,
        "an old acknowledgement cannot clear the newer cleanup mutation"
    );
    upload(&a, &server);
    server.receive(std::slice::from_ref(&original)).unwrap();
    pull(&b, &server, "server");
    assert_eq!(b.get("a").unwrap().unwrap(), deleted);
    assert!(server.get("a").unwrap().unwrap().deleted);
    assert!(b.get("a").unwrap().unwrap().deleted);
}

#[test]
fn cleanup_and_concurrent_edit_converge_on_two_clients_and_server_and_remain_recoverable() {
    for reverse in [false, true] {
        let a = Store::memory().unwrap();
        let b = Store::memory().unwrap();
        let server = Store::memory().unwrap();
        let original = seeded(&a, "shared", "text", 1, false);
        b.receive(std::slice::from_ref(&original)).unwrap();
        assert_eq!(
            a.cleanup(&policy(100), 1, if reverse { "z" } else { "a" })
                .unwrap(),
            1
        );
        b.mutate("shared", "b", |c| {
            c.text = "offline edit".into();
            c.formats = vec![ClipFormat::text("offline edit")];
            c.pinned = true;
        })
        .unwrap();
        if reverse {
            upload(&b, &server);
            upload(&a, &server);
        } else {
            upload(&a, &server);
            upload(&b, &server);
        }
        pull(&a, &server, "server");
        pull(&b, &server, "server");
        let result = server.get("shared").unwrap().unwrap();
        assert_eq!(a.get("shared").unwrap().unwrap(), result);
        assert_eq!(b.get("shared").unwrap().unwrap(), result);
        let conflicts = server.conflicts().unwrap();
        assert!(!conflicts.is_empty());
        assert!(conflicts.iter().any(|(_, c)| c.deleted != result.deleted));
        // Explicit restoration observes all known clocks and becomes causally newer.
        let live_conflict = conflicts.iter().find(|(_, c)| !c.deleted);
        if let Some((id, _)) = live_conflict {
            server.restore_conflict(id, "server").unwrap();
            pull(&a, &server, "restore");
            pull(&b, &server, "restore");
            assert!(!a.get("shared").unwrap().unwrap().deleted);
            assert_eq!(a.get("shared").unwrap(), b.get("shared").unwrap());
        }
    }
}

#[test]
fn cleanup_waits_for_an_in_progress_pin_before_selecting_candidates() {
    use std::sync::mpsc;
    use std::time::Duration;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pin-race.sqlite3");
    let db = Store::open(&path).unwrap();
    seeded(&db, "old", "text", 1, false);
    seeded(&db, "new", "text", yank_core::now_ts(), false);
    let writer = Store::open(&path).unwrap();
    let cleaner = Store::open(&path).unwrap();
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let pin = std::thread::spawn(move || {
        writer
            .mutate("old", "a", |c| {
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                c.pinned = true;
            })
            .unwrap();
    });
    locked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let clean = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let count = cleaner.cleanup(&policy(1), 1, "a").unwrap();
        done_tx.send(count).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = done_rx.recv_timeout(Duration::from_millis(150));
    release_tx.send(()).unwrap();
    pin.join().unwrap();
    clean.join().unwrap();
    assert!(
        premature.is_err(),
        "cleanup must wait for the writer before reading candidates"
    );
    assert_eq!(done_rx.recv().unwrap(), 0);
    let old = db.get("old").unwrap().unwrap();
    assert!(old.pinned && !old.deleted);
}

#[test]
fn different_offline_limits_converge_after_reordered_duplicate_sync() {
    let seed = Store::memory().unwrap();
    let roots: Vec<_> = (1..=7)
        .map(|n| seeded(&seed, &format!("clip-{n}"), "text", n, false))
        .collect();
    let a = Store::memory().unwrap();
    let b = Store::memory().unwrap();
    let server = Store::memory().unwrap();
    a.receive(&roots[..5]).unwrap();
    b.receive(&roots[2..]).unwrap();
    server.receive(&roots).unwrap();
    assert_eq!(a.cleanup(&policy(3), 0, "a").unwrap(), 2);
    let mut p = policy(100);
    p.mode = CleanupMode::PerType;
    p.per_type.insert("text".into(), 2);
    assert_eq!(b.cleanup(&p, 0, "b").unwrap(), 3);
    upload(&b, &server);
    upload(&a, &server);
    // A full old snapshot replay is a realistic reconnect/import retry.
    server.receive(&roots).unwrap();
    pull(&a, &server, "server");
    pull(&b, &server, "server");
    let ids = |db: &Store| {
        let mut ids: Vec<_> = db
            .list("", "all", 100, 0)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&server), vec!["clip-6", "clip-7"]);
    assert_eq!(ids(&a), ids(&server));
    assert_eq!(ids(&b), ids(&server));
    for root in &roots {
        assert_eq!(a.get(&root.id).unwrap(), server.get(&root.id).unwrap());
        assert_eq!(b.get(&root.id).unwrap(), server.get(&root.id).unwrap());
    }
    assert_eq!(a.cleanup(&policy(3), 0, "a").unwrap(), 0);
    assert_eq!(b.cleanup(&p, 0, "b").unwrap(), 0);
}
