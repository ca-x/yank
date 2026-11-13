//! Optional storage, authenticated rendezvous and encrypted packet relay.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{delete, get, post},
};
use clap::Parser;
use iroh_relay::server::{
    Access, AccessControl, ClientRequest, RelayConfig, Server as RelayServer, ServerConfig,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use yank_core::{
    Clip, MAX_WIRE_BYTES, Mutation, Peer, Store, Wire, decode, encode, protocol as pb,
};
mod auth;
use auth::User;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, env = "YANK_BIND", default_value = "127.0.0.1:7219")]
    bind: SocketAddr,
    #[arg(long, env = "YANK_DB", default_value = "yank-storage-v2.sqlite3")]
    db: PathBuf,
    #[arg(long, env = "YANK_ADMIN_USERNAME")]
    admin_username: Option<String>,
    #[arg(long, env = "YANK_ADMIN_PASSWORD")]
    admin_password: Option<String>,
    #[arg(long, env = "YANK_COOKIE_SECURE", default_value_t = false)]
    cookie_secure: bool,
    #[arg(long, env = "YANK_RELAY_BIND", default_value = "0.0.0.0:7220")]
    relay_bind: SocketAddr,
    #[arg(long, env = "YANK_RELAY_URL", default_value = "http://127.0.0.1:7220")]
    relay_url: String,
    #[arg(long, env = "YANK_PUBLIC_URL")]
    public_url: Option<String>,
    #[arg(long, env = "YANK_TLS_CERT")]
    tls_cert: Option<PathBuf>,
    #[arg(long, env = "YANK_TLS_KEY")]
    tls_key: Option<PathBuf>,
    #[arg(long, env = "YANK_QUIC_BIND", default_value = "0.0.0.0:7842")]
    quic_bind: SocketAddr,
    #[arg(long, env = "YANK_STORAGE_DISABLED", default_value_t = false)]
    storage_disabled: bool,
}
type Devices = Arc<Mutex<HashMap<String, (String, Peer, Instant)>>>;
#[derive(Debug)]
struct DeviceAccess(Devices);
impl AccessControl for DeviceAccess {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        let allowed = self
            .0
            .lock()
            .map(|d| {
                d.values().any(|(_, p, t)| {
                    p.id == request.endpoint_id().to_string()
                        && t.elapsed() < Duration::from_secs(90)
                })
            })
            .unwrap_or(false);
        if allowed {
            Access::Allow
        } else {
            Access::Deny { reason: None }
        }
    }
}
#[derive(Clone)]
struct AppState {
    accounts: Arc<Mutex<auth::Accounts>>,
    data: Arc<PathBuf>,
    devices: Devices,
    relay_url: Arc<str>,
    storage: bool,
    secure: bool,
    attempts: Arc<Mutex<HashMap<SocketAddr, (u32, Instant)>>>,
}
impl AppState {
    fn library<T>(
        &self,
        user: &User,
        run: impl FnOnce(&Store) -> yank_core::Result<T>,
    ) -> ApiResult<T> {
        storage(self)?;
        let store = Store::open(self.data.join(format!("{}.sqlite3", user.id)))?;
        Ok(run(&store)?)
    }
}
#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    q: String,
    #[serde(default = "all")]
    filter: String,
    #[serde(default)]
    offset: u32,
}
fn all() -> String {
    "all".into()
}
#[derive(Debug, Deserialize)]
struct PullQuery {
    #[serde(default)]
    since: i64,
}
#[derive(Debug, Serialize)]
struct Health {
    name: &'static str,
    version: &'static str,
    relay_url: String,
    storage: bool,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let accounts = auth::Accounts::open(&args.db)?;
    accounts.bootstrap(
        args.admin_username.as_deref(),
        args.admin_password.as_deref(),
    )?;
    let data = args.db.with_extension("users");
    std::fs::create_dir_all(&data)?;
    let state = AppState {
        accounts: Arc::new(Mutex::new(accounts)),
        data: Arc::new(data),
        devices: Arc::default(),
        relay_url: args.relay_url.into(),
        storage: !args.storage_disabled,
        secure: args.cookie_secure,
        attempts: Arc::default(),
    };
    let mdns = mdns_sd::ServiceDaemon::new().ok();
    if let Some(mdns) = &mdns {
        #[cfg(target_os = "linux")]
        if let Ok(interfaces) = std::fs::read_dir("/sys/class/net") {
            for interface in interfaces.flatten() {
                let name = interface.file_name().to_string_lossy().into_owned();
                if ["veth", "docker", "lzc-br-", "cni", "flannel", "virbr"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                {
                    let _ = mdns.disable_interface(mdns_sd::IfKind::Name(name));
                }
            }
        }
        let id = format!("yank-{}", args.bind.port());
        let properties = [
            ("url", args.public_url.as_deref().unwrap_or("")),
            ("name", "Yank server"),
        ];
        if let Ok(info) = mdns_sd::ServiceInfo::new(
            "_yank-server._tcp.local.",
            &id,
            &format!("{id}.local."),
            "",
            args.bind.port(),
            &properties[..],
        ) {
            let _ = mdns.register(info.enable_addr_auto());
        }
    }
    let quic = match (args.tls_cert, args.tls_key) {
        (Some(cert), Some(key)) => {
            let certs =
                rustls_pemfile::certs(&mut std::io::BufReader::new(std::fs::File::open(cert)?))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
            let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(
                std::fs::File::open(key)?,
            ))?
            .ok_or_else(|| anyhow::anyhow!("TLS key missing"))?;
            let config = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(certs, key)?;
            {
                let mut quic = iroh_relay::server::QuicConfig::new(args.quic_bind);
                quic.server_config = Some(config);
                Some(quic)
            }
        }
        (None, None) => None,
        _ => anyhow::bail!("Set both YANK_TLS_CERT and YANK_TLS_KEY"),
    };
    let mut relay = RelayConfig::new(args.relay_bind);
    relay.key_cache_capacity = Some(4096);
    relay.access = Arc::new(DeviceAccess(state.devices.clone()));
    let mut config = ServerConfig::default();
    config.quic = quic;
    config.relay = Some(relay);
    let _relay = RelayServer::spawn(config).await?;
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    tracing::info!(bind=%args.bind,relay=%args.relay_bind,"Yank storage and relay ready");
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [("content-type", "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/i18n.js",
            get(|| async {
                (
                    [("content-type", "text/javascript; charset=utf-8")],
                    include_str!("../web/i18n.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [("content-type", "text/css; charset=utf-8")],
                    include_str!("../web/app.css"),
                )
            }),
        )
        .route(
            "/icons/{name}",
            get(|Path(name): Path<String>| async move {
                match name.strip_suffix(".svg").and_then(yank_core::icons::svg) {
                    Some(svg) => (
                        [
                            ("content-type", "image/svg+xml"),
                            ("cache-control", "public, max-age=3600"),
                        ],
                        svg,
                    )
                        .into_response(),
                    None => StatusCode::NOT_FOUND.into_response(),
                }
            }),
        )
        .route(
            "/logo.png",
            get(|| async {
                (
                    [("content-type", "image/png")],
                    &include_bytes!("../../yank-client/assets/yank-icon-256.png")[..],
                )
            }),
        )
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/session", get(session))
        .route("/api/password", post(change_password))
        .route("/api/accounts/{id}/password", post(reset_password))
        .route("/api/accounts", get(accounts).post(create_account))
        .route("/api/accounts/{id}", delete(remove_account))
        .route("/api/health", get(health))
        .route("/api/stats", get(stats))
        .route("/api/clips", get(list))
        .route("/api/clips/{id}", delete(remove))
        .route("/api/clips/{id}/pin", post(pin))
        .route("/api/sync/pull", get(pull))
        .route("/api/sync/push", post(push))
        .route("/api/devices", get(devices).post(announce))
        .route("/api/devices/{code}", get(find_device))
        .route("/rpc/login", post(rpc_login))
        .route("/rpc/logout", post(rpc_logout))
        .route("/rpc/health", get(rpc_health))
        .route("/rpc/register", post(rpc_register))
        .route("/rpc/devices", get(rpc_devices))
        .route("/rpc/devices/{code}", get(rpc_find))
        .route("/rpc/sync/pull", get(rpc_pull))
        .route("/rpc/sync/push", post(rpc_push))
        .layer(DefaultBodyLimit::max(MAX_WIRE_BYTES))
        .with_state(state)
}
type ApiResult<T> = std::result::Result<T, ApiError>;
struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<yank_core::Error> for ApiError {
    fn from(e: yank_core::Error) -> Self {
        match e {
            yank_core::Error::Invalid(message) => Self(StatusCode::BAD_REQUEST, message),
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Storage operation failed".into(),
            ),
        }
    }
}
fn internal() -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Service unavailable".into(),
    )
}
fn token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or_else(|| {
            headers
                .get("cookie")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.split(';')
                        .find_map(|c| c.trim().strip_prefix("yank.session=").map(str::to_owned))
                })
        })
}
fn auth(headers: &HeaderMap, state: &AppState) -> ApiResult<User> {
    let user =
        token(headers).and_then(|token| state.accounts.lock().ok()?.verify(&token).ok().flatten());
    user.ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Please sign in".into()))
}
fn storage(state: &AppState) -> ApiResult<()> {
    if state.storage {
        Ok(())
    } else {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "Persistent storage disabled on this server".into(),
        ))
    }
}
fn rate_limit(s: &AppState, addr: SocketAddr) -> ApiResult<()> {
    let mut attempts = s.attempts.lock().map_err(|_| internal())?;
    attempts.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(60));
    let key = SocketAddr::new(addr.ip(), 0);
    if attempts.len() > 4096 && !attempts.contains_key(&key) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Try again later".into(),
        ));
    }
    let entry = attempts.entry(key).or_insert((0, Instant::now()));
    if entry.0 >= 10 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts; wait one minute".into(),
        ));
    }
    entry.0 += 1;
    Ok(())
}
async fn login(
    State(s): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    Json(input): Json<auth::Credentials>,
) -> ApiResult<impl IntoResponse> {
    rate_limit(&s, addr)?;
    let remember = input.remember;
    let account = s.accounts.clone();
    let result = tokio::task::spawn_blocking(move || {
        account
            .lock()
            .map_err(|_| anyhow::anyhow!("Account service unavailable"))?
            .login(&input)
    })
    .await
    .map_err(|_| internal())?
    .map_err(|_| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Incorrect username or password".into(),
        )
    })?;
    let cookie = format!(
        "yank.session={}; HttpOnly; SameSite=Strict; Path=/{}{}",
        result.token,
        if remember { "; Max-Age=2592000" } else { "" },
        if s.secure { "; Secure" } else { "" }
    );
    Ok(([("set-cookie", cookie)], Json(result)))
}
async fn logout(State(s): State<AppState>, h: HeaderMap) -> ApiResult<impl IntoResponse> {
    auth(&h, &s)?;
    if let Some(t) = token(&h) {
        s.accounts
            .lock()
            .map_err(|_| internal())?
            .logout(&t)
            .map_err(|_| internal())?;
    }
    Ok((
        [(
            "set-cookie",
            "yank.session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        StatusCode::NO_CONTENT,
    ))
}
#[derive(Deserialize)]
struct PasswordChange {
    old_password: String,
    new_password: String,
}
#[derive(Deserialize)]
struct PasswordReset {
    new_password: String,
}
fn verified_account(accounts: &auth::Accounts, token: &str) -> ApiResult<User> {
    accounts
        .verify(token)
        .map_err(|_| internal())?
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Please sign in".into()))
}
async fn change_password(
    State(s): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Json(input): Json<PasswordChange>,
) -> ApiResult<StatusCode> {
    auth(&h, &s)?;
    rate_limit(&s, addr)?;
    let token =
        token(&h).ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Please sign in".into()))?;
    let accounts = s.accounts.clone();
    let id = tokio::task::spawn_blocking(move || {
        let mut accounts = accounts.lock().map_err(|_| internal())?;
        // Recheck while holding the same lock as the update, so a revoked request
        // cannot change a password after a concurrent reset or logout.
        let user = verified_account(&accounts, &token)?;
        accounts
            .change_password(&user.id, &input.old_password, &input.new_password)
            .map_err(|_| {
                ApiError(
                    StatusCode::BAD_REQUEST,
                    "Incorrect current password or invalid new password (8–1024 bytes required)"
                        .into(),
                )
            })?;
        Ok::<_, ApiError>(user.id)
    })
    .await
    .map_err(|_| internal())??;
    s.devices
        .lock()
        .map_err(|_| internal())?
        .retain(|_, (owner, _, _)| owner != &id);
    Ok(StatusCode::NO_CONTENT)
}
async fn reset_password(
    State(s): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PasswordReset>,
) -> ApiResult<StatusCode> {
    let user = auth(&h, &s)?;
    if !user.admin {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Administrator required".into(),
        ));
    }
    rate_limit(&s, addr)?;
    let token =
        token(&h).ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Please sign in".into()))?;
    let accounts = s.accounts.clone();
    let id = tokio::task::spawn_blocking(move || {
        let mut accounts = accounts.lock().map_err(|_| internal())?;
        let user = verified_account(&accounts, &token)?;
        if !user.admin {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Administrator required".into(),
            ));
        }
        if user.id == id {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Use password change with your current password".into(),
            ));
        }
        let updated = accounts
            .reset_password(&id, &input.new_password)
            .map_err(|_| {
                ApiError(
                    StatusCode::BAD_REQUEST,
                    "Invalid new password (8–1024 bytes required)".into(),
                )
            })?;
        if !updated {
            return Err(ApiError(StatusCode::NOT_FOUND, "Account not found".into()));
        }
        Ok(id)
    })
    .await
    .map_err(|_| internal())??;
    s.devices
        .lock()
        .map_err(|_| internal())?
        .retain(|_, (owner, _, _)| owner != &id);
    Ok(StatusCode::NO_CONTENT)
}
async fn rpc_logout(
    State(s): State<AppState>,
    h: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<axum::response::Response> {
    let token = h
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Bearer token required".into()))?;
    let accounts = s.accounts.lock().map_err(|_| internal())?;
    verified_account(&accounts, token)?;
    decode::<()>(&body)?;
    accounts.logout(token).map_err(|_| internal())?;
    binary(&())
}
async fn session(State(s): State<AppState>, h: HeaderMap) -> ApiResult<Json<User>> {
    Ok(Json(auth(&h, &s)?))
}
async fn accounts(State(s): State<AppState>, h: HeaderMap) -> ApiResult<Json<Vec<User>>> {
    let user = auth(&h, &s)?;
    if !user.admin {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Administrator required".into(),
        ));
    }
    Ok(Json(
        s.accounts
            .lock()
            .map_err(|_| internal())?
            .users()
            .map_err(|_| internal())?,
    ))
}
async fn create_account(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<auth::NewUser>,
) -> ApiResult<Json<User>> {
    let user = auth(&h, &s)?;
    if !user.admin {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Administrator required".into(),
        ));
    }
    let account = s.accounts.clone();
    let result = tokio::task::spawn_blocking(move || {
        account
            .lock()
            .map_err(|_| anyhow::anyhow!("Unavailable"))?
            .create(input)
    })
    .await
    .map_err(|_| internal())?
    .map_err(|_| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "Username already used or password shorter than 8 bytes".into(),
        )
    })?;
    Ok(Json(result))
}
async fn remove_account(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = auth(&h, &s)?;
    if !user.admin {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Administrator required".into(),
        ));
    }
    s.accounts
        .lock()
        .map_err(|_| internal())?
        .remove(&id, &user.id)
        .map_err(|_| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "Cannot delete your own account".into(),
            )
        })?;
    s.devices
        .lock()
        .map_err(|_| internal())?
        .retain(|_, (owner, _, _)| owner != &id);
    Ok(StatusCode::NO_CONTENT)
}
async fn health(State(s): State<AppState>) -> ApiResult<Json<Health>> {
    Ok(Json(Health {
        name: "yank",
        version: env!("CARGO_PKG_VERSION"),
        relay_url: s.relay_url.to_string(),
        storage: s.storage,
    }))
}
async fn stats(State(s): State<AppState>, h: HeaderMap) -> ApiResult<Json<yank_core::Stats>> {
    let u = auth(&h, &s)?;
    Ok(Json(s.library(&u, Store::stats)?))
}
async fn list(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Clip>>> {
    let u = auth(&h, &s)?;
    Ok(Json(s.library(&u, |db| {
        db.list(&q.q, &q.filter, 100, q.offset)
    })?))
}
async fn pull(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<PullQuery>,
) -> ApiResult<Json<yank_core::Page>> {
    let u = auth(&h, &s)?;
    Ok(Json(s.library(&u, |db| db.page(q.since, 50))?))
}
async fn push(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(m): Json<Mutation>,
) -> ApiResult<Json<yank_core::Ack>> {
    let u = auth(&h, &s)?;
    Ok(Json(s.library(&u, |db| db.accept(&m))?))
}
async fn remove(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let u = auth(&h, &s)?;
    s.library(&u, |db| db.mutate(&id, "server", |c| c.deleted = true))?;
    Ok(StatusCode::NO_CONTENT)
}
async fn pin(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Clip>> {
    let u = auth(&h, &s)?;
    Ok(Json(s.library(&u, |db| {
        db.mutate(&id, "server", |c| c.pinned = !c.pinned)
    })?))
}
async fn devices(State(s): State<AppState>, h: HeaderMap) -> ApiResult<Json<Vec<Peer>>> {
    let u = auth(&h, &s)?;
    let mut d = s.devices.lock().map_err(|_| internal())?;
    d.retain(|_, (_, _, t)| t.elapsed() < Duration::from_secs(90));
    Ok(Json(
        d.values()
            .filter(|(owner, _, _)| owner == &u.id)
            .map(|(_, p, _)| p.clone())
            .collect(),
    ))
}
async fn announce(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(peer): Json<Peer>,
) -> ApiResult<StatusCode> {
    let u = auth(&h, &s)?;
    use sha2::{Digest, Sha256};
    let id: iroh_base::EndpointId = peer
        .id
        .parse()
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid device identity".into()))?;
    let expected = hex::encode(Sha256::digest(id.as_bytes()))[..12].to_uppercase();
    if peer.code != expected
        || peer.name.len() > 100
        || serde_json::to_vec(&peer.address)
            .map_err(|_| internal())?
            .len()
            > 8192
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Invalid device announcement".into(),
        ));
    }
    let mut d = s.devices.lock().map_err(|_| internal())?;
    d.retain(|_, (_, _, t)| t.elapsed() < Duration::from_secs(90));
    let key = format!("{}:{}", u.id, peer.code);
    if d.len() >= 512 && !d.contains_key(&key) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Device capacity reached".into(),
        ));
    }
    d.insert(key, (u.id, peer, Instant::now()));
    Ok(StatusCode::NO_CONTENT)
}
async fn find_device(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(code): Path<String>,
) -> ApiResult<Json<Peer>> {
    let u = auth(&h, &s)?;
    let d = s.devices.lock().map_err(|_| internal())?;
    d.get(&format!("{}:{}", u.id, code.to_uppercase()))
        .filter(|(_, _, t)| t.elapsed() < Duration::from_secs(90))
        .map(|(_, p, _)| Json(p.clone()))
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "Device offline or code unknown".into(),
            )
        })
}

