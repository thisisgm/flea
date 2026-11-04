//! Pure route decisions; pk1 owns their backend and host dispatch.
use super::route::*;
use crate::path::{kind_of_peek, PeekKind, Said};
use flea::i18n::{I18n, Paths};

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pk2-route");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

#[test]
fn a_committed_line_is_peeked_before_anything_lists() {
    assert_eq!(
        commit_line("/home/gm/docs", "/home/gm", "/home/gm"),
        CommitAsk::Peek("/home/gm/docs".into())
    );
    assert_eq!(
        commit_line("docs", "/home/gm", "/home/gm"),
        CommitAsk::Peek("/home/gm/docs".into())
    );
    assert_eq!(commit_line("", "/home/gm", "/home/gm"), CommitAsk::Close);
}

#[test]
fn a_uri_on_another_host_says_its_sentence_and_lists_nothing() {
    assert_eq!(
        commit_line("file://otherhost/x", "/home/gm", "/home/gm"),
        CommitAsk::Say(Said::RefusedUri, false)
    );
}

#[test]
fn a_peek_for_another_line_is_stale() {
    assert_eq!(
        peeked("/a", "/b", false, 0o040755, false, false),
        Peeked::Stale
    );
}

#[test]
fn a_peeked_folder_opens_and_a_missing_path_says_so() {
    assert_eq!(
        peeked("/d", "/d", false, 0o040755, false, false),
        Peeked::OpenFolder("/d".into())
    );
    assert_eq!(
        peeked("/d/gone", "/d/gone", true, 0, false, false),
        Peeked::Say(Said::Missing("/d/gone".into()), true)
    );
}

#[test]
fn a_peeked_file_is_located_by_name_in_its_parent() {
    assert_eq!(
        peeked("/d/a.txt", "/d/a.txt", true, 0o100644, false, false),
        Peeked::LocateFile {
            parent: "/d".into(),
            leaf: "/d/a.txt".into()
        }
    );
}

#[test]
fn a_file_where_a_folder_answers_is_the_backend_own_refusal() {
    for (saving, folder_mode) in [(true, false), (false, true)] {
        assert_eq!(
            peeked("/d/a.txt", "/d/a.txt", true, 0o100644, saving, folder_mode),
            Peeked::ProbeMark("/d/a.txt".into())
        );
    }
}

#[test]
fn a_located_row_is_taken_a_negative_one_says_filtered() {
    assert_eq!(
        located("/d/a.txt", "/d", "/d", "/d/a.txt", 42),
        Located::Row(42)
    );
    assert_eq!(
        located("/d/a.txt", "/d", "/d", "/d/a.txt", -1),
        Located::Filtered("a.txt".into())
    );
}

#[test]
fn a_locate_against_another_listing_is_stale() {
    assert_eq!(located("", "/d", "/d", "/d/a.txt", 42), Located::Stale);
    assert_eq!(
        located("/d/b.txt", "/d", "/d", "/d/a.txt", 42),
        Located::Stale
    );
    assert_eq!(
        located("/d/a.txt", "/other", "/d", "/d/a.txt", 42),
        Located::Stale
    );
}

#[test]
fn the_row_runs_only_once_its_window_holds_it() {
    assert_eq!(take_row("/d/a.txt", 42, true), Some(42));
    assert_eq!(take_row("/d/a.txt", -1, true), None);
    assert_eq!(take_row("/d/a.txt", 42, false), None);
    assert_eq!(take_row("", 42, true), None);
}

#[test]
fn a_failed_listing_drops_the_leaf_it_was_waiting_for() {
    assert_eq!(leaf_on_listing_failed("/d/a.txt", true), "");
    assert_eq!(leaf_on_listing_failed("/d/a.txt", false), "/d/a.txt");
}

#[test]
fn a_folder_that_refused_the_read() {
    assert_eq!(
        peeked("/d", "/d", true, 0o040000, false, false),
        Peeked::OpenFolder("/d".into())
    );
}

#[test]
fn a_file_that_fails_its_scan_carries_nothing_but_itself() {
    assert_eq!(
        peeked("/d/gone", "/d/gone", true, 0, false, false),
        Peeked::Say(Said::Missing("/d/gone".into()), true)
    );
}

// tests/js/pickerpath.js: the missing and filter sentences are whole, with one placeholder each.
#[test]
fn the_missing_sentence_is_whole() {
    assert_eq!(
        Said::Missing("/x/y z".into()).text(&english()),
        "/x/y z does not exist or cannot be reached."
    );
}

#[test]
fn the_filter_sentence_is_whole() {
    assert_eq!(
        Said::Filtered("a.png".into()).text(&english()),
        "a.png is hidden by the file filter or the hidden files setting."
    );
}

#[test]
fn a_dollar_in_a_name_stays_literal() {
    assert_eq!(
        Said::Missing("/a$&b".into()).text(&english()),
        "/a$&b does not exist or cannot be reached."
    );
}

#[test]
fn each_sentence_has_one_placeholder() {
    let i18n = english();
    for key in ["path.missing", "path.filtered"] {
        assert_eq!(
            i18n.tr(key).matches("%1").count(),
            1,
            "{key} keeps one placeholder"
        );
    }
}

// kindOfPeek(false, 0) reads a folder.
#[test]
fn a_readable_folder() {
    assert_eq!(kind_of_peek(false, 0), PeekKind::Folder);
}

// kindOfPeek(true, 33188) reads a file.
#[test]
fn a_file_fails_its_scan_and_carries_its_mode() {
    assert_eq!(kind_of_peek(true, 33188), PeekKind::File);
}

// kindOfPeek(true, 0) reads nothing.
#[test]
fn a_failed_stat_names_nothing() {
    assert_eq!(kind_of_peek(true, 0), PeekKind::Missing);
}
