//! The typed line: its prefill, Tab against a peek on the backend, the commit and Escape, and UTF-16 for the platform.
use super::HOME;
use flea_gui::path::{
    entry::{Commit, Entry, Tab},
    request_key, Peeked, Said,
};

fn peeked(path: &str, hidden: bool, dirs: &[&str]) -> Peeked {
    Peeked {
        path: path.into(),
        hidden,
        dirs: dirs.iter().map(|dir| dir.to_string()).collect(),
        ..Peeked::default()
    }
}

fn typed(line: &str) -> Entry {
    let mut entry = Entry::default();
    assert!(entry.start(HOME));
    entry.replace(None, line);
    entry
}

#[test]
fn opening_prefills_the_pane_path_and_a_slash_selected_whole() {
    let mut entry = Entry::default();
    assert!(entry.start(HOME));
    assert_eq!(
        (entry.text.as_str(), entry.selected.clone(), entry.cursor()),
        ("/home/gm/", 0..9, 9)
    );
    assert!(!entry.start("/etc"), "an open bar keeps its line");
    assert!(entry.close());
    assert!(!entry.close());
    assert!(entry.start("/"));
    assert_eq!(entry.text, "/");
}

#[test]
fn tab_asks_one_peek_takes_only_its_own_answer_and_answers_a_repeat_from_the_kept_rows() {
    let mut entry = typed("/et");
    assert_eq!(entry.cursor(), 3);
    assert_eq!(
        entry.complete(HOME, HOME, false),
        Tab::Asked(request_key("/", false))
    );
    assert_eq!(entry.pending(), Some(&request_key("/", false)));
    assert_eq!(
        entry.complete_with(&peeked("/", true, &["etc"]), HOME, HOME),
        None,
        "the other flag is another request"
    );
    assert_eq!(
        entry.complete_with(&peeked("/", false, &["etc", "home"]), HOME, HOME),
        Some(None)
    );
    assert_eq!((entry.text.as_str(), entry.cursor()), ("/etc/", 5));
    assert!(entry.pending().is_none());
    entry.select_all();
    entry.replace(None, "/h");
    assert_eq!(
        entry.complete(HOME, HOME, false),
        Tab::Answered(None),
        "the kept rows answer without a read"
    );
    assert_eq!(entry.text, "/home/");
    entry.replace(None, "q");
    assert_eq!(
        entry.complete(HOME, HOME, false),
        Tab::Asked(request_key("/home", false))
    );
}

#[test]
fn a_peek_for_a_line_that_moved_on_is_kept_but_changes_nothing() {
    let mut entry = typed("~/Do");
    assert_eq!(
        entry.complete(HOME, HOME, false),
        Tab::Asked(request_key(HOME, false))
    );
    entry.select_all();
    entry.replace(None, "/usr/sh");
    assert_eq!(
        entry.complete_with(
            &peeked(HOME, false, &["Documents", "Downloads"]),
            HOME,
            HOME
        ),
        None
    );
    assert_eq!(entry.text, "/usr/sh");
    let mut dotted = typed("~/.co");
    assert_eq!(
        dotted.complete(HOME, HOME, false),
        Tab::Asked(request_key(HOME, true))
    );
    assert_eq!(
        dotted.complete_with(&peeked(HOME, true, &[".config", ".cache"]), HOME, HOME),
        Some(None)
    );
    assert_eq!(dotted.text, "~/.config/");
}

#[test]
fn tab_that_moves_nothing_says_why() {
    let mut entry = typed("~/zz");
    entry.complete(HOME, HOME, false);
    let said = entry.complete_with(&peeked(HOME, false, &["Desktop"]), HOME, HOME);
    assert_eq!(said, Some(Some(Said::NothingStartsWith(HOME.into()))));
    let mut entry = typed("~/D");
    entry.complete(HOME, HOME, false);
    let said = entry.complete_with(&peeked(HOME, false, &["Desktop", "Documents"]), HOME, HOME);
    assert_eq!(said, Some(Some(Said::ManyStart(2))));
}

