//! tests/columnsmoves.qml controls: a watched refresh across a lateral landing, and the landing's hold, cap, minimum and supersede.
use super::check::Checks;
use super::cols_moves::{answered, Sim};
use super::cols_moves_2::{denied, BASE, ELSEWHERE, ELSEWHERE_RAIL, FIXTURE, GROUP, OTHER};
use flea_gui::columns::{geometry::Slot, TrackState};
use flea_gui::pane::swap::{CAP_MS, MIN_SHOW_MS};
use std::rc::Rc;

const SOURCE: &str = "tests/columnsmoves.qml";

/// The state the controls start from, the arrow move's: the base listed with the cursor on its second row.
fn after_arrow(c: &mut Checks) -> Sim {
    let mut sim = Sim::moves();
    sim.open(BASE, "");
    sim.settle(c, "arrow", 2, 4);
    sim.pane.cursor = 1;
    sim.sync();
    sim.settle(c, "arrow", 3, 5);
    sim
}

/// until(): the condition must come true, else the control times out.
fn until(c: &mut Checks, sim: &mut Sim, label: &str, done: impl Fn(&Sim) -> bool) {
    let ok = sim.wait(done);
    c.ok(206, &format!("{label} timed out"), ok);
}

/// The parent column draws its reading tile.
fn parent_reads(sim: &Sim) -> bool {
    sim.track(Slot::Parent)
        .is_some_and(|view| view.state == TrackState::Loading)
}

#[test]
fn a_watched_refresh_lands_through_a_lateral_landing() {
    let mut c = Checks::new(SOURCE);
    let mut sim = after_arrow(&mut c);
    // The probe writes a denial into the store; here the kept parent's watch is answered by one.
    sim.changed(GROUP);
    if let Some(first) = sim.take_peek(GROUP) {
        sim.peek(&denied(GROUP, first, 0));
    }
    let before = sim.denied_mode(GROUP);
    sim.dirs
        .get_mut(GROUP)
        .expect("the group is listed")
        .push(("fresh".to_owned(), true));
    sim.changed(GROUP);
    c.ok(
        351,
        "changed re-asks the kept parent",
        sim.queued_peek(GROUP),
    );
    sim.open(BASE, "");
    sim.settle(&mut c, "refresh", 5, 0);
    let names = sim.rows_for(GROUP);
    c.ok(
        355,
        "watching refresh survives a lateral landing and updates in place",
        names.iter().any(|name| name == "fresh"),
    );
    c.eq(
        356,
        "successful refresh removes the previous denial",
        (before, sim.denied_mode(GROUP)),
        (Some(0), None),
    );
    c.covers(&[357]);
    c.finish();
}

/// supersede(): a second request before the first landing's ancestor answers keeps what the slot draws.
fn supersede(c: &mut Checks, sim: &mut Sim) {
    sim.freeze_peeks = true;
    sim.open(ELSEWHERE_RAIL, "");
    until(c, sim, "first landing with a pending ancestor", |s| {
        s.landed_at(ELSEWHERE_RAIL)
    });
    let drawn = sim.rows_for(ELSEWHERE);
    c.ok(
        220,
        "first landing holds its pending ancestor",
        sim.waiting(ELSEWHERE) && !drawn.is_empty(),
    );
    sim.freeze_lists = true;
    sim.open(OTHER, "");
    c.ok(
        223,
        "superseding request keeps the on-screen ancestor",
        !sim.rows_for(ELSEWHERE).is_empty(),
    );
    let held: Vec<String> = sim
        .area
        .landing
        .rows(FIXTURE, OTHER, Rc::from([]))
        .iter()
        .map(|row| row.name.clone())
        .collect();
    c.eq(
        224,
        "new hold takes the rows the old slot draws",
        (sim.area.landing.waiting(FIXTURE, OTHER), held),
        (true, drawn),
    );
    let mut blank = false;
    for _ in 0..2 {
        if sim.pane.here == ELSEWHERE_RAIL && !sim.loading(ELSEWHERE) {
            blank |= sim.census().columns.first().is_none_or(|rows| *rows == 0);
        }
        sim.frame();
    }
    c.ok(
        227,
        "every superseding frame keeps the on-screen ancestor until the swap or cap",
        !blank,
    );
    sim.freeze_lists = false;
    sim.freeze_peeks = false;
    until(c, sim, "superseding landing settles", Sim::settled);
    let first = sim.rows_for(FIXTURE).first().cloned();
    c.ok(
        230,
        "superseding landing draws the new ancestor",
        sim.pane.here == OTHER && first.as_deref() == Some("group"),
    );
}

