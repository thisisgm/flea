//! tests/js/filter.js and filter-cursor.js pane flows, the first-key probes and Filter040, over a stub pane and the core matcher as backend.
use super::filter::{Asked, Filter, LineKey, PaneSeam, Refused};
use super::keys::{classify_input, Classified};
use super::present;
use super::wire::Answer;
use flea::keys::{Input, CONTROL, SHIFT};
use flea::ui_logic_sr2::{narrow, Refusal};
use std::collections::BTreeSet;

const LISTING: [&str; 7] = [
    "Screens",
    "scripts",
    "tools",
    "notes.md",
    "readme.txt",
    "screenrecording-2026-08-21.mp4",
    "screenshot-2026-08-30.png",
];

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
        Self {
            names,
            listing: 1,
            cursor: Some(0),
            picked: BTreeSet::new(),
            unmarks: 0,
            scrolled: None,
            focused: 0,
            refills: 0,
        }
    }
    // The backend's answer to one request, matched over every name as the Filter040 producer matches them.
    fn answer(&self, asked: &Asked) -> Answer {
        let rows = self
            .names
            .iter()
            .enumerate()
            .map(|(row, name)| (row, name.as_str()));
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
        let asked = filter
            .typed(&c.to_string(), pane)
            .expect("a character asks");
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
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

fn shown(filter: &Filter, pane: &Stub) -> Option<Vec<usize>> {
    filter.shown(pane.listing).map(<[usize]>::to_vec)
}

#[test]
fn the_query_line_types_narrows_shortens_commits_and_clears() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    assert_eq!(
        filter.typed("\u{7f}", &mut pane),
        None,
        "DEL types nothing into the filter"
    );
    assert_eq!(filter.query(), "");
    let asked = filter.typed("e", &mut pane);
    land(&mut filter, &mut pane, asked);
    assert_eq!(filter.query(), "e", "a letter still types into the filter");

    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    filter.start();
    assert!(filter.typing(), "slash gives the query line the keyboard");
    assert_eq!(
        (filter.query(), shown(&filter, &pane)),
        ("", None),
        "and narrows nothing until something is typed"
    );
    type_into(&mut filter, &mut pane, "screens");
    assert_eq!(filter.query(), "screens", "typing narrows in place");
    assert_eq!(
        shown(&filter, &pane),
        Some(vec![0, 6]),
        "and the rows it leaves are the matches"
    );
    let asked = filter.backspace(&mut pane);
    land(&mut filter, &mut pane, asked);
    assert_eq!(filter.query(), "screen", "backspace shortens the query");
    assert_eq!(
        shown(&filter, &pane),
        Some(vec![0, 5, 6]),
        "and the rows it had hidden come back with it"
    );
    filter.commit(&mut pane);
    assert!(
        !filter.typing(),
        "enter hands the keyboard back to the list"
    );
    assert_eq!(
        filter.query(),
        "screen",
        "and the filter stands, so every action key works over the narrowed rows"
    );
    filter.close(&mut pane);
    assert_eq!(filter.query(), "", "esc clears the query");
    assert_eq!(
        shown(&filter, &pane),
        None,
        "and puts the whole listing back"
    );

    let (mut empty, mut pane) = (Filter::default(), Stub::new());
    empty.start();
    empty.commit(&mut pane);
    assert!(
        !empty.typing() && !empty.up(),
        "enter on an empty query closes the strip rather than standing an empty filter"
    );
}

#[test]
fn a_mark_the_filter_hides_drops_out_and_clearing_restores_nothing() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    pane.picked.extend([3, 5, 6]);
    type_into(&mut filter, &mut pane, "scr");
    assert_eq!(
        pane.picks(),
        vec![5, 6],
        "a row the filter hides drops out of the selection with it"
    );
    assert!(
        pane.unmarks > 0,
        "and the selection moved, so the status bar's count re-reads"
    );
    filter.close(&mut pane);
    assert_eq!(
        pane.picks(),
        vec![5, 6],
        "clearing the filter keeps what survived it and restores nothing"
    );
}

