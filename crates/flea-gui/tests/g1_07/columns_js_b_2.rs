//! tests/js/columns.js runColRoot and runHidden: the ancestors a path shows, Left at the root, the gated ancestor columns and the user's hidden set.
use super::check::Checks;
use super::fixture::{drain_asks, reply, row, Pane, Producers, SETTINGS};
use flea::protocol::Reply;
use flea_gui::columns::geometry::{ancestor_shown, ancestors, Slot};
use flea_gui::columns::peeks::join_names;
use flea_gui::columns::sets::{set, Set, Tokens};
use flea_gui::columns::{Area, TrackRows};
use flea_gui::ipc::{columns::ColumnsAdapter, Adapter};
use flea_gui::navigation::folder_state::{Folders, Writer};
use flea_gui::navigation::Navigator;
use flea_gui::pane::Pane as ListPane;
use std::{cell::RefCell, rc::Rc};

/// BOX: this box's resolved tokens at base-size 14, the name floor 20 advances of 7.8125 px.
const BOX: Tokens = Tokens {
    row_padding_x: 14.,
    gap: 9.,
    icon_size: 23.,
    name_min: 156.,
    mode: 70.,
    size: 70.,
    date: 125.,
    kind: 130.,
    location: 0.,
};

/// Columns.ancestors joined the way the source prints them.
fn chain(path: &str, count: usize) -> String {
    ancestors(path, count).join("|")
}

/// The four optional columns a set draws, in the source's mode,size,date,kind order.
fn flags(s: Set) -> String {
    format!("{},{},{},{}", s.mode, s.size, s.date, s.kind)
}

/// A pane whose first listing landed, so nothing is in flight.
fn settled(path: &str) -> ListPane {
    let mut pane = ListPane::new(path.into(), 620.);
    let rows = vec![row("a", true), row("b.txt", false)];
    let total = rows.len();
    let now = pane.now();
    let listed = Reply::Listed {
        path: path.into(),
        total,
    };
    pane.receive_at(listed, now).expect("a listed line");
    let answer = Reply::Rows {
        start: 0,
        rows,
        listing: 1,
    };
    let landed = pane.receive_at(answer, now).expect("a rows line");
    assert!(landed, "the first listing lands");
    pane
}

fn navigator() -> Navigator {
    let settings = flea::jsondoc::parse("{}").expect("settings JSON");
    Navigator::new(
        Rc::new(RefCell::new(Folders::from_settings(&settings, Vec::new()))),
        Rc::new(RefCell::new(Writer::default())),
    )
}

/// The names a slot's track draws, None while it draws no rows of its own.
fn drawn(area: &Area, slot: Slot) -> Option<String> {
    let view = area.tracks().iter().find(|view| view.slot == slot)?;
    match &view.rows {
        TrackRows::Peek(rows) => Some(join_names(rows)),
        _ => None,
    }
}

/// ui/Ipc.qml's parentItemAt through the IPC adapter: the parent's first row centre, "" where the source answers null.
fn parent_item(area: &Area, pane: &Pane) -> String {
    pane.with(|view| {
        let adapter = ColumnsAdapter {
            area,
            lens: view.lens,
            origin: (0., 0.),
            height: 800.,
            widths: "{}",
            child_empty: None,
            preview: None,
        };
        adapter
            .call("columnParentRowCentre", &["0".to_owned()])
            .map(|answer| answer.stdout.trim_end().to_owned())
            .unwrap_or_default()
    })
}

/// A five-column area over a pane on a file row, with these ancestor replies landed.
fn five_at(here: &str, answers: &[(&str, &[(&str, bool)])]) -> (Area, Pane, Producers) {
    let mut area = Area::new(2400., 800., Some(5.), 0., SETTINGS);
    let mut producers = Producers::default();
    let pane = Pane::at(here, vec![row("x.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    for (at, (path, rows)) in answers.iter().enumerate() {
        let answer = reply(path, rows);
        let now = at as u64 + 1;
        pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), now));
    }
    (area, pane, producers)
}

