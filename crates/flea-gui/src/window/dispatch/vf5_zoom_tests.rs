use super::{preview_tests::*, test_shell::land, vf5_tests::opened};
use crate::window::startup::View;
use flea::jsondoc::Json;
use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext};

#[gpui::test]
fn vf5_grid_zoom_steps_bounds_pixels_and_disabled_setting(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Grid, window, cx)));
    draw(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let bounds = shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .bounds;
            let mut wheel = |shell: &mut crate::state::Shell, delta, expected| {
                shell.wheel(
                    &ScrollWheelEvent {
                        position: bounds.center(),
                        modifiers: Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        delta,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .grid
                        .as_ref()
                        .unwrap()
                        .view
                        .thumb
                        .name(),
                    expected
                );
            };
            for expected in ["large", "xlarge", "huge", "largest", "largest"] {
                wheel(shell, ScrollDelta::Lines(point(0., 3.)), expected);
            }
            wheel(shell, ScrollDelta::Lines(point(0., -3000.)), "small");
            wheel(shell, ScrollDelta::Pixels(point(px(0.), px(24.))), "small");
            wheel(shell, ScrollDelta::Pixels(point(px(0.), px(24.))), "medium");
            shell.change_settings(
                Json::Obj(vec![(
                    "preview".into(),
                    Json::Obj(vec![("ctrlZoom".into(), Json::Bool(false))]),
                )]),
                window,
                cx,
            );
            shell.wheel(
                &ScrollWheelEvent {
                    position: bounds.center(),
                    modifiers: Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    delta: ScrollDelta::Lines(point(0., 3.)),
                    ..Default::default()
                },
                window,
                cx,
            );
            assert_eq!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .grid
                    .as_ref()
                    .unwrap()
                    .view
                    .thumb
                    .name(),
                "medium"
            );
        })
    });
    close(&shell, cx);
}
