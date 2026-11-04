//! tests/js/columns.js peek and neighbour checks as the area drives them: asks, landings, waits.
use super::check::Checks;
use super::fixture::{area, drain_asks, reply, reply_with, row, Pane, Producers, SETTINGS};
use flea::jsondoc::Json;
use flea_gui::columns::carry::{Leaving, ListOrder};
use flea_gui::columns::peeks::{peek_key, sent_key, Facts, Landed};
use flea_gui::columns::preview_host::HOLD_MS;
use flea_gui::columns::Area;
use flea_gui::navigation::folder_state::Order;

const WORK: &str = "/home/user/work";
const PARENT: &str = "/home/user";
const DOCS: &str = "/home/user/work/docs";
/// The order fields a reply echoes for a folder sorted by size, descending.
const SIZE_DESC: &str = ",\"by\":\"size\",\"desc\":true";

fn sent(path: &str, hidden: bool, hidden_last: bool, first: usize, order: &str) -> String {
    sent_key(&peek_key(path, hidden, hidden_last), first, order)
}

/// Every peek line in the outbox parsed, and the outbox emptied.
fn asked(area: &mut Area) -> Vec<Json> {
    area.outbox
        .drain(..)
        .map(|line| flea::jsondoc::parse(&line).expect("peek line is JSON"))
        .collect()
}

fn field<'a>(doc: Option<&'a Json>, key: &str) -> Option<&'a Json> {
    doc.and_then(|doc| doc.get(key))
}

#[test]
fn the_area_matches_a_reply_on_its_sent_count_and_echoed_order() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut area = area();
    let mut producers = Producers::default();
    let mut pane = Pane::at(WORK, vec![row("docs", true), row("a.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    drain_asks(&mut area);
    let label = "the columns view matches a reply on its sent count and echoed order";
    let repair = reply_with(PARENT, &[("work", true)], false, false, 35, "");
    let landed = pane.with(|view| area.peeked(&repair, &view, &mut producers.ports(), 1));
    c.eq(210, label, landed, Landed::Unasked);
    let resorted = reply_with(PARENT, &[("work", true)], false, false, 512, SIZE_DESC);
    let landed = pane.with(|view| area.peeked(&resorted, &view, &mut producers.ports(), 2));
    c.eq(210, label, landed, Landed::Unasked);
    let own = reply(PARENT, &[("work", true)]);
    let landed = pane.with(|view| area.peeked(&own, &view, &mut producers.ports(), 3));
    c.eq(210, label, landed, Landed::Stored);
    // The pane's window size changes; the held column is still found under its key.
    pane.first = 35;
    let ready = pane.with(|view| view.lens.ready(&area.peeks, PARENT));
    c.ok(
        216,
        "while stored rows keep no count, so a resize orphans no column",
        ready,
    );
    c.finish();
}

#[test]
fn the_area_asks_under_the_window_size_and_the_folder_order() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut area = area();
    let mut producers = Producers::default();
    let mut pane = Pane::at(WORK, vec![row("docs", true), row("a.txt", false)]);
    pane.first = 35;
    let by_size = Order {
        key: "size",
        desc: true,
    };
    pane.orders.insert(PARENT.to_owned(), by_size);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let docs = asked(&mut area);
    let parent = docs
        .iter()
        .find(|doc| doc.get("path").and_then(Json::as_str) == Some(PARENT));
    let label = "and asks with the pane's window size and the folder's order";
    c.eq(
        212,
        label,
        field(parent, "first").and_then(Json::as_f64),
        Some(35.),
    );
    c.eq(
        212,
        label,
        field(parent, "by").and_then(Json::as_str),
        Some("size"),
    );
    c.eq(
        212,
        label,
        field(parent, "desc").and_then(Json::as_bool),
        Some(true),
    );
    c.ok(
        212,
        label,
        area.peeks
            .pending
            .has(&sent(PARENT, false, false, 35, "size1")),
    );
    // The folder goes back to name order before its size-sorted reply lands.
    pane.orders.remove(PARENT);
    let stale = reply_with(PARENT, &[("work", true)], false, false, 35, SIZE_DESC);
    let landed = pane.with(|view| area.peeked(&stale, &view, &mut producers.ports(), 1));
    let label = "a reply under an order the folder has left is dropped";
    c.eq(214, label, landed, Landed::StaleOrder);
    c.ok(
        214,
        label,
        area.peeks.rows(&peek_key(PARENT, false, false)).is_none(),
    );
    c.finish();
}

#[test]
fn the_area_tracks_stores_and_drops_only_its_own_asks() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut area = area();
    let mut producers = Producers::default();
    let pane = Pane::at(WORK, vec![row("docs", true), row("a.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    drain_asks(&mut area);
    let own = sent(PARENT, false, false, 512, "");
    let label = "the columns view tracks its own asks";
    let pending = &area.peeks.pending;
    c.ok(
        236,
        label,
        pending.has(&own) && pending.has(&sent(DOCS, false, false, 512, "")),
    );
    c.eq(236, label, pending.len(), 2);
    let stray = reply("/elsewhere", &[("x", false)]);
    let landed = pane.with(|view| area.peeked(&stray, &view, &mut producers.ports(), 1));
    let label = "and stores only a reply it asked for";
    c.eq(237, label, landed, Landed::Unasked);
    c.ok(
        237,
        label,
        area.peeks
            .rows(&peek_key("/elsewhere", false, false))
            .is_none(),
    );
    let answer = reply(PARENT, &[("work", true)]);
    let landed = pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), 2));
    let label = "and drops the ask it stored";
    c.eq(238, label, landed, Landed::Stored);
    c.ok(238, label, !area.peeks.pending.has(&own));
    c.finish();
}

