//! tests/js/trashdates.js, every check carried over.
use super::dates::{day_number, deleted, expired, location, parse_ms, DAY_MS};
use super::words::{deleted_text, location_text};
use crate::i18n::{I18n, Paths};

pub(super) fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pl3");
    I18n::load(Paths { omarchy: none.clone(), language_file: none, catalogs: Vec::new() })
}

// Date.toISOString(): UTC with milliseconds and a Z, through civil_from_days.
fn iso(ms: i64) -> String {
    let (days, rest) = (ms.div_euclid(DAY_MS), ms.rem_euclid(DAY_MS));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s, milli) = (rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{milli:03}Z")
}

#[test]
fn trashdates_labels_are_one_stamp_and_unknown_is_honest() {
    let i18n = english();
    let label = |text: Option<&str>| deleted_text(&i18n, text);
    assert_eq!(label(Some("2026-09-08T01:00:00")), "2026-09-08 01:00", "this morning");
    assert_eq!(label(Some("2026-09-07T23:00:00")), "2026-09-07 23:00", "last night");
    assert_eq!(label(Some("2026-08-30T12:00:00")), "2026-08-30 12:00", "an older day");
    assert_eq!(label(Some("2025-08-30T12:00:00")), "2025-08-30 12:00", "an older year");
    assert_eq!(label(Some("2026-09-08T01:00:00")).chars().count(), 16, "the stamp is always sixteen characters");
    assert_eq!(label(Some("")), "Unknown", "missing date is honest");
    assert_eq!(label(Some("not a date")), "Unknown", "an unreadable date is honest too");
    assert_eq!(label(None), "Unknown", "a null or absent date has no label either");
    assert_eq!(deleted(Some("")), None, "an empty date has no label");
    assert_eq!(deleted(Some("2026-13-01T00:00:00")), None, "a month past December is unreadable");
    assert_eq!(deleted(Some("2026-02-30T00:00:00")), None, "a day past the month is unreadable");
}

#[test]
fn trashdates_location_is_home_relative_and_unknown_without_a_folder() {
    let i18n = english();
    let at = |original: &str| location_text(&i18n, original, "/home/gm");
    assert_eq!(at("/home/gm/Pictures/photo.png"), "~/Pictures", "home child location");
    assert_eq!(at("/home/gm/photo.png"), "~", "home file location");
    assert_eq!(at("/home/gm-other/photo.png"), "/home/gm-other", "similar home prefix stays absolute");
    assert_eq!(at("/photo.png"), "/", "root location");
    assert_eq!(at("photo.png"), "Unknown", "missing location is honest");
    assert_eq!(location("/home/gm/a.txt", ""), Some("/home/gm".into()), "no home leaves the folder absolute");
}

#[test]
fn trashdates_sweep_takes_thirty_days_and_never_an_unreadable_date() {
    let sweep_now = parse_ms("2026-09-11T12:00:00").unwrap();
    let day = DAY_MS;
    assert!(expired(Some(&iso(sweep_now - 30 * day)), sweep_now, 30), "an item deleted 30 days ago is swept");
    assert!(expired(Some(&iso(sweep_now - 31 * day)), sweep_now, 30), "and one deleted 31 days ago is too");
    assert!(!expired(Some(&iso(sweep_now - 29 * day)), sweep_now, 30), "one deleted 29 days ago is left alone");
    assert!(!expired(Some(&iso(sweep_now - 3_600_000)), sweep_now, 30), "one deleted this morning is left alone");
    assert!(!expired(Some("not a date"), sweep_now, 30), "an unreadable date is never swept");
    assert!(!expired(Some(""), sweep_now, 30), "an empty date is never swept");
    assert!(!expired(None, sweep_now, 30), "an absent or null date is never swept");
    assert!(!expired(Some(&iso(sweep_now + 5 * day)), sweep_now, 30), "a date in the future is never swept");
    assert_eq!(parse_ms(&iso(sweep_now)), Some(sweep_now), "a zoned stamp reads back to its instant");
    assert_eq!(parse_ms("2026-09-11T12:00:00+02:00"), parse_ms("2026-09-11T10:00:00Z"), "an offset is honoured");
}

#[test]
fn trashdates_day_number_is_once_a_day() {
    let at = |text: &str| day_number(parse_ms(text).unwrap());
    assert_eq!(at("2026-09-11T00:01:00"), at("2026-09-11T23:59:00"), "the day number is the same all day");
    assert_eq!(at("2026-09-12T09:00:00") - at("2026-09-11T09:00:00"), 1, "and the next day is one more");
    assert!(at("2026-09-11T12:00:00") > 0, "a fresh ui.json has never swept, and today is never day zero");
}
