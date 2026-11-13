//! Encrypted peer transport, discovery and confirmation before trust.
use anyhow::Result;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use yank_core::{
    MAX_WIRE_BYTES, Page, Peer, PeerAddress, Preferences, Store, Wire, decode, encode,
    protocol as pb,
};
const ALPN: &[u8] = b"yank/clipboard/2";
const SERVICE: &str = "_yank._udp.local.";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Association {
    pub server_url: String,
    pub username: String,
}
pub struct Prompt {
    pub peer: Peer,
    pub account: Option<Association>,
    pub reply: Option<oneshot::Sender<Decision>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Decision {
    pub accept: bool,
    pub account: Option<Association>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Discovered {
    pub peer: Peer,
    pub via: String,
    pub seen: Instant,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ServerInstance {
    pub name: String,
    pub url: String,
}
#[derive(Clone, Debug)]
pub(crate) struct PeerPermit {
    id: String,
    generation: u64,
    changed: tokio::sync::watch::Receiver<u64>,
}
pub(crate) struct PeerAccess {
    generation: u64,
    trusted: bool,
    changed: tokio::sync::watch::Sender<u64>,
}
impl PeerAccess {
    fn new(trusted: bool) -> Self {
        let (changed, _) = tokio::sync::watch::channel(0);
        Self {
            generation: 0,
            trusted,
            changed,
        }
    }
    fn change(&mut self, trusted: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.trusted = trusted;
        self.changed.send_replace(self.generation);
    }
}
pub(crate) struct RemoteClipboard {
    pub clip: yank_core::Clip,
    pub permit: PeerPermit,
}
type PairRequest = (Peer, Option<Association>, u64);
pub struct Shared {
    pub pair_epoch: std::sync::atomic::AtomicU64,
    pub peer_access: Mutex<HashMap<String, PeerAccess>>,
    pub auth_revision: std::sync::atomic::AtomicU64,
    pub recovery_pending: AtomicBool,
    pub recovery_backup: Mutex<Option<PathBuf>>,
    pub collecting: AtomicBool,
    pub collected: Mutex<Vec<yank_core::Clip>>,
    pub busy: Mutex<Option<String>>,
    pub remote_clipboard: Mutex<Option<RemoteClipboard>>,
    pub servers: Mutex<HashMap<String, ServerInstance>>,
    pub db: PathBuf,
    pub prefs: Mutex<Preferences>,
    pub identity: Mutex<Option<Peer>>,
    pub discovered: Mutex<HashMap<String, Discovered>>,
    pub prompts: Mutex<Vec<Prompt>>,
    pub pair_requests: Mutex<Vec<PairRequest>>,
    pub association: Mutex<Option<Association>>,
    pub status: Mutex<String>,
    pub stop: AtomicBool,
}
impl Shared {
    pub fn revoke_peer(&self, id: &str) {
        if let Ok(mut peers) = self.peer_access.lock() {
            self.pair_epoch.fetch_add(1, Ordering::SeqCst);
            peers
                .entry(id.into())
                .or_insert_with(|| PeerAccess::new(false))
                .change(false);
        }
        if let Ok(mut requests) = self.pair_requests.lock() {
            requests.retain(|(peer, _, _)| peer.id != id);
        }
    }
    pub fn queue_pair(
        &self,
        peer: Peer,
        account: Option<Association>,
        expected_epoch: Option<u64>,
    ) -> Result<()> {
        let mut peers = self
            .peer_access
            .lock()
            .map_err(|_| anyhow::anyhow!("设备授权不可用"))?;
        anyhow::ensure!(
            expected_epoch.is_none_or(|epoch| epoch == self.pair_epoch.load(Ordering::SeqCst)),
            "配对请求已取消"
        );
        let revision = peers
            .entry(peer.id.clone())
            .or_insert_with(|| PeerAccess::new(false))
            .generation;
        let mut requests = self
            .pair_requests
            .lock()
            .map_err(|_| anyhow::anyhow!("配对服务不可用"))?;
        anyhow::ensure!(requests.len() < 32, "配对请求过多，请稍后重试");
        requests.push((peer, account, revision));
        Ok(())
    }
    fn pair_current(&self, id: &str, revision: u64) -> bool {
        self.peer_access.lock().is_ok_and(|peers| {
            peers
                .get(id)
                .is_some_and(|access| access.generation == revision)
        })
    }
    pub fn finish_unpair(&self, store: &Store, id: &str) -> Result<()> {
        let peers = self
            .peer_access
            .lock()
            .map_err(|_| anyhow::anyhow!("设备授权不可用"))?;
        // A newly confirmed pairing supersedes an older queued unpair command.
        if peers.get(id).is_none_or(|p| !p.trusted) {
            store.untrust(id)?;
        }
        Ok(())
    }
    fn pair_revision(&self, id: &str) -> Result<u64> {
        let trusted = Store::open(&self.db)?
            .peers()?
            .iter()
            .any(|peer| peer.id == id);
        let mut peers = self
            .peer_access
            .lock()
            .map_err(|_| anyhow::anyhow!("设备授权不可用"))?;
        Ok(peers
            .entry(id.into())
            .or_insert_with(|| PeerAccess::new(trusted))
            .generation)
    }
    fn trust_peer(&self, store: &Store, peer: &Peer, revision: u64) -> Result<()> {
        let mut peers = self
            .peer_access
            .lock()
            .map_err(|_| anyhow::anyhow!("设备授权不可用"))?;
        let access = peers
            .entry(peer.id.clone())
            .or_insert_with(|| PeerAccess::new(false));
        anyhow::ensure!(access.generation == revision, "配对已取消");
        store.trust(peer)?;
        access.change(true);
        Ok(())
    }
    fn peer_permit(&self, store: &Store, id: &str) -> Result<PeerPermit> {
        let mut peers = self
            .peer_access
            .lock()
            .map_err(|_| anyhow::anyhow!("设备授权不可用"))?;
        let stored = store.peers()?.iter().any(|p| p.id == id);
        let access = peers
            .entry(id.into())
            .or_insert_with(|| PeerAccess::new(stored));
        if !stored && access.trusted {
            access.change(false);
        }
        anyhow::ensure!(access.trusted && stored, "设备已解除绑定");
        Ok(PeerPermit {
            id: id.into(),
            generation: access.generation,
            changed: access.changed.subscribe(),
        })
    }
    pub fn with_peer<T>(
        &self,
        permit: &PeerPermit,
        apply: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let peers = self
            .peer_access
            .lock()
            .map_err(|_| anyhow::anyhow!("设备授权不可用"))?;
        anyhow::ensure!(
            peers
                .get(&permit.id)
                .is_some_and(|p| p.trusted && p.generation == permit.generation),
            "设备授权已失效"
        );
        apply()
    }
    pub fn status(&self, text: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            *s = text.into();
        }
    }
    /// Replay history without overwriting the OS clipboard; apply subsequent content changes.
    fn receive_page(
        &self,
        store: &Store,
        key: &str,
        page: &Page,
        permit: &PeerPermit,
    ) -> Result<()> {
        let ready = store.value(&format!("{key}.ready"))?.as_deref() == Some(&page.epoch);
        let candidate = if ready {
            page.clips
                .iter()
                .rev()
                .find(|clip| {
                    !clip.deleted
                        && store
                            .get(&clip.id)
                            .ok()
                            .flatten()
                            .is_none_or(|old| old.content_hash() != clip.content_hash())
                })
                .cloned()
        } else {
            None
        };
        store.apply_page(key, page)?;
        if !page.more {
            store.set_value(&format!("{key}.ready"), &page.epoch)?;
        }
        if self.prefs.lock().map(|p| p.apply_remote).unwrap_or(false)
            && let Some(clip) = candidate
            && store
                .get(&clip.id)?
                .is_some_and(|winner| !winner.deleted && winner.version_id == clip.version_id)
            && let Ok(mut pending) = self.remote_clipboard.lock()
        {
            *pending = Some(RemoteClipboard {
                clip,
                permit: permit.clone(),
            });
        }
        Ok(())
    }
    pub fn decide(&self, id: &str, decision: Decision) {
        if let Ok(mut prompts) = self.prompts.lock()
            && let Some(index) = prompts.iter().position(|p| p.peer.id == id)
        {
            let mut prompt = prompts.remove(index);
            if let Some(tx) = prompt.reply.take() {
                let _ = tx.send(decision);
            }
        }
    }
}
#[derive(Serialize, Deserialize)]
enum Request {
    Pair(Peer, Option<Association>),
    Exchange {
        page: Page,
        since: i64,
        epoch: String,
    },
}
#[derive(Serialize, Deserialize)]
enum Response {
    Paired(Decision),
    Exchanged(Page),
    Denied(String),
}

pub async fn run(shared: Arc<Shared>) -> Result<()> {
    let plugin_task = tokio::spawn(plugins(shared.clone()));
    let store = Store::open(&shared.db)?;
    let key = if let Some(key) = store.value("identity")? {
        SecretKey::from_bytes(
            &hex::decode(key)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid device identity"))?,
        )
    } else {
        let key = SecretKey::from_bytes(&rand::random());
        store.set_value("identity", &hex::encode(key.to_bytes()))?;
        key
    };
    while !shared.stop.load(Ordering::Relaxed) {
        let prefs = shared
            .prefs
            .lock()
            .map_err(|_| anyhow::anyhow!("Preferences unavailable"))?
            .clone();
        match session(shared.clone(), key.clone(), prefs).await {
            Ok(()) => {}
            Err(e) => {
                shared.status(format!("连接不可用：{e}"));
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    let _ = plugin_task.await;
    Ok(())
}
fn lan_daemon() -> Option<ServiceDaemon> {
    let daemon = ServiceDaemon::new().ok()?;
    // Container links multiply mDNS probes and are not the user's LAN. Keep ordinary
    // bridges and VPN interfaces available; do not infer a single default-route NIC.
    #[cfg(target_os = "linux")]
    if let Ok(interfaces) = std::fs::read_dir("/sys/class/net") {
        for interface in interfaces.flatten() {
            let name = interface.file_name().to_string_lossy().into_owned();
            if ["veth", "docker", "lzc-br-", "cni", "flannel", "virbr"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                let _ = daemon.disable_interface(mdns_sd::IfKind::Name(name));
            }
        }
    }
    Some(daemon)
}
struct Cadence {
    next: Instant,
    every: Duration,
}
impl Cadence {
    fn ready(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + self.every;
        true
    }
}
fn config_key(p: &Preferences) -> String {
    format!("{}|{}|{}|{}", p.mode, p.server_url, p.relay_url, p.token)
}
async fn session(shared: Arc<Shared>, key: SecretKey, prefs: Preferences) -> Result<()> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let mut relay_url = prefs.relay_url.clone();
    if relay_url.is_empty()
        && !prefs.server_url.is_empty()
        && let Ok(info) = binary_get::<pb::ServerInfo>(
            &http,
            &format!("{}/rpc/health", prefs.server_url.trim_end_matches('/')),
            " ",
        )
        .await
    {
        relay_url = info.relay_url;
    }
    let relay_mode = if prefs.mode == "direct" || relay_url.is_empty() {
        RelayMode::Disabled
    } else {
        RelayMode::Custom([relay_url.parse::<iroh::RelayUrl>()?].into_iter().collect())
    };
    let identity_key = key.clone();
    let mut builder = Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(relay_mode)
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()]);
    if prefs.mode == "relay" && !relay_url.is_empty() {
        builder = builder.clear_ip_transports();
    }
    let endpoint = builder.bind().await?;
    let mdns = lan_daemon();
    let events = mdns.as_ref().and_then(|m| m.browse(SERVICE).ok());
    let server_events = mdns
        .as_ref()
        .and_then(|m| m.browse("_yank-server._tcp.local.").ok());
    let mut registered = false;
    let now = Instant::now();
    let mut announcements = Cadence {
        next: now,
        every: Duration::from_secs(15),
    };
    let mut exchanges = Cadence {
        next: now,
        every: Duration::from_secs(3),
    };
    let mut incoming = tokio::task::JoinSet::new();
    let inbound_limit = Arc::new(tokio::sync::Semaphore::new(32));
    let mut jobs = tokio::task::JoinSet::new();
    let mut active = std::collections::HashSet::<String>::new();
    shared.status(if prefs.mode == "relay" && relay_url.is_empty() {
        "离线 · 未配置中继；本地剪贴板可用"
    } else {
        "设备已启动 · 等待连接"
    });
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            break;
        }
        let current = shared
            .prefs
            .lock()
            .map_err(|_| anyhow::anyhow!("Preferences unavailable"))?
            .clone();
        if config_key(&current) != config_key(&prefs) {
            break;
        }
        let peer = identity(&endpoint, &current.device_name)?;
        if let Ok(mut p) = shared.identity.lock() {
            *p = Some(peer.clone());
        }
        if announcements.ready(Instant::now()) {
            if let Some(m) = &mdns {
                if registered {
                    let _ = m.unregister(&format!("{}.{}", peer.code, SERVICE));
                }
                let addr = endpoint.addr();
                let port = addr.ip_addrs().next().map(|a| a.port()).unwrap_or(0);
                let mut props = HashMap::from([
                    ("id".to_owned(), peer.id.clone()),
                    ("code".into(), peer.code.clone()),
                    ("name".into(), peer.name.chars().take(50).collect()),
                ]);
                use base64::Engine;
                let raw = base64::engine::general_purpose::STANDARD.encode(encode(&peer)?);
                for (i, part) in raw.as_bytes().chunks(180).enumerate() {
                    props.insert(format!("a{i}"), String::from_utf8_lossy(part).into_owned());
                }
                if let Ok(info) = ServiceInfo::new(
                    SERVICE,
                    &peer.code,
                    &format!("yank-{}.local.", peer.code),
                    "",
                    port,
                    Some(props),
                ) {
                    let _ = m.register(info.enable_addr_auto());
                    registered = true;
                }
            }
            let state = shared.clone();
            let self_peer = peer.clone();
            let client = http.clone();
            let settings = current.clone();
            let signing_key = identity_key.clone();
            jobs.spawn(async move {
                let result =
                    server_discovery(&client, &settings, &self_peer, &state, &signing_key).await;
                if let Err(e) = result {
                    state.status(format!("服务器暂不可用：{e}"));
                }
                String::new()
            });
            if current.upload && active.insert("storage".into()) {
                let state = shared.clone();
                let prefs = current.clone();
                jobs.spawn(async move {
                    if let Err(e) = crate::sync::upload(state.clone(), prefs).await {
                        state.status(format!("服务器存储：{e}"));
                    }
                    "storage".into()
                });
            }
        }
        if let Some(events) = &events {
            while let Ok(event) = events.try_recv() {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let id = info.get_property_val_str("id").unwrap_or_default();
                    if id == peer.id {
                        continue;
                    }
                    let mut raw = String::new();
                    for i in 0..40 {
                        if let Some(part) = info.get_property_val_str(&format!("a{i}")) {
                            raw.push_str(part);
                        } else {
                            break;
                        }
                    }
                    use base64::Engine;
                    if let Ok(found) = base64::engine::general_purpose::STANDARD
                        .decode(&raw)
                        .ok()
                        .and_then(|bytes| decode::<Peer>(&bytes).ok())
                        .ok_or(())
                    {
                        if found.id != id || verified_id(&found).is_err() {
                            continue;
                        }
                        if let Ok(mut list) = shared.discovered.lock() {
                            list.insert(
                                found.id.clone(),
                                Discovered {
                                    peer: found,
                                    via: "局域网".into(),
                                    seen: Instant::now(),
                                },
                            );
                        }
                    }
                }
            }
        }
        if let Some(events) = &server_events {
            while let Ok(event) = events.try_recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let configured = info.get_property_val_str("url").unwrap_or_default();
                        let url = if configured.is_empty() {
                            info.get_addresses_v4()
                                .iter()
                                .next()
                                .map(|ip| format!("http://{ip}:{}", info.get_port()))
                                .unwrap_or_default()
                        } else {
                            configured.into()
                        };
                        if !url.is_empty()
                            && let Ok(mut servers) = shared.servers.lock()
                        {
                            servers.insert(
                                info.get_fullname().into(),
                                ServerInstance {
                                    name: info
                                        .get_property_val_str("name")
                                        .unwrap_or("Yank server")
                                        .into(),
                                    url,
                                },
                            );
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, name) => {
                        if let Ok(mut servers) = shared.servers.lock() {
                            servers.remove(&name);
                        }
                    }
                    _ => {}
                }
            }
        }
        if exchanges.ready(Instant::now()) {
            if let Ok(mut list) = shared.discovered.lock() {
                list.retain(|_, p| p.seen.elapsed() < Duration::from_secs(90));
            }
            let requests = shared
                .pair_requests
                .lock()
                .map(|mut r| std::mem::take(&mut *r))
                .unwrap_or_default();
            for (target, account, revision) in requests {
                if active.insert(target.id.clone()) {
                    let ep = endpoint.clone();
                    let state = shared.clone();
                    let me = peer.clone();
                    let mode = current.mode.clone();
                    jobs.spawn(async move {
                        let id = target.id.clone();
                        if let Err(e) =
                            pair(&ep, &mode, target, me, account, revision, &state).await
                        {
                            state.status(format!("配对失败：{e}"));
                        }
                        id
                    });
                }
            }
            let peers = Store::open(&shared.db)?.peers()?;
            for mut target in peers {
                if active.insert(target.id.clone()) {
                    if let Ok(list) = shared.discovered.lock()
                        && let Some(recent) = list.get(&target.id)
                    {
                        target.address = recent.peer.address.clone();
                    }
                    let ep = endpoint.clone();
                    let state = shared.clone();
                    let mode = current.mode.clone();
                    jobs.spawn(async move {
                        let id = target.id.clone();
                        if let Err(e) = exchange(&ep, &mode, &target, &state).await {
                            state.status(format!("{}：{e}", target.name));
                        }
                        id
                    });
                }
            }
        }
        while let Some(result) = jobs.try_join_next() {
            if let Ok(id) = result {
                active.remove(&id);
            }
        }
        while incoming.try_join_next().is_some() {}
        tokio::select! {
            value=endpoint.accept()=>{if let Some(value)=value{if let Ok(permit)=inbound_limit.clone().try_acquire_owned(){let state=shared.clone();incoming.spawn(async move{let _permit=permit;if let Ok(conn)=value.await{let _=tokio::time::timeout(Duration::from_secs(70),receive(conn,state)).await;}});}else{value.refuse();}}}
            _=tokio::time::sleep(Duration::from_millis(100))=>{}
        }
    }
    incoming.abort_all();
    jobs.abort_all();
    if let Some(m) = mdns {
        let _ = m.shutdown();
    }
    endpoint.close().await;
    Ok(())
}
pub fn device_code(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))[..12].to_uppercase()
}
fn identity(ep: &Endpoint, name: &str) -> Result<Peer> {
    let addr = ep.addr();
    Ok(Peer {
        id: addr.id.to_string(),
        name: name.into(),
        code: device_code(addr.id.as_bytes()),
        address: PeerAddress {
            relay_urls: addr.relay_urls().map(ToString::to_string).collect(),
            direct_addresses: addr.ip_addrs().map(ToString::to_string).collect(),
        },
    })
}
fn verified_id(peer: &Peer) -> Result<iroh::PublicKey> {
    let id: iroh::PublicKey = peer.id.parse()?;
    anyhow::ensure!(
        peer.code == device_code(id.as_bytes()),
        "设备码与设备身份不匹配"
    );
    Ok(id)
}
fn address(peer: &Peer, mode: &str) -> Result<EndpointAddr> {
    let mut addr = EndpointAddr::new(verified_id(peer)?);
    for ip in &peer.address.direct_addresses {
        addr = addr.with_ip_addr(ip.parse()?);
    }
    if mode != "direct" {
        for relay in &peer.address.relay_urls {
            addr = addr.with_relay_url(relay.parse()?);
        }
    }
    Ok(addr)
}
async fn request(
    ep: &Endpoint,
    mode: &str,
    peer: &Peer,
    request: Request,
) -> Result<(Response, String)> {
    let conn = tokio::time::timeout(
        Duration::from_secs(12),
        ep.connect(address(peer, mode)?, ALPN),
    )
    .await??;
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(&encode(&request)?).await?;
    send.finish()?;
    let raw = tokio::time::timeout(
        Duration::from_secs(65),
        recv.read_to_end(MAX_WIRE_BYTES + 9),
    )
    .await??;
    let result = decode(&raw)?;
    let path = conn
        .paths()
        .iter()
        .find(|p| p.is_selected())
        .map(|p| {
            match p.remote_addr() {
                iroh::TransportAddr::Ip(_) => "直连",
                iroh::TransportAddr::Relay(_) => "加密中继",
                _ => "自定义链路",
            }
            .to_owned()
        })
        .unwrap_or_else(|| "正在确认链路".into());
    conn.close(0u32.into(), b"done");
    Ok((result, path))
}
async fn pair(
    ep: &Endpoint,
    mode: &str,
    peer: Peer,
    me: Peer,
    account: Option<Association>,
    revision: u64,
    state: &Shared,
) -> Result<()> {
    anyhow::ensure!(state.pair_current(&peer.id, revision), "配对请求已取消");
    match request(ep, mode, &peer, Request::Pair(me, account))
        .await?
        .0
    {
        Response::Paired(decision) if decision.accept => {
            state.trust_peer(&Store::open(&state.db)?, &peer, revision)?;
            if let Some(account) = decision.account
                && let Ok(mut target) = state.association.lock()
            {
                *target = Some(account);
            }
            state.status(format!("已与 {} 配对", peer.name));
            Ok(())
        }
        _ => anyhow::bail!("对方未同意配对"),
    }
}
async fn exchange(ep: &Endpoint, mode: &str, peer: &Peer, state: &Shared) -> Result<()> {
    let store = Store::open(&state.db)?;
    let permit = state.peer_permit(&store, &peer.id)?;
    let (page, remote_epoch, since) = state.with_peer(&permit, || {
        let sent = store
            .value(&format!("peer.sent.{}", peer.id))?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let epoch = store
            .value(&format!("peer.epoch.{}", peer.id))?
            .unwrap_or_default();
        let since = store.cursor(&format!("peer.recv.{}", peer.id), &epoch)?;
        Ok((store.page(sent, 20)?, epoch, since))
    })?;
    let next = page.cursor;
    let mut cancelled = permit.changed.clone();
    let (response, path) = tokio::select! {
        biased;
        _=cancelled.wait_for(|generation|*generation!=permit.generation)=>anyhow::bail!("设备授权已失效"),
        response=request(ep,mode,peer,Request::Exchange{page,since,epoch:remote_epoch.clone()})=>response?,
    };
    match response {
        Response::Exchanged(page) => state.with_peer(&permit, || {
            state.receive_page(&store, &format!("peer.recv.{}", peer.id), &page, &permit)?;
            store.set_value(&format!("peer.epoch.{}", peer.id), &page.epoch)?;
            store.set_value(
                &format!("peer.sent.{}", peer.id),
                &if page.epoch == remote_epoch { next } else { 0 }.to_string(),
            )?;
            state.status(format!("{} · {}", peer.name, path));
            Ok(())
        }),
        Response::Denied(message) => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected peer response"),
    }
}
async fn receive(conn: iroh::endpoint::Connection, state: Arc<Shared>) -> Result<()> {
    let id = conn.remote_id().to_string();
    let (mut send, mut recv) = conn.accept_bi().await?;
    let raw = recv.read_to_end(MAX_WIRE_BYTES + 9).await?;
    let message: Request = decode(&raw)?;
    let mut response_permit = None;
    let response = match message {
        Request::Pair(peer, account) => {
            anyhow::ensure!(
                peer.id == id && peer.code == device_code(conn.remote_id().as_bytes()),
                "Unverified pairing identity"
            );
            let revision = state.pair_revision(&id)?;
            let already = state.peer_permit(&Store::open(&state.db)?, &id).is_ok();
            if already {
                Response::Paired(Decision {
                    accept: true,
                    account: None,
                })
            } else {
                let (tx, rx) = oneshot::channel();
                {
                    let mut prompts = state
                        .prompts
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Pairing unavailable"))?;
                    anyhow::ensure!(
                        prompts.len() < 8 && !prompts.iter().any(|p| p.peer.id == id),
                        "Pairing queue full"
                    );
                    prompts.push(Prompt {
                        peer: peer.clone(),
                        account,
                        reply: Some(tx),
                    });
                }
                let decision = tokio::time::timeout(Duration::from_secs(60), rx)
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_default();
                if let Ok(mut prompts) = state.prompts.lock() {
                    prompts.retain(|p| p.peer.id != id);
                }
                if decision.accept {
                    state.trust_peer(&Store::open(&state.db)?, &peer, revision)?;
                }
                Response::Paired(decision)
            }
        }
        Request::Exchange { page, since, epoch } => {
            let store = Store::open(&state.db)?;
            if let Ok(permit) = state.peer_permit(&store, &id) {
                let response = state.with_peer(&permit, || {
                    state.receive_page(&store, &format!("peer.recv.{id}"), &page, &permit)?;
                    let own_epoch = store.value("epoch")?.unwrap_or_default();
                    Ok(Response::Exchanged(
                        store.page(if own_epoch == epoch { since } else { 0 }, 20)?,
                    ))
                })?;
                response_permit = Some(permit);
                response
            } else {
                Response::Denied("请先确认设备配对".into())
            }
        }
    };
    let payload = encode(&response)?;
    if let Some(permit) = response_permit {
        let mut changed = permit.changed.clone();
        tokio::select! {
            biased;
            _=changed.wait_for(|generation|*generation!=permit.generation)=>anyhow::bail!("设备授权已失效"),
            result=send.write_all(&payload)=>result?,
        }
    } else {
        send.write_all(&payload).await?;
    }
    send.finish()?;
    let _ = send.stopped().await;
    Ok(())
}
async fn server_discovery(
    http: &reqwest::Client,
    prefs: &Preferences,
    me: &Peer,
    state: &Shared,
    key: &SecretKey,
) -> Result<()> {
    if prefs.server_url.is_empty() {
        return Ok(());
    }
    let root = prefs.server_url.trim_end_matches('/');
    let timestamp = yank_core::now_ts();
    let mut proof = encode(me)?;
    proof.extend_from_slice(&timestamp.to_le_bytes());
    let announcement = pb::Announcement {
        peer: Some(me.to_proto()),
        timestamp,
        signature: key.sign(&proof).to_bytes().to_vec(),
    };
    binary_post::<_, ()>(
        http,
        &format!("{root}/rpc/register"),
        &prefs.token,
        &announcement,
    )
    .await?;
    if prefs.token.is_empty() {
        return Ok(());
    }
    let peers: Vec<Peer> = binary_get(http, &format!("{root}/rpc/devices"), &prefs.token).await?;
    if let Ok(mut list) = state.discovered.lock() {
        for peer in peers {
            if peer.id != me.id {
                list.insert(
                    peer.id.clone(),
                    Discovered {
                        peer,
                        via: "同账号".into(),
                        seen: Instant::now(),
                    },
                );
            }
        }
    }
    Ok(())
}
pub async fn binary_get<T: Wire>(http: &reqwest::Client, url: &str, token: &str) -> Result<T> {
    let response = http
        .get(url)
        .bearer_auth(token)
        .header("accept", "application/x-yank")
        .send()
        .await?
        .error_for_status()?;
    let bytes = bounded(response).await?;
    Ok(decode(&bytes)?)
}
pub async fn binary_post<T: Wire, R: Wire>(
    http: &reqwest::Client,
    url: &str,
    token: &str,
    value: &T,
) -> Result<R> {
    let response = http
        .post(url)
        .bearer_auth(token)
        .header("content-type", "application/x-yank")
        .body(encode(value)?)
        .send()
        .await?
        .error_for_status()?;
    let bytes = bounded(response).await?;
    if bytes.is_empty() {
        return Ok(decode(&encode(&())?)?);
    }
    Ok(decode(&bytes)?)
}