#[test]
fn enter_resolves_the_line_and_escape_leaves_the_pane_where_it_is() {
    let mut entry = typed("~/Downloads");
    assert_eq!(
        entry.commit(HOME, HOME, false),
        Commit::Open("/home/gm/Downloads".into())
    );
    assert!(!entry.editing);
    assert_eq!(
        entry.text, "~/Downloads",
        "the line stays in the field after it closes"
    );
    let mut same = Entry::default();
    same.start(HOME);
    assert_eq!(
        same.commit(HOME, HOME, false),
        Commit::Nothing,
        "a settled same path keeps its selection"
    );
    same.start(HOME);
    assert_eq!(
        same.commit(HOME, HOME, true),
        Commit::Open(HOME.into()),
        "a failed listing retries"
    );
    assert_eq!(
        typed("smb://server.lan/share").commit(HOME, HOME, false),
        Commit::Network("smb://server.lan/share".into())
    );
    assert_eq!(
        typed("file://otherbox/etc").commit(HOME, HOME, false),
        Commit::Refused
    );
    assert_eq!(typed("   ").commit(HOME, HOME, false), Commit::Nothing);
    let mut cancelled = typed("/et");
    cancelled.complete(HOME, HOME, false);
    assert!(cancelled.close());
    assert!(!cancelled.editing && cancelled.pending().is_none());
    assert_eq!(
        cancelled.complete_with(&peeked("/", false, &["etc"]), HOME, HOME),
        None
    );
}

#[test]
fn the_caret_walks_whole_characters_and_the_platform_counts_utf16() {
    let mut entry = typed("a\u{e9}\u{20000}b");
    assert_eq!(entry.text.len(), 8);
    assert_eq!(
        (entry.offset_to_utf16(7), entry.offset_from_utf16(4)),
        (4, 7)
    );
    assert_eq!(
        entry.offset_from_utf16(3),
        7,
        "half a surrogate pair rounds past the character"
    );
    assert_eq!(entry.range_to_utf16(&(1..7)), 1..4);
    assert_eq!((entry.previous(7), entry.next(3), entry.next(8)), (3, 7, 8));
    entry.move_to(6);
    assert_eq!(
        entry.cursor(),
        3,
        "a byte inside a character snaps back to its start"
    );
    entry.erase(true);
    assert_eq!(entry.text, "a\u{e9}b");
    entry.erase(false);
    assert_eq!((entry.text.as_str(), entry.cursor()), ("ab", 1));
    entry.move_to(2);
    entry.select_to(0);
    assert_eq!(
        (entry.selected.clone(), entry.reversed, entry.cursor()),
        (0..2, true, 0)
    );
    entry.erase(false);
    assert_eq!(entry.text, "");
    entry.erase(false);
    assert_eq!(entry.text, "", "nothing before the caret erases nothing");
}

#[test]
fn composing_text_stays_marked_until_the_input_method_commits_it() {
    let mut entry = typed("/srv/");
    entry.replace_and_mark(None, "ni", Some(2..2));
    assert_eq!(
        (
            entry.text.as_str(),
            entry.marked.clone(),
            entry.selected.clone()
        ),
        ("/srv/ni", Some(5..7), 7..7)
    );
    entry.replace_and_mark(None, "\u{4f60}", None);
    assert_eq!(
        (entry.text.as_str(), entry.marked.clone()),
        ("/srv/\u{4f60}", Some(5..8))
    );
    entry.replace(None, "\u{4f60}");
    assert_eq!(
        (entry.text.as_str(), entry.marked.clone(), entry.cursor()),
        ("/srv/\u{4f60}", None, 8)
    );
    entry.replace_and_mark(None, "", None);
    assert_eq!(
        entry.marked, None,
        "an emptied composition leaves nothing marked"
    );
}

#[test]
fn the_text_and_its_wash_start_inside_the_frame_border_and_the_caret_sits_one_past_the_text() {
    use super::jump::board;
    use flea_gui::path::field::{band_top, caret_left, frame, text_origin};
    use gpui::{point, px, size, Bounds};
    let s = board();
    let frame = frame(Bounds::new(point(px(80.), px(0.)), size(px(788.), px(26.))));
    assert_eq!(
        (f32::from(frame.origin.y), f32::from(frame.size.height)),
        (2., 23.),
        "the board's field frame"
    );
    assert_eq!(
        f32::from(text_origin(frame, &s)),
        90.,
        "the 1 px border, then the 9 px inset"
    );
    assert_eq!(
        (s.wash, f32::from(band_top(frame, s.wash))),
        (16., 5.),
        "the 16 px wash on rows 5 to 20"
    );
    assert_eq!(
        (s.caret, f32::from(band_top(frame, s.caret))),
        (17., 5.),
        "the 17 px caret on rows 5 to 21"
    );
    assert_eq!(
        f32::from(caret_left(px(90.), px(23.4))),
        114.4,
        "1 px past the advance of Med at 13 px"
    );
}
