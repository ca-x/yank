# Third-party components and references

Yank source is available under MIT OR Apache-2.0. Dependencies retain their own licenses; Cargo.lock records exact resolved versions and sources. Distributors must include applicable dependency notices.

- NSH 0.5.0 (`longbridge/nsh`, commit `f4d68f7f9f31eb7b3ecc8a4cbc31096d5c6238a1`): native UI/runtime. No engine source patches are included in this repository.
- Iroh 1.4.1: authenticated encrypted QUIC peer transport and relay.
- rutis 0.21.0: Ditto adapter lifecycle.
- Protobuf/Prost: wire schema and generated Rust bindings, managed through Buf.
- SQLite/rusqlite: local and per-account storage.
- clipboard-rs, active-win-pos-rs and enigo: desktop clipboard, active-window metadata and authorized input integration.
- self_update and auto-launch: optional desktop updates and login startup.

Clips (https://clips.5km.tech/) is a functional/UI research reference only. Its implementation, branding and assets are not bundled. Yank has independent storage and synchronization semantics. WeChat input-method integration is excluded from this release.

Ditto protocol research and interoperability limits are documented in [docs/ditto-protocol.md](docs/ditto-protocol.md). No Ditto application source is bundled.
