//! The rail's current place, painted by the real producer: HIG trash and rail-favorites draw `.rr.place` as --sel with no edge.
use super::producer::Producer;
use crate::palette::Theme;
use gpui::{div, prelude::*, Context, Hsla, Quad, TestAppContext, Window};
use std::sync::Arc;

// Sample input: the HIG boards' --bg, --surface, --fg and --accent.
const BOARD: &str =
    "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'";

struct Sized(gpui::Entity<Producer>);

impl Render for Sized {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(gpui::px(192.))
            .h(gpui::px(600.))
            .child(self.0.clone())
    }
}

/// Paints the rail with the first row as the place and returns the quads inside that row.
fn place_quads(cx: &mut TestAppContext, focused: bool, mark: bool) -> (Theme, Vec<Quad>, f32) {
    let theme = Theme::from_text(BOARD);
    cx.update(|cx| cx.set_global(theme));
    let state = flea::jsondoc::parse(r#"{"places":{"showHome":true,"showTrash":true}}"#).unwrap();
    let (parent, view) = cx.add_window_view(|_, cx| {
        Sized(cx.new(|cx| {
            let mut producer = Producer::new(
                Arc::new(crate::test_catalogs::english()),
                &state,
                "/bin/false".into(),
                cx,
            );
            producer.set_polling(true, cx);
            producer
        }))
    });
    let producer = parent.read_with(view, |parent, _| parent.0.clone());
    view.run_until_parked();
    let draw = |view: &mut gpui::VisualTestContext| {
        view.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    draw(view);
    let first = producer.read_with(view, |producer, _| producer.laid.borrow().rows[0].index);
    producer.update(view, |producer, cx| {
        let mut model = producer.model.borrow_mut();
        model.lit = Some(first);
        model.cursor = Some(first);
        model.focused = focused;
        model.pointer_focus = false;
        model.mark_focused = mark;
        drop(model);
        cx.notify();
    });
    draw(view);
    let row = producer.read_with(view, |producer, _| producer.laid.borrow().rows[0].rect);
    let (quads, scale) = view.update(|window, _| {
        let scale = window.scale_factor();
        let quads = window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                let (x, y) = (
                    quad.bounds.origin.x.0 / scale,
                    quad.bounds.origin.y.0 / scale,
                );
                (x - row.x).abs() <= 1. && (y - row.y).abs() <= 1.
            })
            .collect();
        (quads, scale)
    });
    (theme, quads, scale)
}

fn edges(quads: &[Quad], ink: Hsla, scale: f32) -> usize {
    quads
        .iter()
        .filter(|quad| {
            quad.background.as_solid() == Some(ink) && quad.bounds.size.width.0 / scale <= 2.5
        })
        .count()
}

#[gpui::test]
fn the_place_has_no_accent_edge_while_the_list_holds_the_keyboard(cx: &mut TestAppContext) {
    let (theme, quads, scale) = place_quads(cx, false, false);
    let grey = theme.foreground.opacity(crate::list::GREY_CURSOR);
    assert!(
        quads
            .iter()
            .any(|quad| quad.background.as_solid() == Some(grey)),
        "the place keeps --sel"
    );
    assert_eq!(
        edges(&quads, theme.accent_frame, scale),
        0,
        "HIG .rr.place draws no accent edge"
    );
}

#[gpui::test]
fn the_rail_cursor_keeps_its_accent_edge_while_the_rail_holds_the_keyboard(
    cx: &mut TestAppContext,
) {
    let (theme, quads, scale) = place_quads(cx, true, false);
    assert_eq!(
        edges(&quads, theme.accent_frame, scale),
        1,
        "HIG .rr.cur draws the 2px accent-frame edge"
    );
}

// EY-105: with the keyboard on the row's eject mark, v0.3.12 turns the row behind it to the grey cursor ground.
#[gpui::test]
fn the_cursor_row_turns_grey_while_its_eject_mark_holds_the_keyboard(cx: &mut TestAppContext) {
    let (theme, quads, _) = place_quads(cx, true, true);
    let grey = theme.foreground.opacity(crate::list::GREY_CURSOR);
    let wash = theme.accent.opacity(crate::list::CURSOR_WASH);
    assert!(
        quads
            .iter()
            .any(|quad| quad.background.as_solid() == Some(grey)),
        "the row takes the grey cursor ground"
    );
    assert!(
        !quads
            .iter()
            .any(|quad| quad.background.as_solid() == Some(wash)),
        "the row drops the accent wash"
    );
}

// EY-565: v0.3.12 SidebarRow.qml keeps the 2px accent edge on the cursor row while its eject mark holds the keyboard.
#[gpui::test]
fn fix_views_2_ey565_the_cursor_row_keeps_its_edge_while_its_eject_mark_holds_the_keyboard(
    cx: &mut TestAppContext,
) {
    let (theme, quads, scale) = place_quads(cx, true, true);
    assert_eq!(
        edges(&quads, theme.accent_frame, scale),
        1,
        "the edge stays over the grey cursor ground"
    );
}
