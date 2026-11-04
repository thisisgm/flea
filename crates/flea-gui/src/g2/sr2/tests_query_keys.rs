//! Filter query keys, selection and viewport refill.
use super::*;

#[test]
fn the_cursor_keys_walk_drawn_rows_and_stop_at_both_ends() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut filter, &mut pane, "o");
    pane.cursor = Some(2);
    filter.step(1, &mut pane);
    assert_eq!(
        (pane.cursor, pane.scrolled),
        (Some(3), Some(1)),
        "a step lands on the next row drawn"
    );
    filter.set_cursor_view(99, &mut pane);
    assert_eq!(
        pane.cursor,
        Some(6),
        "the last view position is the last row drawn"
    );
    filter.step(1, &mut pane);
    assert_eq!(pane.cursor, Some(6), "and down stops there");
    let (mut none, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut none, &mut pane, "zzz");
    none.step(1, &mut pane);
    assert_eq!(
        pane.cursor,
        Some(0),
        "a filter matching nothing has no row to move to"
    );
}

// Issue 287: Ctrl+V and Shift+Insert type the clipboard's first line as one narrowing.
#[test]
fn a_paste_types_the_clipboards_first_line_as_one_narrowing() {
    let paste = |modifiers, key| {
        classify_input(
            "default",
            Input {
                key,
                text: "",
                modifiers,
            },
        )
    };
    assert_eq!(paste(CONTROL, "v"), Classified::Paste, "ctrl+v pastes");
    assert_eq!(
        paste(SHIFT, "insert"),
        Classified::Paste,
        "shift+insert pastes the same way"
    );
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    let asked = filter.typed("qqq\nsecond line", &mut pane);
    assert_eq!(
        filter.query(),
        "qqq",
        "the clipboard's first line goes into the filter"
    );
    land(&mut filter, &mut pane, asked);
    assert_eq!(
        shown(&filter, &pane),
        Some(vec![]),
        "and the paste narrows, standing an empty match list instead of no filter"
    );
    let mut dirty = Filter::default();
    let _ = dirty.typed("a\tb\u{1}c", &mut pane);
    assert_eq!(
        dirty.query(),
        "abc",
        "a tab or control character in the clipboard types nothing"
    );
    let mut blank = Filter::default();
    assert_eq!(
        blank.typed("", &mut pane),
        None,
        "an empty clipboard leaves the query as it was"
    );
    let mut paired = Filter::default();
    let _ = paired.typed("a\u{1f600}b", &mut pane);
    assert_eq!(
        paired.query(),
        "a\u{1f600}b",
        "a whole character beyond the basic plane is kept"
    );
}

#[test]
fn the_query_line_classifies_its_keys_before_the_key_table() {
    let key = |key, text, modifiers| {
        classify_input(
            "vim",
            Input {
                key,
                text,
                modifiers,
            },
        )
    };
    assert_eq!(
        key("j", "j", 0),
        Classified::Key(LineKey::Text("j")),
        "a printable key types even where vim binds it"
    );
    assert_eq!(
        key("down", "", 0),
        Classified::Key(LineKey::Leaves),
        "a cursor key leaves the line"
    );
    assert_eq!(
        key("tab", "\t", 0),
        Classified::Key(LineKey::Tab),
        "Tab is the line's own key, consumed there"
    );
    assert_eq!(
        key("a", "a", CONTROL),
        Classified::Key(LineKey::SelectAll),
        "Ctrl+A is select all"
    );
    assert_eq!(key("Return", "", 0), Classified::Key(LineKey::Enter));
    assert_eq!(key("escape", "", 0), Classified::Key(LineKey::Escape));
    assert_eq!(key("backspace", "", 0), Classified::Key(LineKey::Backspace));
    assert_eq!(
        key("delete", "\u{7f}", 0),
        Classified::Key(LineKey::Other),
        "DEL is not text"
    );
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    filter.start();
    let (consumed, _) = filter.line_key(LineKey::Leaves, &mut pane);
    assert!(
        !consumed && !filter.typing(),
        "a key that leaves the line commits and passes on to the list"
    );
}

// filter-firstkey.qml and filter-held.qml: the first key over a window whose rows all match changes nothing drawn.
#[test]
fn the_first_key_over_rows_that_all_match_moves_nothing_and_refills_nothing() {
    let names: Vec<String> = (0..250)
        .map(|i| {
            if i < 150 {
                format!("k-{i:03}.txt")
            } else {
                format!("z-{i:03}.txt")
            }
        })
        .collect();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(names));
    filter.start();
    let drawn_before: Vec<usize> = (0..30).collect();
    let asked = filter.typed("k", &mut pane).unwrap();
    assert_eq!(asked.query, "k", "the key is one request");
    assert_eq!(
        filter.shown(1),
        None,
        "and nothing is drawn differently before its answer"
    );
    filter.receive(pane.answer(&asked), &mut pane).unwrap();
    let shown = filter.shown(1).unwrap();
    assert_eq!(
        shown.len(),
        150,
        "the narrowed list holds every match in the listing, not only the held rows"
    );
    assert_eq!(
        &shown[..30],
        drawn_before.as_slice(),
        "each drawn row keeps its listing row"
    );
    assert_eq!(
        (pane.cursor, pane.scrolled, pane.refills, pane.unmarks),
        (Some(0), None, 0, 0),
        "nothing moved, scrolled or refilled"
    );
}

