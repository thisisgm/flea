//! tests/js/filter.js and filter-cursor.js pane flows, the first-key probes and Filter040, over a stub pane and the core matcher as backend.
use super::filter::{Asked, Filter, LineKey, PaneSeam, Refused};
use super::keys::{classify_input, Classified};
use super::present;
use super::wire::Answer;
use flea::keys::{Input, CONTROL, SHIFT};
use flea::ui_logic_sr2::{narrow, Refusal};
use std::collections::BTreeSet;

const LISTING: [&str; 7] =
    ["Screens", "scripts", "tools", "notes.md", "readme.txt", "screenrecording-2026-08-21.mp4", "screenshot-2026-08-30.png"];

/// tests/js/filterfixture.js's stub pane: the whole listing's names stand in for the backend's arena.
struct Stub {
    names: Vec<String>,
    listing: usize,
    cursor: Option<usize>,
    picked: BTreeSet<usize>,
    unmarks: usize,
    scrolled: Option<usize>,
    focused: usize,
    refills: usize,
}

impl Stub {
    fn new() -> Self {
        Self::of(LISTING.iter().map(|name| name.to_string()).collect())
    }
    fn of(names: Vec<String>) -> Self {
        Self { names, listing: 1, cursor: Some(0), picked: BTreeSet::new(), unmarks: 0, scrolled: None, focused: 0, refills: 0 }
    }
    // The backend's answer to one request, matched over every name as the Filter040 producer matches them.
    fn answer(&self, asked: &Asked) -> Answer {
        let rows = self.names.iter().enumerate().map(|(row, name)| (row, name.as_str()));
        let narrowed = narrow(&asked.query, rows).unwrap();
        Answer {
            id: asked.id,
            query: asked.query.clone(),
            listing: asked.listing,
            total: self.names.len(),
            exact: Some(narrowed.exact),
            close: narrowed.close,
        }
    }
    fn picks(&self) -> Vec<usize> {
        self.picked.iter().copied().collect()
    }
}

impl PaneSeam for Stub {
    fn listing(&self) -> usize {
        self.listing
    }
    fn total(&self) -> usize {
        self.names.len()
    }
    fn cursor(&self) -> Option<usize> {
        self.cursor
    }
    fn set_cursor(&mut self, row: usize) {
        self.cursor = Some(row);
    }
    fn show_view(&mut self, view: usize) {
        self.scrolled = Some(view);
    }
    fn selected(&self) -> Vec<usize> {
        self.picks()
    }
    fn unmark(&mut self, rows: &[usize]) {
        rows.iter().for_each(|row| {
            self.picked.remove(row);
        });
        self.unmarks += 1;
    }
    fn mark(&mut self, rows: &[usize]) {
        self.picked.extend(rows);
    }
    fn focus_list(&mut self) {
        self.focused += 1;
    }
    fn refill(&mut self) {
        self.refills += 1;
    }
}

// Types each character and lets every answer land, the way a fast backend answers a typist.
fn type_into(filter: &mut Filter, pane: &mut Stub, text: &str) {
    for c in text.chars() {
        let asked = filter.typed(&c.to_string(), pane).expect("a character asks");
        filter.receive(pane.answer(&asked), pane).unwrap();
    }
}

fn land(filter: &mut Filter, pane: &mut Stub, asked: Option<Asked>) {
    if let Some(asked) = asked {
        filter.receive(pane.answer(&asked), pane).unwrap();
    }
}

/// The shipped English catalog, read from the repo with no language file, so the box's own language never leaks in.
pub(super) fn english() -> flea::i18n::I18n {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: root.join("nonexistent-omarchy"),
        language_file: root.join("nonexistent-language"),
        catalogs: vec![root.join("i18n")],
    })
}

fn shown(filter: &Filter, pane: &Stub) -> Option<Vec<usize>> {
    filter.shown(pane.listing).map(<[usize]>::to_vec)
}

