//! The ancestor and child tracks own their refusal scroll independently of the active listing.
use crate::{
    backend::{Backend, Event},
    columns::geometry::Slot,
    window::columns_tests::host,
};
use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent};

/// Less than one row pitch, so the bottom row stays partly below the track.
const CUT_DEPTH: f32 = 23.;
/// Rows in each neighbour track; the refused rename is the last one, f119.txt.
const ROWS: usize = 120;

#[test]
fn rename_bottom_each_neighbour_refusal() {
    for (slot, path, lifted) in [
        (Slot::GreatGrandparent, "/srv", "a"),
        (Slot::Grandparent, "/srv/a", "b"),
        (Slot::Parent, "/srv/a/b", "c"),
        (Slot::Child, "/srv/a/b/c/d01", "seed.txt"),
    ] {
        for cut in [false, true] {
            let mut cx = host::context();
            let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2700., 5);
            let (backend, _lines) = Backend::test_lines();
            host::change(&mut cx, window, |shell, window, cx| {
                let names: Vec<_> = (0..ROWS)
                    .map(|at| {
                        if at == 0 {
                            lifted.to_owned()
                        } else {
                            format!("f{at:03}.txt")
                        }
                    })
                    .collect();
                let rows: Vec<_> = names.iter().map(|name| (name.as_str(), false)).collect();
                host::peek(shell, path, &rows, false, window, cx);
                host::settle(shell, window, cx);
                shell.panes[0].as_mut().unwrap().backend = Some(backend);
                shell.panes[0]
                    .as_mut()
                    .unwrap()
                    .columns
                    .as_mut()
                    .unwrap()
                    .area
                    .scroll_by(slot, 100000.);
            });
            host::frame(&mut cx, window);
            host::change(&mut cx, window, |shell, window, cx| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let paint = columns.painters.get(slot).unwrap().borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                let at = point(
                    drawn.bounds.origin.x + px(60.),
                    drawn.bounds.origin.y
                        + px(119. * drawn.row_height - drawn.scroll + drawn.row_height / 2.),
                );
                drop(paint);
                shell.columns_pointer(
                    0,
                    &MouseDownEvent {
                        position: at,
                        button: MouseButton::Right,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                let id = shell.menu.captured.as_ref().unwrap().id;
                for op in ["snapshot", "applications"] {
                    shell.receive(0, Event::Producer(format!(r#"{{"t":"menuaction","op":"{op}","id":{id},"ok":true,"kinds":[32768],"modes":[33188],"applications":[]}}"#)), window, cx);
                }
                // A chosen row closes the menu before it fires, as external_action.rs does.
                shell.menu.presentation.cancel();
                shell.menu.menu.borrow_mut().close();
                shell.menu.pending = Some(("rename".into(), String::new()));
                shell.activate_menu(window, cx);
                shell.receive(0, Event::Producer(format!(r#"{{"t":"menuaction","op":"validate","id":{id},"ok":true,"action":"rename","paths":["{path}/f119.txt"]}}"#)), window, cx);
                assert_eq!(shell.renaming_row(0), Some(119));
                if cut {
                    shell.panes[0]
                        .as_mut()
                        .unwrap()
                        .columns
                        .as_mut()
                        .unwrap()
                        .area
                        .scroll_by(slot, -CUT_DEPTH);
                }
                shell.rename.edit.as_mut().unwrap().entry.begin("bad/name");
                shell.rename_commit(cx);
            });
            host::frame(&mut cx, window);
            host::change(&mut cx, window, |shell, _, _| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let paint = columns.painters.get(slot).unwrap().borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                let sentence = shell.rename.edit.as_ref().unwrap().line.error_area.unwrap();
                assert!(
                    sentence.bottom() <= drawn.bounds.bottom(),
                    "{slot:?}/cut={cut}: {sentence:?} outside {:?}",
                    drawn.bounds
                );
                assert!(drawn.grow.band > 0.);
                assert!(
                    drawn.grow.top(119, drawn.row_height)
                        + drawn.grow.height(119, drawn.row_height)
                        <= drawn.scroll + f32::from(drawn.bounds.size.height) + 0.01
                );
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.scroll_y,
                    0.,
                    "the active column did not move"
                );
            });
            host::change(&mut cx, window, |shell, _, cx| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let held = columns.area.scroll(slot);
                let bounds = columns
                    .painters
                    .get(slot)
                    .unwrap()
                    .borrow()
                    .drawn
                    .as_ref()
                    .unwrap()
                    .bounds;
                let event = ScrollWheelEvent {
                    position: bounds.center(),
                    delta: ScrollDelta::Lines(point(0., -3.)),
                    modifiers: Modifiers::none(),
                    touch_phase: gpui::TouchPhase::Moved,
                };
                assert!(!shell.wheel_blocked(cx), "the chosen row closed the menu");
                shell.columns_scroll_wheel(0, &event, cx);
                let moved = shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .columns
                    .as_ref()
                    .unwrap()
                    .area
                    .scroll(slot);
                assert!(
                    moved >= held - 0.01,
                    "{slot:?}/cut={cut}: a wheel down pulled the grown row back from {held} to {moved}"
                );
                // Stop the wheel's glide so only the cleared band can move the track.
                shell.panes[0]
                    .as_mut()
                    .unwrap()
                    .columns
                    .as_mut()
                    .unwrap()
                    .scroll[slot.index()]
                .input
                .halt();
                shell.rename_abandon(cx);
            });
            host::frame(&mut cx, window);
            host::change(&mut cx, window, |shell, _, _| {
                let area = &shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .columns
                    .as_ref()
                    .unwrap()
                    .area;
                let end =
                    ROWS as f32 * area.row() + crate::columns::geometry::FOOTER - area.height();
                assert!(
                    (area.scroll(slot) - end).abs() < 0.01,
                    "{slot:?}/cut={cut}: a cleared band leaves the track at its own end {end}, not {}",
                    area.scroll(slot)
                );
            });
            host::change(&mut cx, window, |shell, window, _| {
                shell.stop_for_app_quit();
                window.remove_window();
            });
        }
    }
}
