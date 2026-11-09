//! Picture recipes and the error refusal are board exceptions to the outer-frame rule; the calm unsupported card keeps its frame.
use super::{mounted, painted, update};
use crate::{
    g2::quick_look_overlay::{Body, QuickLook, Subject},
    presentation::{fixture::Fixture, frame_gate},
};
use flea::ui_logic_preview_core::{kinds::State, rect::Shape};
use gpui::{px, size, AppContext, TestAppContext};

const BARE: &[(&str, &str)] = &[
    ("image", "ql-image"),
    ("raw", "ql-raw"),
    ("symlink", "ql-symlink"),
    ("error", "ql-error"),
];

fn check(cx: &mut TestAppContext, name: &str, body: Body, state: State) {
    let theme = frame_gate::theme();
    cx.update(|cx| cx.set_global(theme));
    let (shell, view, cx) = mounted(cx, 8., |_, _, cx| {
        cx.new(|_| QuickLook::new(Default::default()))
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    view.update(cx, |view, cx| {
        view.open(
            Subject {
                name: "photo.png".into(),
                icon: "image-png".into(),
                mode: 0o100644,
                state,
                shape: Shape {
                    kind: state,
                    known: true,
                    w: 32.,
                    h: 16.,
                    orient: 1,
                },
                body,
            },
            true,
            cx,
        )
    });
    painted(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    for radius in [8., 0., 13.] {
        update(&shell, cx, radius);
        painted(cx);
        let bounds = cx
            .debug_bounds("pv1-card")
            .expect("real picture/refusal card mounted");
        if let Some((_, board)) = BARE.iter().find(|(surface, _)| *surface == name) {
            frame_gate::assert_unframed(name, cx, bounds, board);
        } else {
            frame_gate::assert_frame(name, cx, bounds, radius, theme);
        }
    }
}

#[gpui::test]
fn vf_frames_picture_readers(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let fixture = Fixture::new("vf-frames-pictures");
    let path = fixture.path().join("photo.png");
    image::RgbaImage::from_pixel(32, 16, image::Rgba([32, 64, 96, 255]))
        .save(&path)
        .unwrap();
    for (name, body, state) in [
        (
            "image",
            Body::Image {
                path: path.clone(),
                w: 32.,
                h: 16.,
            },
            State::Image,
        ),
        (
            "raw",
            Body::Raw {
                path: path.clone(),
                w: 32.,
                h: 16.,
                caption: "RAW".into(),
            },
            State::Image,
        ),
        (
            "symlink",
            Body::Link {
                path: path.clone(),
                w: 32.,
                h: 16.,
                target: "photo.png".into(),
            },
            State::Symlink,
        ),
    ] {
        check(cx, name, body, state);
    }
    fixture.cleanup();
}

#[gpui::test]
fn vf_frames_refusal_recipes(cx: &mut TestAppContext) {
    for (name, error, state) in [
        ("unsupported", false, State::Unsupported),
        ("error", true, State::Error),
    ] {
        check(
            cx,
            name,
            Body::CannotShow {
                sentence: "This file could not be read.".into(),
                error,
                controls: vec![],
            },
            state,
        );
    }
}
