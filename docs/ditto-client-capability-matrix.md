# Ditto Client Capability Matrix

This matrix is the working parity baseline for `yank-client`. It maps the Ditto-style requirements to the current implementation and keeps gaps explicit. "Done" means code exists in this repository and is covered by available compile/test verification. "Partial" means the user-facing path exists but does not yet match the full Ditto behavior. "Gap" means it is not implemented and must not be described as supported.

## Current Scope

- Desktop client: `crates/yank-client/src/main.rs`
- Shared model, SQLite storage, settings, and i18n: `crates/yank-core/src/lib.rs`, `crates/yank-core/i18n/*.json`
- Sync server and admin UI: `crates/yank-server/src/main.rs`
- CI, release binaries, and server container: `.github/workflows/ci.yml`, `Dockerfile`

## 1. Clipboard History

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Configurable history size | Default 500, configurable, practical unlimited | Done | `Settings::max_history`, GUI input, `Store::enforce_max_history` | Current GUI accepts positive numbers; explicit "unlimited" mode is not added. |
| Text capture | Capture plain text and store persistently | Done | `read_clipboard_snapshot`, `ClipFormat::text`, SQLite `clip_formats` | None for current text path. |
| HTML capture | Preserve HTML plus searchable text fallback | Done | `Clipboard::get().html`, `ClipFormat::html`, `html_to_text` | HTML preview is text-only; no rendered rich preview. |
| RTF capture | Preserve RTF payload | Gap | No RTF format in `ClipFormat`; `arboard` does not expose RTF | Add Windows-specific clipboard format access or a cross-platform crate that supports RTF. |
| Image capture | Preserve image payload | Done | `Clipboard::get_image`, `ClipFormat::image_rgba`, copy-back via `set_image` | Thumbnail preview is textual dimensions only. |
| File list capture | Preserve file paths / file-drop clipboard | Done | `Clipboard::get().file_list`, `ClipFormat::file_list`, copy-back via `set().file_list` | Does not copy file contents, matching clipboard file-list behavior. |
| PNG/JPG/BMP native format fidelity | Ditto stores multiple native image formats | Partial | RGBA image model stores decoded pixels | Native image container formats are normalized to RGBA through `arboard`; exact original format is not retained. |
| Color values | Ditto can treat copied color text as special content | Gap | No color detector/type | Add text classifier and optional color preview/filter. |
| Auto dedupe | Same content keeps one active record and can move to top | Done | `content_hash`, `save_clip_deduplicated`, duplicate GUI toggle | Consecutive vs historical duplicate policy is simplified to content-hash dedupe. |
| Copy timestamp | Save precise copy time | Done | `created_at`, `updated_at` Unix timestamps | UI currently shows raw timestamp, not localized date/time formatting. |
| Source application | Process name and icon | Gap | `source_app` exists but client does not populate it | Requires platform-specific active-window/process capture and icon extraction. |
| Persistent SQLite | Restart does not lose history | Done | `Store::open(paths::database_path())`, `clips`, `clip_formats` | None for local persistence. |

