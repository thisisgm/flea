//! U-DRAG-DROP ready rows: slop history, held list hover and rename refusal.
use super::mounted_tests::{centre, drain, frame, open, Root};
use super::pb_devices_2_tests::{tab_point, tabs};
use crate::{list::bench, state::Shell};
use gpui::{
    point, px, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};

pub(super) fn mounted(cx: &mut TestAppContext, root: &Root) -> WindowHandle<Shell> {
    let (handle, _lines) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| shell.follow_file_drags(window, cx))
        .unwrap();
    frame(cx, handle);
    handle
}

// Press on `from`, travel past the slop to `away`, then rest on `back`, still held.
pub(super) fn out_and_back(
    vcx: &mut VisualTestContext,
    from: Point<Pixels>,
    away: Point<Pixels>,
    back: Point<Pixels>,
) {
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(3.), from.y + px(3.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(away, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    vcx.simulate_mouse_move(
        point(back.x + px(1.), back.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(back, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
}

pub(super) fn status(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(vcx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().status.to_string()
        })
        .unwrap()
}

#[test]
fn ey395_a_folder_dragged_away_and_back_onto_its_own_row_is_refused() {
    let root = Root::new("ey395");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let away = point(folder.x + px(90.), folder.y + px(160.));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    out_and_back(&mut vcx, folder, away, folder);
    drain(&mut lines);
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
    assert_eq!(status(&mut vcx, handle), "Already in this folder.");
}

#[test]
fn ey356_a_held_drag_back_on_its_own_row_lights_no_hover_plate() {
    let root = Root::new("ey356");
    let mut cx = bench::real_cx();
    let handle = mounted(&mut cx, &root);
    let folder = centre(&mut cx, handle, 0);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    // A hover at rest still lights the row.
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_move(folder, None, Modifiers::default());
    vcx.run_until_parked();
    let hovered = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, window, cx| {
                shell.list_states(0, window, cx).hovered
            })
            .unwrap()
    };
    assert_eq!(hovered(&mut vcx), Some(0), "a resting pointer hovers");
    let away = point(folder.x + px(90.), folder.y + px(160.));
    out_and_back(&mut vcx, folder, away, folder);
    assert!(vcx.update(|_, cx| cx.has_active_drag()), "the drag is held");
    assert_eq!(hovered(&mut vcx), None, "the refused own row stays dark");
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
}

#[test]
fn ey393_a_tab_dragged_out_while_renaming_is_refused_and_the_editor_stays() {
    let root = Root::new("ey393");
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
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 1;
            shell.dispatch(
                crate::window::dispatch::Order::Action(crate::window::input::ActionEvent::bare(
                    "rename",
                    flea::keys::Context::Listing,
                )),
                window,
                cx,
            );
        })
        .unwrap();
    frame(&mut cx, handle);
    let tab = handle
        .update(&mut cx, |shell, _, _| tab_point(shell))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(tab, None, Modifiers::default());
    vcx.simulate_mouse_down(tab, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let editing = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, _, _| shell.rename.edit.is_some())
            .unwrap()
    };
    assert!(
        editing(&mut vcx),
        "a press on the tab strip keeps the editor open"
    );
    vcx.simulate_mouse_move(
        point(tab.x, tab.y + px(30.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    // The pointer leaves the window: the platform asks for the tab's payload, which a rename refuses.
    let torn = handle
        .update(&mut vcx, |shell, _, cx| {
            shell.tab_native_payload(cx).is_some()
        })
        .unwrap();
    assert!(!torn, "no window tears off");
    assert_eq!(
        status(&mut vcx, handle),
        "Finish the rename before dragging a tab out."
    );
    assert!(editing(&mut vcx), "the refusal keeps the editor open");
}
