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
fn op4_pointer_hover_on_a_clipped_format_row_leaves_the_body_scroll_alone(cx: &mut TestAppContext) {
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
    let before = view.read_with(vcx, |v, _| v.body_scroll.handle.offset());
    let at = gpui::point(
        row.origin.x + row.size.width / 2.,
        (row.top() + body.bottom()) / 2.,
    );
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.run_until_parked();
    let after = view.read_with(vcx, |v, _| v.body_scroll.handle.offset());
    let (focus, cursor) = view.read_with(vcx, |v, _| {
        (
            v.model.focus,
            match &v.model.card {
                Some(Card::Convert(c)) => c.cursor,
                _ => usize::MAX,
            },
        )
    });
    let moved = vcx.debug_bounds(ROWS[index]).unwrap();
    eprintln!(
        "pointer hover probe: body={body:?} row{index} before={row:?} after={moved:?} pointer={at:?} offset {before:?} -> {after:?} focus={focus:?} cursor={cursor}"
    );
    assert_eq!(focus, Some(Focus::Formats));
    assert_eq!(cursor, index, "hover did not move the cursor to the row");
    assert_eq!(
        before, after,
        "pointer hover alone scrolled the clamped body; the frozen ConvertDialog reveals only after a format choice or a Tab step"
    );
}

fn tab_cycle(cx: &mut TestAppContext, action: Action, height: f32, tabs: usize) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(height)));
    view.update_in(vcx, |view, window, cx| {
        let selection = match action {
            Action::Print => selected(Kind::Pdf, &["a.pdf", "b.pdf"]),
            _ => selected(Kind::Image, &["a.png", "b.png", "c.png"]),
        };
        assert!(view.open(action, selection, window, cx));
        if action == Action::Print {
            let Some(Card::Print(p)) = &view.model.card else {
                panic!()
            };
            let options = p.pending.unwrap().request;
            assert!(view.model.line(&format!(r#"{{"t":"printoptions","id":{options},"ok":true,"destinations":[{{"name":"Office","default":true}}],"printer":"Office","duplex":true,"twoSided":false,"color":true,"grayscale":false,"setupArgv":[]}}"#), &view.catalog));
            view.flush(cx);
        }
    });
    vcx.run_until_parked();
    view.read_with(vcx, |view, _| view.body_scroll.painted.borrow_mut().clear());
    let mut seen = vec![];
    for _ in 0..tabs {
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
        let (focus, offset) =
            view.read_with(vcx, |v, _| (v.model.focus, v.body_scroll.handle.offset()));
        let frames = view.read_with(vcx, |view, _| view.body_scroll.painted.borrow().clone());
        let last = frames.last().cloned();
        eprintln!(
            "pointer tab {action:?} h={height}: focus={focus:?} offset={offset:?} last={last:?}"
        );
        seen.push(focus);
    }
    let frames = view.read_with(vcx, |view, _| view.body_scroll.painted.borrow().clone());
    let clipped: Vec<_> = frames
        .iter()
        .filter(|(target, viewport)| {
            target.size.height <= viewport.size.height
                && (target.top() < viewport.top() || target.bottom() > viewport.bottom())
        })
        .collect();
    eprintln!(
        "pointer tab {action:?} h={height}: {} painted targets, {} clipped, focus path {seen:?}",
        frames.len(),
        clipped.len()
    );
    assert!(!frames.is_empty());
    assert!(clipped.is_empty(), "clipped tab targets: {clipped:?}");
}

#[gpui::test]
fn op4_pointer_print_tab_targets_stay_inside_the_clamped_body(cx: &mut TestAppContext) {
    tab_cycle(cx, Action::Print, 260., 7);
}

#[gpui::test]
fn op4_pointer_combine_tab_targets_stay_inside_the_clamped_body(cx: &mut TestAppContext) {
    tab_cycle(cx, Action::Combine, 400., 8);
}

#[gpui::test]
fn op4_pointer_a_plain_click_on_a_scrolled_preview_never_reorders(cx: &mut TestAppContext) {
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
    let (focus, offset, preview) = view.read_with(vcx, |v, _| {
        (
            v.model.focus,
            v.body_scroll.handle.offset(),
            v.preview_bounds.get().unwrap(),
        )
    });
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
    let before = order(vcx);
    let at = gpui::point(
        preview.origin.x + preview.size.width / 2.,
        preview.bottom().min(body.bottom()) - gpui::px(20.),
    );
    eprintln!(
        "pointer click probe: focus={focus:?} offset={offset:?} body={body:?} preview={preview:?} at={at:?} order={before:?}"
    );
    assert!(at.y > body.top() && at.y > preview.top());
    vcx.simulate_mouse_down(at, gpui::MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let moved = view.read_with(vcx, |v, _| {
        (
            v.body_scroll.handle.offset(),
            v.preview_bounds.get().unwrap(),
        )
    });
    vcx.simulate_mouse_up(at, gpui::MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let after = order(vcx);
    eprintln!(
        "pointer click probe: after press offset/preview={moved:?}; order {before:?} -> {after:?}"
    );
    assert_eq!(
        before, after,
        "a click without pointer motion reordered the combined images"
    );
}