impl Wire for Request {
    type Proto = pb::PeerRequest;
    const KIND: pb::MessageKind = pb::MessageKind::PeerRequest;
    fn to_proto(&self) -> Self::Proto {
        use pb::peer_request::Request as P;
        pb::PeerRequest {
            request: Some(match self {
                Self::Pair(peer, account) => P::Pair(pb::PairRequest {
                    peer: Some(peer.to_proto()),
                    account: account.as_ref().map(|a| pb::AccountHint {
                        server_url: a.server_url.clone(),
                        username: a.username.clone(),
                    }),
                }),
                Self::Exchange { page, since, epoch } => P::Exchange(pb::SyncExchange {
                    page: Some(page.to_proto()),
                    since: *since,
                    epoch: epoch.clone(),
                }),
            }),
        }
    }
    fn from_proto(p: Self::Proto) -> yank_core::Result<Self> {
        use pb::peer_request::Request as P;
        match p.request {
            Some(P::Pair(p)) => Ok(Self::Pair(
                Peer::from_proto(
                    p.peer
                        .ok_or_else(|| yank_core::Error::Invalid("Missing peer".into()))?,
                )?,
                p.account.map(|a| Association {
                    server_url: a.server_url,
                    username: a.username,
                }),
            )),
            Some(P::Exchange(e)) => Ok(Self::Exchange {
                page: Page::from_proto(
                    e.page
                        .ok_or_else(|| yank_core::Error::Invalid("Missing page".into()))?,
                )?,
                since: e.since,
                epoch: e.epoch,
            }),
            _ => Err(yank_core::Error::Invalid("Missing peer request".into())),
        }
    }
}
impl Wire for Response {
    type Proto = pb::PeerResponse;
    const KIND: pb::MessageKind = pb::MessageKind::PeerResponse;
    fn to_proto(&self) -> Self::Proto {
        use pb::peer_response::Response as P;
        pb::PeerResponse {
            response: Some(match self {
                Self::Paired(d) => P::Paired(pb::PairDecision {
                    accept: d.accept,
                    account: d.account.as_ref().map(|a| pb::AccountHint {
                        server_url: a.server_url.clone(),
                        username: a.username.clone(),
                    }),
                }),
                Self::Exchanged(page) => P::Exchanged(page.to_proto()),
                Self::Denied(m) => P::Denied(m.clone()),
            }),
        }
    }
    fn from_proto(p: Self::Proto) -> yank_core::Result<Self> {
        use pb::peer_response::Response as P;
        match p.response {
            Some(P::Paired(d)) => Ok(Self::Paired(Decision {
                accept: d.accept,
                account: d.account.map(|a| Association {
                    server_url: a.server_url,
                    username: a.username,
                }),
            })),
            Some(P::Exchanged(page)) => Ok(Self::Exchanged(Page::from_proto(page)?)),
            Some(P::Denied(m)) => Ok(Self::Denied(m)),
            _ => Err(yank_core::Error::Invalid("Missing peer response".into())),
        }
    }
}

