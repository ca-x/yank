# yank

`yank` is a Rust clipboard manager inspired by Ditto's local-first history and network send/receive model.

## Components

- `yank-core`: shared SQLite storage, clip format model, settings, DTOs, and i18n bundles.
- `yank-client`: Makepad desktop client. It stores clipboard history locally with SQLite and can work without any server configuration.
- `yank-server`: self-hosted sync server with token authentication, REST sync APIs, and a web management UI.

## Current Capabilities

- Local clipboard text capture and history storage.
- Ditto-style clip model with metadata plus multiple typed clipboard formats.
- Soft deletes so deletion events can be synchronized.
- Optional multi-device sync through a self-hosted server.
- Bearer token authentication for all management and sync APIs.
- Web admin page for viewing stats/history and deleting clips.
- Client and server UI language/theme switching.
- Translation files are stored outside application logic in `crates/yank-core/i18n/en.json` and `crates/yank-core/i18n/zh.json`.

## Run

Linux desktop builds need the native libraries used by Makepad/arboard, including X11, Xcursor, ALSA, and PulseAudio development packages.

Start the server:

```bash
YANK_TOKEN=change-me cargo run -p yank-server -- --bind 127.0.0.1:7219 --db yank-server.sqlite3
```

Open the admin UI:

```text
http://127.0.0.1:7219
```

Start the client:

```bash
cargo run -p yank-client
```

Optional client sync bootstrap:

```bash
YANK_SERVER_URL=http://127.0.0.1:7219 YANK_TOKEN=change-me cargo run -p yank-client
```

## API

All protected endpoints require:

```text
Authorization: Bearer <token>
```

- `GET /api/health`
- `GET /api/admin/stats`
- `GET /api/clips`
- `POST /api/clips`
- `DELETE /api/clips/{id}`
- `GET /api/sync/pull?since=<unix_ts>&limit=<n>`
- `POST /api/sync/push`

## Verification

```bash
cargo fmt --check
cargo test --workspace
cargo check --workspace
```
