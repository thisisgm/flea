//! Parity outcomes exercised through the mounted Columns window and its normal IPC entry.
use super::host;
use crate::{backend::Backend, columns::geometry::Slot, state::Shell};
use flea::jsondoc::Json;
use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}
fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    OPENED.with(|paths| paths.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

fn edge_at(
    cx: &mut gpui::TestAppContext,
    handle: gpui::WindowHandle<Shell>,
) -> gpui::Point<gpui::Pixels> {
    handle
        .update(cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            point(
                bounds.left()
                    + px(columns.area.layout.edge() - columns.area.layout.active_x() - 1.),
                bounds.top() + px(100.),
            )
        })
        .unwrap()
}

#[test]
fn pb_columns2_edge_hover_and_drag_reach_the_mounted_surface() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, host::settle);
    host::frame(&mut cx, handle);
    let at = edge_at(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.run_until_parked();
    handle
        .read_with(&vcx, |shell, _| {
            let area = &shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area;
            assert!(area.host.edge.hovered, "the painted edge receives hover");
            assert!(area.edge_paint().line.is_some() && area.edge_paint().mark.is_some());
            let (x, _, width, height) = area.edge_paint().line.unwrap();
            let (mark_x, mark_y, mark_width) = area.edge_paint().mark.unwrap();
            assert!(
                (mark_y + mark_width / 2. - height / 2.).abs() <= 0.5,
                "the move mark is centred on the hairline's middle row"
            );
            assert_eq!(
                mark_x + mark_width / 2.,
                x + width / 2.,
                "EY-168 move mark is centred on the hairline"
            );
        })
        .unwrap();
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    let moved = at + point(px(-70.), px(0.));
    vcx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::none());
    vcx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    handle
        .read_with(&vcx, |shell, _| {
            let area = &shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area;
            assert!(area.share > 0., "dragged width is stored on release");
            assert!(!area.host.edge.dragging);
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("preview")
                    .and_then(|v| v.get("width"))
                    .and_then(Json::as_f64),
                Some(area.share)
            );
        })
        .unwrap();
}

#[test]
fn pb_columns2_edge_double_click_restores_the_default_third() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.change_settings(
            flea::jsondoc::parse(r#"{"preview":{"width":40}}"#).unwrap(),
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, handle);
    let at = edge_at(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    handle
        .read_with(&vcx, |shell, _| {
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("preview")
                    .and_then(|v| v.get("width"))
                    .and_then(Json::as_f64),
                Some(0.)
            );
        })
        .unwrap();
}

#[test]
fn pb_columns2_neighbour_double_click_runs_the_file_opener() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.opener[0].launch = record;
        host::peek(
            shell,
            "/srv/a/b",
            &[("neighbour.txt", false)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, handle);
    let at = handle
        .update(&mut cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Parent).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            point(bounds.left() + px(70.), bounds.top() + px(15.))
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    for count in [1, 2] {
        OPENED.with(|paths| paths.borrow_mut().clear());
        vcx.simulate_event(MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            click_count: count,
            ..Default::default()
        });
        vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        vcx.run_until_parked();
        OPENED.with(|paths| {
            let paths = paths.borrow();
            assert_eq!(paths.len(), usize::from(count == 2));
            if count == 2 {
                assert_eq!(&paths[0][1..], ["--open", "/srv/a/b/neighbour.txt"]);
            }
        });
        if count == 1 {
            handle
                .read_with(&vcx, |shell, _| {
                    let host = shell.panes[0].as_ref().unwrap();
                    assert_eq!(
                        host.pane.place(),
                        "/srv/a/b",
                        "the first tap lists the clicked file's folder"
                    );
                    assert_eq!(
                        host.nav.pending_select.as_deref(),
                        Some("/srv/a/b/neighbour.txt")
                    );
                })
                .unwrap();
        }
    }
    handle
        .read_with(&vcx, |shell, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), "/srv/a/b")
        })
        .unwrap();
}

#[test]
fn pb_columns2_changed_folder_order_reasks_drawn_neighbours() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    let (backend, mut lines) = Backend::test_lines();
    host::change(&mut cx, handle, |shell, window, cx| {
        host::settle(shell, window, cx);
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        let host = shell.panes[0].as_ref().unwrap();
        *host.nav.folders.borrow_mut() = crate::navigation::folder_state::Folders::from_settings(
            &shell.launch.settings.data,
            Vec::new(),
        );
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .folders
            .borrow_mut()
            .remember(
                "/srv/a/b",
                crate::navigation::folder_state::Order {
                    key: "size",
                    desc: true,
                },
            );
        cx.notify();
    });
    cx.run_until_parked();
    host::frame(&mut cx, handle);
    let asked: Vec<_> = std::iter::from_fn(&mut lines).collect();
    assert!(
        asked.iter().any(|line| line.contains(r#""c":"peek""#)
            && line.contains(r#""path":"/srv/a/b""#)
            && line.contains(r#""by":"size""#)),
        "no neighbour re-ask under its new order: {asked:?}"
    );
}

#[test]
fn pb_columns2_named_preview_readers_are_mounted_even_without_a_preview() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        for name in ["columnMarkdownText", "columnArchiveRect"] {
            assert!(host::call(shell, name, &[], window, cx)
                .stdout
                .trim()
                .is_empty());
        }
    });
}

#[test]
fn columns_click_parent_file_single_tap_selects_and_double_tap_opens_only_it() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.opener[0].launch = record;
        host::peek(
            shell,
            "/srv/a/b",
            &[("neighbour.txt", false), ("c", true)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, handle);
    let at = handle
        .update(&mut cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Parent).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            point(bounds.left() + px(70.), bounds.top() + px(15.))
        })
        .unwrap();
    OPENED.with(|paths| paths.borrow_mut().clear());
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.place(), "/srv/a/b");
            assert_eq!(
                host.nav.pending_select.as_deref(),
                Some("/srv/a/b/neighbour.txt"),
                "one tap selects the visible file in its own directory"
            );
        })
        .unwrap();
    vcx.run_until_parked();
    assert!(
        OPENED.with(|paths| paths.borrow().is_empty()),
        "one tap opens nothing"
    );
    vcx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    OPENED.with(|paths| {
        let paths = paths.borrow();
        assert_eq!(paths.len(), 1, "the second tap opens one file: {paths:?}");
        assert_eq!(&paths[0][1..], ["--open", "/srv/a/b/neighbour.txt"]);
    });
}
