//! The joined menu consumes actual painted view geometry and the complete backend wire.
use super::*;
use crate::g2::mn1::join::ProducerReply;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};
use std::{cell::RefCell, rc::Rc};
#[path = "gcomp_menu_u13_fixture.rs"]
mod u13;

fn receive(cx: &mut TestAppContext, handle: Handle, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx);
    });
}

fn menu_request(requests: &mut impl FnMut() -> Option<String>, operation: &str) -> u64 {
    while let Some(line) = requests() {
        let document = flea::jsondoc::parse(&line).unwrap();
        if document.get("c").and_then(Json::as_str) == Some("menuaction")
            && document.get("op").and_then(Json::as_str) == Some(operation)
        {
            return flea::json::field_usize(&line, "id").unwrap() as u64;
        }
    }
    panic!("the mounted menu did not request {operation}");
}

fn ready(cx: &mut TestAppContext, handle: Handle, requests: &mut impl FnMut() -> Option<String>) {
    let id = menu_request(requests, "snapshot");
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    assert_eq!(menu_request(requests, "applications"), id);
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
        ),
    );
}

fn right_press(cx: &mut TestAppContext, handle: Handle, at: gpui::Point<gpui::Pixels>) {
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        at,
        MouseButton::Right,
        Modifiers::default(),
    );
}

#[test]
fn gcomp_batch7_menu_uses_live_font_drawn_list_geometry_and_actual_root_paint() {
    let mut cx = crate::list::bench::real_cx();
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(&mut cx);
    let (backend, mut requests) = Backend::test_lines();
    edit(&mut cx, handle, |shell, window, cx| {
        // Real startup settings are canonical; patching partial {} also changes listing flags.
        shell.launch.settings.data = flea::uistate::from_file("{}");
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        shell.set_view(View::List, window, cx)
    });
    for base in [12, 14, 16] {
        edit(&mut cx, handle, |shell, window, cx| {
            let patch = flea::jsondoc::parse(&format!(
                r#"{{"display":{{"textSize":{{"mode":{base}}}}}}}"#
            ))
            .unwrap();
            let document = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
            shell.apply_settings(document, window, cx);
        });
        frame(&mut cx, handle);
        let at = edit(&mut cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                !host.pane.busy(),
                "font-only patch must not start an unanswered listing"
            );
            let frame = host.input.drawn.expect("real U03 input painted").frame;
            point(
                frame.bounds.left() + px(40.),
                frame.bounds.top() + px(frame.row * 3.5),
            )
        });
        right_press(&mut cx, handle, at);
        edit(&mut cx, handle, |shell, _, _| {
            let capture = shell
                .menu
                .captured
                .as_ref()
                .expect("real right press reached U06");
            assert_eq!(capture.rows, [3]);
            assert_eq!(capture.cursor, 3);
        });
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("down");
        assert_eq!(call(&mut cx, handle, "cursor"), Reply::value("3"));
        ready(&mut cx, handle, &mut requests);
        frame(&mut cx, handle);
        assert_eq!(
            call(&mut cx, handle, "contextMenuVisible"),
            Reply::value("true")
        );
        let bounds = edit(&mut cx, handle, |shell, _, _| {
            let menu = shell.menu.menu.borrow();
            let card = menu.card.as_ref().unwrap();
            let label = card
                .built
                .lines
                .iter()
                .find_map(|line| line.label.as_ref())
                .unwrap();
            assert_eq!(
                label.run.line().font_size,
                px(base as f32),
                "mounted menu shaped the current U11/U01 snapshot"
            );
            card.frame
        });
        let quads = cx
            .update_window(handle.into(), |_, window, _| {
                let scale = window.scale_factor();
                (window.painted_quads(), scale)
            })
            .unwrap();
        assert!(
            quads.0.iter().any(|quad| {
                let actual = quad.bounds;
                (actual.origin.x.0 / quads.1 - bounds.x).abs() < 1.
                    && (actual.origin.y.0 / quads.1 - bounds.y).abs() < 1.
                    && (actual.size.width.0 / quads.1 - bounds.w).abs() < 1.
                    && (actual.size.height.0 / quads.1 - bounds.h).abs() < 1.
            }),
            "menuState must describe a card actually painted by Root"
        );
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("escape");
        assert_eq!(
            call(&mut cx, handle, "contextMenuVisible"),
            Reply::value("false")
        );
    }
}

