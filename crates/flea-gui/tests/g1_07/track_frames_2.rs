//! Element frames for the clip marks, the dividers and a still peek row.
use super::check::Checks;
use super::fixture::{reply, row, Pane, Producers, SETTINGS};
use super::frames::{allocations, frame, held, open, read, real_cx, set};
use super::track_frames::{
    active, files, install_theme, look, misses, palette, peeks, track, Marks,
};
use flea_gui::columns::{
    geometry::{cells, contain_y, Slot},
    paint::{Clip, ClipMarks, Palette, CUT, HAIR},
    preview_host::Cursor,
    Area,
};
use gpui::{Hsla, TestAppContext};
use std::rc::Rc;

#[test]
fn the_clipboard_is_asked_per_row_only_while_it_holds_something() {
    let mut c = Checks::new("tests/columnclip-empty.qml");
    let names = files(('a'..='j').map(|letter| format!("{letter}.txt")));
    let marks = Rc::new(Marks::default());
    let clip: Rc<dyn ClipMarks> = marks.clone();
    let mut cx = real_cx();
    install_theme(&mut cx);
    let window = open(&mut cx, Rc::from([active(400.)]), held(names), 400., 185.);
    set(&mut cx, window, |view| view.clip = Some(clip));
    let clips = |cx: &mut TestAppContext| {
        read(cx, window, |view| {
            let paint = view
                .painters
                .get(Slot::Active)
                .expect("the active track")
                .borrow();
            paint
                .visible()
                .map(|index| (index, paint.clip(index)))
                .collect::<Vec<_>>()
        })
    };
    frame(&mut cx, window);
    c.eq(
        161,
        "an empty clipboard is asked nothing over six painted rows",
        marks.asked.get(),
        0,
    );
    c.ok(
        152,
        "an empty clipboard marks no row",
        clips(&mut cx).iter().all(|(_, clip)| *clip == Clip::None),
    );
    *marks.held.borrow_mut() = Some(("/probe/b.txt".into(), Clip::Copy));
    frame(&mut cx, window);
    let copied = clips(&mut cx);
    c.ok(
        176,
        "the copied row reads its copy mark",
        copied.contains(&(1, Clip::Copy)),
    );
    c.ok(
        181,
        "every other painted row reads none",
        copied
            .iter()
            .all(|(index, clip)| *index == 1 || *clip == Clip::None),
    );
    c.ok(
        184,
        "a held clipboard is asked for every painted row",
        marks.asked.get() >= 6,
    );
    *marks.held.borrow_mut() = Some(("/probe/b.txt".into(), Clip::Cut));
    frame(&mut cx, window);
    let cut = clips(&mut cx);
    c.ok(
        196,
        "the cut row reads its scissors",
        cut.contains(&(1, Clip::Cut)),
    );
    c.ok(
        198,
        "the cut state is the row's clip value",
        cut.iter().filter(|(_, clip)| *clip == Clip::Cut).count() == 1,
    );
    let palette = palette(&mut cx, window);
    let inks = palette.row_ink(Clip::Cut, false, false, false);
    let uncut = palette.row_ink(Clip::None, false, false, false);
    // drag-cut-move: only the file mark ghosts; the name and size keep their contrast.
    c.ok(
        200,
        "a cut row's mark dims to the board opacity, its name, size and scissors stay whole",
        (inks.mark.a - CUT).abs() < 0.01
            && inks.name == uncut.name
            && inks.clip == palette.muted_frame,
    );
    *marks.held.borrow_mut() = None;
    let asked = marks.asked.get();
    frame(&mut cx, window);
    c.eq(
        214,
        "emptied again, a frame asks nothing",
        marks.asked.get() - asked,
        0,
    );
    let scroll = contain_y(9, 10, 0., 185., 31.);
    set(&mut cx, window, |view| view.active_scroll = scroll);
    frame(&mut cx, window);
    c.eq(
        227,
        "scrolled to the last row an empty clipboard is asked nothing",
        (scroll, marks.asked.get() - asked),
        (125., 0),
    );
    c.eq(
        152,
        "scrolled, rows 4 to 9 paint and none is marked",
        clips(&mut cx),
        (4..10).map(|index| (index, Clip::None)).collect::<Vec<_>>(),
    );
    for _ in 0..10 {
        let _ = marks.mark("/probe", "a.txt");
    }
    c.eq(
        239,
        "the counting double counts direct questions",
        marks.asked.get() - asked,
        10,
    );
    c.finish();
}

