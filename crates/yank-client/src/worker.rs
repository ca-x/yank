//! Database and clipboard work stays off the UI thread.
use crate::network::{self, Shared};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use yank_core::{Clip, ClipFormat, Peer, Preferences, Stats, Store};
#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub sequence: u64,
    pub message: String,
    pub copied_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub feedback: Option<Feedback>,
    pub network_status: String,
    pub identity: Option<Peer>,
    pub discovered: Vec<network::Discovered>,
    pub servers: Vec<network::ServerInstance>,
    pub clips: Vec<Clip>,
    pub stats: Stats,
    pub prefs: Preferences,
    pub peers: Vec<Peer>,
    pub conflicts: Vec<(String, Clip)>,
    pub query: String,
    pub filter: String,
    pub error: String,
    pub has_more: bool,
    pub has_earlier: bool,
    pub views: Vec<yank_core::SavedView>,
    pub boards: Vec<String>,
    pub selected: Option<Clip>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSource {
    System,
    Foreground,
}
#[derive(Debug)]
pub enum Command {
    Search(String, String, u32),
    Earlier,
    Select(String),
    PairCode(String, bool, u64),
    Capture(Clip, CaptureSource),
    Pin(String),
    Delete(String),
    DeleteMany(Vec<String>),
    Edit(String, String, String),
    Save(Preferences, u64),
    Login(String, String, String, u64),
    Logout,
    Unpair(String),
    Resolve(String),
    Export(PathBuf),
    Import(PathBuf),
    Copy(Clip, bool),
    CopySaved(String),
    Paste(String),
    SaveView(yank_core::SavedView),
    RemoveView(String),
    SendDitto(Clip),
    CheckUpdate,
}
pub struct Worker {
    pub commands: mpsc::Sender<Command>,
    pub snapshots: mpsc::Receiver<Snapshot>,
    pub shared: Arc<Shared>,
}
impl Worker {
    pub fn start(directory: PathBuf, network_enabled: bool) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        }
        let db = directory.join("yank-v2.sqlite3");
        let recovery = if !db.exists() {
            latest_backup(&directory.join("backups"))?
        } else {
            None
        };
        let store = Store::open(&db)?;
        let prefs = store.preferences()?;
        let shared = Arc::new(Shared {
            pair_epoch: std::sync::atomic::AtomicU64::new(0),
            peer_access: Mutex::default(),
            auth_revision: std::sync::atomic::AtomicU64::new(0),
            recovery_pending: AtomicBool::new(recovery.is_some()),
            recovery_backup: Mutex::new(recovery),
            collecting: AtomicBool::new(false),
            collected: Mutex::default(),
            busy: Mutex::default(),
            remote_clipboard: Mutex::default(),
            servers: Mutex::default(),
            db,
            prefs: Mutex::new(prefs),
            identity: Mutex::new(None),
            discovered: Mutex::default(),
            prompts: Mutex::default(),
            pair_requests: Mutex::default(),
            association: Mutex::default(),
            status: Mutex::new("离线 · 本地记录随时可用".into()),
            stop: AtomicBool::new(false),
        });
        let (commands, rx) = mpsc::channel();
        let (tx, snapshots) = mpsc::channel();
        if network_enabled {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("yank-network".into())
                .spawn(move || {
                    match tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2)
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => {
                            let state = shared.clone();
                            if let Err(e) = rt.block_on(network::run(state)) {
                                shared.status(crate::l10n::message(
                                    "网络不可用：{e}",
                                    &[("e", (e).to_string())],
                                ));
                            }
                        }
                        Err(e) => shared.status(crate::l10n::message(
                            "网络启动失败：{e}",
                            &[("e", (e).to_string())],
                        )),
                    }
                })?;
        }
        let state = shared.clone();
        std::thread::Builder::new()
            .name("yank-storage".into())
            .spawn(move || run(store, state, rx, tx, network_enabled))?;
        Ok(Self {
            commands,
            snapshots,
            shared,
        })
    }
}
fn latest_backup(directory: &std::path::Path) -> anyhow::Result<Option<PathBuf>> {
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut latest: Option<(i64, PathBuf)> = None;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(day) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.strip_prefix("yank-"))
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<i64>().ok())
            && entry.file_type()?.is_file()
            && latest.as_ref().is_none_or(|(old, _)| day > *old)
        {
            latest = Some((day, path));
        }
    }
    Ok(latest.map(|(_, path)| path))
}
fn collect_clip(state: &Shared, clip: Clip) {
    if state.collecting.load(Ordering::Relaxed)
        && let Ok(mut collected) = state.collected.lock()
        && collected.len() < 1000
        && !collected.iter().any(|c| c.id == clip.id)
    {
        collected.push(clip);
    }
}
fn revoke_session(prefs: &Preferences) -> anyhow::Result<()> {
    if prefs.token.is_empty() || prefs.server_url.is_empty() {
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime
        .block_on(async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()?;
            network::binary_post::<(), ()>(
                &client,
                &format!("{}/rpc/logout", prefs.server_url.trim_end_matches('/')),
                &prefs.token,
                &(),
            )
            .await
        })
        .map_err(|e| {
            anyhow::anyhow!(crate::l10n::message(
                "已解除本机账号；服务端会话撤销失败，请在服务端修改密码以撤销会话：{e}",
                &[("e", e.to_string())]
            ))
        })
}
fn run(
    store: Store,
    state: Arc<Shared>,
    rx: mpsc::Receiver<Command>,
    tx: mpsc::Sender<Snapshot>,
    capture: bool,
) {
    let device = match store.value("device") {
        Ok(Some(id)) => id,
        _ => {
            let id = yank_core::new_id();
            let _ = store.set_value("device", &id);
            id
        }
    };
    let clipboard = crate::clipboard::NativeClipboard::new().ok();
    let mut maintenance = Instant::now() - Duration::from_secs(3601);
    let mut update_check = Instant::now() - Duration::from_secs(86401);
    let mut selected = String::new();
    let (mut query, mut filter, mut limit) = (String::new(), "all".to_owned(), 100u32);
    let mut history_expanded = false;
    let mut error = String::new();
    let mut dirty = true;
    let mut last_snapshot = Instant::now();
    let mut thumbnails = std::collections::HashMap::new();
    let mut feedback: Option<Feedback> = None;
    let mut last_capture = Instant::now();
    while !state.stop.load(Ordering::Relaxed) {
        let cmd = rx.recv_timeout(Duration::from_millis(80));
        if let Ok(command) = cmd {
            let long_task = matches!(
                &command,
                Command::Login(..)
                    | Command::Export(..)
                    | Command::Import(..)
                    | Command::SendDitto(..)
            );
            let mut completed_notice = None;
            let mut copied_id = None;
            let result: anyhow::Result<()> = (|| {
                match command {
                    Command::PairCode(code, invite, epoch) => {
                        anyhow::ensure!(
                            state.pair_epoch.load(Ordering::SeqCst) == epoch,
                            "配对请求已取消"
                        );
                        let found = state.discovered.lock().ok().and_then(|d| {
                            d.values()
                                .find(|d| d.peer.code.eq_ignore_ascii_case(&code))
                                .map(|d| d.peer.clone())
                        });
                        let peer = if let Some(peer) = found {
                            peer
                        } else {
                            let prefs = store.preferences()?;
                            anyhow::ensure!(
                                !prefs.server_url.is_empty(),
                                "请先设置发现服务器，或在同一局域网选择设备"
                            );
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()?;
                            rt.block_on(async {
                                let http = reqwest::Client::builder()
                                    .timeout(Duration::from_secs(12))
                                    .build()?;
                                network::binary_get(
                                    &http,
                                    &format!(
                                        "{}/rpc/devices/{}",
                                        prefs.server_url.trim_end_matches('/'),
                                        code.trim().to_uppercase()
                                    ),
                                    &prefs.token,
                                )
                                .await
                            })?
                        };
                        let account = if invite {
                            let p = store.preferences()?;
                            if p.token.is_empty() {
                                None
                            } else {
                                Some(network::Association {
                                    server_url: p.server_url,
                                    username: p.username,
                                })
                            }
                        } else {
                            None
                        };
                        state.queue_pair(peer, account, Some(epoch))?;
                        state.status("配对请求已发送，等待对方确认");
                    }
                    Command::Select(id) => {
                        selected = id;
                    }
                    Command::CheckUpdate => {
                        let state = state.clone();
                        std::thread::Builder::new()
                            .name("yank-update".into())
                            .spawn(move || match crate::integration::update() {
                                Ok(message) => state.status(message),
                                Err(e) => state.status(crate::l10n::message(
                                    "更新失败：{e}",
                                    &[("e", (e).to_string())],
                                )),
                            })?;
                    }
                    Command::Search(q, f, l) => {
                        if query != q || filter != f || l <= limit {
                            history_expanded = false;
                        }
                        query = q;
                        filter = f;
                        limit = l;
                    }
                    Command::Earlier => {
                        history_expanded = true;
                        limit = limit.saturating_add(100);
                    }
                    Command::Capture(mut clip, source) => {
                        #[cfg(not(target_os = "android"))]
                        anyhow::ensure!(
                            source != CaptureSource::System || clipboard.is_some(),
                            "无法验证剪贴板隐私标记，未保存内容"
                        );
                        if source == CaptureSource::System
                            && clipboard.as_ref().is_some_and(|c| c.sensitive())
                        {
                            return Ok(());
                        }
                        clip.device_id = device.clone();
                        let prefs = store.preferences()?;
                        let excluded = if prefs.exclude_pattern.is_empty() {
                            false
                        } else {
                            regex::Regex::new(&prefs.exclude_pattern)?.is_match(&clip.text)
                        };
                        if !excluded {
                            let clip = store.capture(clip, &device)?;
                            if source == CaptureSource::System {
                                store.authorize_local_files(&clip)?;
                            }
                            collect_clip(&state, clip);
                        }
                    }
                    Command::Pin(id) => {
                        store.mutate(&id, &device, |c| c.pinned = !c.pinned)?;
                    }
                    Command::DeleteMany(ids) => {
                        anyhow::ensure!(ids.len() <= 1000, crate::l10n::tr("选择的记录过多"));
                        let mut removed = 0;
                        for id in ids {
                            match store.get(&id) {
                                Ok(Some(clip)) if !clip.deleted => {
                                    store
                                        .mutate(&id, &device, |clip| clip.deleted = true)
                                        .map_err(|error| {
                                            anyhow::anyhow!(crate::l10n::message(
                                                "已删除 {0} 条，其余未处理：{error}",
                                                &[
                                                    ("0", removed.to_string()),
                                                    ("error", error.to_string())
                                                ]
                                            ))
                                        })?;
                                    removed += 1;
                                }
                                Ok(_) => {}
                                Err(error) => anyhow::bail!(crate::l10n::message(
                                    "已删除 {0} 条，其余未处理：{error}",
                                    &[("0", removed.to_string()), ("error", error.to_string())]
                                )),
                            }
                        }
                        completed_notice = Some(crate::l10n::message(
                            "已删除 {0} 条记录",
                            &[("0", removed.to_string())],
                        ));
                    }
                    Command::Delete(id) => {
                        store.mutate(&id, &device, |c| c.deleted = true)?;
                    }
                    Command::Edit(id, text, collection) => {
                        store.mutate(&id, &device, |c| {
                            if !matches!(c.kind.as_str(), "image" | "files") && c.text != text {
                                c.text = text.clone();
                                c.description = text
                                    .lines()
                                    .next()
                                    .unwrap_or("文字")
                                    .chars()
                                    .take(100)
                                    .collect();
                                c.formats = vec![ClipFormat::text(&text)];
                            }
                            c.collection = collection;
                        })?;
                    }
                    Command::Save(prefs, revision) => {
                        let mut current = state
                            .prefs
                            .lock()
                            .map_err(|_| anyhow::anyhow!("设置不可用"))?;
                        if state.auth_revision.load(Ordering::SeqCst) != revision {
                            return Ok(());
                        }
                        let previous = store.preferences()?;
                        if previous.history_range != prefs.history_range {
                            history_expanded = false;
                            limit = 100;
                        }
                        for key in [
                            &prefs.hotkey_show,
                            &prefs.hotkey_next,
                            &prefs.hotkey_pause,
                            &prefs.hotkey_float,
                        ] {
                            if !key.is_empty() {
                                nsh_core::keymap::Keystroke::parse_with(
                                    key,
                                    cfg!(target_os = "macos"),
                                )
                                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                            }
                        }

                        if !prefs.exclude_pattern.is_empty() {
                            regex::Regex::new(&prefs.exclude_pattern)?;
                        }
                        anyhow::ensure!(
                            ["relay", "direct", "auto"].contains(&prefs.mode.as_str()),
                            "未知连接模式"
                        );
                        if !previous.auto_update && prefs.auto_update {
                            update_check = Instant::now() - Duration::from_secs(86401);
                        }
                        prefs.cleanup.validate()?;
                        if previous.launch_at_login != prefs.launch_at_login {
                            crate::integration::launch_at_login(prefs.launch_at_login)?;
                        }
                        store.save_preferences(&prefs)?;
                        *current = prefs.clone();
                        drop(current);
                        maintenance = Instant::now() - Duration::from_secs(61);
                        if !previous.token.is_empty() && previous.token != prefs.token {
                            revoke_session(&previous)?;
                        }
                    }
                    Command::Login(url, username, password, revision) => {
                        let url = url.trim().trim_end_matches('/').to_owned();
                        let parsed = reqwest::Url::parse(&url)?;
                        anyhow::ensure!(
                            ["http", "https"].contains(&parsed.scheme()),
                            "服务器地址需要 http:// 或 https://"
                        );
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()?;
                        let login: yank_core::protocol::Login = rt.block_on(async {
                            let http = reqwest::Client::builder()
                                .timeout(Duration::from_secs(12))
                                .build()?;
                            network::binary_post(
                                &http,
                                &format!("{url}/rpc/login"),
                                "",
                                &yank_core::protocol::Credentials { username, password },
                            )
                            .await
                        })?;
                        let user = login.user.ok_or_else(|| anyhow::anyhow!("无效账号响应"))?;
                        let mut prefs = store.preferences()?;
                        prefs.server_url = url;
                        prefs.username = user.username;
                        prefs.account_id = user.id;
                        prefs.token = login.token;
                        prefs.upload = false;
                        let committed = {
                            let mut current = state
                                .prefs
                                .lock()
                                .map_err(|_| anyhow::anyhow!("设置不可用"))?;
                            if state.auth_revision.load(Ordering::SeqCst) != revision {
                                false
                            } else {
                                store.save_preferences(&prefs)?;
                                *current = prefs.clone();
                                true
                            }
                        };
                        if !committed {
                            let _ = revoke_session(&prefs);
                            anyhow::bail!("登录已取消，旧请求不会重新关联账号");
                        }
                        state.status("登录成功 · 尚未启用服务器存储");
                    }
                    Command::Logout => {
                        let previous = store.preferences()?;
                        let mut prefs = previous.clone();
                        prefs.token.clear();
                        prefs.username.clear();
                        prefs.account_id.clear();
                        prefs.upload = false;
                        store.save_preferences(&prefs)?;
                        *state
                            .prefs
                            .lock()
                            .map_err(|_| anyhow::anyhow!("设置不可用"))? = prefs;
                        revoke_session(&previous)?;
                    }
                    Command::Unpair(id) => {
                        state.finish_unpair(&store, &id)?;
                    }
                    Command::Resolve(id) => {
                        store.restore_conflict(&id, &device)?;
                    }
                    Command::Export(path) => {
                        std::fs::write(path, store.export()?)?;
                        state.status("记录已导出，不包含账号密码或令牌");
                    }
                    Command::Import(path) => {
                        use std::io::Read;
                        let mut bytes = Vec::new();
                        std::fs::File::open(path)?
                            .take(64 * 1024 * 1024 + 1)
                            .read_to_end(&mut bytes)?;
                        anyhow::ensure!(bytes.len() <= 64 * 1024 * 1024, "导入文件超过 64 MiB");
                        let count = store.import(std::str::from_utf8(&bytes)?, &device)?;
                        state.recovery_pending.store(false, Ordering::Relaxed);
                        state.status(crate::l10n::message(
                            "已导入 {count} 条记录",
                            &[("count", (count).to_string())],
                        ));
                    }
                    Command::SendDitto(clip) => {
                        let prefs = store.preferences()?;
                        anyhow::ensure!(!prefs.ditto_remote.is_empty(), "请先设置远端 Ditto 地址");
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()?;
                        rt.block_on(async {
                            tokio::time::timeout(
                                Duration::from_secs(40),
                                yank_ditto::send(&prefs.ditto_remote, &prefs.ditto_password, &clip),
                            )
                            .await?
                        })?;
                        state.status("已向 Ditto 发送记录");
                    }
                    Command::SaveView(view) => {
                        store.save_view(view)?;
                    }
                    Command::RemoveView(id) => {
                        store.remove_view(&id)?;
                    }
                    Command::Paste(id) => {
                        // List rows omit large payloads. Read the saved record here so a
                        // fast Enter never races selection hydration or pastes a stale edit.
                        let clip = saved_paste_clip(&store, &id)?;
                        anyhow::ensure!(
                            !clip.formats.iter().any(ClipFormat::is_file_list)
                                || store.can_copy_local_files(&clip)?,
                            "文件历史仅能复制本机捕获的路径；请在本机重新复制源文件"
                        );
                        clipboard
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("系统剪贴板不可用"))?
                            .write(&clip, false)?;
                        paste()?;
                        store.mark_used(&clip.id)?;
                        state.status("已发送粘贴按键");
                        completed_notice = Some("已发送粘贴按键".to_owned());
                        copied_id = Some(clip.id.clone());
                    }
                    Command::CopySaved(id) => {
                        let clip = store
                            .get(&id)?
                            .filter(|clip| !clip.deleted)
                            .ok_or_else(|| anyhow::anyhow!("记录已不存在"))?;
                        anyhow::ensure!(
                            !clip.formats.iter().any(ClipFormat::is_file_list)
                                || store.can_copy_local_files(&clip)?,
                            "文件历史仅能复制本机捕获的路径；请在本机重新复制源文件"
                        );
                        clipboard
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("系统剪贴板不可用"))?
                            .write(&clip, false)?;
                        store.mark_used(&clip.id)?;
                        state.status("已复制，可以接着使用了");
                        completed_notice = Some("已复制，可以接着使用了".to_owned());
                        copied_id = Some(clip.id.clone());
                    }
                    Command::Copy(clip, plain) => {
                        anyhow::ensure!(
                            plain
                                || !clip.formats.iter().any(ClipFormat::is_file_list)
                                || store.can_copy_local_files(&clip)?,
                            "文件历史仅能复制本机捕获的路径；请在本机重新复制源文件"
                        );
                        clipboard
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("系统剪贴板不可用"))?
                            .write(&clip, plain)?;
                        store.mark_used(&clip.id)?;
                        state.status("已复制，可以接着使用了");
                        completed_notice = Some("已复制，可以接着使用了".to_owned());
                        copied_id = Some(clip.id.clone());
                    }
                }
                Ok(())
            })();
            if long_task && let Ok(mut busy) = state.busy.lock() {
                *busy = None;
            }
            if result.is_ok()
                && let Some(message) = completed_notice
            {
                feedback = Some(Feedback {
                    sequence: feedback
                        .as_ref()
                        .map_or(1, |feedback| feedback.sequence.wrapping_add(1)),
                    message,
                    copied_id,
                });
            }
            error = result.err().map(|e| e.to_string()).unwrap_or_default();
            dirty = true;
        }
        let remote = state
            .remote_clipboard
            .lock()
            .ok()
            .and_then(|mut p| p.take());
        if let Some(remote) = remote {
            let applied = state.with_peer(&remote.permit, || {
                if state.prefs.lock().map(|p| p.apply_remote).unwrap_or(false) {
                    #[cfg(not(target_os = "android"))]
                    if let Some(clipboard) = &clipboard
                        && !remote.clip.formats.iter().any(ClipFormat::is_file_list)
                        && store
                            .get(&remote.clip.id)?
                            .is_some_and(|c| !c.deleted && c.version_id == remote.clip.version_id)
                    {
                        clipboard.write(&remote.clip, false)?;
                    }
                    #[cfg(target_os = "android")]
                    state.status("收到远端记录，可在前台选择复制");
                }
                Ok(())
            });
            if let Err(e) = applied {
                error = e.to_string();
            }
        }
        if capture
            && !state.recovery_pending.load(Ordering::Relaxed)
            && last_capture.elapsed() > Duration::from_secs(1)
        {
            last_capture = Instant::now();
            if let Ok(prefs) = store.preferences()
                && prefs.capture
            {
                match clipboard
                    .as_ref()
                    .map(|c| c.capture(&store, &device, &prefs))
                    .unwrap_or(Ok(None))
                {
                    Ok(Some(clip)) => collect_clip(&state, clip),
                    Ok(None) => {}
                    Err(e) => error = e.to_string(),
                }
            }
        }
        if capture && update_check.elapsed() > Duration::from_secs(86400) {
            update_check = Instant::now();
            if state.prefs.lock().map(|p| p.auto_update).unwrap_or(false)
                && crate::integration::update_unavailable_reason().is_none()
            {
                let state = state.clone();
                let _ = std::thread::Builder::new()
                    .name("yank-update".into())
                    .spawn(move || match crate::integration::update() {
                        Ok(message) => state.status(message),
                        Err(e) => state.status(crate::l10n::message(
                            "自动更新未完成：{e}",
                            &[("e", (e).to_string())],
                        )),
                    });
            }
        }
        if maintenance.elapsed() > Duration::from_secs(60) {
            maintenance = Instant::now();
            match maintain(&store, &state, &device) {
                Ok(deleted) => dirty |= deleted > 0,
                Err(e) => error = e.to_string(),
            }
        }
        if dirty || last_snapshot.elapsed() > Duration::from_millis(700) {
            let snapshot = (|| -> yank_core::Result<Snapshot> {
                let saved = store
                    .views()?
                    .into_iter()
                    .find(|view| format!("smart:{}", view.id) == filter);
                let prefs = store.preferences()?;
                let (search, kind, cutoff) = if let Some(view) = &saved {
                    (
                        view.query.as_str(),
                        view.filter.as_str(),
                        if view.days == 0 {
                            i64::MIN
                        } else {
                            yank_core::now_ts().saturating_sub(i64::from(view.days) * 86400)
                        },
                    )
                } else {
                    (
                        query.as_str(),
                        filter.as_str(),
                        if history_expanded || !query.trim().is_empty() || filter == "pinned" {
                            i64::MIN
                        } else {
                            prefs.history_range.cutoff()
                        },
                    )
                };
                let window = store.history_window(search, kind, cutoff, limit)?;
                let mut clips = window.clips;
                attach_thumbnails(&store, &mut clips, &mut thumbnails);
                let has_more = window.has_more;
                let has_earlier = saved.is_none() && window.has_earlier;
                let network_status = state.status.lock().map(|s| s.clone()).unwrap_or_default();
                let identity = state.identity.lock().ok().and_then(|s| s.clone());
                let mut discovered = state
                    .discovered
                    .lock()
                    .map(|s| s.values().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                discovered.sort_by(|a, b| a.peer.id.cmp(&b.peer.id));
                let mut servers = state
                    .servers
                    .lock()
                    .map(|s| s.values().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                servers.sort_by(|a, b| a.url.cmp(&b.url));
                Ok(Snapshot {
                    feedback: feedback.clone(),
                    network_status,
                    identity,
                    discovered,
                    servers,
                    clips,
                    stats: store.stats()?,
                    prefs: store.preferences()?,
                    peers: store.peers()?,
                    conflicts: store.conflicts()?,
                    query: query.clone(),
                    filter: filter.clone(),
                    error: error.clone(),
                    has_more,
                    has_earlier,
                    views: store.views()?,
                    boards: store.collections()?,
                    selected: store.get(&selected)?,
                })
            })();
            if let Ok(snapshot) = snapshot
                && tx.send(snapshot).is_err()
            {
                break;
            }
            dirty = false;
            last_snapshot = Instant::now();
        }
    }
}
fn png_thumbnail(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > 8 * 1024 * 1024 {
        return None;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(48, 48);
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).ok()?;
    Some(output.into_inner())
}

