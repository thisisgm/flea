use super::{
    host::OutputView,
    model::{Action, Card, Focus},
    tests::selected,
};
use flea::ui_logic_print_convert::output::Kind;
use gpui::{Modifiers, TestAppContext};
use std::sync::Arc;

const ROWS: [&str; 8] = [
    "review-op4-format-0",
    "review-op4-format-1",
    "review-op4-format-2",
    "review-op4-format-3",
    "review-op4-format-4",
    "review-op4-format-5",
    "review-op4-format-6",
    "review-op4-format-7",
];

#[gpui::test]
fn op4_pointer_hover_after_a_chord_leaves_the_body_scroll_alone(cx: &mut TestAppContext) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
    view.update_in(vcx, |view, window, cx| {
        assert!(view.open(
            Action::Convert,
            selected(Kind::Image, &["a.png"]),
            window,
            cx
        ));
    });
    vcx.run_until_parked();
    let body = vcx.debug_bounds("op4-body").unwrap();
    let (index, row) = (0..8)
        .map(|i| (i, vcx.debug_bounds(ROWS[i]).unwrap()))
        .find(|(_, r)| r.top() < body.bottom() && r.bottom() > body.bottom())
        .expect("no format row straddles the clamped body bottom");
    vcx.simulate_keystrokes("ctrl-a");
    vcx.run_until_parked();
    let before = view.read_with(vcx, |v, _| v.body_scroll.handle.offset());
    let at = gpui::point(
        row.origin.x + row.size.width / 2.,
        (row.top() + body.bottom()) / 2.,
    );
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.run_until_parked();
    let after = view.read_with(vcx, |v, _| v.body_scroll.handle.offset());
    let cursor = view.read_with(vcx, |v, _| match &v.model.card {
        Some(Card::Convert(c)) => c.cursor,
        _ => usize::MAX,
    });
    eprintln!(
        "pointer chord hover probe: row{index} pointer={at:?} offset {before:?} -> {after:?} cursor={cursor}"
    );
    assert_eq!(cursor, index, "hover did not move the cursor to the row");
    assert_eq!(
        before, after,
        "a hover after a key chord scrolled the clamped body"
    );
}

#[gpui::test]
fn op4_pointer_a_click_after_a_chord_never_reorders(cx: &mut TestAppContext) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
    view.update_in(vcx, |view, window, cx| {
        assert!(view.open(
            Action::Combine,
            selected(Kind::Image, &["a.png", "b.png"]),
            window,
            cx
        ));
        let image = Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::from_raw(1, 1, vec![255; 4]).unwrap(),
        )]));
        let thumbs = ["a.png", "b.png"]
            .iter()
            .map(|name| super::seam::Thumbnail {
                path: format!("/fixture/{name}"),
                size: (100, 100),
                image: image.clone(),
            })
            .collect::<Vec<_>>();
        assert!(view.set_thumbnails(11, thumbs, cx));
        view.model.setting(Focus::Layout, 1, &view.catalog);
        view.flush(cx);
    });
    vcx.run_until_parked();
    for _ in 0..5 {
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
    }
    let body = vcx.debug_bounds("op4-body").unwrap();
    let preview = view.read_with(vcx, |v, _| v.preview_bounds.get().unwrap());
    let order = |vcx: &mut gpui::VisualTestContext| {
        view.read_with(vcx, |v, _| {
            v.model
                .selection
                .as_ref()
                .unwrap()
                .files
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>()
        })
    };
    let state = |vcx: &mut gpui::VisualTestContext| {
        view.read_with(vcx, |v, _| (v.model.focus, v.body_scroll.handle.offset()))
    };
    let at = gpui::point(
        preview.origin.x + preview.size.width / 2.,
        preview.bottom().min(body.bottom()) - gpui::px(20.),
    );
    assert!(at.y > body.top() && at.y > preview.top());
    let start = order(vcx);
    let click = |vcx: &mut gpui::VisualTestContext| {
        vcx.simulate_mouse_down(at, gpui::MouseButton::Left, Modifiers::default());
        vcx.run_until_parked();
        vcx.simulate_mouse_up(at, gpui::MouseButton::Left, Modifiers::default());
        vcx.run_until_parked();
    };
    eprintln!(
        "pointer chord click probe: start {:?} at={at:?} preview={preview:?}",
        state(vcx)
    );
    click(vcx);
    let first = order(vcx);
    eprintln!(
        "pointer chord click probe: after first click {:?} order {start:?} -> {first:?}",
        state(vcx)
    );
    assert_eq!(start, first, "the first click reordered");
    vcx.simulate_keystrokes("ctrl-c");
    vcx.run_until_parked();
    click(vcx);
    let second = order(vcx);
    eprintln!(
        "pointer chord click probe: after Ctrl+C and a second click {:?} order {first:?} -> {second:?}",
        state(vcx)
    );
    assert_eq!(
        first, second,
        "a click after Ctrl+C reordered the combined images"
    );
}

#[gpui::test]
fn op4_pointer_wheel_after_a_chord_keeps_its_offset(cx: &mut TestAppContext) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
    view.update_in(vcx, |view, window, cx| {
        assert!(view.open(
            Action::Convert,
            selected(Kind::Image, &["a.png"]),
            window,
            cx
        ));
    });
    vcx.run_until_parked();
    let row = vcx.debug_bounds(ROWS[0]).unwrap();
    let at = row.center();
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.run_until_parked();
    let offset = |vcx: &mut gpui::VisualTestContext| {
        view.read_with(vcx, |v, _| (v.model.focus, v.body_scroll.handle.offset()))
    };
    let wheel = |vcx: &mut gpui::VisualTestContext, dy: f32| {
        vcx.simulate_event(gpui::ScrollWheelEvent {
            position: at,
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(dy))),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        vcx.run_until_parked();
    };
    let start = offset(vcx);
    wheel(vcx, -60.);
    let plain = offset(vcx);
    wheel(vcx, 60.);
    let back = offset(vcx);
    vcx.simulate_keystrokes("ctrl-a");
    vcx.run_until_parked();
    wheel(vcx, -60.);
    let chord = offset(vcx);
    eprintln!(
        "pointer chord wheel probe: start={start:?} plain wheel={plain:?} back={back:?} after Ctrl+A wheel={chord:?}"
    );
    assert!(
        plain.1.y < start.1.y,
        "the plain wheel did not scroll the body"
    );
    assert_eq!(back, start);
    assert_eq!(chord, plain, "a stale reveal undid the wheel after Ctrl+A");
}
