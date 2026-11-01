//! EY-499: a drag pressed on a parent column's row lifts that row and meets the same drop gates as the active track.
use super::{at, host, open, Root};
use crate::{columns::geometry::Slot, state::Shell};
use gpui::{point, px, Modifiers, MouseButton, Pixels, Point, VisualTestContext, WindowHandle};

fn lifted(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> Option<String> {
    handle
        .update(vcx, |shell, window, cx| {
            match shell.file_drag_payload(window, cx) {
                Some(gpui::ExternalDragPayload::Mime(payload)) => payload
                    .data
                    .get("text/uri-list")
                    .map(|uris| String::from_utf8_lossy(&uris[..]).into_owned()),
                _ => None,
            }
        })
        .unwrap()
}

fn press_and_move(vcx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
}

// EY-499: the gesture surface sits on the active track only, so a drag pressed on a parent row lifted nothing.
#[test]
fn ey499_a_drag_pressed_on_a_parent_column_row_lifts_that_row() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, _lines) = open(&mut cx, &root);
    let empty = at(&mut cx, handle, Slot::Parent, 0, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    press_and_move(&mut vcx, empty, point(empty.x + px(10.), empty.y));
    let want = format!("file://{}\r\n", root.0.join("Empty").display());
    assert_eq!(lifted(&mut vcx, handle), Some(want));
}

// The drag starts after its first motion, which may already rest on a parent row: the pressed row is the one lifted.
#[test]
fn ey499_an_active_press_lifts_its_file_when_the_first_motion_lands_on_a_parent_row() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, _lines) = open(&mut cx, &root);
    let file = at(&mut cx, handle, Slot::Active, 0, 50.);
    let folder = at(&mut cx, handle, Slot::Parent, 0, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    press_and_move(&mut vcx, file, folder);
    let want = format!("file://{}\r\n", root.0.join("Kinds/item-01.txt").display());
    assert_eq!(lifted(&mut vcx, handle), Some(want));
}

// A peek lift skips the active listing's own-row check, so its drop into itself is refused by path, as any other drop's is.
#[test]
fn ey499_a_parent_column_folder_dropped_into_itself_says_it_is_inside_the_drag() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, mut lines) = open(&mut cx, &root);
    let kinds = at(&mut cx, handle, Slot::Parent, 1, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    // The drag lifts at its first motion, and a release still within the slop of that point is not a drop.
    press_and_move(&mut vcx, kinds, point(kinds.x + px(15.), kinds.y));
    let along = point(kinds.x + px(45.), kinds.y);
    vcx.simulate_mouse_move(along, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(along, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    assert!(
        !sent
            .iter()
            .any(|line| line.contains(r#""c":"collisions""#) || line.contains(r#""c":"transfer""#)),
        "{sent:?}"
    );
    let said = handle
        .read_with(&vcx, |shell, _| {
            shell.panes[0].as_ref().unwrap().status.to_string()
        })
        .unwrap();
    assert!(
        said == "That folder is inside the drag.",
        "the path refusal answers, not another gate: {said:?} {sent:?}"
    );
}

// The keys a peek drag carries are the press's, as the active track's lift takes them, not the ones still down at the threshold.
#[test]
fn ey499_a_peek_drag_pressed_with_ctrl_offers_a_copy_after_ctrl_is_released() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, _lines) = open(&mut cx, &root);
    let empty = at(&mut cx, handle, Slot::Parent, 0, 50.);
    let ctrl = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(empty, None, ctrl);
    vcx.simulate_mouse_down(empty, MouseButton::Left, ctrl);
    vcx.simulate_modifiers_change(Modifiers::none());
    vcx.simulate_mouse_move(
        point(empty.x + px(10.), empty.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.run_until_parked();
    let copy = handle
        .update(&mut vcx, |shell, window, cx| {
            match shell.file_drag_payload(window, cx) {
                Some(gpui::ExternalDragPayload::Mime(payload)) => payload
                    .data
                    .get(flea::ui_logic_dg2::ROWS_MIME)
                    .and_then(|bytes| {
                        flea::ui_logic_dg2::Marker::parse(&String::from_utf8_lossy(&bytes[..]))
                    })
                    .map(|marker| marker.copy),
                _ => None,
            }
        })
        .unwrap();
    assert_eq!(copy, Some(true));
}

// Columns stays mounted after a switch to another view, so a press it recorded there must not lift that view's drag.
#[test]
fn ey499_a_press_left_in_columns_lifts_nothing_once_another_view_draws() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, _lines) = open(&mut cx, &root);
    let empty = at(&mut cx, handle, Slot::Parent, 0, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(empty, None, Modifiers::default());
    vcx.simulate_mouse_down(empty, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    handle
        .update(&mut vcx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().drawn_view = Some(crate::window::startup::View::List);
            shell.begin_file_drag(0, window, cx);
        })
        .unwrap();
    assert_eq!(lifted(&mut vcx, handle), None);
}
