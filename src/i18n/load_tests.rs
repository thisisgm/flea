use super::tests::Fixture;
use super::*;
use crate::jsondoc::{self, Json};

#[test]
fn i50_font_language_file_replace() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "de");
    f.write("installed/i18n/de/flea.json", r#"{"folder.empty":"Leer"}"#);
    let held = I18n::load(f.paths());
    f.write("config/omarchy/replacement", "ja");
    std::fs::rename(
        f.path("config/omarchy/replacement"),
        f.path("config/omarchy/language"),
    )
    .unwrap();
    assert!(!held.sources_unchanged());
    let next = held.prepare_changed().expect("stable replacement");
    assert_eq!(held.language(), "de");
    assert_eq!(held.tr("folder.empty"), "Leer");
    assert_eq!(next.language(), "ja");
    assert_eq!(next.locale().glibc_locale, "ja_JP.UTF-8");
    assert!(next.sources_unchanged());
    assert!(next.prepare_changed().is_none());
}

#[test]
fn i50_font_snapshot_catalog_coherent() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "de");
    f.write("installed/i18n/de/flea.json", r#"{"folder.empty":"Leer"}"#);
    let held = I18n::load(f.paths());
    f.write("installed/i18n/de/flea.json", r#"{"folder.empty":"Neu"}"#);
    let prepared = held.prepare_changed().unwrap();
    assert_eq!(held.tr("folder.empty"), "Leer");
    assert_eq!(prepared.tr("folder.empty"), "Neu");
    f.write("config/omarchy/replacement", "fa");
    std::fs::rename(
        f.path("config/omarchy/replacement"),
        f.path("config/omarchy/language"),
    )
    .unwrap();
    assert!(!prepared.sources_unchanged());
    assert_eq!(prepared.language(), "de");
    assert_eq!(prepared.locale().glibc_locale, "de_DE.UTF-8");
    assert_eq!(prepared.tr("folder.empty"), "Neu");
    let newest = prepared.prepare_changed().unwrap();
    assert_eq!(newest.language(), "fa");
    assert!(newest.locale().right_to_left);
    assert_eq!(newest.trn("items.count", 2), "۲ items");
}

#[test]
fn i50_font_held_text_covers_every_roster_script() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "de");
    f.write(
        "installed/i18n/de/flea.json",
        r#"{"folder.empty":"Leer","items.count":{"one":"%1 Ding","other":"%1 Dinge"}}"#,
    );
    let held = I18n::load(f.paths()).held_text();
    for word in ["العربية", "日本語", "Dansk", "Leer", "Dinge"] {
        assert!(word.chars().all(|c| held.contains(c)), "{word} is not held");
    }
}

#[test]
fn i50_font_held_text_holds_every_formatted_number_mark() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "fa");
    let catalog = I18n::load(f.paths());
    let held = catalog.held_text();
    let shown = [
        catalog.locale().format_count(1234567890),
        catalog.locale().format_number(1.5, 1),
    ]
    .concat();
    assert!(shown.contains('۹') && shown.contains('٫'), "{shown}");
    assert!(
        shown.chars().all(|c| held.contains(c)),
        "{shown} is not held"
    );
}

fn wide_object(count: usize, duplicate: bool) -> String {
    let entries: Vec<String> = (0..count).map(|n| {
        let key = if duplicate { 0 } else { n };
        format!("\"k{key:06}\":\"x\"")
    }).collect();
    format!("{{{}}}", entries.join(","))
}

// A complete-language budget of 4096 labels, errors and plural messages, rather than tiny values.
fn realistic_language_catalog() -> String {
    let pairs = (0..4096).map(|n| {
        let value = if n % 4 == 0 {
            Json::Obj(vec![
                ("one".into(), Json::Str("%1 item selected in %2".into())),
                ("many".into(), Json::Str("%1 items selected in %2".into())),
                ("other".into(), Json::Str("%1 items selected in %2".into())),
            ])
        } else {
            Json::Str("Could not open %1 in %2. Check the file permissions and try again.".into())
        };
        (format!("surface.{:02}.action.{:02}", n / 64, n % 64), value)
    }).collect();
    jsondoc::render(&Json::Obj(pairs))
}

