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

/// Without Omarchy's roster: one locale per catalog Flea ships, English first, so the session's
/// language can still choose one. Names stay the codes, since only Omarchy's roster carries them.
pub(super) fn from_catalogs(dirs: &[std::path::PathBuf]) -> Vec<Locale> {
    let mut codes: Vec<String> = dirs
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .filter(|entry| entry.path().join("flea.json").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|code| valid_code(code) && code != "en")
        .collect();
    codes.sort();
    codes.dedup();
    let mut locales = vec![Locale::default()];
    locales.extend(codes.into_iter().map(|code| Locale {
        name: code.clone(),
        english_name: code.clone(),
        format_locale: code.clone(),
        glibc_locale: "C".into(),
        code,
        ..Locale::default()
    }));
    locales
}

// Sample input: "es_ES.UTF-8" or "pt_BR.utf8@euro"; C and POSIX name no language.
fn session_locale(value: &str) -> Option<(String, Option<String>)> {
    let name = value.split(['.', '@']).next()?;
    let (language, region) = name.split_once('_').map_or((name, None), |(l, r)| (l, Some(r)));
    let valid = (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_lowercase());
    valid.then(|| (language.to_owned(), region.filter(|r| r.len() == 2).map(str::to_ascii_uppercase)))
}

/// The roster entry the session's own locale picks, LC_ALL over LC_MESSAGES over LANG as gettext
/// reads them: the exact region, then the bare language, then any region of that language. The
/// entry then formats numbers and dates the session's way, since that locale is the user's choice.
pub(super) fn pick_from_session(roster: &mut [Locale], get: impl Fn(&str) -> Option<String>) -> Option<String> {
    let value = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| get(key).filter(|value| !value.is_empty()))?;
    let (language, region) = session_locale(&value)?;
    let exact = region.as_ref().map(|region| format!("{language}-{region}"));
    let regional = format!("{language}-");
    let at = roster
        .iter()
        .position(|l| Some(&l.code) == exact.as_ref())
        .or_else(|| roster.iter().position(|l| l.code == language))
        .or_else(|| roster.iter().position(|l| l.code.starts_with(&regional)))?;
    let locale = &mut roster[at];
    locale.glibc_locale = value.clone();
    locale.format_locale = exact.unwrap_or_else(|| language.clone());
    if let Some(mark) = super::format::decimal_mark(&value) {
        locale.decimal = mark;
    }
    super::format::set_grouping(locale);
    Some(locale.code.clone())
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

#[cfg(test)]
mod session_tests {
    use super::{pick_from_session, Locale};

    fn roster(codes: &[&str]) -> Vec<Locale> {
        codes.iter().map(|code| Locale { code: (*code).into(), ..Locale::default() }).collect()
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_owned())
    }

    #[test]
    fn the_exact_region_then_the_language_then_any_region() {
        let mut list = roster(&["en", "es-MX", "pt", "pt-BR"]);
        assert_eq!(pick_from_session(&mut list, env(&[("LANG", "es_ES.UTF-8")])).as_deref(), Some("es-MX"));
        assert_eq!(list[1].glibc_locale, "es_ES.UTF-8");
        assert_eq!(list[1].format_locale, "es-ES");
        assert_eq!(pick_from_session(&mut list, env(&[("LANG", "pt_BR.UTF-8")])).as_deref(), Some("pt-BR"));
        assert_eq!(pick_from_session(&mut list, env(&[("LANG", "pt_PT.UTF-8")])).as_deref(), Some("pt"));
    }

    #[test]
    fn lc_all_wins_and_c_or_an_unknown_language_picks_nothing() {
        let mut list = roster(&["en", "de", "fr"]);
        let pairs = [("LC_ALL", "fr_FR.UTF-8"), ("LC_MESSAGES", "de_DE.UTF-8"), ("LANG", "en_US.UTF-8")];
        assert_eq!(pick_from_session(&mut list, env(&pairs)).as_deref(), Some("fr"));
        assert_eq!(pick_from_session(&mut list, env(&[("LANG", "C.UTF-8")])), None);
        assert_eq!(pick_from_session(&mut list, env(&[("LANG", "ja_JP.UTF-8")])), None);
        assert_eq!(pick_from_session(&mut list, env(&[])), None);
    }
}