#[test]
fn the_query_line_types_narrows_shortens_commits_and_clears() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    assert_eq!(filter.typed("\u{7f}", &mut pane), None, "DEL types nothing into the filter");
    assert_eq!(filter.query(), "");
    let asked = filter.typed("e", &mut pane);
    land(&mut filter, &mut pane, asked);
    assert_eq!(filter.query(), "e", "a letter still types into the filter");

    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    filter.start();
    assert!(filter.typing(), "slash gives the query line the keyboard");
    assert_eq!((filter.query(), shown(&filter, &pane)), ("", None), "and narrows nothing until something is typed");
    type_into(&mut filter, &mut pane, "screens");
    assert_eq!(filter.query(), "screens", "typing narrows in place");
    assert_eq!(shown(&filter, &pane), Some(vec![0, 6]), "and the rows it leaves are the matches");
    let asked = filter.backspace(&mut pane);
    land(&mut filter, &mut pane, asked);
    assert_eq!(filter.query(), "screen", "backspace shortens the query");
    assert_eq!(shown(&filter, &pane), Some(vec![0, 5, 6]), "and the rows it had hidden come back with it");
    filter.commit(&mut pane);
    assert!(!filter.typing(), "enter hands the keyboard back to the list");
    assert_eq!(filter.query(), "screen", "and the filter stands, so every action key works over the narrowed rows");
    filter.close(&mut pane);
    assert_eq!(filter.query(), "", "esc clears the query");
    assert_eq!(shown(&filter, &pane), None, "and puts the whole listing back");

    let (mut empty, mut pane) = (Filter::default(), Stub::new());
    empty.start();
    empty.commit(&mut pane);
    assert!(!empty.typing() && !empty.up(), "enter on an empty query closes the strip rather than standing an empty filter");
}

#[test]
fn a_mark_the_filter_hides_drops_out_and_clearing_restores_nothing() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    pane.picked.extend([3, 5, 6]);
    type_into(&mut filter, &mut pane, "scr");
    assert_eq!(pane.picks(), vec![5, 6], "a row the filter hides drops out of the selection with it");
    assert!(pane.unmarks > 0, "and the selection moved, so the status bar's count re-reads");
    filter.close(&mut pane);
    assert_eq!(pane.picks(), vec![5, 6], "clearing the filter keeps what survived it and restores nothing");
}

#[test]
fn a_sort_asks_again_against_the_new_numbering_and_the_old_answer_never_lands() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut filter, &mut pane, "scr");
    let in_flight = filter.typed("e", &mut pane).unwrap();
    pane.names = ["tools", "scripts", "Screens", "screenshot-2026-08-30.png", "screenrecording-2026-08-21.mp4", "readme.txt", "notes.md"]
        .map(String::from)
        .to_vec();
    pane.listing = 2;
    let fresh = filter.renumbered(&mut pane).unwrap();
    assert_eq!(filter.shown(2), None, "the old numbering's rows are never drawn over the new one");
    assert_eq!(filter.receive(pane.answer(&in_flight), &mut pane), Err(Refused::Stale), "an answer for the old numbering is refused");
    filter.receive(pane.answer(&fresh), &mut pane).unwrap();
    assert_eq!(filter.query(), "scre", "and the reorder leaves the query alone");
    assert_eq!(shown(&filter, &pane), Some(vec![2, 3, 4]), "filtering first and sorting after keeps the filter, over the new order");
}

#[test]
fn only_the_answer_to_the_request_in_force_lands() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    let first = filter.typed("s", &mut pane).unwrap();
    let second = filter.typed("c", &mut pane).unwrap();
    assert_eq!(filter.receive(pane.answer(&first), &mut pane), Err(Refused::Stale), "a superseded keystroke's answer is stale");
    assert_eq!(filter.shown(1), None, "and draws nothing");
    let mut wrong = pane.answer(&second);
    wrong.query = "s".into();
    assert_eq!(filter.receive(wrong, &mut pane), Err(Refused::Stale), "an id answering another query is stale");
    filter.close(&mut pane);
    assert_eq!(filter.receive(pane.answer(&second), &mut pane), Err(Refused::Stale), "a cancelled request's answer is stale");
    let asked = filter.typed("x", &mut pane).unwrap();
    filter.abandon(&mut pane);
    assert_eq!(filter.receive(pane.answer(&asked), &mut pane), Err(Refused::Stale), "after the backend died nothing lands");
    assert!(!filter.up() && filter.asked().is_none(), "and the line is gone");
}

