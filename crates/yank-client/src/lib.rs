//! Yank native application, shared across desktop and Android.
mod clipboard;
mod integration;
mod l10n;
mod network;
mod sync;
mod ui;
#[cfg(not(target_os = "android"))]
mod updater;
mod worker;
use nsh_core::prelude::*;
#[native_module("yank/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}
#[component]
pub fn App() -> impl IntoElement {
    ui::app()
}
