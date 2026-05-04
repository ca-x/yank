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
    let raw = match language {
        Language::En => include_str!("../i18n/en.json"),
        Language::Zh => include_str!("../i18n/zh.json"),
    };
    serde_json::from_str(raw).expect("bundled i18n JSON must be valid")
}

pub fn bundle_json(language: Language) -> &'static str {
    match language {
        Language::En => include_str!("../i18n/en.json"),
        Language::Zh => include_str!("../i18n/zh.json"),
    }
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
}
