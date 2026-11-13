use yank_core::{Ack, Clip, ClipFormat, Mutation, Store, Wire, decode, encode};
#[test]
fn local_capture_is_atomic_and_keeps_duplicate_metadata() {
    let db = Store::memory().unwrap();
    let c = db.capture(Clip::text("a", "hello"), "a").unwrap();
    let c = db
        .mutate(&c.id, "a", |c| {
            c.pinned = true;
            c.collection = "work".into();
        })
        .unwrap();
    let duplicate = db.capture(Clip::text("a", "hello"), "a").unwrap();
    assert_eq!(duplicate.id, c.id);
    assert!(duplicate.pinned);
    assert_eq!(duplicate.collection, "work");
    assert_eq!(db.stats().unwrap().clips, 1);
    assert_eq!(db.pending().unwrap().len(), 1);
}
#[test]
fn replay_has_one_effect_and_rejects_reused_operation_id() {
    let local = Store::memory().unwrap();
    let server = Store::memory().unwrap();
    local.save_local(Clip::text("a", "first"), "a").unwrap();
    let m = local.pending().unwrap().remove(0);
    let first = server.accept(&m).unwrap();
    let second = server.accept(&m).unwrap();
    assert_eq!(first.clip, second.clip);
    assert_eq!(server.page(0, 100).unwrap().clips.len(), 1);
    let mut changed = m;
    changed.clip.text = "different".into();
    assert!(server.accept(&changed).is_err());
}
#[test]
fn old_ack_does_not_remove_or_overwrite_a_new_edit() {
    let db = Store::memory().unwrap();
    let server = Store::memory().unwrap();
    let c = db.save_local(Clip::text("a", "first"), "a").unwrap();
    let pending = db.pending().unwrap().remove(0);
    let ack = server.accept(&pending).unwrap();
    db.mutate(&c.id, "a", |c| c.text = "second".into()).unwrap();
    db.acknowledge(&ack).unwrap();
    assert_eq!(db.get(&c.id).unwrap().unwrap().text, "second");
    assert_eq!(db.pending().unwrap().len(), 1);
    assert_ne!(db.pending().unwrap()[0].operation, pending.operation);
}
#[test]
fn concurrent_edits_converge_without_losing_either_payload() {
    let a = Store::memory().unwrap();
    let b = Store::memory().unwrap();
    let root = a.save_local(Clip::text("a", "root"), "a").unwrap();
    b.receive(std::slice::from_ref(&root)).unwrap();
    let ca = a
        .mutate(&root.id, "a", |c| c.text = "edit a".into())
        .unwrap();
    let cb = b
        .mutate(&root.id, "b", |c| c.text = "edit b".into())
        .unwrap();
    a.receive(&[cb]).unwrap();
    b.receive(&[ca]).unwrap();
    let left = a.get(&root.id).unwrap().unwrap();
    let right = b.get(&root.id).unwrap().unwrap();
    assert_eq!(left, right);
    assert_eq!(a.conflicts().unwrap().len(), 1);
    assert_eq!(b.conflicts().unwrap().len(), 1);
    let loser = a.conflicts().unwrap().remove(0);
    assert_ne!(left.text, loser.1.text);
    a.restore_conflict(&loser.0, "a").unwrap();
    b.receive(&[a.get(&root.id).unwrap().unwrap()]).unwrap();
    assert_eq!(b.get(&root.id).unwrap().unwrap().text, loser.1.text);
}
#[test]
fn clock_skew_cannot_resurrect_a_deleted_record() {
    let db = Store::memory().unwrap();
    let original = db.save_local(Clip::text("a", "value"), "a").unwrap();
    let deleted = db.mutate(&original.id, "a", |c| c.deleted = true).unwrap();
    let mut stale = original;
    stale.updated_at = i64::MAX;
    db.receive(&[stale]).unwrap();
    assert_eq!(db.get(&deleted.id).unwrap().unwrap(), deleted);
    assert!(db.list("", "all", 100, 0).unwrap().is_empty());
}
#[test]
fn revision_pagination_never_skips_same_timestamp_or_full_pages() {
    let db = Store::memory().unwrap();
    for n in 0..131 {
        let mut c = Clip::text("a", &format!("clip {n}"));
        c.updated_at = 1;
        db.save_local(c, "a").unwrap();
    }
    let mut cursor = 0;
    let mut ids = std::collections::HashSet::new();
    loop {
        let p = db.page(cursor, 17).unwrap();
        assert!(p.cursor >= cursor);
        for clip in p.clips {
            ids.insert(clip.id);
        }
        cursor = p.cursor;
        if !p.more {
            break;
        }
    }
    assert_eq!(ids.len(), 131);
}
#[test]
fn cursor_and_received_records_are_committed_together_and_epoch_reset_replays() {
    let source = Store::memory().unwrap();
    let target = Store::memory().unwrap();
    source.save_local(Clip::text("a", "value"), "a").unwrap();
    let p = source.page(0, 50).unwrap();
    target.apply_page("peer", &p).unwrap();
    assert_eq!(target.cursor("peer", &p.epoch).unwrap(), p.cursor);
    assert_eq!(target.cursor("peer", "new server").unwrap(), 0);
    assert_eq!(target.stats().unwrap().clips, 1);
}
#[test]
fn protobuf_preserves_bytes_and_rejects_wrong_message_types() {
    let bytes = vec![0, 255, 1, 2, 128, 0];
    let clip = Clip::from_formats(
        "a",
        "binary",
        vec![ClipFormat {
            format: "application/custom".into(),
            mime: None,
            data: bytes.clone(),
        }],
    );
    let wire = encode(&clip).unwrap();
    let decoded: Clip = decode(&wire).unwrap();
    assert_eq!(decoded, clip);
    assert_eq!(decoded.formats[0].data, bytes);
    assert!(decode::<Ack>(&wire).is_err());
    assert!(decode::<Clip>(&wire[..wire.len() - 1]).is_err());
    // Unknown Protobuf fields survive a mixed-version deployment by being ignored.
    let mut envelope = wire;
    envelope.extend_from_slice(&[0xa0, 0x06, 0x01]);
    assert_eq!(decode::<Clip>(&envelope).unwrap(), clip);
}
#[test]
fn malformed_page_rolls_back_prior_records() {
    let db = Store::memory().unwrap();
    let valid = Clip::text("a", "valid");
    let mut invalid = Clip::text("b", "invalid");
    invalid.id.clear();
    let page = yank_core::Page {
        clips: vec![valid, invalid],
        cursor: 2,
        epoch: "remote".into(),
        more: false,
    };
    assert!(db.apply_page("peer", &page).is_err());
    assert_eq!(db.stats().unwrap().clips, 0);
    assert_eq!(db.cursor("peer", "remote").unwrap(), 0);
}
#[test]
fn export_excludes_credentials_and_import_creates_independent_records() {
    let db = Store::memory().unwrap();
    let c = db.save_local(Clip::text("a", "saved"), "a").unwrap();
    db.set_value("identity", "private-key").unwrap();
    db.set_value("preferences", "token-secret").unwrap();
    let export = db.export().unwrap();
    assert!(!export.contains("private-key"));
    assert!(!export.contains("token-secret"));
    let other = Store::memory().unwrap();
    assert_eq!(other.import(&export, "b").unwrap(), 1);
    assert_ne!(other.list("", "all", 1, 0).unwrap()[0].id, c.id);
}
#[test]
fn invalid_mutation_rolls_back_operation_and_changes() {
    let db = Store::memory().unwrap();
    let mut clip = Clip::text("a", "bad");
    clip.id.clear();
    let mutation = Mutation {
        operation: "bad-op".into(),
        clip,
    };
    assert!(db.accept(&mutation).is_err());
    assert_eq!(db.page(0, 10).unwrap().clips.len(), 0);
}
#[test]
fn native_formats_and_optional_mime_survive_protobuf() {
    let c = Clip::from_formats(
        "a",
        "formats",
        vec![
            ClipFormat::rtf("{\\rtf1 hi}"),
            ClipFormat::html("<b>hi</b>"),
            ClipFormat::file_list(&["/tmp/a".into()]),
        ],
    );
    let d = Clip::from_proto(c.to_proto()).unwrap();
    assert_eq!(c, d);
}

