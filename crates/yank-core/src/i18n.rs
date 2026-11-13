use crate::Language;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct I18nBundle {
    pub locale: String,
    pub messages: BTreeMap<String, String>,
}

impl I18nBundle {
    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        self.messages.get(key).map(String::as_str).unwrap_or(key)
    }
}

pub fn bundle(language: Language) -> I18nBundle {
    cached_bundle(language).clone()
}

pub fn bundle_json(language: Language) -> &'static str {
    match language {
        Language::En => include_str!("../i18n/en.json"),
        Language::Zh => include_str!("../i18n/zh.json"),
    }
}

/// Bundles and the source alias index are parsed once for the life of the process.
pub fn cached_bundle(language: Language) -> &'static I18nBundle {
    use std::sync::OnceLock;
    static EN: OnceLock<I18nBundle> = OnceLock::new();
    static ZH: OnceLock<I18nBundle> = OnceLock::new();
    match language {
        Language::En => EN.get_or_init(|| {
            serde_json::from_str(bundle_json(Language::En)).expect("valid English catalog")
        }),
        Language::Zh => ZH.get_or_init(|| {
            serde_json::from_str(bundle_json(Language::Zh)).expect("valid Chinese catalog")
        }),
    }
}

/// Only the two shipped languages are supported; invalid locale values use Chinese.
pub fn language_or_default(locale: &str) -> Language {
    match locale {
        "en" => Language::En,
        _ => Language::Zh,
    }
}

/// Source aliases let native controls share the catalog without translating user content.
pub fn source_text(language: Language, source: &str) -> String {
    use std::sync::OnceLock;
    static SOURCES: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    let sources = SOURCES.get_or_init(|| {
        cached_bundle(Language::En)
            .messages
            .iter()
            .chain(cached_bundle(Language::Zh).messages.iter())
            .filter(|(key, _)| key.starts_with("client."))
            .map(|(key, value)| (value.as_str(), key.as_str()))
            .collect()
    });
    sources.get(source).map_or_else(
        || source.to_owned(),
        |key| cached_bundle(language).text(key).to_owned(),
    )
}

/// Interpolate named placeholders in one pass. Values are never interpreted as templates.
pub fn interpolate(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &rest[start + 1..start + end];
        if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
            out.push_str(value);
        } else {
            out.push_str(&rest[start..=start + end]);
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn format(language: Language, key: &str, values: &[(&str, String)]) -> String {
    interpolate(cached_bundle(language).text(key), values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_i18n_files_are_valid() {
        for language in [Language::En, Language::Zh] {
            let bundle = bundle(language);
            assert!(!bundle.messages.is_empty());
            assert!(bundle.messages.contains_key("app.title"));
            assert!(bundle.messages.contains_key("admin.refresh"));
        }
    }

    #[test]
    fn bundled_i18n_files_have_matching_keys() {
        let en = bundle(Language::En);
        let zh = bundle(Language::Zh);
        let en_keys = en.messages.keys().collect::<Vec<_>>();
        let zh_keys = zh.messages.keys().collect::<Vec<_>>();

        assert_eq!(en_keys, zh_keys);
    }

    #[test]
    fn catalogs_are_cached_and_invalid_locales_fall_back_to_chinese() {
        assert!(std::ptr::eq(
            cached_bundle(Language::En),
            cached_bundle(Language::En)
        ));
        assert_eq!(cached_bundle(language_or_default("fr")).locale, "zh");
        assert_eq!(cached_bundle(language_or_default("")).locale, "zh");
        assert_eq!(source_text(Language::En, "设置"), "Settings");
        assert_eq!(
            source_text(Language::Zh, "Invalid device identity"),
            "设备身份无效"
        );
        assert_eq!(
            cached_bundle(Language::En).text("missing.key"),
            "missing.key"
        );
    }
    #[test]
    fn interpolation_preserves_unknown_placeholders_and_literal_values() {
        assert_eq!(
            interpolate(
                "{count}: {value}; {missing}",
                &[("count", "2".into()), ("value", "{count}".into())]
            ),
            "2: {count}; {missing}"
        );
        assert_eq!(interpolate("trailing {", &[]), "trailing {");
    }
    #[test]
    fn translations_have_identical_placeholders() {
        fn placeholders(template: &str) -> std::collections::BTreeSet<&str> {
            template
                .split('{')
                .skip(1)
                .filter_map(|part| part.split_once('}').map(|(key, _)| key))
                .collect()
        }
        let en = cached_bundle(Language::En);
        let zh = cached_bundle(Language::Zh);
        for (key, value) in &en.messages {
            assert_eq!(
                placeholders(value),
                placeholders(zh.text(key)),
                "placeholder mismatch: {key}"
            );
        }
    }
}
