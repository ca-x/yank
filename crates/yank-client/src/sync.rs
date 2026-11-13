//! Optional server storage; never enabled implicitly by pairing or login.
//! Storage pages restore local history silently; only peer live updates reach the OS clipboard.
use crate::network::{Shared, binary_get, binary_post};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use yank_core::{Ack, Mutation, Page, Preferences, Store};

fn same_authorization(current: &Preferences, requested: &Preferences) -> bool {
    current.upload
        && requested.upload
        && !requested.token.is_empty()
        && !requested.server_url.is_empty()
        && current.token == requested.token
        && current.account_id == requested.account_id
        && current.server_url == requested.server_url
}

/// Serialize a local commit with revocation. The action must only do local work;
/// holding this guard across network I/O would prevent the UI from revoking access.
fn authorized<T>(
    shared: &Shared,
    prefs: &Preferences,
    revision: u64,
    action: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    let current = shared
        .prefs
        .lock()
        .map_err(|_| anyhow::anyhow!("Preferences unavailable"))?;
    if shared.auth_revision.load(Ordering::SeqCst) != revision
        || !same_authorization(&current, prefs)
    {
        return Ok(None);
    }
    let result = action();
    drop(current);
    result.map(Some)
}

pub async fn upload(shared: Arc<Shared>, prefs: Preferences) -> Result<()> {
    let revision = {
        let current = shared
            .prefs
            .lock()
            .map_err(|_| anyhow::anyhow!("Preferences unavailable"))?;
        if !same_authorization(&current, &prefs) {
            return Ok(());
        }
        shared.auth_revision.load(Ordering::SeqCst)
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()?;
    let root = prefs.server_url.trim_end_matches('/');
    let store = Store::open(&shared.db)?;
    let probe = binary_get::<Page>(
        &client,
        &format!("{root}/rpc/sync/pull?since={}", i64::MAX),
        &prefs.token,
    )
    .await;
    let Some((probe, pending)) =
        authorized(&shared, &prefs, revision, || Ok((probe?, store.pending()?)))?
    else {
        return Ok(());
    };
    for pending in pending {
        if authorized(&shared, &prefs, revision, || Ok(()))?.is_none() {
            return Ok(());
        }
        let ack = binary_post::<_, Ack>(
            &client,
            &format!("{root}/rpc/sync/push"),
            &prefs.token,
            &pending,
        )
        .await;
        if authorized(&shared, &prefs, revision, || {
            store.acknowledge(&ack?)?;
            Ok(())
        })?
        .is_none()
        {
            return Ok(());
        }
    }
    // Server storage has its own cursor: enabling it includes history received from peers,
    // and switching accounts never reuses another account's acknowledgement state.
    let send_key = format!("server.sent.{}.{}.{}", root, prefs.account_id, probe.epoch);
    let Some(local) = authorized(&shared, &prefs, revision, || {
        let sent = store
            .value(&send_key)?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Ok(store.page(sent, 20)?)
    })?
    else {
        return Ok(());
    };
    for clip in &local.clips {
        if authorized(&shared, &prefs, revision, || Ok(()))?.is_none() {
            return Ok(());
        }
        let mutation = Mutation {
            operation: format!(
                "snapshot:{}",
                hex::encode(Sha256::digest(serde_json::to_vec(clip)?))
            ),
            clip: clip.clone(),
        };
        let ack = binary_post::<_, Ack>(
            &client,
            &format!("{root}/rpc/sync/push"),
            &prefs.token,
            &mutation,
        )
        .await;
        if authorized(&shared, &prefs, revision, || ack.map(|_| ()))?.is_none() {
            return Ok(());
        }
    }
    let key = format!("server.cursor.{}.{}", root, prefs.account_id);
    let Some(mut cursor) = authorized(&shared, &prefs, revision, || {
        store.set_value(&send_key, &local.cursor.to_string())?;
        Ok(store.cursor(&key, &probe.epoch)?)
    })?
    else {
        return Ok(());
    };
    let mut epoch = probe.epoch;
    // A reset between probe and pull invalidates the requested cursor. Never commit
    // that response; retry from zero, with a bound on servers that keep resetting.
    for _ in 0..3 {
        if authorized(&shared, &prefs, revision, || Ok(()))?.is_none() {
            return Ok(());
        }
        let page = binary_get::<Page>(
            &client,
            &format!("{root}/rpc/sync/pull?since={cursor}"),
            &prefs.token,
        )
        .await;
        let Some(page) = authorized(&shared, &prefs, revision, || page)? else {
            return Ok(());
        };
        if page.epoch != epoch {
            epoch = page.epoch;
            cursor = 0;
            continue;
        }
        return authorized(&shared, &prefs, revision, || {
            // Storage restores history silently. Peer live updates own OS clipboard
            // delivery; Shared::receive_page would re-lock prefs inside this guard.
            store.apply_page(&key, &page)?;
            Ok(())
        })
        .map(|_| ());
    }
    anyhow::bail!("Server storage epoch changed during all three pull attempts; retry later")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        extract::{Query, State},
        routing::{get, post},
    };
    use std::{
        collections::HashMap,
        sync::{
            Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };
    use yank_core::{Clip, decode, encode};

    fn shared(db: std::path::PathBuf, prefs: Preferences) -> Arc<Shared> {
        Arc::new(Shared {
            pair_epoch: std::sync::atomic::AtomicU64::new(0),
            peer_access: Mutex::default(),
            auth_revision: std::sync::atomic::AtomicU64::new(0),
            recovery_pending: AtomicBool::new(false),
            recovery_backup: Mutex::default(),
            collecting: AtomicBool::new(false),
            collected: Mutex::default(),
            busy: Mutex::default(),
            remote_clipboard: Mutex::default(),
            servers: Mutex::default(),
            db,
            prefs: Mutex::new(prefs),
            identity: Mutex::default(),
            discovered: Mutex::default(),
            prompts: Mutex::default(),
            pair_requests: Mutex::default(),
            association: Mutex::default(),
            status: Mutex::default(),
            stop: AtomicBool::new(false),
        })
    }

    #[derive(Default)]
    struct Gate {
        entered: tokio::sync::Notify,
        resume: tokio::sync::Notify,
    }
    impl Gate {
        async fn pause(&self) {
            self.entered.notify_one();
            self.resume.notified().await;
        }
        async fn wait(&self) {
            tokio::time::timeout(Duration::from_secs(5), self.entered.notified())
                .await
                .expect("mock request did not reach the response gate");
        }
        fn release(&self) {
            self.resume.notify_one();
        }
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Reset {
        Never,
        Once,
        EveryPull,
    }
    #[derive(Clone)]
    struct MockStorage {
        store: Arc<Mutex<Store>>,
        pulls: Arc<Mutex<Vec<i64>>>,
        pushes: Arc<AtomicUsize>,
        push_gate: Option<Arc<Gate>>,
        pull_gate: Option<(i64, Arc<Gate>)>,
        reset: Reset,
        reset_clip: Option<Clip>,
    }
    impl MockStorage {
        fn new() -> Self {
            Self {
                store: Arc::new(Mutex::new(Store::memory().unwrap())),
                pulls: Arc::default(),
                pushes: Arc::default(),
                push_gate: None,
                pull_gate: None,
                reset: Reset::Never,
                reset_clip: None,
            }
        }
        fn epoch(&self) -> String {
            self.store.lock().unwrap().value("epoch").unwrap().unwrap()
        }
        async fn serve(&self) -> (Preferences, tokio::task::JoinHandle<()>) {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = Router::new()
                .route("/rpc/sync/pull", get(mock_pull))
                .route("/rpc/sync/push", post(mock_push))
                .with_state(self.clone());
            let task = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            (
                Preferences {
                    upload: true,
                    server_url: format!("http://{address}"),
                    token: "test-token".into(),
                    account_id: "account-a".into(),
                    ..Default::default()
                },
                task,
            )
        }
    }
    async fn mock_pull(
        State(mock): State<MockStorage>,
        Query(query): Query<HashMap<String, i64>>,
    ) -> Vec<u8> {
        let since = query["since"];
        let ordinary_pulls = {
            let mut pulls = mock.pulls.lock().unwrap();
            pulls.push(since);
            pulls.iter().filter(|since| **since != i64::MAX).count()
        };
        let bytes = {
            let mut store = mock.store.lock().unwrap();
            if since != i64::MAX
                && (mock.reset == Reset::EveryPull
                    || (mock.reset == Reset::Once && ordinary_pulls == 1))
            {
                *store = Store::memory().unwrap();
                store
                    .receive(std::slice::from_ref(mock.reset_clip.as_ref().unwrap()))
                    .unwrap();
            }
            encode(&store.page(since, 20).unwrap()).unwrap()
        };
        if let Some((gated_since, gate)) = &mock.pull_gate
            && since == *gated_since
        {
            gate.pause().await;
        }
        bytes
    }
    async fn mock_push(State(mock): State<MockStorage>, body: Bytes) -> Vec<u8> {
        let mutation: Mutation = decode(&body).unwrap();
        let bytes = encode(&mock.store.lock().unwrap().accept(&mutation).unwrap()).unwrap();
        mock.pushes.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &mock.push_gate {
            gate.pause().await;
        }
        bytes
    }
    fn disable_upload(state: &Shared) {
        let mut prefs = state.prefs.lock().unwrap();
        prefs.upload = false;
        state.auth_revision.fetch_add(1, Ordering::SeqCst);
    }
    async fn finish(task: tokio::task::JoinHandle<Result<()>>) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
    }
    fn cursor_keys(prefs: &Preferences, epoch: &str) -> (String, String) {
        (
            format!(
                "server.sent.{}.{}.{}",
                prefs.server_url, prefs.account_id, epoch
            ),
            format!("server.cursor.{}.{}", prefs.server_url, prefs.account_id),
        )
    }
    async fn pull(
        State(store): State<Arc<Mutex<Store>>>,
        Query(q): Query<HashMap<String, i64>>,
    ) -> Vec<u8> {
        encode(
            &store
                .lock()
                .unwrap()
                .page(*q.get("since").unwrap_or(&0), 20)
                .unwrap(),
        )
        .unwrap()
    }
    async fn push(State(store): State<Arc<Mutex<Store>>>, body: Bytes) -> Vec<u8> {
        encode(
            &store
                .lock()
                .unwrap()
                .accept(&decode::<Mutation>(&body).unwrap())
                .unwrap(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn optional_storage_includes_acknowledged_and_peer_history_and_replays_after_server_reset()
     {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let store = Store::open(&db).unwrap();
        let a = store
            .save_local(Clip::text("a", "already acknowledged"), "a")
            .unwrap();
        for operation in store.pending().unwrap() {
            store
                .acknowledge(&Ack {
                    operation: operation.operation,
                    clip: operation.clip,
                })
                .unwrap();
        }
        let remote = Store::memory().unwrap();
        let b = remote
            .save_local(Clip::text("b", "peer history"), "b")
            .unwrap();
        store.receive(std::slice::from_ref(&b)).unwrap();
        assert!(store.pending().unwrap().is_empty());
        let server = Arc::new(Mutex::new(Store::memory().unwrap()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/rpc/sync/pull", get(pull))
            .route("/rpc/sync/push", post(push))
            .with_state(server.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let prefs = Preferences {
            server_url: format!("http://{address}"),
            token: "test-only".into(),
            account_id: "account-a".into(),
            ..Default::default()
        };
        let shared = shared(db, prefs.clone());
        upload(shared.clone(), prefs.clone()).await.unwrap();
        assert_eq!(
            server.lock().unwrap().stats().unwrap().clips,
            0,
            "login alone cannot upload"
        );
        let mut enabled = prefs.clone();
        enabled.upload = true;
        *shared.prefs.lock().unwrap() = enabled.clone();
        upload(shared.clone(), enabled.clone()).await.unwrap();
        assert_eq!(
            server.lock().unwrap().get(&a.id).unwrap().unwrap().text,
            a.text
        );
        assert_eq!(
            server.lock().unwrap().get(&b.id).unwrap().unwrap().text,
            b.text
        );
        let before = server.lock().unwrap().page(0, 100).unwrap().cursor;
        upload(shared.clone(), enabled.clone()).await.unwrap();
        assert_eq!(
            server.lock().unwrap().page(0, 100).unwrap().cursor,
            before,
            "retry must not append duplicates"
        );
        *server.lock().unwrap() = Store::memory().unwrap();
        upload(shared.clone(), enabled.clone()).await.unwrap();
        assert_eq!(
            server.lock().unwrap().stats().unwrap().clips,
            2,
            "new server epoch replays local history"
        );
        shared.prefs.lock().unwrap().upload = false;
        store
            .save_local(Clip::text("a", "private after opt out"), "a")
            .unwrap();
        upload(shared, enabled).await.unwrap();
        assert_eq!(
            server.lock().unwrap().stats().unwrap().clips,
            2,
            "stale task cannot continue uploads after opt out"
        );
        task.abort();
    }

    #[tokio::test]
    async fn delayed_ack_is_discarded_after_revocation_including_same_preferences_aba() {
        for change in ["upload", "aba", "account", "token", "url"] {
            let temp = tempfile::tempdir().unwrap();
            let db = temp.path().join("local.sqlite3");
            let local = Store::open(&db).unwrap();
            let original = local
                .save_local(Clip::text("local", "pending original"), "local")
                .unwrap();
            let pending = local.pending().unwrap();
            let gate = Arc::new(Gate::default());
            let mut mock = MockStorage::new();
            mock.push_gate = Some(gate.clone());
            {
                let server = mock.store.lock().unwrap();
                server.receive(std::slice::from_ref(&original)).unwrap();
                server
                    .mutate(&original.id, "server", |clip| {
                        clip.text = "server-only update".into();
                        clip.formats = vec![yank_core::ClipFormat::text(&clip.text)];
                    })
                    .unwrap();
            }
            let epoch = mock.epoch();
            let (prefs, server_task) = mock.serve().await;
            let (sent, cursor) = cursor_keys(&prefs, &epoch);
            local.set_value(&sent, "0").unwrap();
            let old_cursor = format!("{epoch}:17");
            local.set_value(&cursor, &old_cursor).unwrap();
            let state = shared(db, prefs.clone());
            let task = tokio::spawn(upload(state.clone(), prefs.clone()));
            gate.wait().await;
            {
                let mut current = state.prefs.lock().unwrap();
                match change {
                    "upload" => current.upload = false,
                    "aba" => {
                        current.upload = false;
                        state.auth_revision.fetch_add(1, Ordering::SeqCst);
                        current.upload = true;
                    }
                    "account" => current.account_id = "account-b".into(),
                    "token" => current.token = "replacement-token".into(),
                    "url" => current.server_url = "http://127.0.0.1:1".into(),
                    _ => unreachable!(),
                }
                state.auth_revision.fetch_add(1, Ordering::SeqCst);
            }
            gate.release();
            finish(task).await.unwrap();
            assert_eq!(local.get(&original.id).unwrap(), Some(original), "{change}");
            assert_eq!(local.pending().unwrap(), pending, "{change}");
            assert_eq!(
                local.value(&sent).unwrap().as_deref(),
                Some("0"),
                "{change}"
            );
            assert_eq!(
                local.value(&cursor).unwrap().as_deref(),
                Some(old_cursor.as_str()),
                "{change}"
            );
            assert_eq!(mock.pushes.load(Ordering::SeqCst), 1, "{change}");
            assert_eq!(*mock.pulls.lock().unwrap(), [i64::MAX], "{change}");
            server_task.abort();
        }
    }
    #[tokio::test]
    async fn delayed_snapshot_ack_cannot_advance_sent_cursor_after_opt_out() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let local = Store::open(&db).unwrap();
        local
            .save_local(Clip::text("local", "already acknowledged"), "local")
            .unwrap();
        for mutation in local.pending().unwrap() {
            local
                .acknowledge(&Ack {
                    operation: mutation.operation,
                    clip: mutation.clip,
                })
                .unwrap();
        }
        let gate = Arc::new(Gate::default());
        let mut mock = MockStorage::new();
        mock.push_gate = Some(gate.clone());
        let epoch = mock.epoch();
        let (prefs, server_task) = mock.serve().await;
        let (sent, cursor) = cursor_keys(&prefs, &epoch);
        local.set_value(&sent, "0").unwrap();
        let old_cursor = format!("{epoch}:0");
        local.set_value(&cursor, &old_cursor).unwrap();
        let state = shared(db, prefs.clone());
        let task = tokio::spawn(upload(state.clone(), prefs));
        gate.wait().await;
        disable_upload(&state);
        gate.release();
        finish(task).await.unwrap();
        assert!(local.pending().unwrap().is_empty());
        assert_eq!(local.value(&sent).unwrap().as_deref(), Some("0"));
        assert_eq!(
            local.value(&cursor).unwrap().as_deref(),
            Some(old_cursor.as_str())
        );
        assert_eq!(*mock.pulls.lock().unwrap(), [i64::MAX]);
        server_task.abort();
    }
    #[tokio::test]
    async fn delayed_pull_cannot_commit_after_revocation() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let local = Store::open(&db).unwrap();
        let gate = Arc::new(Gate::default());
        let mut mock = MockStorage::new();
        mock.pull_gate = Some((0, gate.clone()));
        let remote = mock
            .store
            .lock()
            .unwrap()
            .save_local(Clip::text("server", "private server history"), "server")
            .unwrap();
        let epoch = mock.epoch();
        let (prefs, server_task) = mock.serve().await;
        let (sent, cursor) = cursor_keys(&prefs, &epoch);
        local.set_value(&sent, "0").unwrap();
        let old_cursor = format!("{epoch}:0");
        local.set_value(&cursor, &old_cursor).unwrap();
        let state = shared(db, prefs.clone());
        let task = tokio::spawn(upload(state.clone(), prefs));
        gate.wait().await;
        disable_upload(&state);
        gate.release();
        finish(task).await.unwrap();
        assert!(local.get(&remote.id).unwrap().is_none());
        assert!(local.pending().unwrap().is_empty());
        assert_eq!(local.value(&sent).unwrap().as_deref(), Some("0"));
        assert_eq!(
            local.value(&cursor).unwrap().as_deref(),
            Some(old_cursor.as_str())
        );
        assert!(state.remote_clipboard.lock().unwrap().is_none());
        server_task.abort();
    }
    #[tokio::test]
    async fn delayed_probe_cannot_start_upload_after_revocation() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let local = Store::open(&db).unwrap();
        local
            .save_local(Clip::text("local", "pending"), "local")
            .unwrap();
        let pending = local.pending().unwrap();
        let gate = Arc::new(Gate::default());
        let mut mock = MockStorage::new();
        mock.pull_gate = Some((i64::MAX, gate.clone()));
        let epoch = mock.epoch();
        let (prefs, server_task) = mock.serve().await;
        let (sent, cursor) = cursor_keys(&prefs, &epoch);
        let state = shared(db, prefs.clone());
        let task = tokio::spawn(upload(state.clone(), prefs));
        gate.wait().await;
        disable_upload(&state);
        gate.release();
        finish(task).await.unwrap();
        assert_eq!(local.pending().unwrap(), pending);
        assert!(local.value(&sent).unwrap().is_none());
        assert!(local.value(&cursor).unwrap().is_none());
        assert_eq!(mock.pushes.load(Ordering::SeqCst), 0);
        assert_eq!(*mock.pulls.lock().unwrap(), [i64::MAX]);
        server_task.abort();
    }
    #[tokio::test]
    async fn reset_between_probe_and_pull_retries_zero_without_committing_stale_cursor() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let local = Store::open(&db).unwrap();
        let gate = Arc::new(Gate::default());
        let mut mock = MockStorage::new();
        mock.reset = Reset::Once;
        mock.pull_gate = Some((0, gate.clone()));
        let seed = Store::memory().unwrap();
        let remote = seed
            .save_local(
                Clip::from_formats(
                    "remote",
                    "new epoch history",
                    vec![
                        yank_core::ClipFormat::text("new epoch history"),
                        yank_core::ClipFormat::html("<b>new epoch history</b>"),
                    ],
                ),
                "remote",
            )
            .unwrap();
        mock.reset_clip = Some(remote.clone());
        let old_epoch = mock.epoch();
        let (prefs, server_task) = mock.serve().await;
        let (sent, cursor) = cursor_keys(&prefs, &old_epoch);
        local.set_value(&sent, "0").unwrap();
        let old_cursor = format!("{old_epoch}:100");
        local.set_value(&cursor, &old_cursor).unwrap();
        let state = shared(db, prefs.clone());
        let task = tokio::spawn(upload(state.clone(), prefs));
        gate.wait().await;
        assert_eq!(
            local.value(&cursor).unwrap().as_deref(),
            Some(old_cursor.as_str())
        );
        assert!(local.get(&remote.id).unwrap().is_none());
        assert!(local.pending().unwrap().is_empty());
        gate.release();
        finish(task).await.unwrap();
        let new_epoch = mock.epoch();
        assert_ne!(new_epoch, old_epoch);
        assert_eq!(local.get(&remote.id).unwrap(), Some(remote));
        assert_eq!(local.cursor(&cursor, &new_epoch).unwrap(), 1);
        assert_eq!(*mock.pulls.lock().unwrap(), [i64::MAX, 100, 0]);
        assert!(local.pending().unwrap().is_empty());
        assert!(state.remote_clipboard.lock().unwrap().is_none());
        server_task.abort();
    }
    #[tokio::test]
    async fn continuous_epoch_resets_stop_after_three_pulls_without_committing_cursor() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let local = Store::open(&db).unwrap();
        let mut mock = MockStorage::new();
        mock.reset = Reset::EveryPull;
        let seed = Store::memory().unwrap();
        let remote = seed
            .save_local(Clip::text("remote", "unstable epoch history"), "remote")
            .unwrap();
        mock.reset_clip = Some(remote.clone());
        let old_epoch = mock.epoch();
        let (prefs, server_task) = mock.serve().await;
        let (sent, cursor) = cursor_keys(&prefs, &old_epoch);
        local.set_value(&sent, "0").unwrap();
        let old_cursor = format!("{old_epoch}:100");
        local.set_value(&cursor, &old_cursor).unwrap();
        let state = shared(db, prefs.clone());
        assert!(upload(state, prefs).await.is_err());
        assert!(local.get(&remote.id).unwrap().is_none());
        assert!(local.pending().unwrap().is_empty());
        assert_eq!(local.value(&sent).unwrap().as_deref(), Some("0"));
        assert_eq!(
            local.value(&cursor).unwrap().as_deref(),
            Some(old_cursor.as_str())
        );
        assert_eq!(*mock.pulls.lock().unwrap(), [i64::MAX, 100, 0, 0]);
        server_task.abort();
    }
    #[tokio::test]
    async fn server_storage_restores_live_history_without_queueing_os_clipboard() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("local.sqlite3");
        let local = Store::open(&db).unwrap();
        let mock = MockStorage::new();
        let first = mock
            .store
            .lock()
            .unwrap()
            .save_local(Clip::text("server", "history"), "server")
            .unwrap();
        let (prefs, server_task) = mock.serve().await;
        let state = shared(db, prefs.clone());
        upload(state.clone(), prefs.clone()).await.unwrap();
        let updated = mock
            .store
            .lock()
            .unwrap()
            .mutate(&first.id, "server", |clip| {
                clip.text = "live server content".into();
                clip.formats = vec![yank_core::ClipFormat::text(&clip.text)];
            })
            .unwrap();
        upload(state.clone(), prefs).await.unwrap();
        assert_eq!(local.get(&first.id).unwrap(), Some(updated));
        assert!(state.remote_clipboard.lock().unwrap().is_none());
        server_task.abort();
    }
}