fn dividers(area: &Area) -> Vec<(Slot, Option<f32>)> {
    area.tracks()
        .iter()
        .map(|view| (view.slot, view.divider_x()))
        .collect()
}

#[test]
fn each_track_draws_one_divider_where_its_edge_is() {
    let mut c = Checks::new("tests/columndividers.qml");
    let mut area = Area::new(900., 600., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut pane = Pane::at("/a/b/c", vec![row("d", true), row("e.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let landed = reply("/a/b/c/d", &[("x.txt", false)]);
    pane.with(|view| area.peeked(&landed, &view, &mut producers.ports(), 0));
    let active = |area: &Area| {
        dividers(area)
            .into_iter()
            .find(|(slot, _)| *slot == Slot::Active)
            .and_then(|(_, x)| x)
    };
    c.eq(
        90,
        "a child folder draws the active track's divider",
        active(&area),
        Some(599.),
    );
    pane.cursor = 1;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    // The swap's owner releases the file hold once the preview is ready.
    producers.swap.holding = false;
    let file = Cursor {
        row: true,
        is_dir: false,
        child: "",
    };
    area.host.show_cursor_row(file, false, false);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    c.eq(
        90,
        "a file preview with the column on draws it",
        active(&area),
        Some(599.),
    );
    area.settings.column = false;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 2));
    c.eq(
        90,
        "a populated file track keeps its edge with previews off",
        active(&area),
        Some(599.),
    );
    assert!(!area.host.third_shown(area.settings));
    assert!(matches!(
        area.tracks().last().unwrap().rows,
        flea_gui::columns::TrackRows::Blank
    ));
    pane.go("/a/b/c", Vec::new());
    area.settings.column = true;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 3));
    c.eq(90, "an empty third draws none", active(&area), None);
    let mut wide = Area::new(2400., 600., Some(5.), 0., SETTINGS);
    let deep = Pane::at("/a/b/c/d", Vec::new());
    deep.with(|view| wide.sync(&view, &mut producers.ports(), 4));
    let drawn: Vec<f32> = dividers(&wide).into_iter().filter_map(|(_, x)| x).collect();
    c.eq(
        107,
        "at 2400 px the ancestors' dividers sit at their right edges",
        drawn,
        vec![479., 959., 1439.],
    );
    wide.resize(1800., 600.);
    deep.with(|view| wide.sync(&view, &mut producers.ports(), 5));
    let drawn: Vec<f32> = dividers(&wide).into_iter().filter_map(|(_, x)| x).collect();
    c.eq(107, "at 1800 px two", drawn, vec![449., 899.]);
    c.finish();
    let mut c = Checks::new("tests/columnscost.qml");
    let parent = dividers(&area)
        .into_iter()
        .find(|(slot, _)| *slot == Slot::Parent)
        .and_then(|(_, x)| x);
    c.eq(
        113,
        "at 900 px the parent's divider sits at 299 and the active track's empty third draws none",
        (parent, active(&area)),
        (Some(299.), None),
    );
    let mut cx = real_cx();
    install_theme(&mut cx);
    let window = open(
        &mut cx,
        Rc::clone(area.tracks()),
        held(Vec::new()),
        900.,
        600.,
    );
    let palette = palette(&mut cx, window);
    c.eq(
        122,
        "the divider inks with the hairline rung",
        palette.hairline(),
        palette.foreground.opacity(HAIR),
    );
    c.finish();
}