async fn plugins(shared: Arc<Shared>) -> Result<()> {
    use rutis::host::*;
    let root = rutis::Ctx::root().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let db = shared.db.clone();
    root.provide(yank_ditto::ClipSink(Arc::new(move |clip| {
        if let Ok(store) = Store::open(&db) {
            let device = store
                .value("device")
                .ok()
                .flatten()
                .unwrap_or_else(|| "ditto-adapter".into());
            let _ = store.capture(clip, &device);
        }
    })))
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    while !shared.stop.load(Ordering::Relaxed) {
        let prefs = shared
            .prefs
            .lock()
            .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
            .clone();
        if !prefs.ditto_enabled {
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }
        let config = (prefs.ditto_listen.clone(), prefs.ditto_password.clone());
        let listen = match prefs.ditto_listen.parse() {
            Ok(value) => value,
            Err(e) => {
                shared.status(format!("Ditto 监听地址无效：{e}"));
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        let plugin = root.plugin(yank_ditto::DittoPlugin {
            listen,
            password: prefs.ditto_password,
        });
        if let Err(e) = (&plugin).await {
            shared.status(format!("Ditto 插件：{e}"));
        }
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let changed = shared
                .prefs
                .lock()
                .map(|p| {
                    !p.ditto_enabled || (p.ditto_listen.clone(), p.ditto_password.clone()) != config
                })
                .unwrap_or(true);
            if changed || shared.stop.load(Ordering::Relaxed) {
                break;
            }
        }
        plugin
            .dispose()
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }
    root.shutdown()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}

async fn bounded(mut response: reqwest::Response) -> Result<Vec<u8>> {
    anyhow::ensure!(
        response.content_length().unwrap_or(0) <= MAX_WIRE_BYTES as u64,
        "Server message too large"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= MAX_WIRE_BYTES,
            "Server message too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(path: PathBuf) -> Arc<Shared> {
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
            db: path,
            prefs: Mutex::new(Preferences {
                mode: "direct".into(),
                ..Default::default()
            }),
            identity: Mutex::new(None),
            discovered: Mutex::default(),
            prompts: Mutex::default(),
            pair_requests: Mutex::default(),
            association: Mutex::default(),
            status: Mutex::default(),
            stop: AtomicBool::new(false),
        })
    }
    #[test]
    fn replay_does_not_replace_clipboard_and_live_changes_obey_preference() {
        let temp = tempfile::tempdir().unwrap();
        let state = state(temp.path().join("receiver.sqlite3"));
        let source = Store::memory().unwrap();
        let peer = Peer {
            id: "test-source".into(),
            name: "test".into(),
            code: "test".into(),
            address: PeerAddress::default(),
        };
        Store::open(&state.db).unwrap().trust(&peer).unwrap();
        let permit = state
            .peer_permit(&Store::open(&state.db).unwrap(), &peer.id)
            .unwrap();
        let target = Store::open(&state.db).unwrap();
        let first = source
            .save_local(yank_core::Clip::text("a", "history"), "a")
            .unwrap();
        let page = source.page(0, 20).unwrap();
        state
            .receive_page(&target, "peer.test", &page, &permit)
            .unwrap();
        assert!(state.remote_clipboard.lock().unwrap().is_none());
        let next = source
            .save_local(yank_core::Clip::text("a", "live"), "a")
            .unwrap();
        let live = source.page(page.cursor, 20).unwrap();
        state
            .receive_page(&target, "peer.test", &live, &permit)
            .unwrap();
        assert_eq!(
            state
                .remote_clipboard
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .clip
                .id,
            next.id
        );
        state
            .receive_page(&target, "peer.test", &live, &permit)
            .unwrap();
        assert!(
            state.remote_clipboard.lock().unwrap().is_none(),
            "retries cannot overwrite clipboard"
        );
        state.prefs.lock().unwrap().apply_remote = false;
        source
            .mutate(&first.id, "a", |c| c.text = "changed".into())
            .unwrap();
        let disabled = source.page(live.cursor, 20).unwrap();
        state
            .receive_page(&target, "peer.test", &disabled, &permit)
            .unwrap();
        assert!(state.remote_clipboard.lock().unwrap().is_none());
        assert_eq!(target.get(&first.id).unwrap().unwrap().text, "changed");
        let reset = Store::memory().unwrap();
        reset
            .save_local(yank_core::Clip::text("a", "reinstalled history"), "a")
            .unwrap();
        state.prefs.lock().unwrap().apply_remote = true;
        state
            .receive_page(&target, "peer.test", &reset.page(0, 20).unwrap(), &permit)
            .unwrap();
        assert!(
            state.remote_clipboard.lock().unwrap().is_none(),
            "new epochs must replay silently"
        );
    }
    async fn endpoint() -> Endpoint {
        Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .alpns(vec![ALPN.to_vec()])
            .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .unwrap()
            .bind()
            .await
            .unwrap()
    }
    #[test]
    fn periodic_work_is_independent_of_incoming_connection_frequency() {
        let start = Instant::now();
        let mut busy = Cadence {
            next: start,
            every: Duration::from_secs(3),
        };
        let mut idle = Cadence {
            next: start,
            every: Duration::from_secs(3),
        };
        let high_traffic = (0..=6000)
            .filter(|ms| busy.ready(start + Duration::from_millis(*ms)))
            .count();
        let low_traffic = [0, 100, 2999, 3000, 6000]
            .into_iter()
            .filter(|ms| idle.ready(start + Duration::from_millis(*ms)))
            .count();
        assert_eq!(
            high_traffic, 3,
            "incoming accepts cannot repeatedly schedule the same heartbeat or stall time"
        );
        assert_eq!(high_traffic, low_traffic);
    }
    #[tokio::test]
    async fn drained_pair_intent_cannot_regrant_trust_after_unpair() {
        let temp = tempfile::tempdir().unwrap();
        let a = endpoint().await;
        let b = endpoint().await;
        let state = state(temp.path().join("a.sqlite3"));
        let store = Store::open(&state.db).unwrap();
        let me = identity(&a, "a").unwrap();
        let peer = identity(&b, "b").unwrap();
        store.trust(&peer).unwrap();
        let code_epoch = state.pair_epoch.load(Ordering::SeqCst);
        state.queue_pair(peer.clone(), None, None).unwrap();
        let (queued, account, revision) = state.pair_requests.lock().unwrap().pop().unwrap();
        state.revoke_peer(&peer.id);
        state.finish_unpair(&store, &peer.id).unwrap();
        assert!(
            pair(&a, "direct", queued, me, account, revision, &state)
                .await
                .is_err()
        );
        assert!(store.peers().unwrap().is_empty());
        assert!(
            state
                .queue_pair(peer.clone(), None, Some(code_epoch))
                .is_err(),
            "slow code lookup cannot renew cancelled intent"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), b.accept())
                .await
                .is_err()
        );
        state.queue_pair(peer, None, None).unwrap();
        let (peer, _, revision) = state.pair_requests.lock().unwrap().pop().unwrap();
        assert!(
            state.pair_current(&peer.id, revision),
            "a new explicit user confirmation remains possible"
        );
        a.close().await;
        b.close().await;
    }
    #[tokio::test]
    async fn queued_exchange_cannot_read_or_send_new_content_after_unpair() {
        let temp = tempfile::tempdir().unwrap();
        let a = endpoint().await;
        let b = endpoint().await;
        let sa = state(temp.path().join("a.sqlite3"));
        let sb = state(temp.path().join("b.sqlite3"));
        let me = identity(&a, "a").unwrap();
        let peer = identity(&b, "b").unwrap();
        let local = Store::open(&sa.db).unwrap();
        local.trust(&peer).unwrap();
        Store::open(&sb.db).unwrap().trust(&me).unwrap();
        let (release, wait) = oneshot::channel();
        let queued = sa.clone();
        let ep = a.clone();
        let target = peer.clone();
        let job = tokio::spawn(async move {
            wait.await.unwrap();
            exchange(&ep, "direct", &target, &queued).await
        });
        sa.revoke_peer(&peer.id);
        sa.finish_unpair(&local, &peer.id).unwrap();
        let private = local
            .save_local(yank_core::Clip::text("a", "created after revoke"), "a")
            .unwrap();
        release.send(()).unwrap();
        assert!(job.await.unwrap().is_err());
        assert!(
            Store::open(&sb.db)
                .unwrap()
                .get(&private.id)
                .unwrap()
                .is_none()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), b.accept())
                .await
                .is_err(),
            "revoked queued job must not connect"
        );
        a.close().await;
        b.close().await;
    }
    #[tokio::test]
    async fn revoking_an_inflight_exchange_cancels_wait_and_rejects_its_response() {
        let temp = tempfile::tempdir().unwrap();
        let a = endpoint().await;
        let b = endpoint().await;
        let sa = state(temp.path().join("a.sqlite3"));
        let local = Store::open(&sa.db).unwrap();
        let peer = identity(&b, "b").unwrap();
        local.trust(&peer).unwrap();
        local
            .save_local(yank_core::Clip::text("a", "before revoke"), "a")
            .unwrap();
        let remote = Store::memory().unwrap();
        let incoming = remote
            .save_local(yank_core::Clip::text("b", "must not be applied"), "b")
            .unwrap();
        let page = remote.page(0, 20).unwrap();
        let (seen, arrived) = oneshot::channel();
        let (release, wait) = oneshot::channel();
        let ep = b.clone();
        let server = tokio::spawn(async move {
            let conn = ep.accept().await.unwrap().await.unwrap();
            let (mut send, mut recv) = conn.accept_bi().await.unwrap();
            let request: Request =
                decode(&recv.read_to_end(MAX_WIRE_BYTES + 9).await.unwrap()).unwrap();
            assert!(matches!(request, Request::Exchange { .. }));
            seen.send(()).unwrap();
            wait.await.unwrap();
            let _ = send
                .write_all(&encode(&Response::Exchanged(page)).unwrap())
                .await;
            let _ = send.finish();
        });
        let ep = a.clone();
        let state = sa.clone();
        let target = peer.clone();
        let job = tokio::spawn(async move { exchange(&ep, "direct", &target, &state).await });
        tokio::time::timeout(Duration::from_secs(5), arrived)
            .await
            .unwrap()
            .unwrap();
        sa.revoke_peer(&peer.id);
        sa.finish_unpair(&local, &peer.id).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), job)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        release.send(()).unwrap();
        server.await.unwrap();
        assert!(local.get(&incoming.id).unwrap().is_none());
        a.close().await;
        b.close().await;
    }
    #[test]
    fn queued_clipboard_value_cannot_be_applied_after_source_revocation() {
        let temp = tempfile::tempdir().unwrap();
        let state = state(temp.path().join("receiver.sqlite3"));
        let target = Store::open(&state.db).unwrap();
        let peer = Peer {
            id: "source".into(),
            name: "source".into(),
            code: "source".into(),
            address: PeerAddress::default(),
        };
        target.trust(&peer).unwrap();
        let permit = state.peer_permit(&target, &peer.id).unwrap();
        let source = Store::memory().unwrap();
        source
            .save_local(yank_core::Clip::text("a", "history"), "a")
            .unwrap();
        let first = source.page(0, 20).unwrap();
        state
            .with_peer(&permit, || {
                state.receive_page(&target, "peer.test", &first, &permit)
            })
            .unwrap();
        source
            .save_local(yank_core::Clip::text("a", "live"), "a")
            .unwrap();
        let live = source.page(first.cursor, 20).unwrap();
        state
            .with_peer(&permit, || {
                state.receive_page(&target, "peer.test", &live, &permit)
            })
            .unwrap();
        let queued = state.remote_clipboard.lock().unwrap().take().unwrap();
        state.revoke_peer(&peer.id);
        state.finish_unpair(&target, &peer.id).unwrap();
        let mut called = false;
        assert!(
            state
                .with_peer(&queued.permit, || {
                    called = true;
                    Ok(())
                })
                .is_err()
        );
        assert!(!called);
    }
    #[tokio::test]
    async fn direct_pairing_requires_confirmation_and_rejects_untrusted_exchange() {
        let temp = tempfile::tempdir().unwrap();
        let a = endpoint().await;
        let b = endpoint().await;
        let sa = state(temp.path().join("a.sqlite3"));
        let sb = state(temp.path().join("b.sqlite3"));
        let store_a = Store::open(&sa.db).unwrap();
        let store_b = Store::open(&sb.db).unwrap();
        let mut secret = yank_core::Clip::text("a", "private before pairing");
        secret.clock.insert("a".into(), 1);
        store_a.save_local(secret, "a").unwrap();
        let remote = identity(&b, "Device B").unwrap();
        let me = identity(&a, "Device A").unwrap();
        let receiver = b.clone();
        let state_b = sb.clone();
        let accepting = tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            while let Some(incoming) = receiver.accept().await {
                let state = state_b.clone();
                tasks.spawn(async move {
                    if let Ok(conn) = incoming.await {
                        let _ = receive(conn, state).await;
                    }
                });
            }
        });
        let denied = request(
            &a,
            "direct",
            &remote,
            Request::Exchange {
                page: store_a.page(0, 20).unwrap(),
                since: 0,
                epoch: String::new(),
            },
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(denied, Response::Denied(_)));
        assert_eq!(store_b.stats().unwrap().clips, 0);
        let client = a.clone();
        let dest = remote.clone();
        let local = me.clone();
        let state_a = sa.clone();
        let revision = state_a.pair_revision(&dest.id).unwrap();
        let pairing = tokio::spawn(async move {
            pair(&client, "direct", dest, local, None, revision, &state_a).await
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if !sb.prompts.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(store_b.peers().unwrap().is_empty());
        assert_eq!(store_b.stats().unwrap().clips, 0);
        sb.decide(
            &me.id,
            Decision {
                accept: true,
                account: None,
            },
        );
        pairing.await.unwrap().unwrap();
        assert_eq!(store_a.peers().unwrap().len(), 1);
        assert_eq!(store_b.peers().unwrap().len(), 1);
        exchange(&a, "direct", &remote, &sa).await.unwrap();
        assert_eq!(store_b.stats().unwrap().clips, 1);
        store_b.untrust(&me.id).unwrap();
        let rejected = exchange(&a, "direct", &remote, &sa).await;
        assert!(rejected.is_err());
        accepting.abort();
        a.close().await;
        b.close().await;
    }
    #[tokio::test]
    async fn relay_only_mode_exchanges_encrypted_protobuf_without_ip_paths() {
        use iroh_relay::server::{RelayConfig, Server, ServerConfig};
        let mut config = ServerConfig::default();
        config.relay = Some(RelayConfig::new(
            "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let relay = Server::spawn(config).await.unwrap();
        let url: iroh::RelayUrl = format!("http://{}", relay.http_addr().unwrap())
            .parse()
            .unwrap();
        let map: iroh::RelayMap = [url.clone()].into_iter().collect();
        let a = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(RelayMode::Custom(map.clone()))
            .clear_ip_transports()
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let b = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(RelayMode::Custom(map))
            .clear_ip_transports()
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let sa = state(temp.path().join("a.sqlite3"));
        let sb = state(temp.path().join("b.sqlite3"));
        let mut me = identity(&a, "a").unwrap();
        let mut remote = identity(&b, "b").unwrap();
        me.address.relay_urls = vec![url.to_string()];
        remote.address.relay_urls = vec![url.to_string()];
        Store::open(&sa.db).unwrap().trust(&remote).unwrap();
        Store::open(&sb.db).unwrap().trust(&me).unwrap();
        Store::open(&sa.db)
            .unwrap()
            .save_local(yank_core::Clip::text("a", "relay payload"), "a")
            .unwrap();
        let server = b.clone();
        let receiving = sb.clone();
        let task = tokio::spawn(async move {
            let incoming = server.accept().await.unwrap().await.unwrap();
            receive(incoming, receiving).await.unwrap();
        });
        tokio::time::timeout(Duration::from_secs(20), exchange(&a, "relay", &remote, &sa))
            .await
            .unwrap()
            .unwrap();
        task.await.unwrap();
        assert_eq!(
            Store::open(&sb.db).unwrap().list("", "all", 10, 0).unwrap()[0].text,
            "relay payload"
        );
        assert!(sa.status.lock().unwrap().contains("加密中继"));
        assert!(a.addr().ip_addrs().next().is_none());
        a.close().await;
        b.close().await;
        drop(relay);
    }
}
