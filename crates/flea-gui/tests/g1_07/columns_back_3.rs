//! tests/js/columnsback.js runLanding and runGuard: what a landing keeps, and the gap that holds.
use super::check::Checks;
use super::columns_back::{
    answer, hold, key, leaving, list_rows, paths, peek_shape, DIR_FACTS, SOURCE,
};
use super::fixture::{area, row, Pane, Producers};
use flea_gui::columns::carry::{Leaving, Plan};
use flea_gui::columns::peeks::{sent_key, Facts, Lens, PeekRow, Peeks};
use flea_gui::columns::preview_host::{PreviewSettings, Shown};
use flea_gui::navigation::folder_state::Order;
use std::rc::Rc;

/// No folder holds an order of its own.
fn none(_: &str) -> Option<Order> {
    None
}

/// Every folder re-sorted by size, descending.
fn size(_: &str) -> Option<Order> {
    Some(Order {
        key: "size",
        desc: true,
    })
}

static NONE: fn(&str) -> Option<Order> = none;
static SIZE: fn(&str) -> Option<Order> = size;

fn lens<'a>(here: &'a str, hidden: bool, orders: &'a dyn Fn(&str) -> Option<Order>) -> Lens<'a> {
    Lens {
        here,
        hidden,
        hidden_last: false,
        first: 35,
        orders,
    }
}

/// True when both reads hold the very same rows, the identity the oracle's === compares.
fn same(a: Option<&Rc<[PeekRow]>>, b: Option<&Rc<[PeekRow]>>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if Rc::ptr_eq(a, b))
}

/// Every map of the store empty, the oracle's all-empty landing.
fn bare(peeks: &Peeks) -> bool {
    let mut held = peeks.clone();
    held.version = 0;
    format!("{held:?}") == format!("{:?}", Peeks::default())
}

/// No column rows held, the oracle's empty peeked map.
fn no_rows(peeks: &Peeks) -> bool {
    format!("{peeks:?}").starts_with("Peeks { rows: {}")
}

fn rows(dir: &str, n: usize) -> Vec<PeekRow> {
    peek_shape(&list_rows(dir, n))
}

#[test]
fn a_landing_keeps_answered_drawn_columns_and_the_seed() {
    let mut c = Checks::new(SOURCE);
    c.covers(&[9]);
    let listing = list_rows("c", 40);
    let plan = Plan::new(&leaving(&listing), &paths(&["/a"])).expect("a plan from a listing");
    let mut held = Peeks::default();
    let parent_facts = Facts {
        dev: 1,
        writable: true,
    };
    hold(&mut held, &answer("/a", rows("a", 2), parent_facts, None));
    hold(&mut held, &answer("/a/b", rows("b", 2), DIR_FACTS, None));
    hold(
        &mut held,
        &answer("/a/b/c/c0", rows("x", 2), DIR_FACTS, None),
    );
    let at_b = lens("/a/b", false, &NONE);
    let asks = paths(&["/a"]);
    let next = Plan::land(Some(&plan), &asks, "", &held, &at_b);
    c.ok(
        96,
        "the landing keeps the shown ancestor",
        same(next.rows(&key("/a")), held.rows(&key("/a"))),
    );
    c.eq(
        97,
        "with its order and facts",
        (next.order(&key("/a")), next.source("/a").map(|s| s.dev)),
        (Some(""), Some(1)),
    );
    c.ok(
        98,
        "drops the columns no longer shown",
        next.rows(&key("/a/b")).is_none() && next.rows(&key("/a/b/c/c0")).is_none(),
    );
    c.ok(
        99,
        "seeds the folder left as the child",
        same(next.rows(&key("/a/b/c")), plan.rows.as_ref()),
    );
    c.eq(
        100,
        "under its present order, with its facts",
        (
            next.order(&key("/a/b/c")),
            next.source("/a/b/c").map(|s| s.dev),
        ),
        (Some(""), Some(7)),
    );
    c.eq(
        101,
        "with no outstanding ask when all peeks already answered",
        next.pending.len(),
        0,
    );
    c.ok(
        102,
        "leaves the maps it was handed alone",
        held.rows(&key("/a/b")).is_some(),
    );
    c.ok(
        103,
        "no carry clears everything",
        bare(&Plan::land(None, &asks, "", &held, &at_b)),
    );
    let at_x = lens("/x", false, &NONE);
    c.ok(
        104,
        "a carry for another target clears everything",
        no_rows(&Plan::land(
            Some(&plan),
            &asks,
            "",
            &Peeks::default(),
            &at_x,
        )),
    );
    let fresh = Plan::land(Some(&plan), &asks, "", &Peeks::default(), &at_b);
    c.ok(
        105,
        "an ancestor whose reply is still out is not kept",
        fresh.rows(&key("/a")).is_none(),
    );
    let resorted = lens("/a/b", false, &SIZE);
    let other_order = Plan::land(Some(&plan), &asks, "", &held, &resorted);
    c.ok(
        107,
        "an ancestor kept under another order is not reused",
        other_order.rows(&key("/a")).is_none(),
    );
    c.ok(
        108,
        "a folder re-sorted since the request is not seeded",
        other_order.rows(&key("/a/b/c")).is_none(),
    );
    let mut asked = Peeks::default();
    hold(&mut asked, &answer("/a/b/c", rows("z", 1), DIR_FACTS, None));
    let answered = Plan::land(Some(&plan), &paths(&["/a/b/c"]), "", &asked, &at_b);
    let first = answered
        .rows(&key("/a/b/c"))
        .and_then(|rows| rows.first().map(|row| row.name.clone()));
    c.eq(
        112,
        "a seed never replaces rows a peek already answered",
        first,
        Some("z0".to_owned()),
    );
    c.finish();
}

