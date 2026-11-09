//! Owned Copy To geometry and focus colours, measured from the rendered card.
use super::{host::Overlay, model::CopyFocus, palette::CardPalette};
use crate::{list::Palette, palette::Theme};
use gpui::{px, size, Bounds, Pixels, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector).expect(selector)
}

fn frame_is(cx: &mut VisualTestContext, selector: &'static str, ink: gpui::Hsla) {
    let rect = bounds(cx, selector);
    let (scale, quads) = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.scale_factor(), window.painted_quads())
    });
    assert!(
        quads
            .iter()
            .any(|q| { q.border_color == ink && q.bounds == rect.scale(scale) }),
        "{selector} must paint its own frame in {ink:?}"
    );
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
) -> (gpui::Entity<Overlay>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        window.set_rem_size(px(14.));
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = CardPalette::from_list(&Palette::inherited(&window.text_style(), theme), 8.);
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            palette,
            "default",
        )
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    view.update(cx, |view, cx| view.copy_to(0, false, path, cx));
    cx.run_until_parked();
    (view, cx)
}

#[gpui::test]
fn fix9_copy_to_geometry_follows_the_theme_ladder(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "/fx/v/work");
    for (base, width, button, title) in [
        (12., 473., 26., 14.),
        (14., 552., 30., 16.),
        (16., 630., 35., 18.),
    ] {
        cx.update(|window, cx| {
            window.set_rem_size(px(base));
            view.update(cx, |_, cx| cx.notify());
        });
        cx.run_until_parked();
        let card = bounds(cx, "op1-card");
        assert_eq!(card.size.width, px(width), "card width at {base}");
        assert_eq!(
            bounds(cx, "op1-field").size.height,
            px(button),
            "field at {base}"
        );
        assert_eq!(
            bounds(cx, "op1-button-0").size.height,
            px(button),
            "button at {base}"
        );
        let title_line = cx.update(|window, _| {
            px((title * 1.2_f32 * window.scale_factor()).ceil() / window.scale_factor())
        });
        assert_eq!(bounds(cx, "op1-title").size.height, title_line);
        assert!((f32::from(card.center().y) - 350.).abs() <= 0.5);
    }
}

#[gpui::test]
fn fix9_copy_to_uses_accent_for_field_selection_and_default(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::from_text("background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'")));
    let (view, cx) = open(cx, "/fx/v/work");
    let accent = Theme::from_text("accent='#d28a41'").accent;
    let quads = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    });
    let palette = view.read_with(cx, |view, _| view.palette);
    assert_eq!(palette.accent, accent);
    assert_eq!(palette.accent_frame, accent);
    frame_is(cx, "op1-field", accent);
    frame_is(cx, "op1-button-1", accent);
    assert!(
        quads
            .iter()
            .any(|q| q.background == gpui::Background::from(palette.accent.opacity(0.24))),
        "selected path wash"
    );
    assert!(
        quads
            .iter()
            .any(|q| q.background == gpui::Background::from(palette.accent.opacity(0.14))),
        "default wash"
    );
    view.update(cx, |view, cx| {
        if let Some(super::model::Dialog::CopyTo { focus, .. }) = view.model.dialog.as_mut() {
            *focus = CopyFocus::Confirm;
        }
        cx.notify();
    });
    cx.run_until_parked();
    frame_is(cx, "op1-button-1", palette.foreground);
    frame_is(cx, "op1-field", palette.muted_frame);
    cx.update(|_, cx| {
        cx.set_global(Theme::from_text("background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e6e9ef'"));
        view.update(cx, |view, cx| view.copy_to(0, false, "/fx/v/work", cx));
    });
    cx.run_until_parked();
    let blue = Theme::from_text("accent='#1e66f5'").accent;
    frame_is(cx, "op1-field", blue);
    frame_is(cx, "op1-button-1", blue);
}

#[gpui::test]
fn fix9_empty_copy_to_keeps_the_focused_field_without_selection_or_default_wash(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        cx.set_global(Theme::from_text(
            "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'",
        ))
    });
    let (view, cx) = open(cx, "");
    frame_is(cx, "op1-field", Theme::from_text("accent='#d28a41'").accent);
    assert!(cx.debug_bounds("op1-selection").is_none());
    assert_eq!(bounds(cx, "op1-caret").size.height, px(16.));
    let palette = view.read_with(cx, |view, _| view.palette);
    let quads = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    });
    for alpha in [0.14, 0.24] {
        assert!(!quads
            .iter()
            .any(|q| q.background == gpui::Background::from(palette.accent.opacity(alpha))));
    }
}
