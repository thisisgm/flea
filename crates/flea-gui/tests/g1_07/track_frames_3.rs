//! Element frames for a big landing, a hidden view and the preview edge.
use super::check::Checks;
use super::fixture::{drain_asks, reply, row, Pane, Producers, SETTINGS};
use super::frames::{frame, held, open, read, real_cx, set};
use super::track_frames::{files, look, misses, paints, visible};
use flea_gui::columns::{
    edge::{EdgeElement, EdgePaint},
    geometry::Slot,
    paint::{Focus, CACHE_SCREENS},
    Area,
};
use flea_gui::pane::visible_rows;
use gpui::{div, prelude::*, px, size, TestAppContext, Window};
use std::rc::Rc;

#[test]
fn a_big_landing_paints_every_viewport_row_and_a_hidden_view_keeps_none() {
    let mut c = Checks::new("tests/columnsprebuild.qml");
    let mut area = Area::new(2536., 1381., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let thousand = files((0..1000).map(|i| format!("f_{i:04}.txt")));
    let pane = Pane::at("/fx/big", thousand.clone());
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let _ = drain_asks(&mut area);
    let width = area
        .tracks()
        .iter()
        .find(|view| view.slot == Slot::Active)
        .map_or(0., |view| view.width);
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        Rc::clone(area.tracks()),
        held(thousand),
        2536.,
        1381.,
    );
    frame(&mut cx, window);
    let rows = visible_rows(1381.);
    c.eq(
        197,
        "the landing paints the 45 viewport rows",
        visible(&mut cx, window, Slot::Active),
        (0..rows).collect::<Vec<_>>(),
    );
    let whole = look(&mut cx, window, Slot::Active, |paint, _, _| {
        paint.visible().all(|index| paint.prepared(index).is_some())
    });
    c.ok(198, "no viewport row is held back undrawn", whole);
    c.ok(303, "every drawn index paints its row", whole);
    c.ok(
        199,
        "viewport row i paints f_i",
        paints(&mut cx, window, width, 0..rows, |i| format!("f_{i:04}.txt")),
    );
    let peek = |cx: &mut TestAppContext| {
        read(cx, window, |view| {
            [Slot::Parent, Slot::Child]
                .iter()
                .filter_map(|slot| view.painters.get(*slot))
                .map(|paint| (paint.borrow().cache.len(), paint.borrow().cache.misses))
                .collect::<Vec<_>>()
        })
    };
    let (shaped, peeks_before) = (misses(&mut cx, window, Slot::Active), peek(&mut cx));
    for step in 0..10 {
        set(&mut cx, window, |view| {
            view.focus = Rc::new(Focus {
                cursor: Some(step),
                ..Focus::default()
            })
        });
        frame(&mut cx, window);
    }
    let len = read(&mut cx, window, |view| {
        view.painters
            .get(Slot::Active)
            .map_or(0, |paint| paint.borrow().cache.len())
    });
    c.eq(
        320,
        "frames after the landing reshape nothing and stay inside the bound",
        (
            misses(&mut cx, window, Slot::Active) - shaped,
            len <= rows * CACHE_SCREENS,
        ),
        (0, true),
    );
    c.eq(
        306,
        "the active track's frames leave every peek's rows alone",
        peek(&mut cx),
        peeks_before,
    );
    c.eq(
        211,
        "frames between key events reshape nothing",
        misses(&mut cx, window, Slot::Active) - shaped,
        0,
    );
    for (line, total) in [(361, 20), (305, 1)] {
        let rows = files((0..total).map(|i| format!("big{i:04}")));
        set(&mut cx, window, |view| view.held = held(rows));
        frame(&mut cx, window);
        c.eq(
            line,
            "a short folder paints exactly its rows",
            visible(&mut cx, window, Slot::Active),
            (0..total).collect::<Vec<_>>(),
        );
    }
    // Hidden over the one-row folder, the active track parks all but its live row, and the return reshapes nothing.
    set(&mut cx, window, |view| view.painters.park());
    let kept = read(&mut cx, window, |view| {
        view.painters
            .get(Slot::Active)
            .map_or(0, |paint| paint.borrow().cache.len())
    });
    c.ok(345, "a hidden column holds only its kept row", kept <= 1);
    c.ok(404, "hidden again it still holds at most one", kept <= 1);
    let parked = misses(&mut cx, window, Slot::Active);
    frame(&mut cx, window);
    c.eq(
        345,
        "shown again the kept row draws with no reshape",
        misses(&mut cx, window, Slot::Active) - parked,
        0,
    );
    c.finish();
}

#[test]
fn the_edge_paints_its_line_and_mark_while_hovered() {
    let mut c = Checks::new("tests/columns-pwid.qml");
    let mut area = Area::new(1200., 600., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let pane = Pane::at("/probe/studio", vec![row("deep", true)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let landed = reply("/probe/studio/deep", &[("x.txt", false)]);
    pane.with(|view| area.peeked(&landed, &view, &mut producers.ports(), 0));
    area.edge_hover(800.);
    let paint = area.edge_paint();
    c.eq(
        258,
        "the hovered edge centres its 16 px mark on the 1 px line at 799, and on the middle row",
        (paint.mark, paint.line),
        (Some((791.5, 292., 16.)), Some((799., 0., 1., 600.))),
    );
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1200.), px(600.)), |_, _| Edge(paint));
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the edge window is open");
    area.edge_hover(100.);
    c.eq(
        258,
        "away from the zone the edge paints nothing",
        area.edge_paint(),
        Default::default(),
    );
    c.finish();
}

struct Edge(EdgePaint);

impl gpui::Render for Edge {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        div().size_full().child(EdgeElement::new(self.0))
    }
}