#[test]
fn at_the_root_no_ancestor_repeats_the_active_column() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        372,
        "at / no ancestor column is shown",
        chain("/", 3).as_str(),
        "",
    );
    c.eq(
        373,
        "at /home only the parent (/) is",
        chain("/home", 3).as_str(),
        "/",
    );
    c.eq(
        374,
        "at /home/gm parent and grandparent are",
        chain("/home/gm", 3).as_str(),
        "/|/home",
    );
    c.eq(
        375,
        "one ancestor asked is the parent",
        chain("/home/gm", 1).as_str(),
        "/home",
    );
    c.eq(
        376,
        "three deep names three ancestors",
        chain("/a/b/c", 3).as_str(),
        "/|/a|/a/b",
    );
    c.eq(
        377,
        "the root shows no parent",
        ancestor_shown("/", 1),
        false,
    );
    c.eq(
        378,
        "a child of the root shows its parent",
        ancestor_shown("/home", 1),
        true,
    );
    c.eq(
        379,
        "a child of the root shows no grandparent",
        ancestor_shown("/home", 2),
        false,
    );
    c.eq(
        380,
        "depth two shows two ancestors",
        ancestor_shown("/home/gm", 2),
        true,
    );
    c.eq(
        381,
        "depth two shows no third ancestor",
        ancestor_shown("/home/gm", 3),
        false,
    );
    let mut distinct = true;
    for path in ["/", "/home", "/home/gm", "/a/b/c/d"] {
        let mut columns = ancestors(path, 3);
        columns.push(path.to_owned());
        distinct &= columns.windows(2).all(|pair| pair[0] != pair[1]);
    }
    c.ok(
        390,
        "no shown ancestor ever equals the column to its right",
        distinct,
    );
    let mut root = settled("/");
    let mut nav = navigator();
    nav.pending_select = Some("kept".to_owned());
    let climbed = nav.parent(&mut root);
    let opened = nav.take();
    c.ok(
        394,
        "left at / opens nothing",
        climbed == Ok(false) && opened.is_empty() && !root.busy() && root.path() == "/",
    );
    c.eq(
        395,
        "and plants no select on the climb that did not happen",
        nav.pending_select.as_deref(),
        Some("kept"),
    );
    // A width step re-reads the neighbour list: 800 px asks no ancestor, 1100 px asks the parent.
    let mut area = Area::new(800., 800., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let pane = Pane::at("/x/y", vec![row("a.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let narrow = drain_asks(&mut area);
    area.resize(1100., 800.);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    let wide = drain_asks(&mut area);
    c.eq(
        397,
        "refresh computes its asks fresh, see runNeighbourAsks",
        (narrow, wide),
        (Vec::new(), vec!["/x".to_owned()]),
    );
    c.finish();
}

#[test]
fn an_ancestor_column_draws_rows_only_while_its_ancestor_shows() {
    let mut c = Checks::new("tests/js/columns.js");
    // At /a the root is answered, yet only the parent slot names a directory of its own.
    let root_rows: &[(&str, bool)] = &[("a", true), ("tmp", true)];
    let (mut area, mut pane, mut producers) = five_at("/a", &[("/", root_rows)]);
    let great = drawn(&area, Slot::GreatGrandparent);
    let grand = drawn(&area, Slot::Grandparent);
    let parent = drawn(&area, Slot::Parent);
    let shown_item = parent_item(&area, &pane);
    pane.go("/", vec![row("x.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 9));
    let root_parent = drawn(&area, Slot::Parent);
    let hidden_item = parent_item(&area, &pane);
    let deep_answers: &[(&str, &[(&str, bool)])] = &[
        ("/a", &[("b", true)]),
        ("/a/b", &[("c", true)]),
        ("/a/b/c", &[("d", true)]),
    ];
    let (deep, _deep_pane, _deep_producers) = five_at("/a/b/c/d", deep_answers);
    c.eq(
        400,
        "great-grandparent draws rows only while its ancestor shows",
        (great, drawn(&deep, Slot::GreatGrandparent)),
        (None, Some("b".to_owned())),
    );
    c.eq(
        402,
        "grandparent draws rows only while its ancestor shows",
        (grand, drawn(&deep, Slot::Grandparent)),
        (None, Some("c".to_owned())),
    );
    c.eq(
        404,
        "parent draws rows only while its ancestor shows",
        (parent, root_parent, drawn(&deep, Slot::Parent)),
        (Some("a|tmp".to_owned()), None, Some("d".to_owned())),
    );
    c.ok(
        406,
        "parent reader answers null while its ancestor is hidden",
        !shown_item.is_empty() && hidden_item.is_empty(),
    );
    c.finish();
}

#[test]
fn a_hidden_column_never_draws_and_width_still_wins() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        414,
        "an empty hidden set draws every column the width affords",
        flags(set(2000., &BOX, &[])).as_str(),
        "true,true,true,true",
    );
    c.eq(
        418,
        "a hidden column does not draw at a width that would afford it",
        flags(set(2000., &BOX, &["size", "kind"])).as_str(),
        "true,false,true,false",
    );
    c.eq(
        422,
        "width still wins over a column the user wants back, Mode's own floor included",
        flags(set(200., &BOX, &["kind"])).as_str(),
        "false,false,false,false",
    );
    // set takes a slice, so a caller with no hidden set passes the empty one.
    c.eq(
        426,
        "a caller that passes no hidden set draws as before",
        flags(set(2000., &BOX, &[])).as_str(),
        "true,true,true,true",
    );
    c.finish();
}