#[test]
fn three_concurrent_replicas_converge_in_different_merge_orders() {
    let a = Store::memory().unwrap();
    let b = Store::memory().unwrap();
    let c = Store::memory().unwrap();
    let root = a.save_local(Clip::text("a", "root"), "a").unwrap();
    b.receive(std::slice::from_ref(&root)).unwrap();
    c.receive(std::slice::from_ref(&root)).unwrap();
    let ca = a.mutate(&root.id, "a", |c| c.text = "A".into()).unwrap();
    let cb = b.mutate(&root.id, "b", |c| c.text = "B".into()).unwrap();
    let cc = c.mutate(&root.id, "c", |c| c.text = "C".into()).unwrap();
    a.receive(&[cb.clone(), cc.clone()]).unwrap();
    b.receive(&[cc, ca.clone()]).unwrap();
    c.receive(&[ca, cb]).unwrap();
    for _ in 0..3 {
        let values = [
            a.get(&root.id).unwrap().unwrap(),
            b.get(&root.id).unwrap().unwrap(),
            c.get(&root.id).unwrap().unwrap(),
        ];
        for db in [&a, &b, &c] {
            db.receive(&values).unwrap();
        }
    }
    assert_eq!(a.get(&root.id).unwrap(), b.get(&root.id).unwrap());
    assert_eq!(b.get(&root.id).unwrap(), c.get(&root.id).unwrap());
}
#[test]
fn binary_bodies_are_loaded_only_for_a_selected_record_or_replication() {
    let db = Store::memory().unwrap();
    let body = vec![123; 1024 * 1024];
    let clip = db
        .save_local(
            Clip::from_formats(
                "a",
                "image",
                vec![ClipFormat {
                    format: "PNG".into(),
                    mime: Some("image/png".into()),
                    data: body.clone(),
                }],
            ),
            "a",
        )
        .unwrap();
    let metadata = db.list("", "all", 100, 0).unwrap();
    assert!(metadata[0].formats[0].data.is_empty());
    assert_eq!(db.get(&clip.id).unwrap().unwrap().formats[0].data, body);
    assert_eq!(db.page(0, 20).unwrap().clips[0].formats[0].data, body);
    assert_eq!(db.pending().unwrap()[0].clip.formats[0].data, body);
}
#[test]
fn detaching_an_account_always_disables_upload_even_on_the_same_server() {
    let mut prefs = yank_core::Preferences {
        server_url: "https://same.example".into(),
        account_id: "alice".into(),
        username: "alice".into(),
        token: "secret".into(),
        upload: true,
        ..Default::default()
    };
    prefs.detach_account();
    assert!(prefs.token.is_empty());
    assert!(prefs.account_id.is_empty());
    assert!(!prefs.upload);
    assert_eq!(prefs.server_url, "https://same.example");
}
#[test]
fn sensitive_clipboard_formats_are_unconditionally_excluded() {
    for format in [
        "org.nspasteboard.ConcealedType",
        "ExcludeClipboardContentFromMonitorProcessing",
        "application/x-keepassxc-password",
    ] {
        assert!(yank_core::confidential_formats(&[format.into()]));
    }
    assert!(!yank_core::confidential_formats(&["text/plain".into()]));
}