fn attach_thumbnails(
    store: &Store,
    clips: &mut [Clip],
    cache: &mut std::collections::HashMap<String, Option<Vec<u8>>>,
) {
    let key_of = |clip: &Clip| format!("{}:{}:{:?}", clip.id, clip.version_id, clip.clock);
    let keys = clips
        .iter()
        .take(128)
        .map(key_of)
        .collect::<std::collections::HashSet<_>>();
    cache.retain(|key, _| keys.contains(key));
    let mut budget = 4;
    for clip in clips
        .iter_mut()
        .take(128)
        .filter(|clip| clip.kind == "image")
    {
        let Some(index) = clip
            .formats
            .iter()
            .position(|format| format.mime.as_deref() == Some("image/png"))
        else {
            continue;
        };
        let key = key_of(clip);
        if !cache.contains_key(&key) && budget > 0 {
            budget -= 1;
            let thumbnail = store.get(&clip.id).ok().flatten().and_then(|record| {
                record
                    .formats
                    .get(index)
                    .and_then(|format| png_thumbnail(&format.data))
            });
            cache.insert(key.clone(), thumbnail);
        }
        if let Some(Some(data)) = cache.get(&key) {
            clip.formats[index].data = data.clone();
        }
    }
}

fn maintain(store: &Store, state: &Shared, device: &str) -> anyhow::Result<usize> {
    if state.recovery_pending.load(Ordering::Relaxed) {
        return Ok(0);
    }
    let prefs = store.preferences()?;
    let deleted = store.cleanup(&prefs.cleanup, prefs.retention_days, device)?;
    if !prefs.backup_daily {
        return Ok(deleted);
    }
    let day = yank_core::now_ts() / 86400;
    let key = format!("backup.{day}");
    if store.value(&key)?.is_some() {
        return Ok(deleted);
    }
    let directory = state
        .db
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("backups");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("yank-{day}.json"));
    let temp = path.with_extension("tmp");
    if !path.exists() {
        std::fs::write(&temp, store.export()?)?;
        std::fs::rename(&temp, &path)?;
    }
    store.set_value(&key, "done")?;
    for entry in std::fs::read_dir(&directory)?.flatten() {
        let date = entry.file_name();
        if let Some(old) = date
            .to_str()
            .and_then(|s| s.strip_prefix("yank-"))
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<i64>().ok())
            && old < day - 6
        {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(deleted)
}