#[test]
fn a_lateral_landing_keeps_the_parent_and_the_old_third() {
    let mut c = Checks::new(SOURCE);
    let listing = list_rows("c", 40);
    let lateral_pane = Leaving {
        target: "/a/b/d",
        ..leaving(&listing)
    };
    let plan = Plan::new(&lateral_pane, &paths(&["/a/b"])).expect("a lateral plan");
    let kid = "/a/b/c/kid";
    let mut lateral = Peeks::default();
    let parent_facts = Facts {
        dev: 8,
        writable: false,
    };
    hold(
        &mut lateral,
        &answer("/a/b", rows("parent", 2), parent_facts, Some(0)),
    );
    hold(
        &mut lateral,
        &answer(kid, rows("child", 2), DIR_FACTS, None),
    );
    let at_d = lens("/a/b/d", false, &NONE);
    let asks = paths(&["/a/b"]);
    let kept = Plan::land(Some(&plan), &asks, kid, &lateral, &at_d);
    c.ok(
        122,
        "a lateral landing keeps the identical parent rows",
        same(kept.rows(&key("/a/b")), lateral.rows(&key("/a/b"))),
    );
    let parent = kept.source("/a/b");
    c.eq(
        123,
        "with the parent's denial and drag facts",
        (
            kept.denied(&key("/a/b")),
            parent.as_ref().map(|s| s.dev),
            parent.map(|s| s.writable),
        ),
        (Some(0), Some(8), Some(false)),
    );
    c.ok(
        124,
        "and the old third folder while its replacement waits",
        same(kept.rows(&key(kid)), lateral.rows(&key(kid))),
    );
    let parent_ask = sent_key(&key("/a/b"), 35, "");
    let gone_ask = sent_key(&key("/gone"), 35, "");
    lateral.pending.track(&parent_ask);
    lateral.pending.track(&parent_ask);
    lateral.pending.track(&gone_ask);
    let outstanding = Plan::land(Some(&plan), &asks, kid, &lateral, &at_d).pending;
    c.eq(
        129,
        "outstanding watching refreshes of a kept parent still land",
        outstanding.count(&parent_ask),
        2,
    );
    c.eq(
        130,
        "an outstanding peek of a folder no longer drawn is dropped",
        outstanding.count(&gone_ask),
        0,
    );
    let hidden = lens("/a/b/d", true, &NONE);
    c.ok(
        132,
        "another hidden-files key carries none of the old peeks",
        no_rows(&Plan::land(Some(&plan), &asks, kid, &lateral, &hidden)),
    );
    c.finish();
}

/// One moveThird over the oracle's guard root: a file or no cursor under a shown folder.
fn moves(carry: bool, in_flight: bool, cursor_row: bool) -> String {
    let mut area = area();
    area.settings = PreviewSettings {
        column: false,
        automatic: false,
        manual_hold: false,
    };
    area.carry = carry.then(|| Plan {
        target: "/a/b".into(),
        seed: "/a/b/c".into(),
        tag: String::new(),
        pre: Vec::new(),
        rows: None,
        facts: DIR_FACTS,
    });
    area.host.shown = Shown {
        has_row: true,
        is_dir: true,
        child: "/a/b/c0".into(),
    };
    // A fallback armed before the move, so its stop reads as the deadline cleared.
    area.host.fallback_at = Some(1);
    let before = area.host.shown.clone();
    let rows = if cursor_row {
        vec![row("x", false)]
    } else {
        Vec::new()
    };
    let mut pane = Pane::at("/a/b", rows);
    pane.in_flight = in_flight;
    let mut producers = Producers::default();
    pane.with(|view| area.move_third(&view, &mut producers.ports(), true, 0));
    let shown = u8::from(area.host.shown != before);
    let stopped = u8::from(area.host.fallback_at.is_none());
    format!("{shown} shown, {stopped} stopped")
}

#[test]
fn the_landing_gap_alone_holds_the_third_column() {
    let mut c = Checks::new(SOURCE);
    c.covers(&[10]);
    c.eq(
        155,
        "the landing gap keeps the third column without moving it",
        moves(true, true, false),
        "0 shown, 0 stopped".to_owned(),
    );
    c.eq(
        156,
        "the same move with the flight over lands the third column",
        moves(true, false, false),
        "1 shown, 1 stopped".to_owned(),
    );
    c.eq(
        157,
        "the same move with no carry lands the third column",
        moves(false, true, false),
        "1 shown, 1 stopped".to_owned(),
    );
    c.eq(
        158,
        "the same move with a cursor row already landed lands the third column",
        moves(true, true, true),
        "1 shown, 1 stopped".to_owned(),
    );
    c.finish();
}
