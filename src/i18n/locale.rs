use super::document;
use crate::jsondoc::Json;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locale {
    pub code: String,
    pub name: String,
    pub english_name: String,
    pub format_locale: String,
    pub glibc_locale: String,
    pub decimal: String,
    pub zero_digit: char,
    pub right_to_left: bool,
    pub(super) separator: String,
    pub(super) grouping: Vec<u8>,
}

impl Default for Locale {
    fn default() -> Self {
        Self {
            code: "en".into(), name: "English".into(), english_name: "English".into(),
            format_locale: "en-US".into(), glibc_locale: "en_US.UTF-8".into(),
            decimal: ".".into(), zero_digit: '0', right_to_left: false,
            separator: ",".into(), grouping: vec![3],
        }
    }
}

// Sample input: "en", "fil" or "pt-BR"; no directory separators are accepted.
pub(super) fn valid_code(code: &str) -> bool {
    let (language, region) = code.split_once('-').map_or((code, None), |(a, b)| (a, Some(b)));
    (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_lowercase())
        && region.map_or(true, |r| r.len() == 2 && r.bytes().all(|b| b.is_ascii_alphabetic()))
}

// Sample input: {"de":{"name":"Deutsch","formatLocale":"de-DE","locale":"de_DE.UTF-8","decimal":","}}.
pub(super) fn parse(text: &str, path: &Path) -> Vec<Locale> {
    let mut locales = Vec::new();
    let document = if text.is_empty() { Ok(Json::Null) } else { document::parse(text) };
    if let Err(error) = &document {
        eprintln!("flea: refusing i18n roster {}: {error}", path.display());
    }
    if let Ok(Json::Obj(entries)) = document {
        for (code, entry) in entries {
            if !valid_code(&code) || entry.as_object().is_none() {
                continue;
            }
            let string = |key| entry.get(key).and_then(Json::as_str).filter(|s| !s.is_empty());
            let zero_digit = string("zeroDigit").filter(|s| s.chars().count() == 1)
                .and_then(|s| s.chars().next())
                .filter(|c| c.is_numeric() && char::from_u32(*c as u32 + 9).is_some())
                .unwrap_or('0');
            let mut locale = Locale {
                name: string("name").unwrap_or(&code).into(),
                english_name: string("englishName").unwrap_or(&code).into(),
                format_locale: string("formatLocale").unwrap_or("en-US").into(),
                glibc_locale: string("locale").unwrap_or("C").into(),
                decimal: string("decimal").unwrap_or(".").into(), zero_digit,
                right_to_left: string("direction") == Some("rtl"), code,
                separator: String::new(), grouping: Vec::new(),
            };
            super::format::set_grouping(&mut locale);
            locales.push(locale);
        }
    }
    if !locales.iter().any(|l| l.code == "en") {
        locales.push(Locale::default());
    }
    locales.sort_by(|a, b| (a.code != "en", &a.english_name).cmp(&(b.code != "en", &b.english_name)));
    locales
}
