//! Filter040's Tab, inert until the recursive producer joins, and what the pane says while an answer is out.
use super::filter::{Asked, Filter, LineKey, PaneSeam};
use super::{present, wire};
use flea::json::{field_str, field_usize};
use flea::ui_logic_listing_filter::narrow;

const WORK: [&str; 9] = [
    "A",
    "Code",
    "Docs",
    "Empty",
    "Locked",
    "Many",
    "Media",
    "notes.txt",
    "todo.md",
];

struct Pane {
    folder: Vec<String>,
    cursor: Option<usize>,
    picked: Vec<usize>,
}

impl Pane {
    fn new() -> Self {
        Self {
            folder: WORK.map(String::from).to_vec(),
            cursor: Some(0),
            picked: Vec::new(),
        }
    }

    // The backend at BASE, answering from the request line itself over the folder's own listing.
    fn serve(&self, request: &str) -> String {
        let (query, id) = (
            field_str(request, "query").unwrap(),
            field_usize(request, "id").unwrap(),
        );
        let rows = narrow(
            &query,
            self.folder
                .iter()
                .enumerate()
                .map(|(row, name)| (row, name.as_str())),
        )
        .unwrap();
        let list = |rows: &[usize]| {
            rows.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            r#"{{"t":"filtered","id":{id},"query":"{query}","n":{},"rows":[{}],"close":[{}],"ms":0.1,"listing":1}}"#,
            self.folder.len(),
            list(&rows.exact),
            list(&rows.close)
        )
    }
}

impl PaneSeam for Pane {
    fn listing(&self) -> usize {
        1
    }
    fn total(&self) -> usize {
        self.folder.len()
    }
    fn cursor(&self) -> Option<usize> {
        self.cursor
    }
    fn set_cursor(&mut self, row: usize) {
        self.cursor = Some(row);
    }
    fn show_view(&mut self, _: usize) {}
    fn selected(&self) -> Vec<usize> {
        self.picked.clone()
    }
    fn unmark(&mut self, rows: &[usize]) {
        self.picked.retain(|row| !rows.contains(row));
    }
    fn mark(&mut self, rows: &[usize]) {
        self.picked.extend_from_slice(rows);
    }
    fn focus_list(&mut self) {}
    fn refill(&mut self) {}
}

fn ask(filter: &mut Filter, pane: &mut Pane, asked: Asked) {
    let line = pane.serve(&wire::request(asked.id, &asked.query));
    filter
        .receive(wire::parse(&line).unwrap().unwrap(), pane)
        .unwrap();
}

#[test]
fn filter040_while_an_answer_is_out_the_pane_speaks_for_the_answer_it_draws() {
    let i18n = super::tests_flow::english();
    let (mut filter, mut pane) = (Filter::default(), Pane::new());
    for key in ["o", "z"] {
        let asked = filter.typed(key, &mut pane).unwrap();
        ask(&mut filter, &mut pane, asked);
    }
    assert_eq!(
        present::no_match(&filter, 1, &i18n).as_deref(),
        Some("NOTHING MATCHES oz")
    );
    let pending = filter.backspace(&mut pane).unwrap();
    assert_eq!(filter.query(), "o", "the line shows what is typed");
    assert_eq!(
        present::no_match(&filter, 1, &i18n).as_deref(),
        Some("NOTHING MATCHES oz"),
        "the headline names the query its rows answer"
    );
    assert_eq!(
        present::strip(&filter, 1, "work", &i18n).summary,
        "0 of 9 shown",
        "and the counts stay that answer's"
    );
    filter.select_all(&mut pane);
    assert!(
        pane.picked.is_empty(),
        "Ctrl+A takes the rows drawn, which are that answer's"
    );
    ask(&mut filter, &mut pane, pending);
    assert_eq!(
        present::no_match(&filter, 1, &i18n),
        None,
        "once o lands its rows are drawn"
    );
    assert_eq!(
        present::strip(&filter, 1, "work", &i18n).summary,
        "5 of 9 shown"
    );
}

// Tab stays inert on the line until the joined recursive producer exists: no request, no scope, nothing redrawn.
#[test]
fn filter040_tab_is_inert_until_the_recursive_producer_joins() {
    let (mut filter, mut pane) = (Filter::default(), Pane::new());
    assert_eq!(
        filter.line_key(LineKey::Tab, &mut pane),
        (true, None),
        "Tab before a key asks nothing and is consumed"
    );
    assert_eq!(filter.shown(1), None, "and draws nothing");
    let asked = filter.typed("notes", &mut pane).unwrap();
    ask(&mut filter, &mut pane, asked);
    let before = filter.counts(1);
    assert_eq!(
        filter.line_key(LineKey::Tab, &mut pane),
        (true, None),
        "Tab over a standing filter asks nothing"
    );
    assert_eq!(
        (filter.asked(), filter.counts(1)),
        (None, before),
        "and the folder's answer stands as it was"
    );
    let next = filter.typed("x", &mut pane).unwrap();
    ask(&mut filter, &mut pane, next);
    assert_eq!(filter.query(), "notesx", "the next key narrows as before");
    assert_eq!(
        filter.counts(1).map(|counts| counts.tested),
        Some(9),
        "over the folder's own listing"
    );
}