#[test]
fn issue_92_the_cursor_follows_the_rows_the_filter_draws() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    pane.cursor = Some(3);
    type_into(&mut filter, &mut pane, "2026");
    assert_eq!(pane.cursor, Some(5), "a cursor the filter hides lands on the first row it left standing");
    assert_eq!(pane.scrolled, Some(0), "and the view scrolls to the top of the narrowed listing");
    let (mut kept, mut pane) = (Filter::default(), Stub::new());
    pane.cursor = Some(6);
    type_into(&mut kept, &mut pane, "2026");
    assert_eq!((pane.cursor, pane.scrolled), (Some(6), None), "a cursor the filter keeps does not move and nothing scrolls");

    let (mut nomatch, mut pane) = (Filter::default(), Stub::new());
    pane.cursor = Some(3);
    type_into(&mut nomatch, &mut pane, "zzz");
    let refused = nomatch.targets(&pane, |row| row < 7);
    assert_eq!(refused, Err(Refusal::HiddenByFilter), "an operation on a hidden cursor goes nowhere");
    let i18n = english();
    assert_eq!(present::refusal(refused.unwrap_err(), &i18n), "That row is hidden by the filter.", "and says so");
    pane.cursor = None;
    assert_eq!(present::refusal(nomatch.targets(&pane, |_| true).unwrap_err(), &i18n), "There is nothing to act on.");
    pane.cursor = Some(3);
    assert_eq!(present::refusal(nomatch.targets(&pane, |_| false).unwrap_err(), &i18n), "That row has not loaded yet.");
}

#[test]
fn the_cursor_keys_walk_drawn_rows_and_stop_at_both_ends() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut filter, &mut pane, "o");
    pane.cursor = Some(2);
    filter.step(1, &mut pane);
    assert_eq!((pane.cursor, pane.scrolled), (Some(3), Some(1)), "a step lands on the next row drawn");
    filter.set_cursor_view(99, &mut pane);
    assert_eq!(pane.cursor, Some(6), "the last view position is the last row drawn");
    filter.step(1, &mut pane);
    assert_eq!(pane.cursor, Some(6), "and down stops there");
    let (mut none, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut none, &mut pane, "zzz");
    none.step(1, &mut pane);
    assert_eq!(pane.cursor, Some(0), "a filter matching nothing has no row to move to");
}

// Issue 287: Ctrl+V and Shift+Insert type the clipboard's first line as one narrowing.
#[test]
fn a_paste_types_the_clipboards_first_line_as_one_narrowing() {
    let paste = |modifiers, key| classify_input("default", Input { key, text: "", modifiers });
    assert_eq!(paste(CONTROL, "v"), Classified::Paste, "ctrl+v pastes");
    assert_eq!(paste(SHIFT, "insert"), Classified::Paste, "shift+insert pastes the same way");
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    let asked = filter.typed("qqq\nsecond line", &mut pane);
    assert_eq!(filter.query(), "qqq", "the clipboard's first line goes into the filter");
    land(&mut filter, &mut pane, asked);
    assert_eq!(shown(&filter, &pane), Some(vec![]), "and the paste narrows, standing an empty match list instead of no filter");
    let mut dirty = Filter::default();
    let _ = dirty.typed("a\tb\u{1}c", &mut pane);
    assert_eq!(dirty.query(), "abc", "a tab or control character in the clipboard types nothing");
    let mut blank = Filter::default();
    assert_eq!(blank.typed("", &mut pane), None, "an empty clipboard leaves the query as it was");
    let mut paired = Filter::default();
    let _ = paired.typed("a\u{1f600}b", &mut pane);
    assert_eq!(paired.query(), "a\u{1f600}b", "a whole character beyond the basic plane is kept");
}

#[test]
fn the_query_line_classifies_its_keys_before_the_key_table() {
    let key = |key, text, modifiers| classify_input("vim", Input { key, text, modifiers });
    assert_eq!(key("j", "j", 0), Classified::Key(LineKey::Text("j")), "a printable key types even where vim binds it");
    assert_eq!(key("down", "", 0), Classified::Key(LineKey::Leaves), "a cursor key leaves the line");
    assert_eq!(key("tab", "\t", 0), Classified::Key(LineKey::Tab), "Tab is the line's own key, consumed there");
    assert_eq!(key("a", "a", CONTROL), Classified::Key(LineKey::SelectAll), "Ctrl+A is select all");
    assert_eq!(key("Return", "", 0), Classified::Key(LineKey::Enter));
    assert_eq!(key("escape", "", 0), Classified::Key(LineKey::Escape));
    assert_eq!(key("backspace", "", 0), Classified::Key(LineKey::Backspace));
    assert_eq!(key("delete", "\u{7f}", 0), Classified::Key(LineKey::Other), "DEL is not text");
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    filter.start();
    let (consumed, _) = filter.line_key(LineKey::Leaves, &mut pane);
    assert!(!consumed && !filter.typing(), "a key that leaves the line commits and passes on to the list");
}

