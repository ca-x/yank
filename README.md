# yank

`yank` is a Rust clipboard manager inspired by Ditto's local-first history and network send/receive model.

## Components

- `yank-core`: shared SQLite storage, clip format model, settings, DTOs, and i18n bundles.
- `yank-client`: Makepad desktop client. It stores clipboard history locally with SQLite and can work without any server configuration.
- `yank-server`: self-hosted sync server with token authentication, REST sync APIs, and a web management UI.

## Current Capabilities

- Desktop client workflow: automatic clipboard polling, searchable history, selection, copy-back, delete, pin/unpin, and text editing.
- Local clipboard capture and history storage for text, HTML, images, and file lists without requiring a server.
- Ditto-style clip model with metadata plus multiple typed clipboard formats.
- Configurable client behavior from the GUI, including auto-capture, capture format toggles, duplicate handling, history limit, capture interval, sync connection, and shortcuts.
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

Run the server container:

```bash
docker run --rm -p 7219:7219 -e YANK_TOKEN=change-me -v yank-data:/data ghcr.io/ca-x/yank:main
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

The desktop client does not require users to edit configuration files. Settings are visible and editable in the GUI:

- Language and theme.
- Auto-capture on/off, duplicate policy, capture interval, max history, and device ID display/copy.
- Capture format toggles for text, HTML, images, and file lists.
- Sync server URL and token.
- Window shortcuts for search/history, copy selected, delete selected, pin/unpin, edit, capture, and sync.

The current shortcut layer is active while the client window has focus. System-wide global hotkeys are intentionally kept as a platform-specific follow-up rather than hidden configuration.

The detailed Ditto parity baseline is tracked in [docs/ditto-client-capability-matrix.md](docs/ditto-client-capability-matrix.md). Do not treat unsupported Ditto features such as RTF capture, source application icons, tray integration, global hotkeys, groups, or import/export as complete until that matrix is updated with implementation evidence.

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

GitHub Actions builds and uploads release binaries for `yank-client` and `yank-server` on Linux, macOS, and Windows. It also builds the server Docker image on PRs and pushes multi-arch images to GHCR and Docker Hub on `main`, `v*` tags, and manual dispatch when credentials are configured.
