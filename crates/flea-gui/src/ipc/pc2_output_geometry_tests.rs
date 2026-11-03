//! EY-598: the shared op4 receipt names the outer frame that the production Root paints.
use super::readers3_tests::{open, read, update};
use crate::g2::op4::model::Action;
use flea::ui_logic_op4::{output::Kind, selection::Source};
use gpui::{px, size, AppContext, TestAppContext, VisualTestContext};

fn card(cx: &mut TestAppContext, action: Action) {
    for theme in [
        crate::palette::Theme::default(),
        crate::presentation::frame_gate::theme(),
    ] {
        let handle = open(cx);
        update(cx, handle, |shell, window, cx| {
            cx.set_global(theme);
            let sources = if action == Action::Convert {
                vec![Source {
                    path: "/files/photo.png".into(),
                    kind: Kind::Image,
                }]
            } else {
                vec![
                    Source {
                        path: "/files/a.png".into(),
                        kind: Kind::Image,
                    },
                    Source {
                        path: "/files/b.jpg".into(),
                        kind: Kind::Image,
                    },
                ]
            };
            assert!(shell.open_output(0, action, 0, sources, window, cx));
        });
        for width in [1100., 360.] {
            VisualTestContext::from_window(handle.into(), cx)
                .simulate_resize(size(px(width), px(700.)));
            cx.update_window(handle.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let frame = visual
                .debug_bounds("op4-card")
                .expect("the production card paints");
            let title = visual
                .debug_bounds("op4-title")
                .expect("the title paints independently");
            let (scale, quads) = handle
                .update(cx, |_, window, _| {
                    (window.scale_factor(), window.painted_quads())
                })
                .unwrap();
            let painted = quads
                .iter()
                .find(|q| q.bounds == frame.scale(scale) && q.border_widths.top.0 > 0.)
                .expect("the actual border quad");
            let (observed, observed_title) = update(cx, handle, |shell, _, cx| {
                let view = shell.output.view.as_ref().unwrap().read(cx);
                (
                    view.observed.card.get().unwrap(),
                    view.observed.title.get().unwrap(),
                )
            });
            assert_eq!(
                observed.scale(scale),
                painted.bounds,
                "{action:?}, window width {width}: receipt must include every painted border edge"
            );
            assert_eq!(
                observed_title, title,
                "title observation must retain its own box"
            );
            if action == Action::Convert {
                let expected = crate::ipc::rows::rect_of(
                    frame.origin.x.into(),
                    frame.origin.y.into(),
                    frame.size.width.into(),
                    frame.size.height.into(),
                );
                assert_eq!(
                    read(cx, handle, "convertCardRect", &[]),
                    flea::ipc::wire::Reply::value(expected)
                );
                let expected = crate::ipc::rows::centre_of(
                    title.origin.x.into(),
                    title.origin.y.into(),
                    title.size.width.into(),
                    title.size.height.into(),
                );
                assert_eq!(
                    read(cx, handle, "convertTitleCentre", &[]),
                    flea::ipc::wire::Reply::value(expected)
                );
            }
        }
    }
}

#[gpui::test]
fn pc2_ey598_convert_reader_uses_outer_painted_frame(cx: &mut TestAppContext) {
    card(cx, Action::Convert);
}

#[gpui::test]
fn pc2_ey598_print_observation_uses_outer_painted_frame(cx: &mut TestAppContext) {
    card(cx, Action::Print);
}

#[gpui::test]
fn pc2_ey598_combine_observation_uses_outer_painted_frame(cx: &mut TestAppContext) {
    card(cx, Action::Combine);
}

#[gpui::test]
fn pc2_contract_convert_geometry_retained_after_close_and_other_cards(cx: &mut TestAppContext) {
    let handle = open(cx);
    let empty = flea::ipc::wire::Reply::value("");
    assert_eq!(read(cx, handle, "convertCardRect", &[]), empty);
    assert_eq!(read(cx, handle, "convertTitleCentre", &[]), empty);
    let draw = |cx: &mut TestAppContext| {
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    let show = |cx: &mut TestAppContext, action| {
        update(cx, handle, |shell, window, cx| {
            assert!(shell.open_output(
                0,
                action,
                0,
                vec![Source {
                    path: "/files/photo.png".into(),
                    kind: Kind::Image,
                }],
                window,
                cx
            ));
        });
    };
    // Building another output card does not build the lazily retained Convert observer.
    show(cx, Action::Print);
    draw(cx);
    assert_eq!(read(cx, handle, "convertCardRect", &[]), empty);
    assert_eq!(read(cx, handle, "convertTitleCentre", &[]), empty);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    draw(cx);
    show(cx, Action::Convert);
    draw(cx);
    let rect = read(cx, handle, "convertCardRect", &[]);
    let title = read(cx, handle, "convertTitleCentre", &[]);
    assert_ne!(rect, empty);
    assert_ne!(title, empty);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(cx, handle, "convertOpen", &[]),
        flea::ipc::wire::Reply::value("false")
    );
    assert_eq!(read(cx, handle, "convertCardRect", &[]), rect);
    assert_eq!(read(cx, handle, "convertTitleCentre", &[]), title);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert!(
        visual.debug_bounds("op4-card").is_none(),
        "retention does not reopen the card"
    );
    show(cx, Action::Print);
    draw(cx);
    assert_eq!(read(cx, handle, "convertCardRect", &[]), rect);
    assert_eq!(read(cx, handle, "convertTitleCentre", &[]), title);
}

#[gpui::test]
fn pc2_contract_convert_retention_records_this_single_paint(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        assert!(shell.open_output(
            0,
            Action::Convert,
            0,
            vec![Source {
                path: "/files/photo.png".into(),
                kind: Kind::Image
            }],
            window,
            cx,
        ));
    });
    let output = update(cx, handle, |shell, _, _| shell.output.view.clone().unwrap());
    for width in [1100., 820., 360.] {
        VisualTestContext::from_window(handle.into(), cx)
            .simulate_resize(size(px(width), px(700.)));
        cx.update_window(handle.into(), |_, window, cx| {
            let observed = &output.read(cx).observed;
            // Empty all samples so a callback using an earlier paint cannot satisfy this check.
            for sample in [
                &observed.card,
                &observed.title,
                &observed.convert_card,
                &observed.convert_title,
            ] {
                sample.set(None);
            }
            window.refresh();
            window.draw(cx).clear(cx);
            let observed = &output.read(cx).observed;
            assert!(observed.card.get().is_some());
            assert!(observed.title.get().is_some());
            assert_eq!(
                observed.convert_card.get(),
                observed.card.get(),
                "card, width {width}"
            );
            assert_eq!(
                observed.convert_title.get(),
                observed.title.get(),
                "title, width {width}"
            );
        })
        .unwrap();
    }
}