#[test]
fn the_landing_keeps_only_the_drawn_folders_asks() {
    let mut c = Checks::new("tests/js/columns.js");
    let label = "and takes only the landing's matching drawn-folder asks";
    let mut area = Area::new(2300., 800., Some(5.), 0., SETTINGS);
    let mut producers = Producers::default();
    let mut pane = Pane::at("/a/b/c/d", vec![row("f.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    c.eq(
        239,
        label,
        drain_asks(&mut area),
        vec!["/a".to_owned(), "/a/b".to_owned(), "/a/b/c".to_owned()],
    );
    // A step into e: the incoming ancestors are /a/b, /a/b/c and /a/b/c/d, so /a leaves.
    let order = ListOrder {
        by: "name",
        desc: false,
        folders_first: true,
        group_by_kind: false,
    };
    pane.with(|view| {
        let leaving = Leaving {
            here: "/a/b/c/d",
            target: "/a/b/c/d/e",
            tag: "",
            window_size: 512,
            total: 1,
            held_start: 0,
            narrowed: false,
            rows: &pane.rows,
            order,
            facts: Facts {
                dev: 0,
                writable: true,
            },
        };
        area.listing_requested(leaving, &view.lens, 1);
    });
    pane.go("/a/b/c/d/e", vec![row("g.txt", false)]);
    pane.with(|view| area.path_landed(&view, &mut producers.ports(), 2));
    let pending = &area.peeks.pending;
    c.eq(
        239,
        label,
        pending.count(&sent("/a/b", false, false, 512, "")),
        1,
    );
    c.eq(
        239,
        label,
        pending.count(&sent("/a/b/c", false, false, 512, "")),
        1,
    );
    c.ok(239, label, !pending.has(&sent("/a", false, false, 512, "")));
    c.finish();
}

/// The work folder synced, one flag toggled, synced again: the parent re-asked, under the new key.
fn toggled(hidden: bool, hidden_last: bool) -> (bool, bool) {
    let mut area = area();
    let mut producers = Producers::default();
    let mut pane = Pane::at(WORK, vec![row("docs", true), row("a.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    drain_asks(&mut area);
    pane.hidden = hidden;
    pane.hidden_last = hidden_last;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    let asked_again = drain_asks(&mut area).iter().any(|ask| ask == PARENT);
    let under_new_key = area
        .peeks
        .pending
        .has(&sent(PARENT, hidden, hidden_last, 512, ""));
    (asked_again, under_new_key)
}

#[test]
fn a_flag_toggle_re_asks_the_shown_ancestors_under_the_new_key() {
    let mut c = Checks::new("tests/js/columns.js");
    let (asked_again, under_new_key) = toggled(true, false);
    let label = "and re-asks the shown ancestors under the new key";
    c.ok(241, label, asked_again);
    c.ok(241, label, under_new_key);
    let (asked_again, under_new_key) = toggled(false, true);
    let label = "and the order it watches carries both flags";
    c.ok(243, label, asked_again);
    c.ok(243, label, under_new_key);
    c.finish();
}

#[test]
fn refresh_asks_the_fresh_neighbour_list() {
    let mut c = Checks::new("tests/js/columns.js");
    let label = "refresh asks the fresh neighbour list";
    let mut area = Area::new(800., 800., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut pane = Pane::at("/x/y", vec![row("z.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    c.eq(320, label, drain_asks(&mut area), Vec::<String>::new());
    // One width step asks the parent in the same pass, with no second event.
    area.resize(1100., 800.);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    c.eq(320, label, drain_asks(&mut area), vec!["/x".to_owned()]);
    // One path step asks the new parent in the landing's own refresh.
    pane.go("/p/q", vec![row("r.txt", false)]);
    pane.with(|view| area.path_landed(&view, &mut producers.ports(), 2));
    c.eq(
        321,
        "and reads no stale show gate there",
        drain_asks(&mut area),
        vec!["/p".to_owned()],
    );
    c.finish();
}

#[test]
fn a_folder_wait_stops_the_old_work_cap_first() {
    let mut c = Checks::new("tests/js/columns.js");
    let label = "a folder wait stops the old work cap first";
    let mut area = area();
    let mut producers = Producers::default();
    let mut pane = Pane::at(WORK, vec![row("a.txt", false), row("docs", true)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    drain_asks(&mut area);
    producers.swap.calls.clear();
    // The cursor steps onto a folder whose peek has not answered.
    pane.cursor = 1;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 5));
    c.eq(
        323,
        label,
        producers.swap.calls.clone(),
        vec!["stopCap".to_owned()],
    );
    c.eq(323, label, area.host.fallback_at, Some(5 + HOLD_MS));
    c.finish();
}
