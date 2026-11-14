# Native workspace refinement

The desktop remains Rust NSH. System window chrome is the default; optional
traffic lights share the sidebar brand row, with a compact header fallback.
The visual direction is a compact clipboard tool, not a dashboard.

- A 220-point sidebar is for destinations. Content kinds belong to history chips.
- History rows have bounded previews, type artwork, time, a favorite mark and
  trailing shortcut keycaps. Semantic surfaces separate levels.
- Details open in reading mode. Paste/copy are primary; editing, queue, favorite,
  float and delete are secondary; metadata and Ditto are low-frequency actions.
- Drafts survive selection changes. Saving or discarding is explicit; destructive
  operations retain confirmation and existing replicated tombstone semantics.
- Settings use visible choices, trailing switches, a centered reading width and
  an explicit draft save bar. Saving must not silently overwrite a newer snapshot.
- Success feedback is transient, failures dismissible, connection state persistent.
- Chinese and English, light/dark, 1180×780 and narrow layouts are acceptance targets.
  Text expansion tests supplement, but do not replace, real screenshots.
- Keep the existing NSH updater and package-manager safety gates unchanged.

Validation: focused UI and Web tests, workspace tests, formatting, clippy, real
native and browser screenshots, then tagged release with verified artifacts.
