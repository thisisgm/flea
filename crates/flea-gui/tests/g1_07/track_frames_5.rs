//! Element frames for a Back landing and a held-key sweep: rows shape once, at the frame that first shows them.
use super::check::Checks;
use super::cols_back_qml::launch;
use super::cols_moves::Sim;
use super::frames::{frame, held, open, real_cx, set, View};
use super::track_frames::{files, misses, peeks, track, visible};
use flea::protocol::RowFacts;
use flea_gui::columns::{
    geometry::{contain_y, Slot},
    paint::Focus,
    TrackRows,
};
use flea_gui::pane::visible_rows;
use gpui::{TestAppContext, WindowHandle};
use std::{collections::BTreeSet, rc::Rc};

const SLOTS: [Slot; 3] = [Slot::Parent, Slot::Active, Slot::Child];

/// The window draws what the area and the pane hold now, the cursor under the pane's focus.
fn show(cx: &mut TestAppContext, window: WindowHandle<View>, sim: &Sim) {
    set(cx, window, |view| {
        view.tracks = Rc::clone(sim.area.tracks());
        view.held = held(sim.pane.rows.clone());
        view.focus = Rc::new(Focus {
            cursor: Some(sim.pane.cursor),
            ..Focus::default()
        });
    });
    frame(cx, window);
}

fn shaped(cx: &mut TestAppContext, window: WindowHandle<View>) -> [u64; 3] {
    SLOTS.map(|slot| misses(cx, window, slot))
}

#[test]
fn a_back_landing_shapes_every_row_in_its_first_frame_and_a_down_none() {
    let mut c = Checks::new("tests/columnsback.qml");
    let mut sim = launch();
    sim.run(200);
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        Rc::clone(sim.area.tracks()),
        held(Vec::new()),
        1300.,
        600.,
    );
    show(&mut cx, window, &sim);
    let landed_from = sim.landings.len();
    sim.open("/a/b", "/a/b/c");
    let landed = sim.wait(|s| s.landings.len() > landed_from);
    c.ok(
        384,
        "the Back to /a/b landed",
        landed && sim.pane.here == "/a/b",
    );
    show(&mut cx, window, &sim);
    let first = shaped(&mut cx, window);
    let child = visible(&mut cx, window, Slot::Child);
    c.ok(
        381,
        "the landing's first frame draws the folder left in the child",
        !child.is_empty(),
    );
    let mut late = [0; 3];
    let mut dropped = 0;
    sim.watch(320, |s| {
        show(&mut cx, window, s);
        let now = shaped(&mut cx, window);
        for (at, count) in now.into_iter().enumerate() {
            late[at] += count - first[at];
        }
        dropped += usize::from(visible(&mut cx, window, Slot::Child) != child);
    });
    c.eq(
        384,
        "rows shaped in any track after the landing's first frame",
        late,
        [0; 3],
    );
    c.eq(
        381,
        "frames whose child dropped a row it drew at the landing",
        dropped,
        0,
    );
    // A Down after the landing moves the cursor plate only, so the parent and the active track shape nothing.
    let before = shaped(&mut cx, window);
    sim.pane.cursor = 1;
    sim.sync();
    let mut built = 0;
    sim.watch(200, |s| {
        show(&mut cx, window, s);
        let now = shaped(&mut cx, window);
        built = (now[0] - before[0]) + (now[1] - before[1]);
    });
    c.eq(
        282,
        "rows the Down's settle shaped outside the child",
        built,
        0,
    );
    c.finish();
}

/// Three tracks over a 300-file listing: a drawn parent, the active listing and the file's preview.
fn sweep(replace_at: Option<usize>) -> (usize, usize, [u64; 2], usize) {
    let (width, height) = (400., 600.);
    let rows: Vec<RowFacts> = files((0..300).map(|i| format!("held-{i:03}.txt")));
    let parent = peeks(&[("held", true), ("other", true)]);
    let tracks: Rc<[_]> = Rc::from([
        track(Slot::Parent, 0., width, parent, Some("held")),
        track(Slot::Active, width, width, TrackRows::Listing, None),
        track(Slot::Child, width * 2., width, TrackRows::Preview, None),
    ]);
    let mut cx = real_cx();
    let window = open(&mut cx, tracks, held(rows.clone()), width * 3., height);
    frame(&mut cx, window);
    let start = [
        misses(&mut cx, window, Slot::Parent),
        misses(&mut cx, window, Slot::Active),
    ];
    let mut seen: BTreeSet<usize> = visible(&mut cx, window, Slot::Active).into_iter().collect();
    let first = seen.len();
    let mut scroll = 0.;
    let mut live = 0;
    // Seventy j presses: the cursor moves one row per frame and the scroll contains it in one pass.
    for cursor in 1..=70 {
        scroll = contain_y(cursor, rows.len(), scroll, height, 31.);
        let replaced = replace_at == Some(cursor);
        set(&mut cx, window, |view| {
            view.focus = Rc::new(Focus {
                cursor: Some(cursor),
                ..Focus::default()
            });
            view.active_scroll = scroll;
            if replaced {
                // The rows replacement: the same names with new facts, as a window refresh lands them.
                let mut fresh = rows.clone();
                fresh.iter_mut().for_each(|row| row.size += 1);
                view.held = held(fresh);
            }
        });
        frame(&mut cx, window);
        let drawn = visible(&mut cx, window, Slot::Active);
        if replaced {
            live = drawn.len();
        }
        seen.extend(drawn);
    }
    let ends = [
        misses(&mut cx, window, Slot::Parent),
        misses(&mut cx, window, Slot::Active),
    ];
    let spent = [ends[0] - start[0], ends[1] - start[1]];
    (first, seen.len(), spent, live)
}

#[test]
fn a_held_key_sweep_shapes_each_shown_row_once() {
    let mut c = Checks::new("tests/columns-held.qml");
    let (first, shown, spent, _) = sweep(None);
    c.ok(
        183,
        "the sweep showed rows past its first screen",
        shown > visible_rows(600.),
    );
    c.eq(
        183,
        "the parent track shapes no row while the cursor moves",
        spent[0],
        0,
    );
    // A list view binds every row it shows at least once, so one shape per shown row is its floor.
    c.eq(
        183,
        "the active track shapes each newly shown row once",
        spent[1],
        (shown - first) as u64,
    );
    let (_, _, replaced, live) = sweep(Some(35));
    c.ok(190, "the replacement landed on live rows", live > 0);
    c.ok(
        190,
        "the rows replacement reshapes each live row at most once",
        replaced[1] - spent[1] <= live as u64,
    );
    c.finish();
}