#[test]
fn linear_document_keeps_json_values_escapes_and_last_duplicate_without_member_rescans() {
    let value = document::parse(r#"{"same":"old","same":"new","text":"é\\\"\n\u0041","array":[null,true,false,-1.5e2,{"n":1}]}"#).unwrap();
    assert_eq!(value.get("same").and_then(Json::as_str), Some("new"));
    assert_eq!(value.get("text").and_then(Json::as_str), Some("é\\\"\nA"));
    let array = value.get("array").and_then(Json::as_array).unwrap();
    assert_eq!(array.len(), 5);
    assert_eq!(array[0], Json::Null);
    assert_eq!(array[1].as_bool(), Some(true));
    assert_eq!(array[2].as_bool(), Some(false));
    assert_eq!(array[3].as_f64(), Some(-150.0));
    assert_eq!(array[4].get("n").and_then(Json::as_f64), Some(1.0));
    let text = realistic_language_catalog();
    assert!(text.len() < document::MAX_BYTES);
    let catalog = catalog::parse_checked(&text, false).unwrap();
    assert_eq!(catalog.len(), 4096);
    assert_eq!(catalog["surface.00.action.00"].get("other").and_then(Json::as_str), Some("%1 items selected in %2"));
}

#[test]
fn bounded_document_rejects_wide_nested_duplicate_and_array_work() {
    let accepted = wide_object(document::MAX_ENTRIES, false);
    assert_eq!(catalog::parse_checked(&accepted, false).unwrap().len(), document::MAX_ENTRIES);
    for duplicate in [false, true] {
        let text = wide_object(document::MAX_ENTRIES + 1, duplicate);
        assert!(text.len() < document::MAX_BYTES);
        assert!(catalog::parse_checked(&text, false).unwrap_err().contains("exceeds 16384 entries"));
        let nested = format!("{{\"nested\":{text}}}");
        assert!(document::parse(&nested).unwrap_err().contains("exceeds 16384 entries"));
    }
    let array = format!("{{\"nested\":[{}]}}", vec!["0"; document::MAX_ENTRIES].join(","));
    assert!(document::parse(&array).unwrap_err().contains("exceeds 16384 entries"));
    let too_deep = format!("{}0{}", "[".repeat(jsondoc::MAX_DEPTH), "]".repeat(jsondoc::MAX_DEPTH));
    assert!(document::parse(&too_deep).unwrap_err().contains("nesting limit"));
}

#[test]
fn bounded_document_rejects_oversize_and_malformed_json_with_clear_errors() {
    let oversized = " ".repeat(document::MAX_BYTES + 1);
    assert_eq!(document::parse(&oversized).unwrap_err(), "document exceeds 1048576 bytes");
    for text in ["", "{", "[1,]", "{\"a\":1,}", "{\"a\" 1}", "true false", "01", "1e400", "\"\\q\"", "\"\\u+041\"", "\"raw\nline\""] {
        assert!(document::parse(text).is_err(), "accepted invalid JSON: {text}");
    }
}

#[test]
fn runtime_load_refuses_entry_and_byte_limits_without_accepting_partial_catalogs() {
    let f = Fixture::new();
    f.registry();
    let text = wide_object(40_000, false);
    assert_eq!(text.len(), 560_001);
    f.write("installed/i18n/en/flea.json", &text);
    assert!(catalog::load(&f.path("installed/i18n/en/flea.json"), false).is_empty());
    let i = I18n::load(f.paths());
    assert_eq!(i.tr("k000000"), "k000000");
    assert_eq!(i.tr("folder.empty"), "This folder is empty");
    let text = wide_object(280_000, false);
    assert_eq!(text.len(), 3_920_001);
    f.write("installed/i18n/en/flea.json", &text);
    assert!(catalog::read(&f.path("installed/i18n/en/flea.json")).is_none());
    let i = I18n::load(f.paths());
    assert_eq!(i.tr("k000000"), "k000000");
    assert_eq!(i.tr("folder.empty"), "This folder is empty");
    f.write("omarchy/default/i18n/locales.json", &wide_object(40_000, false));
    let i = I18n::load(f.paths());
    assert_eq!(i.roster().len(), 1);
    assert_eq!(i.language(), "en");
}

#[test]
fn shared_clock_preserves_trash_timestamp_and_localized_date_fields() {
    let before = crate::backend::trashdate::deletion_date_local();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let locale = Locale { glibc_locale: "C".into(), ..Locale::default() };
    let date = locale.format_date(now).unwrap();
    let after = crate::backend::trashdate::deletion_date_local();
    assert_eq!(before.len(), 19);
    assert_eq!(after.len(), 19);
    assert_eq!(&before[10..11], "T");
    assert_eq!(&after[10..11], "T");
    assert!([before[..16].replace('T', " "), after[..16].replace('T', " ")].contains(&date));
    assert!(crate::backend::trashdate::local_time(i64::MAX).is_none());
}

#[cfg(not(debug_assertions))]
#[test]
fn release_realistic_catalog_public_load_stays_below_250ms_in_three_runs() {
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "de");
    let text = realistic_language_catalog();
    f.write("installed/i18n/en/flea.json", &text);
    f.write("installed/i18n/de/flea.json", &text);
    let mut elapsed = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let i = I18n::load(f.paths());
        let time = start.elapsed();
        assert_eq!(i.language(), "de");
        assert_eq!(i.trn_args("surface.00.action.00", 2, &["Work"]), "2 items selected in Work");
        assert_eq!(i.tr("surface.63.action.63"), "Could not open %1 in %2. Check the file permissions and try again.");
        assert!(time < Duration::from_millis(250), "full-language load exceeded 250 ms: {time:?}");
        elapsed.push(time.as_secs_f64() * 1000.0);
    }
    elapsed.sort_by(f64::total_cmp);
    eprintln!("i18n release full-language public load: 4096 entries, {} bytes per catalog, two catalogs, 3 runs, {:.3}..{:.3} ms, bound 250 ms", text.len(), elapsed[0], elapsed[2]);
    let hostile = wide_object(280_000, false);
    f.write("installed/i18n/de/flea.json", &hostile);
    let mut elapsed = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let i = I18n::load(f.paths());
        let time = start.elapsed();
        assert_eq!(i.tr("k000000"), "k000000");
        assert_eq!(i.tr("folder.empty"), "This folder is empty");
        assert!(time < Duration::from_millis(250), "oversize refusal exceeded 250 ms: {time:?}");
        elapsed.push(time.as_secs_f64() * 1000.0);
    }
    elapsed.sort_by(f64::total_cmp);
    eprintln!("i18n release oversized public load: {} bytes refused, 3 runs, {:.3}..{:.3} ms, bound 250 ms", hostile.len(), elapsed[0], elapsed[2]);
}