// filter-firstkey.qml and filter-held.qml: the first key over a window whose rows all match changes nothing drawn.
#[test]
fn the_first_key_over_rows_that_all_match_moves_nothing_and_refills_nothing() {
    let names: Vec<String> = (0..250).map(|i| if i < 150 { format!("k-{i:03}.txt") } else { format!("z-{i:03}.txt") }).collect();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(names));
    filter.start();
    let drawn_before: Vec<usize> = (0..30).collect();
    let asked = filter.typed("k", &mut pane).unwrap();
    assert_eq!(asked.query, "k", "the key is one request");
    assert_eq!(filter.shown(1), None, "and nothing is drawn differently before its answer");
    filter.receive(pane.answer(&asked), &mut pane).unwrap();
    let shown = filter.shown(1).unwrap();
    assert_eq!(shown.len(), 150, "the narrowed list holds every match in the listing, not only the held rows");
    assert_eq!(&shown[..30], drawn_before.as_slice(), "each drawn row keeps its listing row");
    assert_eq!((pane.cursor, pane.scrolled, pane.refills, pane.unmarks), (Some(0), None, 0, 0), "nothing moved, scrolled or refilled");
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
    assert_eq!(pane.refills, 1, "the clear refills the unfiltered viewport without another scroll or Escape");
    assert_eq!(pane.cursor, Some(1900), "around the row the cursor stands on");
    let asked = filter.typed("x", &mut pane);
    let _ = filter.backspace(&mut pane);
    assert!(asked.is_some() && pane.refills == 1, "a filter cleared before its answer drew nothing, so nothing refills");
}

#[test]
fn filter040_a_typo_draws_its_close_match_and_ctrl_a_marks_only_exact_rows() {
    let work = ["A", "Code", "Docs", "Locked", "Many", "Media", "notes.txt", "photo.png", "todo.md"].map(String::from).to_vec();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(work));
    filter.start();
    type_into(&mut filter, &mut pane, "ntoes");
    assert_eq!(shown(&filter, &pane), Some(vec![6]), "with no exact match the close one is drawn");
    let counts = filter.counts(1).unwrap();
    assert_eq!((counts.shown, counts.tested, counts.close), (0, 9, 1), "0 of 9 shown, 1 close match");
    assert_eq!(pane.cursor, Some(6), "the cursor stands on it");
    filter.select_all(&mut pane);
    assert!(pane.picked.is_empty(), "Ctrl+A never marks a close match");
    assert!(!filter.typing() && pane.focused == 1, "and still hands the keyboard to the list");
}

#[test]
fn filter040_ctrl_a_marks_every_row_shown_and_drops_the_caret() {
    let many: Vec<String> = (1..=80).map(|i| format!("item-{i:02}.txt")).collect();
    let (mut filter, mut pane) = (Filter::default(), Stub::of(many));
    filter.start();
    type_into(&mut filter, &mut pane, "item-7?.txt");
    let (consumed, _) = filter.line_key(LineKey::SelectAll, &mut pane);
    assert!(consumed);
    assert_eq!(pane.picks(), (69..79).collect::<Vec<_>>(), "item-70.txt to item-79.txt are marked");
    assert!(!filter.typing() && filter.query() == "item-7?.txt", "the query stands without its caret");
    assert_eq!(pane.focused, 1, "and the list has the keyboard, so ten files took three steps");
    let next = filter.typed("x", &mut pane).unwrap();
    filter.receive(pane.answer(&next), &mut pane).unwrap();
    assert!(pane.picked.is_empty(), "typing after Ctrl+A narrows again and drops the marks it hides");
}

// Issue 164 and PR 317: Escape clears the filter first and keeps the marks; a second Escape is the list's, which deselects.
#[test]
fn escape_unwinds_the_filter_before_the_marks() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut filter, &mut pane, "scr");
    filter.commit(&mut pane);
    pane.picked.extend([5, 6]);
    assert!(filter.escape(&mut pane), "Escape takes the filter first");
    assert_eq!((filter.query(), pane.picks()), ("", vec![5, 6]), "and leaves the marks it kept");
    assert!(!filter.escape(&mut pane), "the next Escape passes to the list");
}
