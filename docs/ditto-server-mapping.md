# Ditto Server Mapping

This project does not reuse Ditto source code. It maps the relevant server-side behavior into a Rust, HTTP-based design.

## Ditto Behaviors Referenced

- `ClipboardViewer.cpp`: observes clipboard changes and adds new clips automatically.
- Quick paste window behavior: searchable history list, focused selection, copy/paste from a selected clip, deletion, editing, and sticky/favorite clips.
- `Server.cpp`: accepts incoming clip transfers, collects clip formats, and adds received clips to the local database.
- `Client.cpp`: sends clip metadata and one or more clipboard formats to another host.
- `AutoSendToClientThread.cpp`: sends newly saved clips to configured peers.
- `DatabaseUtilities.cpp`: records deletes separately through `MainDeletes`.
- `Clip_ImportExport.cpp`: represents each clip as a main row plus typed format payloads.

## yank Mapping

- `clips` and `clip_formats` tables preserve the Ditto split between clip metadata and format payloads.
- The Makepad client polls the OS clipboard, saves enabled text, HTML, image, and file-list formats automatically, exposes searchable history, and can copy a chosen historical item back to the clipboard.
- Pin/unpin maps Ditto sticky/favorite style retention into a cross-platform `pinned` flag.
- Editable text clips update the primary text, description, format payload, hash, and sync timestamp together.
- Client settings are GUI-driven: no user-facing behavior requires editing a config file.
- `/api/sync/push` replaces Ditto's direct socket sender with a token-authenticated HTTP push.
- `/api/sync/pull` lets clients fetch changes from a self-hosted server instead of configuring peer IP lists.
- Soft deletes use `deleted_at` and `sync_events`, matching Ditto's need to propagate deletion state.
- The client remains local-first; sync is skipped unless both server URL and token are configured.

## Intentional Differences

- Transport is HTTP JSON instead of Ditto's Windows socket struct protocol.
- Token authentication is mandatory for sync/admin endpoints.
- RTF, source process icons, tray integration, LAN discovery, groups, and import/export are tracked as explicit client parity gaps in `docs/ditto-client-capability-matrix.md`.
- Shortcuts are configurable and work inside the client window. System-wide global hotkeys need a platform-specific layer and are not hidden behind manual config.