#[test]
fn issue_315_clearing_a_deep_filter_refills_the_viewport_once() {
    let names: Vec<String> = (0..2000).map(|i| format!("item-{i:04}.txt")).collect();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(names));
    pane.cursor = Some(1500);
    type_into(&mut filter, &mut pane, "item-19");
    assert_eq!(filter.shown(1).map(<[usize]>::len), Some(100));
    assert_eq!(pane.cursor, Some(1900), "the deep match takes the cursor");
    filter.close(&mut pane);
    assert_eq!(
        pane.refills, 1,
        "the clear refills the unfiltered viewport without another scroll or Escape"
    );
    assert_eq!(
        pane.cursor,
        Some(1900),
        "around the row the cursor stands on"
    );
    let asked = filter.typed("x", &mut pane);
    let _ = filter.backspace(&mut pane);
    assert!(
        asked.is_some() && pane.refills == 1,
        "a filter cleared before its answer drew nothing, so nothing refills"
    );
}

#[test]
fn filter040_a_typo_draws_its_close_match_and_ctrl_a_marks_only_exact_rows() {
    let work = [
        "A",
        "Code",
        "Docs",
        "Locked",
        "Many",
        "Media",
        "notes.txt",
        "photo.png",
        "todo.md",
    ]
    .map(String::from)
    .to_vec();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(work));
    filter.start();
    type_into(&mut filter, &mut pane, "ntoes");
    assert_eq!(
        shown(&filter, &pane),
        Some(vec![6]),
        "with no exact match the close one is drawn"
    );
    let counts = filter.counts(1).unwrap();
    assert_eq!(
        (counts.shown, counts.tested, counts.close),
        (0, 9, 1),
        "0 of 9 shown, 1 close match"
    );
    assert_eq!(pane.cursor, Some(6), "the cursor stands on it");
    filter.select_all(&mut pane);
    assert!(pane.picked.is_empty(), "Ctrl+A never marks a close match");
    assert!(
        !filter.typing() && pane.focused == 1,
        "and still hands the keyboard to the list"
    );
}

#[test]
fn filter040_ctrl_a_marks_every_row_shown_and_drops_the_caret() {
    let many: Vec<String> = (1..=80).map(|i| format!("item-{i:02}.txt")).collect();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(many));
    filter.start();
    type_into(&mut filter, &mut pane, "item-7?.txt");
    // The list's Ctrl+A over the standing filter.
    filter.select_all(&mut pane);
    assert_eq!(
        pane.picks(),
        (69..79).collect::<Vec<_>>(),
        "item-70.txt to item-79.txt are marked"
    );
    assert!(
        !filter.typing() && filter.query() == "item-7?.txt",
        "the query stands without its caret"
    );
    assert_eq!(
        pane.focused, 1,
        "and the list has the keyboard, so ten files took three steps"
    );
    let next = filter.typed("x", &mut pane).unwrap();
    filter.receive(pane.answer(&next), &mut pane).unwrap();
    assert!(
        pane.picked.is_empty(),
        "typing after Ctrl+A narrows again and drops the marks it hides"
    );
}

/// EY-115: Ctrl+A leaves the typed line as it was, so Backspace edits the query and typing extends it.
#[test]
fn fixchrome_ey115_ctrl_a_on_the_query_line_keeps_the_line() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    filter.start();
    type_into(&mut filter, &mut pane, "nt");
    let (consumed, asked) = filter.line_key(LineKey::SelectAll, &mut pane);
    assert!(
        consumed && asked.is_none(),
        "the line takes Ctrl+A and asks nothing"
    );
    assert!(pane.picked.is_empty(), "no row is marked");
    assert!(
        filter.typing() && filter.query() == "nt",
        "the caret and the query stay"
    );
    assert_eq!(pane.focused, 0, "the list never takes the keyboard");
    let (consumed, _) = filter.line_key(LineKey::Backspace, &mut pane);
    assert!(consumed && filter.typing(), "Backspace stays on the line");
    assert_eq!(filter.query(), "n");
    let mut empty = Filter::default();
    let mut other = Stub::new();
    empty.start();
    empty.line_key(LineKey::SelectAll, &mut other);
    empty.line_key(LineKey::Backspace, &mut other);
    assert!(
        empty.up() && empty.query().is_empty(),
        "an empty strip stays open"
    );
    assert_eq!(other.focused, 0);
}

// Issue 164 and PR 317: Escape clears the filter first and keeps the marks; a second Escape is the list's, which deselects.
#[test]
fn escape_unwinds_the_filter_before_the_marks() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut filter, &mut pane, "scr");
    filter.commit(&mut pane);
    pane.picked.extend([5, 6]);
    assert!(filter.escape(&mut pane), "Escape takes the filter first");
    assert_eq!(
        (filter.query(), pane.picks()),
        ("", vec![5, 6]),
        "and leaves the marks it kept"
    );
    assert!(
        !filter.escape(&mut pane),
        "the next Escape passes to the list"
    );
}
