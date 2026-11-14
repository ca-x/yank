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
    if let Some(value) = cached_bundle(language).messages.get(source) {
        return value.clone();
    }
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

pub fn pseudo_source_text(source: &str) -> String {
    let known = [Language::En, Language::Zh].iter().any(|language| {
        cached_bundle(*language)
            .messages
            .iter()
            .any(|(key, value)| key.starts_with("client.") && (key == source || value == source))
    });
    if !known {
        return source.to_owned();
    }
    let translated = source_text(Language::En, source);
    let padding = "·".repeat((translated.chars().count() as f32 * 0.4).ceil() as usize);
    format!("⟦{translated}{padding}⟧")
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
    fn pseudo_language_expands_labels_but_preserves_content_and_placeholders() {
        let source = "client.detail.edit";
        let english = source_text(Language::En, source);
        let expanded = pseudo_source_text(source);
        assert!(expanded.contains(&english));
        assert!(expanded.chars().count() as f32 >= english.chars().count() as f32 * 1.4);
        assert_eq!(
            pseudo_source_text("my clipboard {0} 中文"),
            "my clipboard {0} 中文"
        );
        assert!(pseudo_source_text("已删除 {0} 条记录").contains("{0}"));
        for language in [Language::En, Language::Zh] {
            assert!(
                cached_bundle(language)
                    .messages
                    .keys()
                    .all(|key| { !key.starts_with("app.") && !key.starts_with("client.message_") })
            );
        }
    }

    #[test]
    fn bundled_i18n_files_are_valid() {
        for language in [Language::En, Language::Zh] {
            let bundle = bundle(language);
            assert!(!bundle.messages.is_empty());
            assert!(bundle.messages.contains_key("client.detail.edit"));
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
        assert_eq!(source_text(Language::En, "复制"), "Copy");
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