#[test]
fn default_lists_are_seeded_once_and_backup_restores_views_and_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.sqlite3");
    let db = Store::open(&path).unwrap();
    assert_eq!(db.views().unwrap().len(), 5);
    db.remove_view("default-images").unwrap();
    drop(db);
    let db = Store::open(&path).unwrap();
    assert_eq!(
        db.views().unwrap().len(),
        4,
        "deleted defaults must not return on restart"
    );
    let clip = db
        .save_local(
            Clip::from_formats(
                "a",
                "rich",
                vec![ClipFormat::text("rich"), ClipFormat::html("<b>rich</b>")],
            ),
            "a",
        )
        .unwrap();
    db.mutate(&clip.id, "a", |c| {
        c.pinned = true;
        c.collection = "work".into();
    })
    .unwrap();
    db.save_view(yank_core::SavedView {
        id: "work-links".into(),
        name: "工作链接".into(),
        query: "reference".into(),
        filter: "link".into(),
        days: 7,
        icon: "link".into(),
    })
    .unwrap();
    let restored = Store::memory().unwrap();
    assert_eq!(
        restored.import(&db.export().unwrap(), "restored").unwrap(),
        1
    );
    let saved = restored.list("", "all", 10, 0).unwrap().remove(0);
    let saved = restored.get(&saved.id).unwrap().unwrap();
    assert!(saved.pinned);
    assert_eq!(saved.collection, "work");
    assert_eq!(saved.formats[1].html_value(), Some("<b>rich</b>"));
    assert!(
        restored
            .views()
            .unwrap()
            .iter()
            .any(|v| v.name == "工作链接" && v.days == 7)
    );
}

