# Ditto Server Mapping

This project does not reuse Ditto source code. It maps the relevant server-side behavior into a Rust, HTTP-based design.

## Ditto Behaviors Referenced

- `Server.cpp`: accepts incoming clip transfers, collects clip formats, and adds received clips to the local database.
- `Client.cpp`: sends clip metadata and one or more clipboard formats to another host.
- `AutoSendToClientThread.cpp`: sends newly saved clips to configured peers.
- `DatabaseUtilities.cpp`: records deletes separately through `MainDeletes`.
- `Clip_ImportExport.cpp`: represents each clip as a main row plus typed format payloads.

## yank Mapping

- `clips` and `clip_formats` tables preserve the Ditto split between clip metadata and format payloads.
- `/api/sync/push` replaces Ditto's direct socket sender with a token-authenticated HTTP push.
- `/api/sync/pull` lets clients fetch changes from a self-hosted server instead of configuring peer IP lists.
- Soft deletes use `deleted_at` and `sync_events`, matching Ditto's need to propagate deletion state.
- The client remains local-first; sync is skipped unless both server URL and token are configured.

## Intentional Differences

- Transport is HTTP JSON instead of Ditto's Windows socket struct protocol.
- Token authentication is mandatory for sync/admin endpoints.
- The first implementation focuses on text and structured format storage. File-drop transfer can be added later using the same `clip_formats` model.