fn binary<T: Wire>(value: &T) -> ApiResult<axum::response::Response> {
    Ok(([("content-type", "application/x-yank")], encode(value)?).into_response())
}
async fn rpc_health(State(s): State<AppState>) -> ApiResult<axum::response::Response> {
    binary(&pb::ServerInfo {
        name: "yank".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        relay_url: s.relay_url.to_string(),
        storage: s.storage,
    })
}
async fn rpc_login(
    State(s): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    body: axum::body::Bytes,
) -> ApiResult<axum::response::Response> {
    if body.len() > 4096 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Credentials too large".into(),
        ));
    }
    let input: pb::Credentials = decode(&body)?;
    rate_limit(&s, addr)?;
    let accounts = s.accounts.clone();
    let login = tokio::task::spawn_blocking(move || {
        accounts
            .lock()
            .map_err(|_| anyhow::anyhow!("Unavailable"))?
            .login(&auth::Credentials {
                username: input.username,
                password: input.password,
                remember: false,
            })
    })
    .await
    .map_err(|_| internal())?
    .map_err(|_| {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "Incorrect username or password".into(),
        )
    })?;
    binary(&pb::Login {
        token: login.token,
        user: Some(pb::User {
            id: login.user.id,
            username: login.user.username,
            admin: login.user.admin,
        }),
        expires: login.expires,
    })
}
async fn rpc_devices(
    State(s): State<AppState>,
    h: HeaderMap,
) -> ApiResult<axum::response::Response> {
    let user = auth(&h, &s)?;
    let d = s.devices.lock().map_err(|_| internal())?;
    let peers: Vec<Peer> = d
        .values()
        .filter(|(owner, _, t)| owner == &user.id && t.elapsed() < Duration::from_secs(90))
        .map(|(_, p, _)| p.clone())
        .collect();
    binary(&peers)
}
async fn rpc_find(
    State(s): State<AppState>,
    Path(code): Path<String>,
) -> ApiResult<axum::response::Response> {
    let d = s.devices.lock().map_err(|_| internal())?;
    let peer = d
        .values()
        .find(|(_, p, t)| {
            p.code.eq_ignore_ascii_case(&code) && t.elapsed() < Duration::from_secs(90)
        })
        .map(|(_, p, _)| p)
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "Device offline or code unknown".into(),
            )
        })?;
    binary(peer)
}
async fn rpc_register(
    State(s): State<AppState>,
    h: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<StatusCode> {
    let announcement: pb::Announcement = decode(&body)?;
    let peer = Peer::from_proto(
        announcement
            .peer
            .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Missing device".into()))?,
    )?;
    let id: iroh_base::EndpointId = peer
        .id
        .parse()
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid identity".into()))?;
    let signature: [u8; 64] = announcement
        .signature
        .try_into()
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid signature".into()))?;
    let mut signed = encode(&peer)?;
    signed.extend_from_slice(&announcement.timestamp.to_le_bytes());
    id.verify(&signed, &iroh_base::Signature::from_bytes(&signature))
        .map_err(|_| ApiError(StatusCode::UNAUTHORIZED, "Invalid device proof".into()))?;
    if yank_core::now_ts().abs_diff(announcement.timestamp) > 90 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Device clock differs by more than 90 seconds".into(),
        ));
    }
    use sha2::{Digest, Sha256};
    if peer.code != hex::encode(Sha256::digest(id.as_bytes()))[..12].to_uppercase() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Invalid device code".into(),
        ));
    }
    let owner = if token(&h).is_some() {
        auth(&h, &s)?.id
    } else {
        "guest".into()
    };
    let mut devices = s.devices.lock().map_err(|_| internal())?;
    devices.retain(|_, (_, _, t)| t.elapsed() < Duration::from_secs(90));
    if devices.len() >= 512 && !devices.contains_key(&peer.id) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Device capacity reached".into(),
        ));
    }
    devices.insert(peer.id.clone(), (owner, peer, Instant::now()));
    Ok(StatusCode::NO_CONTENT)
}
async fn rpc_pull(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<PullQuery>,
) -> ApiResult<axum::response::Response> {
    let u = auth(&h, &s)?;
    binary(&s.library(&u, |db| db.page(q.since, 20))?)
}
async fn rpc_push(
    State(s): State<AppState>,
    h: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<axum::response::Response> {
    let u = auth(&h, &s)?;
    let mutation: Mutation = decode(&body)?;
    binary(&s.library(&u, |db| db.accept(&mutation))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    fn state() -> (tempfile::TempDir, AppState) {
        let temp = tempfile::tempdir().unwrap();
        let accounts = auth::Accounts::open(&temp.path().join("accounts.sqlite3")).unwrap();
        accounts
            .bootstrap(Some("admin"), Some("test-password-42"))
            .unwrap();
        let state = AppState {
            accounts: Arc::new(Mutex::new(accounts)),
            data: Arc::new(temp.path().to_path_buf()),
            devices: Arc::default(),
            relay_url: "http://localhost:7220".into(),
            storage: true,
            secure: false,
            attempts: Arc::default(),
        };
        (temp, state)
    }
    fn login(s: &AppState, name: &str, password: &str) -> auth::Login {
        s.accounts
            .lock()
            .unwrap()
            .login(&auth::Credentials {
                username: name.into(),
                password: password.into(),
                remember: false,
            })
            .unwrap()
    }
    async fn call(
        s: AppState,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        request(s, method, path, token, body, "application/x-yank").await
    }
    async fn call_json(
        s: AppState,
        path: &str,
        token: Option<&str>,
        body: serde_json::Value,
    ) -> (StatusCode, Vec<u8>) {
        request(
            s,
            "POST",
            path,
            token,
            serde_json::to_vec(&body).unwrap(),
            "application/json",
        )
        .await
    }
    async fn request(
        s: AppState,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Vec<u8>,
        content_type: &str,
    ) -> (StatusCode, Vec<u8>) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", content_type);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let mut request = request.body(Body::from(body)).unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        ));
        let response = router(s).oneshot(request).await.unwrap();
        let status = response.status();
        let body = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, body)
    }
    fn other(s: &AppState) -> auth::Login {
        s.accounts
            .lock()
            .unwrap()
            .create(auth::NewUser {
                username: "other".into(),
                password: "other-password-42".into(),
                admin: false,
            })
            .unwrap();
        login(s, "other", "other-password-42")
    }
    async fn binary_login(s: AppState, username: &str, password: &str) -> (StatusCode, Vec<u8>) {
        call(
            s,
            "POST",
            "/rpc/login",
            None,
            encode(&pb::Credentials {
                username: username.into(),
                password: password.into(),
            })
            .unwrap(),
        )
        .await
    }
    #[tokio::test]
    async fn binary_logout_requires_bearer_and_revokes_only_that_session() {
        let (_temp, s) = state();
        let first = login(&s, "admin", "test-password-42");
        let second = login(&s, "admin", "test-password-42");
        let cookie_request = Request::builder()
            .method("POST")
            .uri("/rpc/logout")
            .header("cookie", format!("yank.session={}", first.token))
            .body(Body::from(encode(&()).unwrap()))
            .unwrap();
        assert_eq!(
            router(s.clone())
                .oneshot(cookie_request)
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(
                s.clone(),
                "POST",
                "/rpc/logout",
                Some(&first.token),
                encode(&pb::Credentials {
                    username: "admin".into(),
                    password: "test-password-42".into(),
                })
                .unwrap()
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let (status, body) = call(
            s.clone(),
            "POST",
            "/rpc/logout",
            Some(&first.token),
            encode(&()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        decode::<()>(&body).unwrap();
        assert_eq!(
            call(
                s.clone(),
                "GET",
                "/api/session",
                Some(&first.token),
                Vec::new()
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(
                s.clone(),
                "POST",
                "/rpc/logout",
                Some(&first.token),
                encode(&()).unwrap()
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(s, "GET", "/api/session", Some(&second.token), Vec::new())
                .await
                .0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn password_change_requires_old_password_and_revokes_all_own_sessions() {
        let (_temp, s) = state();
        let first = login(&s, "admin", "test-password-42");
        let second = login(&s, "admin", "test-password-42");
        let other = other(&s);
        for input in [
            serde_json::json!({"old_password":"wrong-password", "new_password":"changed-password"}),
            serde_json::json!({"old_password":"test-password-42", "new_password":"short"}),
        ] {
            assert_eq!(
                call_json(s.clone(), "/api/password", Some(&first.token), input)
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
            assert!(
                s.accounts
                    .lock()
                    .unwrap()
                    .verify(&second.token)
                    .unwrap()
                    .is_some()
            );
        }
        assert_eq!(
            call_json(
                s.clone(),
                "/api/password",
                Some(&first.token),
                serde_json::json!({
                    "old_password":"test-password-42", "new_password":"changed-password",
                    "id":other.user.id,
                })
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        for token in [&first.token, &second.token] {
            assert_eq!(
                call(s.clone(), "GET", "/rpc/sync/pull", Some(token), Vec::new())
                    .await
                    .0,
                StatusCode::UNAUTHORIZED
            );
        }
        let cookie_request = Request::builder()
            .uri("/api/session")
            .header("cookie", format!("yank.session={}", second.token))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router(s.clone())
                .oneshot(cookie_request)
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            binary_login(s.clone(), "admin", "test-password-42").await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            binary_login(s.clone(), "admin", "changed-password").await.0,
            StatusCode::OK
        );
        assert_eq!(
            binary_login(s.clone(), "other", "other-password-42")
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(s, "GET", "/api/session", Some(&other.token), Vec::new())
                .await
                .0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn admin_reset_revokes_target_sessions_and_preserves_tenant_isolation() {
        let (_temp, s) = state();
        let admin = login(&s, "admin", "test-password-42");
        let target = other(&s);
        let sibling = login(&s, "other", "other-password-42");
        let private = Clip::text("admin-device", "admin private");
        s.library(&admin.user, |db| {
            db.accept(&Mutation {
                operation: "private".into(),
                clip: private,
            })
        })
        .unwrap_or_else(|_| panic!("store private clip"));
        let path = format!("/api/accounts/{}/password", target.user.id);
        assert_eq!(
            call_json(
                s.clone(),
                &path,
                Some(&admin.token),
                serde_json::json!({"new_password":"reset-password"})
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        for token in [&target.token, &sibling.token] {
            assert_eq!(
                call(s.clone(), "GET", "/api/session", Some(token), Vec::new())
                    .await
                    .0,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            binary_login(s.clone(), "other", "other-password-42")
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, body) = binary_login(s.clone(), "other", "reset-password").await;
        assert_eq!(status, StatusCode::OK);
        let fresh: pb::Login = decode(&body).unwrap();
        assert_eq!(fresh.user.as_ref().unwrap().id, target.user.id);
        assert!(!fresh.user.as_ref().unwrap().admin);
        let (status, body) = call(
            s.clone(),
            "GET",
            "/rpc/sync/pull",
            Some(&fresh.token),
            Vec::new(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(decode::<yank_core::Page>(&body).unwrap().clips.is_empty());
        let (status, body) = call(
            s.clone(),
            "GET",
            "/rpc/sync/pull",
            Some(&admin.token),
            Vec::new(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(decode::<yank_core::Page>(&body).unwrap().clips.len(), 1);
        // Bootstrap remains startup-only and cannot reset either existing account.
        s.accounts
            .lock()
            .unwrap()
            .bootstrap(Some("other"), Some("bootstrap-password"))
            .unwrap();
        assert_eq!(
            binary_login(s.clone(), "other", "reset-password").await.0,
            StatusCode::OK
        );
        assert_eq!(
            binary_login(s, "admin", "test-password-42").await.0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn reset_denies_non_admin_without_disclosing_other_accounts() {
        let (_temp, s) = state();
        let admin = login(&s, "admin", "test-password-42");
        let user = other(&s);
        let existing = format!("/api/accounts/{}/password", admin.user.id);
        let unknown = "/api/accounts/missing-account/password";
        let payload = serde_json::json!({"new_password":"reset-password"});
        let denied = call_json(s.clone(), &existing, Some(&user.token), payload.clone()).await;
        assert_eq!(denied.0, StatusCode::FORBIDDEN);
        assert_eq!(
            denied,
            call_json(s.clone(), unknown, Some(&user.token), payload.clone()).await
        );
        assert_eq!(
            call_json(s.clone(), &existing, None, payload.clone())
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call_json(s.clone(), &existing, Some(&admin.token), payload.clone())
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call_json(s.clone(), unknown, Some(&admin.token), payload)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            binary_login(s.clone(), "admin", "test-password-42").await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(s, "GET", "/api/session", Some(&user.token), Vec::new())
                .await
                .0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn login_and_password_changes_share_the_attempt_budget() {
        let (_temp, s) = state();
        let admin = login(&s, "admin", "test-password-42");
        for _ in 0..4 {
            assert_eq!(
                binary_login(s.clone(), "missing", "wrong-password").await.0,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                call_json(
                    s.clone(),
                    "/api/login",
                    None,
                    serde_json::json!({"username":"missing", "password":"wrong-password"})
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
        }
        for _ in 0..2 {
            assert_eq!(call_json(s.clone(), "/api/password", Some(&admin.token), serde_json::json!({"old_password":"wrong-password", "new_password":"changed-password"})).await.0, StatusCode::BAD_REQUEST);
        }
        assert_eq!(
            binary_login(s.clone(), "admin", "test-password-42").await.0,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(call_json(s.clone(), "/api/password", Some(&admin.token), serde_json::json!({"old_password":"test-password-42", "new_password":"changed-password"})).await.0, StatusCode::TOO_MANY_REQUESTS);
        assert!(
            s.accounts
                .lock()
                .unwrap()
                .verify(&admin.token)
                .unwrap()
                .is_some()
        );
    }
    #[tokio::test]
    async fn binary_login_and_user_storage_are_isolated() {
        let (_temp, s) = state();
        s.accounts
            .lock()
            .unwrap()
            .create(auth::NewUser {
                username: "other".into(),
                password: "other-password-42".into(),
                admin: false,
            })
            .unwrap();
        let (status, body) = call(
            s.clone(),
            "POST",
            "/rpc/login",
            None,
            encode(&pb::Credentials {
                username: "admin".into(),
                password: "test-password-42".into(),
            })
            .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let admin: pb::Login = decode(&body).unwrap();
        let other = login(&s, "other", "other-password-42");
        let mut clip = Clip::text("device", "private value");
        clip.clock.insert("device".into(), 1);
        let m = Mutation {
            operation: "one".into(),
            clip,
        };
        assert_eq!(
            call(
                s.clone(),
                "POST",
                "/rpc/sync/push",
                Some(&admin.token),
                encode(&m).unwrap()
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, body) = call(
            s.clone(),
            "GET",
            "/rpc/sync/pull",
            Some(&other.token),
            Vec::new(),
        )
        .await;
        assert!(decode::<yank_core::Page>(&body).unwrap().clips.is_empty());
        assert_eq!(
            call(s, "GET", "/api/accounts", Some(&other.token), Vec::new())
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn storage_disabled_preserves_discovery_but_rejects_data() {
        let (_temp, mut s) = state();
        let user = login(&s, "admin", "test-password-42");
        s.storage = false;
        assert_eq!(
            call(
                s.clone(),
                "GET",
                "/rpc/sync/pull",
                Some(&user.token),
                Vec::new()
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(s, "GET", "/rpc/devices", Some(&user.token), Vec::new())
                .await
                .0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn logout_revokes_the_session() {
        let (_temp, s) = state();
        let user = login(&s, "admin", "test-password-42");
        assert_eq!(
            call(
                s.clone(),
                "POST",
                "/api/logout",
                Some(&user.token),
                Vec::new()
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(s, "GET", "/api/stats", Some(&user.token), Vec::new())
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    #[test]
    fn bootstrap_never_overwrites_an_existing_password() {
        let (_temp, s) = state();
        s.accounts
            .lock()
            .unwrap()
            .bootstrap(Some("replacement"), Some("new-password-42"))
            .unwrap();
        assert_eq!(s.accounts.lock().unwrap().users().unwrap().len(), 1);
        assert_eq!(
            login(&s, "admin", "test-password-42").user.username,
            "admin"
        );
    }
    #[tokio::test]
    async fn unauthenticated_and_wrong_message_requests_are_rejected() {
        let (_temp, s) = state();
        assert_eq!(
            call(s.clone(), "GET", "/rpc/sync/pull", None, Vec::new())
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let user = login(&s, "admin", "test-password-42");
        assert_eq!(
            call(
                s,
                "POST",
                "/rpc/sync/push",
                Some(&user.token),
                encode(&()).unwrap()
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
}
