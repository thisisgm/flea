//! EY-266: the open card answers permissionsState, so a sweep reads where Tab lands; Apply keeps the cursor's row.
use super::fixture::{choose, edit, inspect, next, receive, setup};
use crate::window::root::Root;
use flea::jsondoc::{parse, Json};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

fn state(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Json {
    let reply = edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        let request = flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: "permissionsState".into(),
            arguments: vec![],
        };
        shell.ipc(
            crate::ipc::server::Event::Call { request, answer },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    });
    assert_eq!(reply.code, 0, "{}", reply.stderr);
    parse(reply.stdout.trim()).unwrap()
}

fn control<'a>(state: &'a Json, name: &str) -> &'a Json {
    state
        .get("controls")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .find(|control| control.get("name").and_then(Json::as_str) == Some(name))
        .unwrap()
}

fn focused(state: &Json) -> Vec<&str> {
    state
        .get("controls")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter(|control| control.get("focused").and_then(Json::as_bool) == Some(true))
        .map(|control| control.get("name").and_then(Json::as_str).unwrap())
        .collect()
}

#[gpui::test]
fn m9_permissions_state_reads_the_open_card_and_where_tab_lands(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    let open = state(cx, handle);
    assert_eq!(open.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(open.get("inputReady").and_then(Json::as_bool), Some(false));
    assert_eq!(
        open.get("path").and_then(Json::as_str),
        Some("/files/notes.txt")
    );
    assert_eq!(open.get("mode").and_then(Json::as_str), Some("0644"));
    assert_eq!(open.get("busy").and_then(Json::as_bool), Some(false));
    assert_eq!(focused(&open), ["Apply"], "the board opens on Apply");
    // v0.3.12's Tab ; Return started from Cancel; from the board's Apply, Tab wraps to the first box.
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("tab enter");
    let moved = state(cx, handle);
    assert_eq!(focused(&moved), ["Owner read"]);
    let owner_read = control(&moved, "Owner read");
    assert_eq!(
        owner_read.get("checked").and_then(Json::as_bool),
        Some(true)
    );
    while let Some(line) = requests() {
        assert!(
            !line.contains(r#""op":"apply""#),
            "Return on a box applies nothing: {line}"
        );
    }
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("shift-tab enter");
    assert!(next(&mut requests, "permissions", "apply").contains(r#""mode":"0644""#));
    assert_eq!(
        state(cx, handle).get("busy").and_then(Json::as_bool),
        Some(true)
    );
}

#[gpui::test]
fn m9_permissions_apply_rereads_keeping_the_cursor_row_and_marks(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0];
    });
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("enter");
    next(&mut requests, "permissions", "apply");
    receive(
        cx,
        handle,
        r#"{"t":"permissions","id":1,"op":"apply","ok":true,"mode":"0644"}"#,
    );
    cx.run_until_parked();
    // v0.3.12 re-read with keepSelection; a plain refresh lands on row 0 with nothing marked.
    let kept = edit(cx, handle, |shell, _, _| {
        let anchor = shell.panes[0].as_ref().unwrap().nav.anchor.clone();
        anchor.map(|anchor| (anchor.name, anchor.marks.len()))
    });
    assert_eq!(kept, Some(("notes.txt".to_owned(), 1)));
}

#[gpui::test]
fn uish_misc_permissions_reader_observes_painted_controls(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    let mut previous = None;
    for (width, height) in [(1100., 700.), (1400., 900.)] {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
        visual.run_until_parked();
        let frame = visual.debug_bounds("op3-card").unwrap();
        let painted: Vec<_> = [
            "op3-check-0",
            "op3-check-1",
            "op3-check-2",
            "op3-check-3",
            "op3-check-4",
            "op3-check-5",
            "op3-check-6",
            "op3-check-7",
            "op3-check-8",
        ]
        .into_iter()
        .map(|selector| visual.debug_bounds(selector).unwrap())
        .collect();
        let open = state(cx, handle);
        let rect = crate::ipc::rows::rect_of(
            f32::from(frame.origin.x),
            f32::from(frame.origin.y),
            f32::from(frame.size.width),
            f32::from(frame.size.height),
        );
        assert_eq!(open.get("rect").and_then(Json::as_str), Some(rect.as_str()));
        if let Some(previous) = previous.replace(rect.clone()) {
            assert_ne!(previous, rect, "a resize must replace the painted bounds");
        }
        let controls = open.get("controls").and_then(Json::as_array).unwrap();
        for (at, bounds) in painted.iter().enumerate() {
            let bit = (1u32 << (8 - at)).to_string();
            let check = controls
                .iter()
                .find(|control| control.get("bit") == Some(&Json::Num(bit.clone())))
                .unwrap();
            let expected = crate::ipc::rows::centre_of_bounds(*bounds);
            assert_eq!(
                check.get("centre").and_then(Json::as_str),
                Some(expected.as_str())
            );
            // Sample centre: "128 256".
            let point: Vec<f32> = expected
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let before = state(cx, handle).get("mode").cloned();
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let position = gpui::point(gpui::px(point[0]), gpui::px(point[1]));
            visual.simulate_mouse_move(position, None, gpui::Modifiers::none());
            visual.simulate_mouse_down(position, gpui::MouseButton::Left, gpui::Modifiers::none());
            visual.simulate_mouse_up(position, gpui::MouseButton::Left, gpui::Modifiers::none());
            visual.run_until_parked();
            assert_ne!(state(cx, handle).get("mode").cloned(), before);
            assert_eq!(
                edit(cx, handle, |shell, _, _| shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .pane
                    .cursor),
                0
            );
        }
    }
    let open = state(cx, handle);
    assert!(control(&open, "Cancel")
        .get("centre")
        .and_then(Json::as_str)
        .is_some());
}
