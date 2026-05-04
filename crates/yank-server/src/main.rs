use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header, header::AUTHORIZATION},
    response::{Html, IntoResponse},
    routing::{delete, get, post},
};
use clap::Parser;
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tower_http::trace::TraceLayer;
use tracing::info;
use yank_core::{
    APP_NAME, Clip, DEFAULT_SERVER_BIND, HealthResponse, Language, PullClipsResponse,
    PushClipRequest, PushClipResponse, Store, StoreStats, i18n, now_ts,
};

#[derive(Debug, Parser)]
#[command(name = "yank-server", about = "Self-hosted yank clipboard sync server")]
struct Args {
    #[arg(long, env = "YANK_BIND", default_value = DEFAULT_SERVER_BIND)]
    bind: SocketAddr,

    #[arg(long, env = "YANK_DB", default_value = "yank-server.sqlite3")]
    db: PathBuf,

    #[arg(long, env = "YANK_TOKEN")]
    token: String,
}

#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<Store>>,
    token: Arc<str>,
}

#[derive(Debug, Deserialize)]
struct PullQuery {
    #[serde(default)]
    since: i64,
    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_limit() -> u32 {
    100
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yank_server=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();
    let store = Store::open(&args.db)
        .with_context(|| format!("opening sqlite database {}", args.db.display()))?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        token: Arc::from(args.token),
    };
    let app = router(state);

    info!(bind = %args.bind, db = %args.db.display(), "starting yank server");
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(admin_page))
        .route("/assets/i18n/{locale}.json", get(i18n_asset))
        .route("/api/health", get(health))
        .route("/api/admin/stats", get(stats))
        .route("/api/clips", get(list_clips))
        .route("/api/clips", post(push_clip))
        .route("/api/clips/{id}", delete(delete_clip))
        .route("/api/sync/pull", get(pull_clips))
        .route("/api/sync/push", post(push_clip))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn admin_page() -> Html<&'static str> {
    Html(ADMIN_HTML)
}

async fn i18n_asset(Path(locale): Path<String>) -> impl IntoResponse {
    let language = Language::parse(&locale).unwrap_or(Language::En);
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        i18n::bundle_json(language),
    )
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        name: APP_NAME,
        version: env!("CARGO_PKG_VERSION"),
        server_time: now_ts(),
    })
}

async fn stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<StoreStats>, ApiError> {
    ensure_auth(&headers, &state)?;
    let store = state.store.lock().map_err(|_| ApiError::internal())?;
    Ok(Json(store.stats().map_err(ApiError::from)?))
}

async fn list_clips(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Clip>>, ApiError> {
    ensure_auth(&headers, &state)?;
    let store = state.store.lock().map_err(|_| ApiError::internal())?;
    Ok(Json(store.list_clips(200).map_err(ApiError::from)?))
}

async fn pull_clips(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullClipsResponse>, ApiError> {
    ensure_auth(&headers, &state)?;
    let limit = query.limit.clamp(1, 1000);
    let store = state.store.lock().map_err(|_| ApiError::internal())?;
    let clips = store
        .list_clips_since(query.since, limit)
        .map_err(ApiError::from)?;
    Ok(Json(PullClipsResponse {
        clips,
        server_time: now_ts(),
    }))
}

async fn push_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PushClipRequest>,
) -> Result<Json<PushClipResponse>, ApiError> {
    ensure_auth(&headers, &state)?;
    let store = state.store.lock().map_err(|_| ApiError::internal())?;
    let clip = store.save_clip(&request.clip).map_err(ApiError::from)?;
    Ok(Json(PushClipResponse { clip }))
}

async fn delete_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    ensure_auth(&headers, &state)?;
    let store = state.store.lock().map_err(|_| ApiError::internal())?;
    if store.delete_clip(&id).map_err(ApiError::from)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

fn ensure_auth(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let expected = format!("Bearer {}", state.token);
    let actual = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    if actual == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(ApiError::unauthorized())
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: &'static str,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "server.error.unauthorized",
        }
    }

    fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: "server.error.not_found",
        }
    }

    fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "server.error.internal",
        }
    }
}

