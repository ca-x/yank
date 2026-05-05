# Ditto Client Capability Matrix

This matrix is the parity baseline for `yank-client` after the Makepad 2.0 client pass. It is intentionally concrete: "Done" means the repository has a user-facing path plus compile/test coverage where practical, "Partial" means the path exists but is not a full Ditto clone, and "Gap" means it must not be presented as implemented.

## Scope

- Client UI and interaction: `crates/yank-client/src/main.rs`
- Shared storage/settings/i18n: `crates/yank-core/src/lib.rs`, `crates/yank-core/i18n/*.json`
- Release automation: `.github/workflows/*.yml`, `Dockerfile`

## Clipboard History

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Configurable history size | Done | `Settings::max_history`, GUI field, SQLite pruning | `0` is the unlimited sentinel; no separate "infinite" picker. |
| Text/HTML/image/file capture | Done | `read_clipboard_snapshot`, `ClipFormat::{text,html,image_rgba,file_list}` | Native image container fidelity is normalized to RGBA. |
| RTF-like content | Partial | RTF text detector stores `ClipFormat::rtf` | True Windows native RTF format enumeration is not implemented. |
| Color values | Done | `detect_color_value`, `ClipFormat::color`, type filter labels | No visual color swatch yet. |
| Auto dedupe | Done | `content_hash`, `save_clip_deduplicated`, duplicate policy GUI | Ditto's exact consecutive-only mode is simplified. |
| Copy timestamps | Done | `created_at`, `updated_at`, localized row/detail formatting | None for current UI. |
| Source application | Partial | `current_source_application`, `source_app` in rows/details/privacy rules | Source icon extraction is not implemented. |
| Persistent SQLite | Done | `Store::open(paths::database_path())` | None. |

## Quick Paste Panel

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Ditto-like main panel | Done | Separate main/settings pages, search field, numbered rows, context menu panel | It is a Makepad main window, not a native transient popup window. |
| Global show hotkey | Done | `global-hotkey`, `GlobalHotkeyAction::ShowHistory` | OS conflict feedback depends on registration errors. |
| Arrow/Enter/Esc/number navigation | Done | `handle_ditto_style_key`, row focus, `number_key_index` | Exact Ditto focus cycling is still simplified. |
| First ten item shortcuts | Done | In-window Ctrl+1-0/Ctrl+Shift+1-0 and configurable global original/plain-text position hotkeys | Global position hotkeys default blank like Ditto. |
| First ten hotkey options | Done | `first_ten_hotkeys`, `first_ten_plain_hotkeys`, send paste, move-to-top, active-group toggles in GUI | Ditto's Win-key checkbox is represented by text shortcuts such as `Win+1` or `Super+1`. |
| List display options | Done | line count, leading whitespace, pasted marker, scroll bar, RTF draw toggle, list font size | No native font-family picker; Makepad font family remains app default. |
| Image preview | Partial | Image rows/details show dimensions and can restore images | Hover thumbnail rendering is not implemented. |
| Position memory | Partial | Cursor/caret/previous setting exists in GUI | Makepad window placement is not fully platform-positioned yet. |
| Close on blur | Partial | Main-page transient panels close on window lost focus | The whole app window is not hidden on blur to avoid losing settings/editing state. |

## Search

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Real-time search | Done | `TextInput::changed`, find-as-you-type, `/q`, `/f`, `/s`, `/d` prefixes | None for current path. |
| Regex/wildcard/simple search | Done | `SearchMode`, GUI toggles, regex error status | No pinyin initials index. |
| Type filters | Done | all/pinned/text/image/files filters | Date range controls are text-prefix based, not date pickers. |
| Source/date filtering | Partial | source/date search scopes | No dedicated dropdown/date range UI. |
| FTS/fuzzy ranking | Gap | Current search scans loaded clips with string/regex matching | Add FTS5 or an indexed fuzzy search layer for very large histories. |

## Groups / Pinned

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Pin/top and sticky ordering | Done | `pinned`, sticky position commands, menu actions | None for current behavior. |
| Named groups | Done | `clip_groups`, group panel create/rename/delete/sort/assign | Drag-and-drop between groups is not implemented. |
| Group hotkeys | Done | group `hotkey`, GUI field, global registry | OS-level registration can fail if another app owns the hotkey. |
| Quick templates | Done | `{date}`, `{time}`, `{clipboard}` template expansion on paste | No dedicated template editor beyond normal clip text editing. |

