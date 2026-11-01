//! tests/js/columnsback.js runReads: what a drawn column reads from the peek store.
use super::check::Checks;
use flea::protocol::RowFacts;
use flea_gui::columns::carry::{Leaving, ListOrder};
use flea_gui::columns::peeks::{peek_key, tag_of, Facts, Landed, PeekRow, Peeked, Peeks, Source};
use flea_gui::navigation::folder_state::Folders;
use std::rc::Rc;

pub const SOURCE: &str = "tests/js/columnsback.js";

/// The oracle's state: folders first, no kind grouping, over the pane's name ascending order.
pub const STATE: ListOrder<'static> = ListOrder {
    by: "name",
    desc: false,
    folders_first: true,
    group_by_kind: false,
};

/// The listing's device and writability the oracle pane's backend names.
pub const DIR_FACTS: Facts = Facts {
    dev: 7,
    writable: true,
};

pub fn key(path: &str) -> String {
    peek_key(path, false, false)
}

pub fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

/// listRows: n rows named dir0.., the first a folder, the rest plain text files.
pub fn list_rows(dir: &str, n: usize) -> Vec<RowFacts> {
    (0..n)
        .map(|i| RowFacts {
            depth: 0,
            open: false,
            name: format!("{dir}{i}"),
            dir: i == 0,
            size: 13,
            mtime: 1,
            mode: if i == 0 { 16877 } else { 33188 },
            icon: if i == 0 { "folder" } else { "text-x-generic" }.into(),
            link: None,
            real_modes: true,
            thumbless: false,
            kind: String::new(),
            inline: None,
            tags: None,
        })
        .collect()
}

/// A listing's rows in a peek's own shape: name, folder flag and icon alone.
pub fn peek_shape(rows: &[RowFacts]) -> Vec<PeekRow> {
    rows.iter()
        .map(|row| PeekRow {
            name: row.name.clone(),
            dir: row.dir,
            icon: row.icon.clone(),
        })
        .collect()
}

/// A peeked reply under the default order at the oracle's window size of 35.
pub fn answer(path: &str, rows: Vec<PeekRow>, facts: Facts, denied: Option<u32>) -> Peeked {
    Peeked {
        path: path.into(),
        hidden: false,
        hidden_last: false,
        first: 35,
        order: String::new(),
        total: rows.len(),
        facts,
        denied,
        rows: Rc::from(rows),
    }
}

/// Holds one column in a store the way a landed reply does: asked, then answered.
pub fn hold(peeks: &mut Peeks, reply: &Peeked) {
    peeks.pending.track(&reply.sent());
    assert_eq!(peeks.land(reply, ""), Landed::Stored, "fixture reply lands");
}

/// The oracle area(): at /a/b/c listing /a/b, 40 rows of 300 held from 0, a window of 35.
pub fn leaving(rows: &[RowFacts]) -> Leaving<'_> {
    Leaving {
        here: "/a/b/c",
        target: "/a/b",
        tag: "",
        window_size: 35,
        total: 300,
        held_start: 0,
        narrowed: false,
        rows,
        order: STATE,
        facts: DIR_FACTS,
    }
}

#[test]
fn reads_answer_only_landed_columns() {
    let mut c = Checks::new(SOURCE);
    c.covers(&[7]);
    let empty = Peeks::default();
    c.eq(
        36,
        "an unanswered column draws no rows",
        empty.rows(&key("/a")).map(|rows| rows.len()).unwrap_or(0),
        0,
    );
    let mut held = Peeks::default();
    hold(
        &mut held,
        &answer("/a", peek_shape(&list_rows("a", 2)), DIR_FACTS, None),
    );
    c.eq(
        39,
        "an answered column draws its rows",
        held.rows(&key("/a")).map(|rows| rows.len()).unwrap_or(0),
        2,
    );
    c.ok(
        40,
        "an empty path draws nothing",
        held.rows(&key("")).is_none() && !held.answered(&key(""), ""),
    );
    let mut denied = Peeks::default();
    hold(&mut denied, &answer("/d", Vec::new(), DIR_FACTS, Some(0)));
    c.eq(
        43,
        "a denial whose stat failed keeps mode 0",
        denied.denied(&key("/d")),
        Some(0),
    );
    c.eq(
        44,
        "a path never denied reads -1",
        denied.denied(&key("/e")),
        None,
    );
    let mut facts = Peeks::default();
    let named = Facts {
        dev: 7,
        writable: false,
    };
    hold(&mut facts, &answer("/a", Vec::new(), named, None));
    let source = |dir: &str, dev: u64, writable: bool| Source {
        dir: dir.into(),
        dev,
        writable,
    };
    c.eq(
        45,
        "a drag names the peeked device and writability",
        facts.source("/a"),
        Some(source("/a", 7, false)),
    );
    c.eq(
        47,
        "an unpeeked folder drags as writable on device 0",
        facts.source("/b"),
        Some(source("/b", 0, true)),
    );
    c.eq(48, "an empty path drags nothing", facts.source(""), None);
    let settings =
        flea::jsondoc::parse(r#"{"folderSorts":{},"rememberSort":true}"#).expect("settings JSON");
    let folders = Folders::from_settings(&settings, Vec::new());
    c.eq(
        49,
        "a folder with no order of its own asks under none",
        tag_of(folders.peek_order("/a")),
        String::new(),
    );
    c.finish();
}
