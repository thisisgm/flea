// search.js's strip and status words through the English catalog, plus a translated witness with English fallback.
use super::*;
use crate::i18n::{I18n, Paths};
use std::path::{Path, PathBuf};

pub(super) fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: vec![crate::i18n::test_catalogs::english_only()],
    })
}

#[test]
fn the_strip_note_pairs_the_count_with_its_crawl() {
    let en = english();
    // search.js:9, search.js:10, search.js:11
    assert_eq!(note(&en, 9, true, false, false), "9 found · still scanning");
    assert_eq!(note(&en, 9, false, false, false), "9 found");
    assert_eq!(note(&en, 1204, true, false, false), "1,204 found · still scanning");
    // search.js:15, search.js:16, search.js:17
    assert_eq!(note(&en, 0, false, false, false), "0 found");
    assert_eq!(note(&en, 12, true, true, false), "");
    assert_eq!(note(&en, 12, false, true, false), "");
    // The everywhere board's note while a share is mounted.
    assert_eq!(note(&en, 2, true, false, true), "2 found · still scanning · shares skipped");
}

#[test]
fn the_no_match_headline_keeps_the_query_as_typed() {
    let en = english();
    // search.js:12: the query's own $ signs and case stay literal after the capitals.
    assert_eq!(nothing_matches(&en, "a$&b$$c$'"), "NOTHING MATCHES a$&b$$c$'");
    assert_eq!(nothing_matches(&en, "zqq"), "NOTHING MATCHES zqq");
    assert_eq!(nothing_matches(&en, "%1Ab"), "NOTHING MATCHES %1Ab", "an argument is never read as a hole");
    // search.js:14
    assert_eq!(nothing_matches(&en, ""), "");
}

#[test]
fn the_status_line_counts_while_walking_and_times_the_landing() {
    let en = english();
    // search.js:20, search.js:22
    assert_eq!(status_line(&en, true, 12, 4120, 300.), "12 found · Searching, 4,120 scanned");
    assert_eq!(status_line(&en, false, 1, 18204, 412.), "1 found in 0.4 s");
    // search.js:24 as search-empty changes it: the bar drops "0 found in 0.0 s" and counts 0 items instead.
    assert_eq!(status_line(&en, false, 0, 18204, 412.), "");
    assert_eq!(status_line(&en, true, 0, 18204, 412.), "0 found · Searching, 18,204 scanned");
}

#[test]
fn the_scope_reads_with_a_tilde_and_esc_says_what_it_does() {
    let en = english();
    // search.js:27, search.js:28, search.js:29
    assert_eq!(tilde("/home/gm/Work/claude/flea", "/home/gm"), "~/Work/claude/flea");
    assert_eq!(tilde("/home/gm", "/home/gm"), "~");
    assert_eq!(tilde("/usr/share", "/home/gm"), "/usr/share");
    assert_eq!(tilde("/home/gmx", "/home/gm"), "/home/gmx");
    assert_eq!(scope_label(&en, Some("/fx/v/work"), "", Scope::Folder, Kind::Names), "in /fx/v/work");
    assert_eq!(scope_label(&en, Some("/home/gm"), "/home/gm", Scope::Home, Kind::Names), "in ~");
    assert_eq!(scope_label(&en, None, "/home/gm", Scope::Everywhere, Kind::Names), "everywhere");
    assert_eq!(scope_label(&en, Some("/fx/v/work"), "", Scope::Folder, Kind::Contents), "contents in /fx/v/work");
    // search.js:44, search.js:45
    assert_eq!(way_out(&en, true, false), "esc cancels, then returns");
    assert_eq!([way_out(&en, false, false), way_out(&en, true, true), way_out(&en, false, true)], ["esc returns"; 3]);
}

// The l1 fixture-root rule: everything this test writes is under a marked mktemp root inside the worktree.
struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/sr1");
        std::fs::create_dir_all(&base).unwrap();
        let output = std::process::Command::new("mktemp").arg("-d").arg(base.join("i18n.XXXXXX")).output().unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(&base) && path != base);
        std::fs::write(path.join(".sr1-test-root"), b"flea sr1 test\n").unwrap();
        Self(path)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/sr1");
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty() && self.0.starts_with(&base) && self.0 != base);
        assert!(self.0.join(".sr1-test-root").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn a_translated_catalog_takes_its_own_words_digits_and_english_fallback() {
    let root = Root::new();
    let write = |name: &str, text: &str| {
        let path = root.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("omarchy/default/i18n/locales.json", include_str!("../i18n/fixtures/locales.json"));
    write("language", "de\n");
    write("catalogs/de/flea.json", r#"{"search.note.landed":{"one":"%1 gefunden","other":"%1 gefunden"},"search.nothing_matches":"Keine Treffer für %1"}"#);
    let de = I18n::load(Paths {
        omarchy: root.0.join("omarchy"),
        language_file: root.0.join("language"),
        catalogs: vec![root.0.join("catalogs"), crate::i18n::test_catalogs::english_only()],
    });
    assert_eq!(de.language(), "de");
    assert_eq!(note(&de, 1204, false, false, false), "1.204 gefunden");
    assert_eq!(nothing_matches(&de, "zqq"), "KEINE TREFFER FÜR zqq");
    // A string the translation lacks falls back to English, never to its key.
    assert_eq!(way_out(&de, false, false), "esc returns");
    assert_eq!(status_line(&de, false, 3, 10, 1250.), "3 found in 1,2 s");
}
