//! EY-264 and EY-321: a row drag held over a folder row lights it with its caption, and held on another tab selects that tab, each saying what a release does.
use super::mounted_tests::{centre, frame, open, Root};
use super::pb_devices_2_tests::{tab_point, tabs};
use super::rail_drop_tests::hold;
use crate::{list::bench, state::Shell};
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

// The drag's status line and the folder row the list draws as the drop target, with its caption.
fn aimed(
    cx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
) -> (Option<(usize, String)>, Vec<String>) {
    handle
        .update(cx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            let target = states
                .drop_target
                .zip(states.drop_label.map(|label| label.to_string()));
            let lines = shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.clone())
                    .collect()
            });
            (target, lines)
        })
        .unwrap()
}

fn mounted(cx: &mut TestAppContext, root: &Root) -> WindowHandle<Shell> {
    let (handle, _lines) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| shell.follow_file_drags(window, cx))
        .unwrap();
    frame(cx, handle);
    handle
}

#[test]
fn ey264_a_drag_held_over_a_folder_row_lights_it_and_says_the_move() {
    let root = Root::new("rowaim");
    let mut cx = bench::real_cx();
    let handle = mounted(&mut cx, &root);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, folder, Modifiers::default());
    let (target, lines) = aimed(&mut vcx, handle);
    assert_eq!(target, Some((0, "move here".to_owned())));
    assert!(
        lines.contains(&"Move 1 item to alpha · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
    vcx.simulate_mouse_move(file, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    let (target, lines) = aimed(&mut vcx, handle);
    assert_eq!(target, None, "the carried row is no target");
    // FileDrag.leaveTarget: an own drag off every target keeps its line with no folder name.
    assert_eq!(
        lines,
        vec!["Move 1 item to a folder · ctrl copies, shift moves".to_owned()]
    );
}

#[test]
fn ey321_a_drag_resting_on_another_tab_selects_it_and_says_the_move() {
    let root = Root::new("tabaim");
    let dest = format!("{}/alpha", root.src());
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            tabs(shell, &dest);
            shell.follow_file_drags(window, cx);
        })
        .unwrap();
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let tab = handle
        .update(&mut cx, |shell, _, _| tab_point(shell))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, tab, Modifiers::default());
    let current = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, _, _| shell.strip.tabs[0].current())
            .unwrap()
    };
    assert_eq!(current(&mut vcx), 0, "the switch waits for the rest");
    let (_, lines) = aimed(&mut vcx, handle);
    assert!(
        lines.contains(&format!("Move 1 item to {dest} · ctrl copies, shift moves")),
        "{lines:?}"
    );
    vcx.executor().advance_clock(Duration::from_millis(
        crate::tabs::bar::HOVER_SWITCH_MS + 50,
    ));
    vcx.run_until_parked();
    assert_eq!(current(&mut vcx), 1, "a rest on the tab selects it");
}
