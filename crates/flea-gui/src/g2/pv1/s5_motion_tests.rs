//! The image reveal clock starts with drawable pixels, including late decodes and refusals.
use super::*;
use crate::g2::pv1::{Body, Subject};
use flea::ui_logic_pv1::{kinds::State, rect::Shape};
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
