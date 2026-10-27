use super::super::columns_tests::host;
use super::*;
use crate::columns::geometry::Slot;

#[test]
fn native31_registered_column_active_rect_is_actual_mounted_track() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, handle);
    for width in [1400., 1100.] {
        cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(width), gpui::px(700.)));
        host::frame(&mut cx, handle);
        let mut click_at = None;
        host::change(&mut cx, handle, |shell, window, cx| {
            let reply = call(shell, "columnActiveRect", window, cx);
            assert_eq!(
                reply.code, 0,
                "mounted columnActiveRect fell through: {}",
                reply.stderr
            );
            let rect: Vec<f32> = reply
                .stdout
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            let bounds = {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                paint
                    .drawn
                    .as_ref()
                    .expect("the active track was actually prepainted")
                    .bounds
            };
            let left = f32::from(bounds.origin.x).round();
            let top = f32::from(bounds.origin.y).round();
            let right = f32::from(bounds.origin.x + bounds.size.width).round();
            let bottom = f32::from(bounds.origin.y + bounds.size.height).round();
            assert_eq!(rect, [left, top, right - left, bottom - top]);
            assert!(left > 0. && top > 0. && right > left && bottom > top);
            shell.panes[0].as_mut().unwrap().pane.cursor = 1;
            click_at = Some(gpui::point(
                bounds.left() + gpui::px(10.),
                bounds.top() + gpui::px(10.),
            ));
            assert_eq!(call(shell, "columnPdfPages", window, cx), Reply::value("0"));
        });
        let mut visual = gpui::VisualTestContext::from_window(handle.into(), &cx);
        let point = click_at.unwrap();
        let modifiers = gpui::Modifiers {
            control: true,
            ..Default::default()
        };
        visual.simulate_mouse_move(point, None, modifiers);
        visual.simulate_mouse_down(point, gpui::MouseButton::Left, modifiers);
        visual.simulate_mouse_up(point, gpui::MouseButton::Left, modifiers);
        host::change(&mut visual, handle, |shell, _, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.cursor,
                0,
                "IPC geometry disagreed with actual pointer ownership"
            );
        });
    }
    host::change(&mut cx, handle, |shell, _, cx| {
        shell.slots.covers[0] = Some(cx.new(|_| Cover).into());
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        assert_eq!(
            call(shell, "columnActiveRect", window, cx),
            Reply::value(""),
            "covered Columns leaked its retained rectangle"
        );
        shell.slots.covers[0] = None;
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        assert_eq!(
            call(shell, "columnActiveRect", window, cx),
            Reply::value(""),
            "hidden Columns leaked its old rectangle"
        );
    });
}

#[test]
fn native31_file_without_preview_paints_its_mounted_right_hairline() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"preview":{"column":false}}"#).unwrap();
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, handle);
    for width in [1100., 1400.] {
        cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(width), gpui::px(700.)));
        for populated in [true, false] {
            host::change(&mut cx, handle, |shell, window, cx| {
                let host = shell.panes[0].as_mut().unwrap();
                let rows = if populated {
                    vec![host::row("source.txt", false)]
                } else {
                    Vec::new()
                };
                host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
                    listing: host.pane.held.listing + 1,
                    total: rows.len(),
                    start: 0,
                    rows,
                });
                host.pane.cursor = 0;
                host.changed(&shell.locale);
                shell.sync_columns(window, cx);
            });
            host::frame(&mut cx, handle);
            host::change(&mut cx, handle, |shell, window, cx| {
                let reply = call(shell, "columnActiveRect", window, cx);
                assert_eq!(reply.code, 0, "{}", reply.stderr);
                // columnActiveRect stdout: "494 27 302 646".
                let rect: Vec<f32> = reply
                    .stdout
                    .split_whitespace()
                    .map(|value| value.parse().unwrap())
                    .collect();
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                assert!(!columns.area.settings.column);
                assert!(!columns.area.host.third_shown(columns.area.settings));
                assert!(matches!(
                    columns.area.tracks().last().unwrap().rows,
                    crate::columns::TrackRows::Blank
                ));
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                let bounds = paint.drawn.as_ref().unwrap().bounds;
                assert_eq!(
                    rect,
                    [
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    ],
                    "the edge must use the actual mounted rectangle"
                );
                let edge = rect[0] + rect[2] - 1.;
                let bottom = rect[1] + rect[3];
                let ink = paint.painted.unwrap().foreground.opacity(0.12);
                let scale = window.scale_factor();
                let commands = window.painted_quads();
                let hairlines = commands
                    .iter()
                    .filter(|quad| {
                        let bounds = quad.bounds;
                        (bounds.origin.x.0 / scale - edge).abs() < 0.01
                            && (bounds.origin.y.0 / scale - rect[1]).abs() < 0.01
                            && (bounds.size.width.0 / scale - 1.).abs() < 0.01
                            && (bounds.size.height.0 / scale - rect[3]).abs() < 0.01
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    hairlines.len(),
                    usize::from(populated),
                    "populated={populated}: expected one full-height 12% foreground paint command, none for no row"
                );
                if let Some(quad) = hairlines.first() {
                    assert_eq!(quad.background.as_solid(), Some(ink));
                    let mask = quad.content_mask.bounds;
                    assert!(mask.origin.x.0 / scale <= edge);
                    assert!(mask.origin.y.0 / scale <= rect[1]);
                    assert!((mask.origin.x.0 + mask.size.width.0) / scale >= edge + 1.);
                    assert!((mask.origin.y.0 + mask.size.height.0) / scale >= bottom);
                }
            });
        }
    }
}

struct Cover;
impl gpui::Render for Cover {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        gpui::div()
    }
}
