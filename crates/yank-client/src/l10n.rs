//! Deferred messages keep background feedback translatable after a language change.
use nsh_core::{
    element::{IntoReactive, Reactive, SharedString},
    prelude::*,
};
use yank_core::{Language, i18n};
const PREFIX: &str = "\u{1e}yank-message:";
thread_local! { static LANGUAGE: std::cell::RefCell<Option<Signal<String>>> = const { std::cell::RefCell::new(None) }; }
pub fn init(language: String) {
    LANGUAGE.with(|slot| *slot.borrow_mut() = Some(signal(language)));
}
pub fn set(language: String) {
    LANGUAGE.with(|slot| {
        if let Some(value) = slot.borrow().as_ref()
            && value.get() != language
        {
            value.set(language);
        }
    });
}
pub fn language() -> Language {
    LANGUAGE.with(|slot| {
        slot.borrow().as_ref().map_or(Language::Zh, |value| {
            i18n::language_or_default(&value.get())
        })
    })
}
pub fn tr(source: &str) -> String {
    if LANGUAGE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|language| language.get() == "en-XA")
    }) {
        return i18n::pseudo_source_text(source);
    }
    i18n::source_text(language(), source)
}
pub fn message(source: &str, values: &[(&str, String)]) -> String {
    format!(
        "{PREFIX}{}",
        serde_json::to_string(&(source, values)).expect("message serializes")
    )
}
pub fn render(value: &str) -> String {
    if let Some(data) = value.strip_prefix(PREFIX)
        && let Ok((source, values)) = serde_json::from_str::<(String, Vec<(String, String)>)>(data)
    {
        let values = values
            .iter()
            .map(|(key, value)| {
                (
                    key.as_str(),
                    if value.starts_with(PREFIX) || matches!(key.as_str(), "error" | "e" | "path") {
                        render(value)
                    } else {
                        value.clone()
                    },
                )
            })
            .collect::<Vec<_>>();
        return i18n::interpolate(&tr(&source), &values);
    }
    tr(value)
}
/// Native labels always bind the locale signal, including otherwise static labels.
pub fn label(value: impl IntoReactive<SharedString>) -> Reactive<SharedString> {
    match value.into_reactive() {
        Reactive::Static(value) => Reactive::Bound(std::rc::Rc::new(move || render(&value).into())),
        Reactive::Bound(value) => Reactive::Bound(std::rc::Rc::new(move || {
            let value = value();
            if value.starts_with(PREFIX) {
                render(&value).into()
            } else {
                value
            }
        })),
    }
}
/// Arbitrary clipboard and user content is never translated by source alias.
pub fn text(value: impl IntoReactive<SharedString>) -> nsh_core::element::Text {
    let value = value.into_reactive();
    nsh_core::element::text(move || {
        let value = value.get();
        if value.starts_with(PREFIX) {
            render(&value)
        } else {
            value.to_string()
        }
    })
}
