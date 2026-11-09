//! The image reveal clock starts with drawable pixels, including late decodes and refusals.
use super::*;
use crate::g2::quick_look_overlay::{Body, Subject};
use flea::ui_logic_preview_core::{kinds::State, rect::Shape};
use gpui::{RenderImage, TestAppContext};
use std::{sync::Arc, time::Duration};

#[gpui::test]
fn s5_quick_look_picture_fade_waits_for_its_raster(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(false));
    let (look, cx) =
        cx.add_window_view(|_, _| QuickLook::new(Arc::new(crate::test_catalogs::english())));
    look.update(cx, |look, cx| {
        look.open(
            Subject {
                name: "photo.png".into(),
                icon: "image-png".into(),
                mode: 0o100644,
                state: State::Image,
                shape: Shape {
                    kind: State::Image,
                    known: true,
                    w: 32.,
                    h: 24.,
                    orient: 1,
                },
                body: Body::Image {
                    path: "/s5/photo.png".into(),
                    w: 32.,
                    h: 24.,
                },
            },
            true,
            cx,
        );
        // Hold the decode independently of the deterministic animation clock.
        look.raster_in_flight = true;
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(80));
    draw(cx);
    let hidden = look.update(cx, |look, cx| {
        look.motion
            .sample(false, cx.background_executor().now(), false)
    });
    assert_eq!(hidden.opacity, 0., "no fade time is spent without pixels");
    // The real request has already installed its size and path. Land pixels there.
    look.update(cx, |look, cx| {
        let picture = look.svg.as_mut().expect("the decode was requested");
        picture.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(32, 24),
        )])));
        cx.notify();
    });
    draw(cx);
    let first = look.update(cx, |look, cx| {
        look.motion
            .sample(true, cx.background_executor().now(), false)
    });
    assert_eq!(first.opacity, 0.);
    assert_eq!(first.rise, 10., "the whole opening rise remains");
    cx.executor().advance_clock(Duration::from_millis(16));
    draw(cx);
    let next = look.update(cx, |look, cx| {
        look.motion
            .sample(true, cx.background_executor().now(), false)
    });
    assert!(next.opacity > 0. && next.opacity < 0.5, "{next:?}");
    assert!(next.rise > 5. && next.rise < 10., "{next:?}");
    cx.update(|window, _| window.remove_window());
}

fn picture(name: &str) -> Subject {
    Subject {
        name: name.into(),
        icon: "image-png".into(),
        mode: 0o100644,
        state: State::Image,
        shape: Shape {
            kind: State::Image,
            known: true,
            w: 32.,
            h: 24.,
            orient: 1,
        },
        body: Body::Image {
            path: format!("/s5/{name}").into(),
            w: 32.,
            h: 24.,
        },
    }
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn land(look: &gpui::Entity<QuickLook>, cx: &mut gpui::VisualTestContext) -> gpui::ImageId {
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(
        image::RgbaImage::new(32, 24),
    )]));
    let id = image.id;
    look.update(cx, |look, cx| {
        let picture = look.svg.as_mut().expect("the decode was requested");
        picture.image = Some(image);
        if !look.revealed {
            look.reveal(cx);
        }
        cx.notify();
    });
    id
}

/// Every frame from here for `ms` draws the card whole: scrim and card at full opacity, no rise.
fn steady(look: &gpui::Entity<QuickLook>, cx: &mut gpui::VisualTestContext, ms: u64, what: &str) {
    for _ in 0..ms / 16 {
        cx.executor().advance_clock(Duration::from_millis(16));
        draw(cx);
        let frame = look.read_with(cx, |look, _| look.motion.drawn());
        assert_eq!((frame.opacity, frame.rise), (1., 0.), "{what}: {frame:?}");
    }
}

/// A shown picture, its decode then held so a move's new subject has nothing drawable.
fn shown(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<QuickLook>,
    &mut gpui::VisualTestContext,
    gpui::ImageId,
) {
    cx.update(|cx| cx.set_reduce_motion(false));
    let (look, cx) =
        cx.add_window_view(|_, _| QuickLook::new(Arc::new(crate::test_catalogs::english())));
    look.update(cx, |look, cx| {
        look.open(picture("a.png"), false, cx);
        look.raster_in_flight = true;
    });
    draw(cx);
    let first = land(&look, cx);
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(400));
    draw(cx);
    steady(&look, cx, 32, "the first picture is shown");
    (look, cx, first)
}

#[gpui::test]
fn b17_quick_look_move_between_pictures_swaps_without_a_fade(cx: &mut TestAppContext) {
    let (look, cx, first) = shown(cx);
    look.update(cx, |look, cx| look.open(picture("b.png"), false, cx));
    draw(cx);
    steady(
        &look,
        cx,
        240,
        "the old picture holds while the next decodes",
    );
    look.read_with(cx, |look, _| {
        assert!(
            !look.retired_ids.contains(&first),
            "the held picture still draws"
        );
    });
    let next = land(&look, cx);
    draw(cx);
    look.read_with(cx, |look, _| {
        assert!(
            look.retired_ids.contains(&first),
            "the swap lets the held picture go"
        );
        assert!(look.held.is_none());
        assert_eq!(look.svg_raster().map(|image| image.id), Some(next));
    });
    steady(&look, cx, 48, "the new picture is shown");
    cx.update(|window, _| window.remove_window());
}

#[gpui::test]
fn b17_quick_look_move_past_the_cap_shows_the_new_card_never_the_listing(cx: &mut TestAppContext) {
    let (look, cx, first) = shown(cx);
    look.update(cx, |look, cx| {
        look.open(
            Subject {
                name: "notes.txt".into(),
                icon: "text-plain".into(),
                mode: 0o100644,
                state: State::Text,
                shape: Shape::unknown(State::Text),
                body: Body::Text {
                    lines: Vec::new(),
                    count: String::new(),
                },
            },
            false,
            cx,
        )
    });
    draw(cx);
    steady(
        &look,
        cx,
        64,
        "the old picture holds while the text is asked for",
    );
    look.update(cx, |look, cx| look.reveal_cap(cx));
    draw(cx);
    look.read_with(cx, |look, _| {
        assert!(look.retired_ids.contains(&first));
        assert!(look.held.is_none());
    });
    steady(&look, cx, 240, "the text card's loading state is shown");
    look.update(cx, |look, cx| look.close(cx));
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(16));
    draw(cx);
    let closing = look.read_with(cx, |look, _| look.motion.drawn());
    assert!(closing.opacity < 1., "a close still fades: {closing:?}");
    cx.update(|window, _| window.remove_window());
}