fn saved_paste_clip(store: &Store, id: &str) -> anyhow::Result<Clip> {
    let clip = store
        .get(id)?
        .ok_or_else(|| anyhow::anyhow!("记录已不存在，请重新选择"))?;
    anyhow::ensure!(!clip.deleted, "记录已删除，无法粘贴");
    Ok(clip)
}

#[cfg(not(target_os = "android"))]
fn paste() -> anyhow::Result<()> {
    use enigo::{Direction, Enigo, Key, Keyboard};
    #[cfg(target_os = "linux")]
    anyhow::ensure!(
        std::env::var_os("WAYLAND_DISPLAY").is_none(),
        "Wayland 当前无法确认全局输入授权；内容已复制，请按粘贴快捷键"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let active = active_win_pos_rs::get_active_window()
            .map_err(|_| anyhow::anyhow!("无法确认粘贴目标；内容已复制，请手动粘贴"))?;
        if active.process_id != u64::from(std::process::id()) && active.process_id != 0 {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "目标窗口尚未恢复焦点；内容已复制，请手动粘贴"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut enigo = Enigo::new(&enigo::Settings::default())?;
    let modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    enigo.key(modifier, Direction::Press)?;
    let result = enigo.key(Key::Unicode('v'), Direction::Click);
    let released = enigo.key(modifier, Direction::Release);
    result?;
    released?;
    Ok(())
}
#[cfg(target_os = "android")]
fn paste() -> anyhow::Result<()> {
    anyhow::bail!("Android 不允许模拟其他应用的粘贴操作")
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Bytes, extract::State, routing::post};
    use tokio::sync::Notify;
    use yank_core::{decode, encode, protocol as pb};

    #[test]
    fn thumbnails_are_bounded_and_do_not_replace_saved_payloads() {
        let image = image::RgbaImage::new(160, 80);
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let original = encoded.into_inner();
        let thumbnail = png_thumbnail(&original).unwrap();
        let decoded = image::load_from_memory(&thumbnail).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (48, 24));
        assert!(png_thumbnail(b"not a PNG").is_none());
        assert!(png_thumbnail(&vec![0; 8 * 1024 * 1024 + 1]).is_none());
        let store = Store::memory().unwrap();
        let mut clip = Clip::text("test", "image");
        clip.kind = "image".into();
        clip.formats = vec![ClipFormat {
            format: "PNG".into(),
            mime: Some("image/png".into()),
            data: original.clone(),
        }];
        let saved = store.capture(clip, "test").unwrap();
        let mut summaries = vec![saved.clone()];
        let mut cache = std::collections::HashMap::new();
        attach_thumbnails(&store, &mut summaries, &mut cache);
        assert_eq!(summaries[0].formats[0].data, thumbnail);
        assert_eq!(
            saved_paste_clip(&store, &saved.id).unwrap().formats[0].data,
            original
        );
    }
    #[test]
    fn paste_loads_current_complete_payload_and_rejects_tombstones() {
        let store = Store::memory().unwrap();
        let captured = store
            .capture(Clip::text("test", "initial"), "test")
            .unwrap();
        let updated = store
            .mutate(&captured.id, "test", |clip| {
                clip.text = "saved edit".into();
                clip.formats = vec![
                    ClipFormat::text("saved edit"),
                    ClipFormat {
                        format: "test/binary".into(),
                        mime: None,
                        data: vec![42; 16_384],
                    },
                ];
            })
            .unwrap();
        assert_eq!(saved_paste_clip(&store, &captured.id).unwrap(), updated);
        store
            .mutate(&captured.id, "test", |clip| clip.deleted = true)
            .unwrap();
        assert!(
            saved_paste_clip(&store, &captured.id).is_err(),
            "a queue ID must not resurrect deleted content"
        );
        assert!(saved_paste_clip(&store, "missing").is_err());
    }

    struct LoginGate {
        entered: Notify,
        release: Notify,
        revoked: AtomicBool,
    }
    async fn login(State(gate): State<Arc<LoginGate>>, body: Bytes) -> Vec<u8> {
        let credentials = decode::<pb::Credentials>(&body).unwrap();
        assert_eq!(credentials.username, "test");
        gate.entered.notify_one();
        gate.release.notified().await;
        encode(&pb::Login {
            token: "newly-issued-session".into(),
            user: Some(pb::User {
                id: "account".into(),
                username: "test".into(),
                admin: false,
            }),
            expires: yank_core::now_ts() + 3600,
        })
        .unwrap()
    }
    async fn logout(State(gate): State<Arc<LoginGate>>, body: Bytes) -> Vec<u8> {
        decode::<()>(&body).unwrap();
        gate.revoked.store(true, Ordering::SeqCst);
        encode(&()).unwrap()
    }
    #[tokio::test]
    async fn cancelled_login_cannot_restore_account_and_revokes_new_session() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::start(directory.path().into(), false).unwrap();
        let gate = Arc::new(LoginGate {
            entered: Notify::new(),
            release: Notify::new(),
            revoked: AtomicBool::new(false),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/rpc/login", post(login))
            .route("/rpc/logout", post(logout))
            .with_state(gate.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        worker
            .commands
            .send(Command::Login(
                url,
                "test".into(),
                "test-password".into(),
                0,
            ))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), gate.entered.notified())
            .await
            .unwrap();
        {
            let mut p = worker.shared.prefs.lock().unwrap();
            worker.shared.auth_revision.fetch_add(1, Ordering::SeqCst);
            p.detach_account();
        }
        worker.commands.send(Command::Logout).unwrap();
        gate.release.notify_one();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !gate.revoked.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(worker.shared.prefs.lock().unwrap().token.is_empty());
        assert!(
            Store::open(&worker.shared.db)
                .unwrap()
                .preferences()
                .unwrap()
                .token
                .is_empty()
        );
        drop(worker);
        server.abort();
    }
    #[test]
    fn stale_queued_settings_cannot_reenable_account_or_upload() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::start(directory.path().into(), false).unwrap();
        worker.shared.auth_revision.store(1, Ordering::SeqCst);
        let stale = Preferences {
            token: "revoked-session".into(),
            account_id: "old-account".into(),
            upload: true,
            ..Default::default()
        };
        worker.commands.send(Command::Save(stale, 0)).unwrap();
        worker
            .commands
            .send(Command::Search("processed".into(), "all".into(), 100))
            .unwrap();
        loop {
            let snapshot = worker
                .snapshots
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            if snapshot.query == "processed" {
                assert!(snapshot.prefs.token.is_empty());
                assert!(!snapshot.prefs.upload);
                break;
            }
        }
        assert!(worker.shared.prefs.lock().unwrap().token.is_empty());
        assert!(
            !Store::open(&worker.shared.db)
                .unwrap()
                .preferences()
                .unwrap()
                .upload
        );
    }
    #[test]
    fn network_changes_refresh_snapshot_without_a_database_change() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::start(directory.path().into(), false).unwrap();
        let initial = worker
            .snapshots
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        worker.shared.status("test relay connected");
        worker.shared.servers.lock().unwrap().insert(
            "test".into(),
            network::ServerInstance {
                name: "Test server".into(),
                url: "http://127.0.0.1:1".into(),
            },
        );
        loop {
            let snapshot = worker
                .snapshots
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            if snapshot.network_status == "test relay connected" {
                assert_eq!(snapshot.stats, initial.stats);
                assert_eq!(snapshot.clips, initial.clips);
                assert_eq!(snapshot.servers.len(), 1);
                assert_ne!(
                    snapshot, initial,
                    "network changes must invalidate UI snapshot equality"
                );
                break;
            }
        }
    }
}