#[test]
fn a_peek_row_with_no_clip_paints_whole_and_still() {
    let mut c = Checks::new("tests/columnscost.qml");
    let mut cx = real_cx();
    install_theme(&mut cx);
    let one = Rc::from([track(
        Slot::Parent,
        0.,
        314.,
        peeks(&[("columnscost.txt", false)]),
        None,
    )]);
    let none = Rc::from([track(Slot::Parent, 0., 314., peeks(&[]), None)]);
    let window = open(&mut cx, one, held(Vec::new()), 314., 200.);
    let bare = open(&mut cx, none, held(Vec::new()), 314., 200.);
    for _ in 0..3 {
        frame(&mut cx, window);
        frame(&mut cx, bare);
    }
    let before = misses(&mut cx, window, Slot::Parent);
    let heap = allocations();
    frame(&mut cx, window);
    let drawn = allocations() - heap;
    let heap = allocations();
    frame(&mut cx, bare);
    let empty = allocations() - heap;
    c.eq(
        276,
        "a still frame of the row reshapes nothing and allocates as an empty track does",
        (misses(&mut cx, window, Slot::Parent) - before, drawn),
        (0, empty),
    );
    let clip = look(&mut cx, window, Slot::Parent, |paint, _, _| paint.clip(0));
    c.eq(282, "no clipboard leaves the row uncut", clip, Clip::None);
    let palette = palette(&mut cx, window);
    let plain = palette.row_ink(Clip::None, false, false, true);
    c.eq(
        284,
        "an uncut peek row inks its name and mark with their own roles",
        (plain.name, plain.mark),
        (palette.muted, palette.muted_frame),
    );
    c.eq(
        286,
        "an uncut row draws no clip glyph",
        clip == Clip::None,
        true,
    );
    let marks = Rc::new(Marks::default());
    *marks.held.borrow_mut() = Some(("/probe/columnscost.txt".into(), Clip::Copy));
    let held_clip: Rc<dyn ClipMarks> = marks.clone();
    set(&mut cx, window, |view| view.clip = Some(held_clip));
    frame(&mut cx, window);
    let (copied, x) = look(&mut cx, window, Slot::Parent, |paint, _, _| {
        (
            paint.clip(0),
            cells(314., false, false).clip_x() - (314. - 14. - 14. - 12. - 12.),
        )
    });
    c.eq(
        288,
        "a copied row's mark sits in its reserved slot",
        (copied, x),
        (Clip::Copy, 0.),
    );
    // drag-cut-move: a cut ghosts the file mark alone, so the name keeps its alpha on every row.
    for (line, ink, label, cut_alpha) in [
        (
            310,
            name_ink as fn(&Palette, Clip) -> Hsla,
            "none, scissors, none, copy: the name never dims",
            name_ink(&palette, Clip::None).a,
        ),
        (
            312,
            mark_ink,
            "none, scissors, none, copy: only the cut mark dims",
            CUT,
        ),
    ] {
        let alphas: Vec<f32> = [Clip::None, Clip::Cut, Clip::None, Clip::Copy]
            .into_iter()
            .map(|clip| ink(&palette, clip).a)
            .collect();
        let want = [
            ink(&palette, Clip::None).a,
            cut_alpha,
            ink(&palette, Clip::None).a,
            ink(&palette, Clip::None).a,
        ];
        c.ok(
            line,
            label,
            alphas
                .iter()
                .zip(want)
                .all(|(got, want)| (got - want).abs() < 0.01),
        );
    }
    let cut = palette.row_ink(Clip::Cut, false, false, true);
    c.ok(
        311,
        "the cut alpha replaces the mark's, with no layer on top, and the name keeps its own ink",
        (cut.mark.a - CUT).abs() < 0.01 && cut.name == plain.name,
    );
    c.finish();
}

fn name_ink(palette: &Palette, clip: Clip) -> Hsla {
    palette.row_ink(clip, false, false, true).name
}

fn mark_ink(palette: &Palette, clip: Clip) -> Hsla {
    palette.row_ink(clip, false, false, true).mark
}
