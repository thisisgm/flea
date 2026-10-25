//! Element frames for a hidden view's return, every realised row of a short column, and a cut row's inks.
use super::check::Checks;
use super::fixture::{drain_asks, reply, row, Pane, Producers, SETTINGS};
use super::frames::{frame, held, open, read, real_cx, set};
use super::track_frames::{active, files, install_theme, misses, palette, peeks, track, visible};
use flea_gui::columns::{
    geometry::Slot,
    paint::{Clip, CUT},
    Area, TrackRows, TrackState,
};
use gpui::TestAppContext;
use std::rc::Rc;

const SLOTS: [Slot; 3] = [Slot::Parent, Slot::Active, Slot::Child];

#[test]
fn a_kept_return_draws_every_track_whole_and_builds_nothing() {
    let mut c = Checks::new("tests/columnshidden.qml");
    let mut area = Area::new(1200., 620., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut listing = vec![row("b", true)];
    listing.extend(files((0..60).map(|i| format!("f{i:02}.txt"))));
    let mut pane = Pane::at("/fx/colhid/a", listing.clone());
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    for path in drain_asks(&mut area) {
        let answer = reply(&path, &[("a", true), ("x.txt", false)]);
        pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), 1));
    }
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        Rc::clone(area.tracks()),
        held(listing),
        1200.,
        620.,
    );
    frame(&mut cx, window);
    // A scroll down a screen and back leaves rows shaped past the viewport, which a hide drains.
    for scroll in [620., 0.] {
        set(&mut cx, window, |view| view.active_scroll = scroll);
        frame(&mut cx, window);
    }
    let held_rows = |cx: &mut TestAppContext| {
        read(cx, window, |view| {
            view.painters
                .get(Slot::Active)
                .map_or(0, |paint| paint.borrow().cache.len())
        })
    };
    let live = visible(&mut cx, window, Slot::Active);
    let pooled = held_rows(&mut cx);
    pane.with(|view| area.set_visible(false, &view, &mut producers.ports(), 2));
    set(&mut cx, window, |view| view.painters.park());
    c.eq(
        327,
        "hide drains the rows a shown scroll pooled and keeps its live rows",
        (pooled > live.len(), held_rows(&mut cx)),
        (true, live.len()),
    );
    pane.cursor = 1;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 3));
    pane.cursor = 0;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 4));
    c.eq(
        339,
        "hidden cursor steps ask for nothing",
        drain_asks(&mut area),
        Vec::<String>::new(),
    );
    let built = read(&mut cx, window, |view| view.painters.constructed);
    let before: u64 = SLOTS
        .iter()
        .map(|slot| misses(&mut cx, window, *slot))
        .sum();
    pane.with(|view| area.set_visible(true, &view, &mut producers.ports(), 5));
    let whole = area.tracks().iter().all(|view| {
        view.state == TrackState::Rows
            && !matches!(&view.rows, TrackRows::Peek(rows) if rows.is_empty())
    });
    c.ok(
        384,
        "the return's own call draws every column whole",
        whole && area.tracks().len() == 3,
    );
    let tracks = Rc::clone(area.tracks());
    set(&mut cx, window, |view| view.tracks = tracks);
    frame(&mut cx, window);
    let after: u64 = SLOTS
        .iter()
        .map(|slot| misses(&mut cx, window, *slot))
        .sum();
    let rebuilt = read(&mut cx, window, |view| view.painters.constructed);
    c.eq(
        388,
        "a kept return builds no rows and no track",
        (after - before, rebuilt - built),
        (0, 0),
    );
    let first = visible(&mut cx, window, Slot::Active);
    let mut steady = true;
    for _ in 0..5 {
        frame(&mut cx, window);
        steady &= visible(&mut cx, window, Slot::Active) == first;
    }
    let total: u64 = SLOTS
        .iter()
        .map(|slot| misses(&mut cx, window, *slot))
        .sum();
    c.ok(
        385,
        "the frames after the return draw the same rows with no blank and no reshape",
        steady && !first.is_empty() && total == after,
    );
    c.finish();
}

#[test]
fn a_short_column_draws_every_row() {
    let mut c = Checks::new("tests/columns-peekdrag.qml");
    let names: Vec<(String, bool)> = (0..5).map(|i| (format!("p{i}.txt"), false)).collect();
    let rows: Vec<(&str, bool)> = names
        .iter()
        .map(|(name, dir)| (name.as_str(), *dir))
        .collect();
    let tracks = Rc::from([track(Slot::Parent, 0., 400., peeks(&rows), None), {
        let mut bare = active(400.);
        bare.x = 400.;
        bare
    }]);
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        tracks,
        held(files(names.iter().map(|(name, _)| name.clone()))),
        800.,
        400.,
    );
    frame(&mut cx, window);
    c.eq(
        122,
        "every peek row is drawn",
        visible(&mut cx, window, Slot::Parent),
        (0..5).collect::<Vec<_>>(),
    );
    c.eq(
        168,
        "every bare row is drawn",
        visible(&mut cx, window, Slot::Active),
        (0..5).collect::<Vec<_>>(),
    );
    c.finish();
}

#[test]
fn a_cut_row_dims_its_content_and_an_uncut_row_keeps_its_roles() {
    let mut c = Checks::new("tests/columnrow-geom.qml");
    let mut cx = real_cx();
    install_theme(&mut cx);
    let window = open(
        &mut cx,
        Rc::from([active(294.)]),
        held(Vec::new()),
        294.,
        200.,
    );
    let palette = palette(&mut cx, window);
    let (cut, plain) = (
        palette.row_ink(Clip::Cut, false, false, false),
        palette.row_ink(Clip::None, false, false, false),
    );
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    // drag-cut-move: a cut row ghosts its mark alone, so its name keeps the contrast an uncut one has.
    c.ok(
        296,
        "a cut row's name inks in its own role, as an uncut one does",
        cut.name == plain.name && near(plain.name.a, palette.foreground.a),
    );
    c.ok(
        299,
        "its size shares the name's undimmed ink",
        cut.name == palette.foreground,
    );
    c.ok(
        302,
        "its kind mark dims with it",
        near(cut.mark.a, CUT) && plain.mark == palette.muted_frame,
    );
    c.ok(
        309,
        "an uncut row applies no dim",
        plain.name == palette.foreground && plain.mark == palette.muted_frame,
    );
    c.finish();
}