## 2. Quick Paste Panel

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Global hotkey opens panel | Default Ctrl+` anywhere | Partial | Configurable shortcut fields and in-window handlers | Current shortcuts work only while the client window has focus. Add platform global hotkey layer. |
| Floating near cursor | Popup near pointer and closes on blur | Gap | Main Makepad window only | Add tray/background process plus transient quick-paste window. |
| List recent N | Show numbered recent rows | Done | 20 GUI rows, `Store::list_clips`, `row_text` | Row count is fixed at 20 in code; make it GUI-configurable if required. |
| Content preview | Truncated text, type marker, time, source icon | Partial | Row text uses summary and i18n type labels | Time/source icon are not displayed in the row. |
| Keyboard navigation | Arrow selection, Enter paste, Esc close | Partial | Enter copies selected; number keys copy rows | Arrow and Esc handling are not implemented yet. |
| Number quick paste | 1-9 paste direct row | Done | Primary+number shortcut in `handle_key_down` | Current modifier requirement should be checked against desired no-modifier quick panel behavior. |
| Image hover thumbnail | Hover image preview | Gap | Image detail shows dimensions only | Add thumbnail cache/rendering in Makepad UI. |
| Window position memory | Fixed/follow cursor | Gap | No window positioning setting | Add GUI setting and platform window positioning. |

## 3. Search

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Real-time search | Filter as user types | Done | `TextInput::changed`, `Store::search_clips` | None for current simple search. |
| Full text fuzzy search | Fuzzy and optional pinyin initials | Partial | SQLite `LIKE` on `description` and `primary_text` | Add FTS5/fuzzy ranking and optional pinyin indexing. |
| Regex search | Optional regex mode | Gap | No regex setting or query path | Add GUI toggle and safe regex search path. |
| Type filter | Text/image/file filters | Gap | Capture type labels exist; no filter controls | Add type filter controls and query predicates. |
| Time filter | Date range filter | Gap | No date controls | Add date range UI and SQL predicates. |
| Source filter | By source application | Gap | Source capture not implemented | Depends on source application capture. |

## 4. Groups / Pinned

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Pin top | Important clips remain at top | Done | `pinned`, `toggle_clip_pinned`, order by `pinned DESC` | None for pin behavior. |
| Custom groups | Named groups and item membership | Gap | No groups table/model | Add `groups`, `clip_group_memberships`, GUI management. |
| Group hotkeys | Hotkey per group | Gap | No group model | Depends on groups and global hotkey layer. |
| Group management | Rename/sort/delete | Gap | No group GUI | Add settings panel section. |
| Templates | Variables like date/time/clipboard | Gap | No template engine | Add explicit template clip type and expansion rules. |

## 5. Paste Options

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Paste as plain text | Strip formatting | Partial | Text format is stored and can be copied when selected | No separate Shift+Enter/plain-text command yet. |
| Paste original format | Preserve original formats | Partial | File/image/HTML/text copy-back priority exists | Multi-format simultaneous restore is limited by `arboard`; HTML+text is preserved, other types are restored one primary type at a time. |
| Transform paste | Case/trim/newline transforms | Gap | No transform UI or commands | Add transform menu and pure functions with tests. |
| Merge paste | Merge multiple clips | Gap | Single selection only | Add multi-select model and separator setting. |
| Delayed paste | Delay before paste | Gap | No paste automation, only copy-back | Add paste injector and delay setting after global panel exists. |

## 6. Network Sync

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Multi-device sync | Share clips through network | Partial | `SyncClient`, `/api/sync/push`, `/api/sync/pull` | Sync is server-mediated, not peer-to-peer LAN. |
| LAN discovery | Auto-discover peers | Gap | Manual server URL in GUI | Add mDNS/UDP discovery if peer/server discovery is required. |
| Encryption | AES/shared-key style | Partial | HTTPS-capable HTTP client plus bearer token auth | Payload encryption at application level is not implemented. |
| Selective sync | All/text/pinned filters | Gap | No sync filter settings | Add settings and server/client filtering. |
| Sync log | Show sync events in GUI | Gap | `sync_events` exists internally | Add GUI log view and event details. |

## 7. Privacy / Security

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| App blacklist | Do not capture selected apps | Gap | No source app capture/filter | Depends on source application capture. |
| Content rules | Regex skip rules | Gap | No rule model | Add GUI-managed rules and tests. |
| Master password | Lock history view | Gap | No encryption/lock UI | Add encrypted store or app-level lock design. |
| Clear history | Delete all records | Gap | Single-delete exists | Add clear-all GUI action and sync semantics. |
| Secure erase | Overwrite database | Gap | SQLite soft delete only | Needs storage-level design; SQLite secure erase has tradeoffs. |
| Incognito/pause | Temporarily pause capture | Done | Auto capture toggle in GUI | Tray/shortcut pause is not implemented. |

## 8. System Integration

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Start at login | Auto-start minimized | Gap | No installer/login item | Add platform installer/startup integration. |
| System tray | Tray menu for open/pause/clear/exit | Gap | No tray integration | Add tray crate/platform layer. |
| Global hotkeys | All hotkeys user-configurable and conflict-aware | Partial | GUI shortcut fields and parser | No OS registration/conflict detection. |
| Context menu | OS right-click integration | Gap | No shell extension | Platform-specific extension required. |
| Multi-monitor popup | Open near current monitor/cursor | Gap | No popup positioning | Depends on quick-paste window. |

## 9. Data Management

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Export TXT/CSV/JSON | Export all or filtered | Gap | No export API/GUI | Add export commands and GUI flow. |
| Import backup | Restore history | Gap | No import API/GUI | Add importer with validation. |
| Auto backup | Scheduled database backup | Gap | No scheduler/setting | Add background scheduler and destination GUI setting. |
| Auto clean by age/count | Remove old records | Partial | Count-based `max_history` pruning | Age-based cleanup is not implemented. |
| Storage size limit | Cap database size | Gap | No size monitor | Add size accounting and cleanup policy. |

## 10. Settings Center

| Requirement | Ditto behavior | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Language | Multi-language GUI | Done | `i18n/en.json`, `i18n/zh.json`, language toggle | Add Japanese or more locales if required. |
| Theme | Light/dark/system | Partial | Light/dark toggle | Follow-system is not implemented. |
| Font size | Adjustable list font | Gap | No font setting | Add GUI setting and live style binding. |
| Max records | GUI setting | Done | `max_history_input` | "Unlimited" sentinel not implemented. |
| Sync port | Server bind config | Partial | Server CLI/env, client server URL GUI | Server port is not editable from desktop client GUI. |
| Hotkeys | Central GUI management | Partial | Hotkey settings panel | OS-global registration and conflict detection are missing. |

## 11. Non-Functional Requirements

| Requirement | Target | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- | --- |
| Panel latency | < 100 ms | Gap | No benchmark | Add startup/panel benchmark after quick panel exists. |
| Search latency | < 200 ms within 10k records | Gap | No benchmark; SQL LIKE only | Add FTS5 and benchmark fixture. |
| Memory | < 50 MB background | Gap | No measurement | Add release profiling workflow. |
| Compatibility | Windows 10/11 x64; optional macOS | Partial | CI builds Linux/macOS/Windows | Runtime clipboard behavior still needs manual OS QA. |
| Portable and installer packages | exe portable plus installer | Partial | CI uploads binaries | Installer/MSI/DMG packaging not implemented. |
| Crash recovery | Restore history after abnormal exit | Partial | SQLite persistence | No explicit crash-recovery test. |

## Release / Automation

| Requirement | yank status | Evidence | Gap / next work |
| --- | --- | --- | --- |
| Client binaries | Done | GitHub Actions `binaries` job builds `yank-client` on Linux/macOS/Windows | Packaging installers remains open. |
| Server binaries | Done | GitHub Actions `binaries` job builds `yank-server` on Linux/macOS/Windows | None for raw binaries. |
| Server Docker image | Done | Docker Buildx job builds multi-arch image from `Dockerfile` | Local Docker build not verified in this environment because Docker is unavailable. |
| GHCR push | Done | `ghcr.io/${{ github.repository }}` metadata/action config | Push only outside PRs. |
| Docker Hub push | Done | Optional `${DOCKERHUB_USERNAME}/yank` image when secrets exist | Requires `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` repository secrets. |

## Immediate Implementation Backlog

1. Add source app capture and row time formatting.
2. Add real quick-paste popup with arrow/Esc behavior.
3. Add global hotkey registration and conflict validation.
4. Add image thumbnail preview.
5. Add type/time filters and FTS5 search.
6. Add groups schema and GUI.
7. Add privacy filters before enabling source-app sensitive capture by default.