impl From<yank_core::YankError> for ApiError {
    fn from(_: yank_core::YankError) -> Self {
        Self::internal()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let messages = i18n::bundle(Language::En);
        (self.status, messages.text(self.message).to_owned()).into_response()
    }
}

const ADMIN_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>yank</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f7f3ec;
      --fg: #171410;
      --muted: #6d645b;
      --panel: #fffaf1;
      --line: #d7cec1;
      --accent: #0d6b62;
      --danger: #b92b2b;
      --shadow: 0 16px 44px rgba(42, 30, 14, .13);
    }
    [data-theme="dark"] {
      color-scheme: dark;
      --bg: #121515;
      --fg: #f3efe7;
      --muted: #aaa197;
      --panel: #1d2221;
      --line: #343b38;
      --accent: #69c8b8;
      --danger: #ff7777;
      --shadow: 0 18px 54px rgba(0, 0, 0, .32);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background:
        linear-gradient(90deg, rgba(13, 107, 98, .08), transparent 38%),
        var(--bg);
      color: var(--fg);
      font: 15px/1.5 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    main { width: min(1120px, calc(100vw - 32px)); margin: 32px auto; }
    header {
      display: flex;
      align-items: end;
      justify-content: space-between;
      gap: 20px;
      padding-bottom: 18px;
      border-bottom: 1px solid var(--line);
    }
    h1 { margin: 0; font-size: clamp(34px, 5vw, 72px); letter-spacing: 0; line-height: .9; }
    .sub { margin: 12px 0 0; color: var(--muted); max-width: 620px; }
    .toolbar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; justify-content: flex-end; }
    input, button {
      height: 38px;
      border: 1px solid var(--line);
      background: var(--panel);
      color: var(--fg);
      border-radius: 7px;
      padding: 0 12px;
      font: inherit;
    }
    input { width: min(360px, 100%); }
    button { cursor: pointer; }
    button.primary { background: var(--accent); border-color: var(--accent); color: var(--bg); }
    button.danger { color: var(--danger); }
    .stats {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 12px;
      margin: 22px 0;
    }
    .stat, .clip {
      background: color-mix(in srgb, var(--panel) 92%, transparent);
      border: 1px solid var(--line);
      border-radius: 8px;
      box-shadow: var(--shadow);
    }
    .stat { padding: 18px; min-height: 104px; }
    .stat strong { display: block; font-size: 30px; line-height: 1; margin-bottom: 12px; }
    .stat span { color: var(--muted); }
    .clips { display: grid; gap: 10px; }
    .clip { display: grid; grid-template-columns: 1fr auto; gap: 14px; padding: 14px; }
    .clip p { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
    .meta { color: var(--muted); font-size: 12px; margin-top: 8px; }
    .empty { color: var(--muted); padding: 32px 0; }
    @media (max-width: 720px) {
      header { display: block; }
      .toolbar { justify-content: flex-start; margin-top: 18px; }
      .stats { grid-template-columns: repeat(2, minmax(0, 1fr)); }
      .clip { grid-template-columns: 1fr; }
      input { width: 100%; }
    }
  </style>
</head>
<body data-theme="light">
  <main>
    <header>
      <div>
        <h1>yank</h1>
        <p class="sub" data-i18n="admin.tagline"></p>
      </div>
      <div class="toolbar">
        <input id="token" type="password" autocomplete="current-password">
        <button id="saveToken" class="primary" data-i18n="admin.save"></button>
        <button id="theme"></button>
        <button id="lang"></button>
      </div>
    </header>

    <section class="stats">
      <div class="stat"><strong id="clipCount">-</strong><span data-i18n="admin.clips"></span></div>
      <div class="stat"><strong id="deletedCount">-</strong><span data-i18n="admin.deleted"></span></div>
      <div class="stat"><strong id="deviceCount">-</strong><span data-i18n="admin.devices"></span></div>
      <div class="stat"><strong id="newest">-</strong><span data-i18n="admin.newest"></span></div>
    </section>

    <div class="toolbar" style="justify-content: space-between; margin-bottom: 12px">
      <h2 style="margin:0" data-i18n="admin.history"></h2>
      <button id="refresh" class="primary" data-i18n="admin.refresh"></button>
    </div>
    <section id="clips" class="clips"></section>
  </main>
  <script>
    let lang = localStorage.getItem("yank.lang") || "en";
    let theme = localStorage.getItem("yank.theme") || "light";
    let i18n = { locale: lang, messages: {} };
    const tokenInput = document.querySelector("#token");
    tokenInput.value = localStorage.getItem("yank.token") || "";

    function t(key) { return i18n.messages[key] || key; }
    async function loadI18n() {
      const res = await fetch(`/assets/i18n/${lang}.json`);
      i18n = await res.json();
    }
    function applyChrome() {
      document.body.dataset.theme = theme;
      document.documentElement.lang = lang;
      document.querySelectorAll("[data-i18n]").forEach(el => el.textContent = t(el.dataset.i18n));
      tokenInput.placeholder = t("admin.token");
      document.querySelector("#theme").textContent = theme === "light" ? t("admin.dark") : t("admin.light");
      document.querySelector("#lang").textContent = t("admin.lang_toggle");
    }
    async function api(path, options = {}) {
      const token = localStorage.getItem("yank.token") || "";
      const res = await fetch(path, {
        ...options,
        headers: { "Authorization": `Bearer ${token}`, ...(options.headers || {}) }
      });
      if (!res.ok) throw new Error(await res.text());
      if (res.status === 204) return null;
      return res.json();
    }
    function fmt(ts) {
      return ts ? new Date(ts * 1000).toLocaleString() : "-";
    }
    async function refresh() {
      const [stats, clips] = await Promise.all([api("/api/admin/stats"), api("/api/clips")]);
      clipCount.textContent = stats.clip_count;
      deletedCount.textContent = stats.deleted_count;
      deviceCount.textContent = stats.device_count;
      newest.textContent = fmt(stats.newest_clip_at);
      const wrap = document.querySelector("#clips");
      wrap.innerHTML = "";
      if (!clips.length) {
        wrap.innerHTML = `<div class="empty">${t("admin.empty")}</div>`;
        return;
      }
      for (const clip of clips) {
        const item = document.createElement("article");
        item.className = "clip";
        const text = clip.primary_text || clip.description;
        const formatCount = t("admin.format_count").replace("{count}", clip.formats.length);
        item.innerHTML = `
          <div><p></p><div class="meta">${clip.device_id} · ${fmt(clip.updated_at)} · ${formatCount}</div></div>
          <button class="danger">${t("admin.delete")}</button>`;
        item.querySelector("p").textContent = text;
        item.querySelector("button").onclick = async () => { await api(`/api/clips/${clip.id}`, { method: "DELETE" }); refresh(); };
        wrap.appendChild(item);
      }
    }
    document.querySelector("#saveToken").onclick = () => {
      localStorage.setItem("yank.token", tokenInput.value);
      refresh().catch(err => alert(err.message));
    };
    document.querySelector("#theme").onclick = () => {
      theme = theme === "light" ? "dark" : "light";
      localStorage.setItem("yank.theme", theme);
      applyChrome();
    };
    document.querySelector("#lang").onclick = () => {
      lang = lang === "en" ? "zh" : "en";
      localStorage.setItem("yank.lang", lang);
      loadI18n().then(() => {
        applyChrome();
        refresh().catch(() => {});
      });
    };
    document.querySelector("#refresh").onclick = () => refresh().catch(err => alert(err.message));
    loadI18n().then(() => {
      applyChrome();
      if (tokenInput.value) refresh().catch(() => {});
    });
  </script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        AppState {
            store: Arc::new(Mutex::new(Store::open_memory().unwrap())),
            token: Arc::from("secret-token"),
        }
    }

    #[test]
    fn rejects_missing_token() {
        let headers = HeaderMap::new();
        let error = ensure_auth(&headers, &state()).unwrap_err();
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn accepts_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer secret-token".parse().unwrap());
        assert!(ensure_auth(&headers, &state()).is_ok());
    }
}
