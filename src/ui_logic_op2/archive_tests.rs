//! Port of tests/js/archive.js and tests/js/archives.js: tables, capabilities, submenu and routing.
use super::archive::*;
use super::english;

fn both() -> Caps {
    Caps { seven_zip: true, zip: true, archive: true }
}

#[test]
fn no_second_copy_of_mn1s_menu_logic() {
    let source = include_str!("archive.rs");
    for banned in [
        "fn image_kind",
        "fn is_disk_image",
        "fn extract_hint_key",
        "fn extract_entry(",
        "enum Extract",
        "fn format_entries",
        "struct FormatEntry",
        "fn password_entry",
        "enum PasswordEntry",
    ] {
        assert!(!source.contains(banned), "mn1 owns the menu logic, op2 keeps no copy: {banned}");
    }
}

#[test]
fn longest_extension_matches_first() {
    assert!(is_archive("backup.tar.gz"));
    assert!(!is_archive("backup.gz"));
    assert_eq!(extract_dir("backup.tar.gz"), "backup");
}

#[test]
fn every_tabled_form_is_an_archive() {
    for ext in [".zip", ".7z", ".tgz", ".tar", ".tar.xz", ".tar.zst", ".tar.bz2"] {
        assert!(is_archive(&format!("x{ext}")), "{ext}");
    }
}

#[test]
fn rar_reads_in_any_case_and_unpacks_under_its_name() {
    assert!(is_archive("holiday.rar"));
    assert!(is_archive("HOLIDAY.RAR"));
    assert_eq!(extract_dir("holiday.rar"), "holiday");
}

#[test]
fn nothing_else_is_an_archive() {
    assert!(!is_archive("notes.txt"));
    assert!(!is_archive("x.zipper"));
    assert!(!is_archive("zip"));
}

#[test]
fn extract_unpacks_into_the_archives_own_name() {
    assert_eq!(extract_dir("photos.zip"), "photos");
    assert_eq!(extract_dir("backup.tar.zst"), "backup");
    assert_eq!(extract_dir("plain"), "plain");
}

#[test]
fn both_tools_read_every_class() {
    for ext in [".zip", ".7z", ".tar.gz", ".rar", ".tgz"] {
        assert!(can_extract(&format!("x{ext}"), both()), "{ext}");
    }
}

#[test]
fn seven_zip_alone_reads_its_class_and_zip_but_no_tar() {
    let caps = Caps { seven_zip: true, zip: true, archive: false };
    assert!(can_extract("x.zip", caps));
    assert!(can_extract("x.rar", caps));
    assert!(!can_extract("x.tar.zst", caps));
}

#[test]
fn bsdtar_alone_reads_everything_but_7z() {
    let caps = Caps { seven_zip: false, zip: true, archive: true };
    assert!(can_extract("x.zip", caps));
    assert!(!can_extract("x.7z", caps));
}

#[test]
fn neither_tool_reads_nothing() {
    let caps = Caps::default();
    assert!(!can_extract("x.zip", caps));
    assert!(!can_extract("x.tar", caps));
}

#[test]
fn exact_boundaries_hold() {
    assert!(can_extract(".zip", Caps { zip: true, ..Caps::default() }));
    assert!(can_extract(".rar", Caps { zip: true, ..Caps::default() }));
    assert!(can_extract(".7z", Caps { seven_zip: true, ..Caps::default() }));
}

#[test]
fn one_row_compresses_under_its_name() {
    assert_eq!(archive_stem(&["notes.txt"], "claude"), "notes");
    assert_eq!(archive_stem(&["a.txt", "b.txt"], "claude"), "claude");
    assert_eq!(archive_stem(&["a.txt", "b.txt"], ""), "archive");
}

#[test]
fn widened_formats_read_through_7z_alone() {
    for ext in [".deb", ".rpm", ".apk", ".jar", ".cab", ".cpio", ".AppImage", ".iso"] {
        assert!(is_archive(&format!("x{ext}")), "{ext}");
    }
    assert_eq!(extract_dir("driver.deb"), "driver");
    assert_eq!(extract_dir("debian-13.iso"), "debian-13");
    assert!(!can_extract("debian-13.iso", both()));
    assert!(!can_extract("debian-13.ISO", both()));
    for ext in [".deb", ".rpm", ".apk", ".jar", ".cab", ".cpio", ".AppImage"] {
        assert!(can_extract(&format!("x{ext}"), both()), "{ext}");
    }
    let tar_only = Caps { seven_zip: false, zip: true, archive: true };
    assert!(!can_extract("x.deb", tar_only));
    assert!(!can_extract("x.iso", tar_only));
}

#[test]
fn backend_codes_route_never_sentences() {
    assert!(needs_password("encrypted"));
    assert!(!needs_password("the archive tool wrote nothing"));
    assert!(wrong_password("wrong-password"));
    assert!(!wrong_password("the archive tool failed"));
    assert_eq!(done_action(false, "encrypted"), "password");
    assert_eq!(done_action(false, "wrong-password"), "wrong");
    assert_eq!(done_action(false, "the archive tool failed"), "");
    assert_eq!(done_action(true, ""), "");
}