/// timedFallback(): the cap turns held rows into the reading tile, and a reply inside its minimum stays covered.
fn timed_fallback(c: &mut Checks, sim: &mut Sim) {
    sim.freeze_peeks = true;
    let began = sim.now;
    sim.open(ELSEWHERE_RAIL, "");
    until(c, sim, "timed fallback lands", |s| {
        s.landed_at(ELSEWHERE_RAIL)
    });
    let parent = sim.track(Slot::Parent);
    c.ok(
        242,
        "pending ancestor starts with held rows",
        parent.is_some_and(|view| view.path == ELSEWHERE) && !sim.rows_for(ELSEWHERE).is_empty(),
    );
    // The probe hides the column here; a track has no hidden state, so its tile counts as shown when the cap draws it.
    sim.advance_to(began + CAP_MS + 35);
    c.ok(
        246,
        "real cap timer replaces held rows with reading",
        sim.loading(ELSEWHERE) && sim.rows_for(ELSEWHERE).is_empty(),
    );
    c.eq(
        248,
        "cap builds the LoadingState",
        sim.track(Slot::Parent).map(|view| view.state),
        Some(TrackState::Loading),
    );
    let shown_at = began + CAP_MS;
    c.eq(
        252,
        "LoadingState reports when its mark is shown",
        sim.area.landing.minimum_deadline(),
        Some(shown_at + MIN_SHOW_MS),
    );
    sim.advance_to(shown_at + MIN_SHOW_MS / 2);
    sim.freeze_peeks = false;
    let now = sim.now;
    sim.advance_to(now);
    let landed = answered(&sim.area, &sim.pane, ELSEWHERE);
    c.ok(
        254,
        "reply during the shown minimum stays covered",
        landed && sim.loading(ELSEWHERE) && sim.waiting(ELSEWHERE),
    );
    until(c, sim, "real minimum timer releases the reply", |s| {
        !s.waiting(ELSEWHERE)
    });
    c.ok(
        257,
        "real minimum timer releases only after the mark stands MIN_SHOW_MS",
        sim.now >= shown_at + MIN_SHOW_MS
            && !parent_reads(sim)
            && !sim.rows_for(ELSEWHERE).is_empty(),
    );
}

/// loadingSupersede(): a request while the reading tile shows carries it, and its minimum, into the next landing.
fn loading_supersede(c: &mut Checks, sim: &mut Sim) {
    sim.open(OTHER, "");
    until(
        c,
        sim,
        "reading supersede starts from a settled listing",
        Sim::settled,
    );
    sim.freeze_peeks = true;
    sim.open(ELSEWHERE_RAIL, "");
    until(c, sim, "ancestor's real cap shows reading", |s| {
        !s.pane.in_flight && parent_reads(s)
    });
    let appeared_at = sim.now;
    sim.open(OTHER, "");
    c.ok(
        278,
        "superseding request keeps the shown reading state",
        parent_reads(sim),
    );
    until(c, sim, "reading supersede lands", |s| s.landed_at(OTHER));
    let fixture = sim
        .track(Slot::Parent)
        .is_some_and(|view| view.path == FIXTURE);
    c.ok(
        281,
        "superseding landing carries the shown reading state",
        fixture && parent_reads(sim) && sim.area.landing.minimum_deadline().is_some(),
    );
    // The slot's tile stayed shown across its new path, so the minimum still runs from the first mark.
    let deadline = sim.area.landing.minimum_deadline();
    c.ok(
        281,
        "superseding landing keeps the minimum its shown mark started",
        deadline.is_some_and(|due| due <= appeared_at + MIN_SHOW_MS),
    );
    sim.freeze_peeks = false;
    until(c, sim, "reading supersede's reply lands", Sim::settled);
    c.ok(
        284,
        "superseding reading state keeps its shown minimum before releasing to the new rows",
        sim.now >= appeared_at + MIN_SHOW_MS
            && !parent_reads(sim)
            && !sim.rows_for(FIXTURE).is_empty(),
    );
}

/// noCarryClears() and noCarrySingleStep(): a landing begun without a carry keeps no earlier landing's held rows.
fn no_carry(c: &mut Checks, sim: &mut Sim) {
    sim.freeze_peeks = true;
    sim.open(ELSEWHERE_RAIL, "");
    until(c, sim, "no-carry landing holds its drawn column", |s| {
        s.landed_at(ELSEWHERE_RAIL)
    });
    // The probe reads the first held key; at three columns the one incoming ancestor is the parent.
    let held = ELSEWHERE;
    c.ok(
        297,
        "no-carry setup holds a drawn column",
        sim.waiting(held),
    );
    sim.set_visible(false);
    sim.open(OTHER, "");
    until(c, sim, "no-carry hidden hop lands", |s| s.landed_at(OTHER));
    sim.open(ELSEWHERE_RAIL, "");
    until(c, sim, "no-carry hidden return lands", |s| {
        s.landed_at(ELSEWHERE_RAIL)
    });
    sim.set_visible(true);
    c.ok(
        306,
        "hidden navigations keep no previous landing",
        !sim.keeps_previous(),
    );
    c.ok(
        307,
        "hidden navigations wait on no stale held row",
        !sim.waiting(held),
    );
    sim.freeze_peeks = false;
    until(c, sim, "no-carry landing settles", Sim::settled);
    sim.freeze_peeks = true;
    sim.open(OTHER, "");
    until(
        c,
        sim,
        "single-step no-carry landing holds its drawn column",
        |s| s.landed_at(OTHER),
    );
    c.ok(
        319,
        "single-step no-carry setup holds its current landing",
        sim.landing_target_is(OTHER) && sim.waiting(FIXTURE),
    );
    sim.begin_without_carry();
    c.ok(
        321,
        "begin with no carry keeps no previous landing",
        !sim.keeps_previous(),
    );
    c.ok(
        322,
        "begin with no carry waits on nothing",
        !sim.waiting(FIXTURE),
    );
    sim.freeze_peeks = false;
    until(c, sim, "single-step no-carry landing settles", Sim::settled);
}

#[test]
fn a_landing_holds_supersedes_and_releases_its_ancestors() {
    let mut c = Checks::new(SOURCE);
    let mut sim = after_arrow(&mut c);
    // The refresh control's lateral landing, which the supersede controls start from.
    sim.open(BASE, "");
    sim.settle(&mut c, "refresh", 5, 0);
    supersede(&mut c, &mut sim);
    timed_fallback(&mut c, &mut sim);
    loading_supersede(&mut c, &mut sim);
    no_carry(&mut c, &mut sim);
    // The control log line and the chain from one control to the next are this test's own steps.
    c.covers(&[195, 231, 243, 259, 286, 298]);
    c.finish();
}
