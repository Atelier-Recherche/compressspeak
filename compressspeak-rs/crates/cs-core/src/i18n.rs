//! Simple JSON-backed i18n (fr/en/de/es).

use crate::config::Language;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;

static STRINGS: RwLock<Option<HashMap<String, HashMap<String, String>>>> = RwLock::new(None);
static LANG: RwLock<Language> = RwLock::new(Language::Fr);

const EMBEDDED: &str = include_str!("../../../assets/i18n/strings.json");

pub fn init() {
    let map: HashMap<String, HashMap<String, String>> =
        serde_json::from_str(EMBEDDED).unwrap_or_default();
    *STRINGS.write().unwrap() = Some(map);
}

pub fn set_language(lang: Language) {
    *LANG.write().unwrap() = lang;
}

pub fn language() -> Language {
    *LANG.read().unwrap()
}

pub fn tr(key: &str) -> String {
    let lang = language().code();
    let guard = STRINGS.read().unwrap();
    if let Some(map) = guard.as_ref() {
        if let Some(entry) = map.get(key) {
            if let Some(s) = entry.get(lang) {
                return s.clone();
            }
            if let Some(s) = entry.get("fr") {
                return s.clone();
            }
            if let Some(s) = entry.get("en") {
                return s.clone();
            }
        }
    }
    key.to_string()
}

pub fn tr_args(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = tr(key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Load from Value for tests / hot reload.
#[allow(dead_code)]
pub fn load_from_value(v: Value) {
    if let Ok(map) = serde_json::from_value(v) {
        *STRINGS.write().unwrap() = Some(map);
    }
}
