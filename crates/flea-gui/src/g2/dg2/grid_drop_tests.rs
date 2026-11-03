//! EY-445 griddrop: a tile dragged in Grid says its line while held and drops on a folder tile, as a List row does.
use super::mounted_tests::{drain, frame, open_with, Root};
use super::rail_drop_tests::hold;
use crate::{list::bench, state::Shell, window::startup::View};
use gpui::{
    point, px, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};

fn grid(
    cx: &mut TestAppContext,
    root: &Root,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (handle, lines) = open_with(cx, root.src(), 1);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    (handle, lines)
}

// The centre of a drawn tile's slot, the part a press and a drop both hit.
fn tile(cx: &mut TestAppContext, handle: WindowHandle<Shell>, index: usize) -> Point<Pixels> {
    handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let tile = state.drawn_row(index).expect("the grid drew the tile");
            state.bounds.origin
                + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                )
        })
        .unwrap()
}

fn lines_shown(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> Vec<String> {
    handle
        .update(vcx, |shell, _, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.clone())
                    .collect()
            })
        })
        .unwrap()
}

#[test]
fn ey445_griddrop_a_held_tile_names_the_folder_tile_and_says_a_folder_elsewhere() {
    let root = Root::new("gridheld");
    let mut cx = bench::real_cx();
    let (handle, _lines) = grid(&mut cx, &root);
    let folder = tile(&mut cx, handle, 0);
    let file = tile(&mut cx, handle, 1);
    let other = tile(&mut cx, handle, 2);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, folder, Modifiers::default());
    assert_eq!(
        lines_shown(&mut vcx, handle),
        vec!["Move 1 item to alpha · ctrl copies, shift moves".to_owned()]
    );
    vcx.simulate_mouse_move(other, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(
        lines_shown(&mut vcx, handle),
        vec!["Move 1 item to a folder · ctrl copies, shift moves".to_owned()]
    );
    vcx.simulate_mouse_up(other, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(
        !lines_shown(&mut vcx, handle)
            .iter()
            .any(|line| line.contains("to a folder")),
        "the drag's line leaves with the release"
    );
}

#[test]
fn ey445_griddrop_a_file_tile_dropped_on_a_folder_tile_moves_into_it() {
    let root = Root::new("gridmove");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = grid(&mut cx, &root);
    let folder = tile(&mut cx, handle, 0);
    let file = tile(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, folder, Modifiers::default());
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let sent = drain(&mut lines);
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"collisions""#)),
        "no collisions probe among {sent:?}"
    );
    assert!(
        sent.iter().all(|line| !line.contains(r#""c":"transfer""#)),
        "a transfer must wait for collision resolution: {sent:?}"
    );
    handle
        .update(&mut vcx, |shell, window, cx| {
            let line = r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#;
            shell.receive(
                0,
                crate::backend::Event::Operations(line.into()),
                window,
                cx,
            );
        })
        .unwrap();
    vcx.run_until_parked();
    let transfer = drain(&mut lines)
        .into_iter()
        .find(|line| line.contains(r#""c":"transfer""#))
        .expect("a transfer after the empty collisions answer");
    assert!(transfer.contains(r#""op":"move""#), "{transfer}");
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "{transfer}"
    );
    assert!(
        transfer.contains(&format!(r#""dest":"{}/alpha""#, root.src())),
        "{transfer}"
    );
}

#[path = "grid_press_tests.rs"]
mod press;