## Paste Options

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Original-format paste | Done | `restore_clip_to_clipboard`, image/file/html/text restore | Limited by `arboard` to supported clipboard formats. |
| Plain-text paste | Done | Shift+Enter, menu action, configurable Text Only Paste hotkey, first-ten plain-text position hotkeys | Text Only Paste uses current clipboard text exposed by the OS. |
| Transform paste | Done | upper/lower/case/trim/no LF/add LF/camel/slug/path/ascii/time/GUID actions | Ditto add-in scripting is not cloned. |
| Merge paste | Done | multi-select plus separator/reverse settings | Mixed non-text merge is limited to image horizontal/vertical. |
| Delayed paste | Done | GUI delay settings and platform paste injectors | Linux requires one of `xdotool`, `wtype`, `ydotool`, or `dotool`. |

## Sync / Network

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Multi-device sync | Partial | `SyncClient`, server push/pull APIs, GUI server/token | Server-mediated sync, not Ditto peer LAN sync. |
| LAN discovery | Gap | No mDNS/UDP discovery | Add discovery protocol. |
| Encryption/auth | Partial | Bearer token and HTTPS-capable URL | No payload-level AES shared-key encryption. |
| Selective sync | Gap | No per-type/pinned-only sync settings | Add sync filter settings and server/client enforcement. |
| Sync log | Partial | status text and stats exist | No full sync event log table in GUI. |

## Privacy / Security

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| App blacklist | Done | GUI ignored apps field, source-app matching | Source-app detection varies by OS/window manager. |
| Content rules | Done | GUI regex/text rules, `rules_match_text` | No rule tester UI. |
| Pause/incognito | Done | capture toggle, tray pause/resume | No timed pause mode. |
| Clear history | Done | GUI/tray/menu clear and delete non-pinned | Secure erase/overwrite is not implemented. |
| Master password | Gap | No lock/encrypted-history flow | Needs storage encryption and unlock UI. |

## System Integration

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Tray menu | Done on Linux | `ksni` tray commands for open/settings/capture/sync/pause/exit | Windows/macOS native tray path is not implemented. |
| Global hotkeys | Done | show/capture/copy-and-capture/sync/text-only/original-and-plain first-ten/copy-buffer/group/clip hotkeys, including Win/Super parsing | Some plain shortcuts intentionally remain in-window only to avoid stealing OS/app keys. |
| Startup/taskbar toggles | Partial | GUI settings persist | Real OS startup registration/installer integration is incomplete. |
| Context menu/shell extension | Gap | No OS shell extension | Platform-specific work needed. |
| Multi-monitor popup | Partial | Window can be restored and panel is visible | True current-monitor popup positioning is incomplete. |

## Data Management

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Export/import | Done | JSON export/import, selected text export, file-content import | CSV export is not implemented. |
| Backup/compact/verify | Done | GUI utilities and storage maintenance | No daily scheduler yet. |
| Auto clean by age/count/size | Done | max history, expire days, max DB MB maintenance | Maintenance runs on save/action, not a background daily timer. |

## Settings Center

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| General/types/keyboard/copy buffers/quick paste/sync/stats/utilities/advanced/about tabs | Done | settings page tab buttons and card visibility | None for current tab split. |
| Language/theme | Done | i18n JSON bundles and runtime theme reapply | Follow-system theme is not implemented. |
| Hotkey conflict validation | Done | parser/canonical conflict checks across main, alternate activate, first-ten original/plain, copy-buffer, group, and per-clip hotkeys | External OS conflicts are detected only when registration fails. |
| All customer settings in GUI | Partial | Current settings exposed in Makepad GUI | Low-level server bind port remains server-side config. |

## Release / Automation

| Requirement | Status | Evidence | Remaining gap |
| --- | --- | --- | --- |
| Client/server binaries | Done | tag-only binary workflow uploads release assets | Installer/MSI/DMG packaging is not implemented. |
| Docker image | Done | tag-only Docker workflow builds/pushes GHCR and optional Docker Hub | Requires Docker Hub secrets for Docker Hub push. |
| CI split | Done | CI, binaries, Docker workflows are separate and tag-gated for release artifacts | None for current automation. |

## Current Backlog

1. Replace the main-window quick paste surface with a true transient popup positioned near cursor/caret/previous monitor.
2. Add hover image thumbnails and optional source application icons.
3. Add FTS5/fuzzy search with optional pinyin initials.
4. Add LAN discovery and payload encryption if Ditto peer sync parity is required.
5. Add master password/encrypted history and secure erase semantics.
6. Add Windows/macOS tray/startup/installers and optional shell extension.