#[gpui::test]
fn gcomp_batch7_unclaimed_producer_wire_is_forwarded_once_without_a_synthetic_consumer(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let sink = observed.clone();
    let subscription = edit(cx, handle, |_, _, cx| {
        cx.subscribe(&cx.entity(), move |_, _, event: &ProducerReply, _| {
            sink.borrow_mut().push((event.side, event.line.clone()));
        })
    });
    let line = r##"{ "t":"shebang", "id":987, "path":"/files/script", "answer":"#!/bin/sh", "foreign":[1,2] }"##;
    receive(cx, handle, line);
    cx.run_until_parked();
    assert_eq!(&*observed.borrow(), &[(0, line.to_string())]);
    drop(subscription);
}

#[test]
fn gcomp_batch7_grid_menu_captures_the_actual_drawn_tile() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    frame(&mut cx, handle);
    let (backend, mut requests) = Backend::test_lines();
    let (at, row) = edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        let state = host.grid.as_ref().unwrap().state.borrow();
        let tile = &state.drawn()[1];
        (
            state.bounds.origin
                + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                ),
            tile.listing,
        )
    });
    right_press(&mut cx, handle, at);
    edit(&mut cx, handle, |shell, _, _| {
        assert_eq!(shell.menu.captured.as_ref().unwrap().rows, [row]);
    });
    ready(&mut cx, handle, &mut requests);
    frame(&mut cx, handle);
    assert_eq!(
        call(&mut cx, handle, "contextMenuVisible"),
        Reply::value("true")
    );
    assert_eq!(call(&mut cx, handle, "viewMode"), Reply::value("grid"));
}

#[test]
fn gcomp_batch7_columns_background_mounts_the_real_u06_menu() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests) = super::batch::columns(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        crate::window::columns_tests::host::settle(shell, window, cx)
    });
    frame(&mut cx, handle);
    let at = edit(&mut cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let painter = columns
            .painters
            .get(crate::columns::geometry::Slot::Active)
            .unwrap()
            .borrow();
        let bounds = painter
            .drawn
            .as_ref()
            .expect("actual U09 active track")
            .bounds;
        point(bounds.center().x, bounds.bottom() - px(5.))
    });
    right_press(&mut cx, handle, at);
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        let capture = shell
            .menu
            .captured
            .as_ref()
            .expect("U09 background reached actual U06");
        assert!(capture.background);
        assert_eq!(capture.path, shell.panes[0].as_ref().unwrap().path);
    });
    assert_eq!(
        call(&mut cx, handle, "contextMenuVisible"),
        Reply::value("true")
    );
    assert_eq!(call(&mut cx, handle, "viewMode"), Reply::value("columns"));
}

#[test]
fn gcomp_batch7_loading_menu_prepares_omi_after_its_real_slot_is_placed() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    frame(&mut cx, handle);
    let (backend, _requests) = Backend::test_lines();
    let at = edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        let frame = host.input.drawn.unwrap().frame;
        point(
            frame.bounds.left() + px(50.),
            frame.bounds.top() + px(frame.row / 2.),
        )
    });
    right_press(&mut cx, handle, at);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        assert!(shell.menu.presentation.loading);
        let mark = &shell.menu.presentation.mark;
        assert!(mark.slot().size.width > px(0.), "actual menu Slot placed");
        assert!(
            mark.counters.writes.get() > 0,
            "prepared outside the immutable Shell borrow"
        );
        assert!(mark.counters.frames.get() > 0, "actual Root painted Omi");
        assert!(
            shell.panes[0].as_ref().unwrap().drawn_list,
            "held List stays mounted during menu discovery"
        );
    });
    assert_eq!(
        call(&mut cx, handle, "contextMenuVisible"),
        Reply::value("false")
    );
}

#[test]
fn gcomp_batch7_received_menu_refusal_reaches_the_actual_painted_status_producer() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    frame(&mut cx, handle);
    let (backend, mut requests) = Backend::test_lines();
    let expected = edit(&mut cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        shell.locale.catalog.tr("menu.refusal.moved")
    });
    VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("m");
    let id = menu_request(&mut requests, "snapshot");
    receive(
        &mut cx,
        handle,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":false}}"#),
    );
    frame(&mut cx, handle);
    assert_eq!(call(&mut cx, handle, "statusError"), Reply::value("true"));
    edit(&mut cx, handle, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert!(shell.slots.status.is_some());
        let line = footer.line.read(cx);
        let painted = line
            .painted
            .get("primary")
            .expect("actual mounted status painted refusal");
        assert_eq!(painted.text.as_ref(), expected);
        assert!(painted.bounds.size.width > px(0.));
        assert!(painted.bounds.size.height > px(0.));
    });
}

#[path = "gcomp_menu_stale_tests.rs"]
mod stale;