#[test]
fn invalid_view_rolls_back_entire_backup_import() {
    let original = Store::memory().unwrap();
    original
        .save_local(Clip::text("a", "must not be partially imported"), "a")
        .unwrap();
    let mut backup: serde_json::Value = serde_json::from_str(&original.export().unwrap()).unwrap();
    backup["views"][0]["name"] = "".into();
    let target = Store::memory().unwrap();
    assert!(target.import(&backup.to_string(), "b").is_err());
    assert_eq!(target.stats().unwrap().clips, 0);
    assert_eq!(target.stats().unwrap().pending, 0);
    assert!(target.page(0, 20).unwrap().clips.is_empty());
}

#[test]
fn causal_successor_cannot_be_replaced_by_its_ancestor_in_any_arrival_order() {
    let root = Store::memory()
        .unwrap()
        .save_local(Clip::text("a", "root"), "a")
        .unwrap();
    let mut a = root.clone();
    a.text = "A".into();
    a.clock.insert("a".into(), 2);
    a.version_id = "f0000000-0000-4000-8000-000000000000".into();
    a.write_order = 2;
    a.write_actor = "a".into();
    let mut b = root.clone();
    b.text = "B".into();
    b.clock.insert("b".into(), 1);
    b.version_id = "80000000-0000-4000-8000-000000000000".into();
    b.write_order = 2;
    b.write_actor = "b".into();
    let mut c = a.clone();
    c.text = "C".into();
    c.deleted = true;
    c.clock.insert("a".into(), 3);
    c.version_id = "10000000-0000-4000-8000-000000000000".into();
    c.write_order = 3;
    let versions = [a, b, c];
    let mut expected = None;
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let db = Store::memory().unwrap();
        for index in order {
            db.receive(std::slice::from_ref(&versions[index])).unwrap();
        }
        db.receive(&versions).unwrap();
        let result = db.get(&root.id).unwrap().unwrap();
        assert_eq!(result.text, "C", "arrival order {order:?}");
        assert!(result.deleted);
        if let Some(expected) = &expected {
            assert_eq!(&result, expected, "arrival order {order:?}");
        } else {
            expected = Some(result);
        }
    }
}