#[test]
fn board_offers_7z_first_whatever_the_backend_order() {
    assert_eq!(board_formats(&["zip", "tar", "7z"]), vec!["7z".to_string(), "zip".to_string()]);
    assert_eq!(board_formats(&[]), vec!["7z".to_string()]);
}

#[test]
fn backend_stopped_names_the_archive_or_not() {
    let i18n = english();
    assert_eq!(
        backend_stopped_line(&i18n, Some("backup.7z")),
        "backup.7z could not be opened because the Flea backend stopped; reopen Flea and try again."
    );
    assert_eq!(
        backend_stopped_line(&i18n, None),
        "This archive could not be opened because the Flea backend stopped; reopen Flea and try again."
    );
}

#[test]
fn menu_submenu_and_password_read_through_mn1() {
    use crate::ui_logic_mn1::{rows, Text};
    let dotted = rows::compress_entries(&["zip".into(), "tar.zst".into()]);
    assert_eq!(dotted.len(), 2, "a dotted format keeps its own row");
    assert_eq!(
        dotted.iter().map(|e| e.id.clone()).collect::<Vec<_>>().join("|"),
        "zip|tar.zst",
        "dotted rows carry their full label"
    );
    assert_eq!(
        dotted
            .iter()
            .map(|e| match &e.label {
                Some(Text::Data(label)) => label.clone(),
                _ => String::new(),
            })
            .collect::<Vec<_>>(),
        vec![".zip".to_string(), ".tar.zst".to_string()],
        "each format label names its own extension"
    );
    let entries = rows::compress_entries(&["zip".into(), "7z".into()]);
    let ids: Vec<&str> =
        entries.iter().map(|e| if e.separator { "sep" } else { e.id.as_ref() }).collect();
    assert_eq!(ids.join("|"), "zip|7z|sep|compressPassword", "the submenu is the probed table plus the password row");
    let password = entries.iter().find(|e| e.id == "compressPassword").expect("password row");
    assert_eq!(password.glyph, Some("key"), "the password row wears the key mark");
    assert_eq!(password.label, Some(Text::Key("menu.compress.password")));
    assert!(
        entries.iter().filter(|e| !e.separator && e.id != "compressPassword").all(|e| e.glyph == Some("file")),
        "each format row draws the file mark"
    );
    assert!(rows::compress_entries(&[]).is_empty(), "an empty table offers nothing");
    assert!(
        rows::compress_entries(&["zip".into()]).iter().all(|e| e.id != "compressPassword"),
        "without 7z the password row is hidden"
    );
}

#[test]
fn menu_extract_row_read_through_mn1() {
    use crate::ui_logic_mn1::{rows, testkit, Entry, Text};
    let mut p = testkit::state();
    p.row_is_archive = true;
    p.can_extract = false;
    p.archive_formats = vec!["zip".into(), "tar".into()];
    p.archive_name = "x.zip".into();
    let mut row = Entry::leaf("extract", Text::Data("Extract".into()));
    assert!(rows::extract_entry(&mut row, &p, 1), "a recognized archive with no reader keeps its Extract row");
    assert!(row.disabled);
    assert_eq!(row.hint, Some(Text::Key("menu.extract.no_7zip")), "the disabled row names the missing program");
    p.can_extract = true;
    let mut row = Entry::leaf("extract", Text::Data("Extract".into()));
    assert!(rows::extract_entry(&mut row, &p, 1), "a supported archive offers Extract");
    assert!(!row.disabled);
    p.archive_name = "debian-13.iso".into();
    let mut row = Entry::leaf("extract", Text::Data("Extract".into()));
    assert!(!rows::extract_entry(&mut row, &p, 1), "a disk image lists only and never extracts");
    p.archive_name = "debian-13.ISO".into();
    let mut row = Entry::leaf("extract", Text::Data("Extract".into()));
    assert!(!rows::extract_entry(&mut row, &p, 1), "an uppercase disk image lists only and never extracts");
    p.can_extract = false;
    p.archive_formats = vec!["7z".into(), "zip".into()];
    p.archive_name = "pack.tar".into();
    let mut row = Entry::leaf("extract", Text::Data("Extract".into()));
    assert!(rows::extract_entry(&mut row, &p, 1), "a tar archive with no tar reader keeps its Extract row");
    assert_eq!(row.hint, Some(Text::Key("menu.extract.no_bsdtar")), "the disabled row names the missing helper");
    p.archive_formats = vec!["zip".into()];
    p.archive_name = "pack.7z".into();
    let mut row = Entry::leaf("extract", Text::Data("Extract".into()));
    assert!(rows::extract_entry(&mut row, &p, 1), "a 7z archive with no reader keeps its Extract row");
    assert_eq!(row.hint, Some(Text::Key("menu.extract.no_tool")), "a bare archive blames neither tool by name");
}

#[test]
fn a_folded_tail_cuts_at_its_own_length() {
    // U+212A KELVIN SIGN lowercases to "k" but is three bytes, so the stem is cut in the original name.
    assert_eq!(extract_dir("x.ap\u{212A}"), "x");
    assert_eq!(extract_dir("\u{20AC}.AP\u{212A}"), "\u{20AC}");
    assert_eq!(extract_dir("Backup.TAR.GZ"), "Backup");
}