#[test]
fn a_sort_asks_again_against_the_new_numbering_and_the_old_answer_never_lands() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    type_into(&mut filter, &mut pane, "scr");
    let in_flight = filter.typed("e", &mut pane).unwrap();
    pane.names = [
        "tools",
        "scripts",
        "Screens",
        "screenshot-2026-08-30.png",
        "screenrecording-2026-08-21.mp4",
        "readme.txt",
        "notes.md",
    ]
    .map(String::from)
    .to_vec();
    pane.listing = 2;
    let fresh = filter.renumbered(&mut pane).unwrap();
    assert_eq!(
        filter.shown(2),
        None,
        "the old numbering's rows are never drawn over the new one"
    );
    assert_eq!(
        filter.receive(pane.answer(&in_flight), &mut pane),
        Err(Refused::Stale),
        "an answer for the old numbering is refused"
    );
    filter.receive(pane.answer(&fresh), &mut pane).unwrap();
    assert_eq!(
        filter.query(),
        "scre",
        "and the reorder leaves the query alone"
    );
    assert_eq!(
        shown(&filter, &pane),
        Some(vec![2, 3, 4]),
        "filtering first and sorting after keeps the filter, over the new order"
    );
}

#[test]
fn only_the_answer_to_the_request_in_force_lands() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    let first = filter.typed("s", &mut pane).unwrap();
    let second = filter.typed("c", &mut pane).unwrap();
    assert_eq!(
        filter.receive(pane.answer(&first), &mut pane),
        Err(Refused::Stale),
        "a superseded keystroke's answer is stale"
    );
    assert_eq!(filter.shown(1), None, "and draws nothing");
    let mut wrong = pane.answer(&second);
    wrong.query = "s".into();
    assert_eq!(
        filter.receive(wrong, &mut pane),
        Err(Refused::Stale),
        "an id answering another query is stale"
    );
    filter.close(&mut pane);
    assert_eq!(
        filter.receive(pane.answer(&second), &mut pane),
        Err(Refused::Stale),
        "a cancelled request's answer is stale"
    );
    let asked = filter.typed("x", &mut pane).unwrap();
    filter.abandon(&mut pane);
    assert_eq!(
        filter.receive(pane.answer(&asked), &mut pane),
        Err(Refused::Stale),
        "after the backend died nothing lands"
    );
    assert!(
        !filter.up() && filter.asked().is_none(),
        "and the line is gone"
    );
}

#[test]
fn issue_92_the_cursor_follows_the_rows_the_filter_draws() {
    let (mut filter, mut pane) = (Filter::default(), Stub::new());
    pane.cursor = Some(3);
    type_into(&mut filter, &mut pane, "2026");
    assert_eq!(
        pane.cursor,
        Some(5),
        "a cursor the filter hides lands on the first row it left standing"
    );
    assert_eq!(
        pane.scrolled,
        Some(0),
        "and the view scrolls to the top of the narrowed listing"
    );
    let (mut kept, mut pane) = (Filter::default(), Stub::new());
    pane.cursor = Some(6);
    type_into(&mut kept, &mut pane, "2026");
    assert_eq!(
        (pane.cursor, pane.scrolled),
        (Some(6), None),
        "a cursor the filter keeps does not move and nothing scrolls"
    );

    let (mut nomatch, mut pane) = (Filter::default(), Stub::new());
    pane.cursor = Some(3);
    type_into(&mut nomatch, &mut pane, "zzz");
    let refused = nomatch.targets(&pane, |row| row < 7);
    assert_eq!(
        refused,
        Err(Refusal::HiddenByFilter),
        "an operation on a hidden cursor goes nowhere"
    );
    let i18n = english();
    assert_eq!(
        present::refusal(refused.unwrap_err(), &i18n),
        "That row is hidden by the filter.",
        "and says so"
    );
    pane.cursor = None;
    assert_eq!(
        present::refusal(nomatch.targets(&pane, |_| true).unwrap_err(), &i18n),
        "There is nothing to act on."
    );
    pane.cursor = Some(3);
    assert_eq!(
        present::refusal(nomatch.targets(&pane, |_| false).unwrap_err(), &i18n),
        "That row has not loaded yet."
    );
}

#[path = "tests_query_keys.rs"]
mod query_keys;