#[test]
fn metadata_mutation_locks_before_reading_and_keeps_new_formats_recoverable() {
    use std::sync::mpsc;
    use std::time::Duration;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("race.sqlite3");
    let db = Store::open(&path).unwrap();
    let root = db.save_local(Clip::text("a", "old"), "a").unwrap();
    let remote = Store::memory().unwrap();
    remote.receive(std::slice::from_ref(&root)).unwrap();
    let incoming = remote
        .mutate(&root.id, "b", |c| {
            c.text = "new rich".into();
            c.formats = vec![ClipFormat::html("<b>new rich</b>")];
        })
        .unwrap();
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let receiver = Store::open(&path).unwrap();
    let writer = Store::open(&path).unwrap();
    let id = root.id.clone();
    let a = std::thread::spawn(move || {
        writer
            .mutate(&id, "a", |c| {
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                c.pinned = true;
            })
            .unwrap()
    });
    locked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let b = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        receiver.receive(&[incoming]).unwrap();
        finished_tx.send(()).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = finished_rx.recv_timeout(Duration::from_millis(150));
    release_tx.send(()).unwrap();
    a.join().unwrap();
    b.join().unwrap();
    assert!(
        premature.is_err(),
        "network cannot commit between metadata read and save"
    );
    let clip = db.get(&root.id).unwrap().unwrap();
    assert_eq!(clip.formats[0].html_value(), Some("<b>new rich</b>"));
    assert!(
        db.conflicts()
            .unwrap()
            .iter()
            .any(|(_, c)| c.pinned && c.text == "old"),
        "concurrent losing edit remains recoverable"
    );
    db.mutate(&root.id, "a", |c| c.pinned = true).unwrap();
    let clip = db.get(&root.id).unwrap().unwrap();
    assert!(clip.pinned);
    assert_eq!(clip.formats[0].html_value(), Some("<b>new rich</b>"));
}

#[test]
fn remote_file_paths_cannot_gain_local_capability_by_forging_device_identity() {
    let local = Store::memory().unwrap();
    let peer = Store::memory().unwrap();
    let files = Clip::from_formats(
        "local-device",
        "file",
        vec![ClipFormat::file_list(&["/tmp/yank-fixture-file".into()])],
    );
    let owned = local.capture(files, "local-device").unwrap();
    assert!(
        !local.can_copy_local_files(&owned).unwrap(),
        "data fields cannot grant OS file access"
    );
    local.authorize_local_files(&owned).unwrap();
    assert!(local.can_copy_local_files(&owned).unwrap());
    let restored_valid = Store::memory().unwrap();
    restored_valid
        .import(&local.export().unwrap(), "local-device")
        .unwrap();
    let restored_clip = restored_valid.list("", "all", 10, 0).unwrap().remove(0);
    let restored_clip = restored_valid.get(&restored_clip.id).unwrap().unwrap();
    assert!(
        !restored_valid.can_copy_local_files(&restored_clip).unwrap(),
        "even a currently valid local capability must not be exported"
    );
    peer.receive(std::slice::from_ref(&owned)).unwrap();
    let mut forged = peer
        .mutate(&owned.id, "peer", |c| {
            c.formats = vec![ClipFormat::file_list(&["/tmp/yank-unobserved-path".into()])]
        })
        .unwrap();
    forged.device_id = "local-device".into();
    local.receive(&[forged]).unwrap();
    let current = local.get(&owned.id).unwrap().unwrap();
    assert_eq!(current.device_id, "local-device");
    assert!(
        !local.can_copy_local_files(&current).unwrap(),
        "a peer cannot authorize another path by spoofing device_id"
    );
    let restored = Store::memory().unwrap();
    restored
        .import(&local.export().unwrap(), "local-device")
        .unwrap();
    let clip = restored.list("", "all", 10, 0).unwrap().remove(0);
    let clip = restored.get(&clip.id).unwrap().unwrap();
    assert!(
        !restored.can_copy_local_files(&clip).unwrap(),
        "backup cannot transfer local file capabilities"
    );
}
