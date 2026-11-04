//! Open With folds native wheel and touchpad input like the shipped menu list.
use super::menus_tests::settle;
use super::openwith_controls_tests::prepared;
use crate::g2::op3::openwith::Stop;
use gpui::{point, px, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase};

fn applications() -> String {
    (0..20)
        .map(|at| {
            format!(r#"{{"id":"app{at}.desktop","label":"App {at:02}","icon":"","default":false}}"#)
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[gpui::test]
fn openwith_wheel_steps_only_the_list_and_reveals_the_highlight(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = prepared(cx, &applications());
    let (inside, outside, row0, pane_scroll) = shell.read_with(cx, |shell, _| {
        let open = shell.open_with.as_ref().unwrap();
        (
            open.frame.part("list").unwrap().bounds.center(),
            open.frame.part("search").unwrap().bounds.center(),
            open.frame.part("row0").unwrap().bounds,
            shell.panes[0].as_ref().unwrap().pane.scroll_y,
        )
    });
    for (position, lines, cursor) in [
        (outside, -3., 0),
        (inside, -1.5, 0),
        (inside, -1.5, 1),
        (inside, -3., 2),
        (inside, -18., 8),
        (inside, 300., 0),
    ] {
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(point(0., lines)),
            touch_phase: TouchPhase::Moved,
            modifiers: Default::default(),
        });
        settle(cx);
        shell.read_with(cx, |shell, _| {
            let open = shell.open_with.as_ref().unwrap();
            assert_eq!(open.card.cursor, cursor, "one application per raw notch");
            assert_eq!(open.card.focus, Stop::Search, "wheel retains search focus");
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, pane_scroll);
            if cursor == 2 {
                assert_eq!(
                    open.frame.part("row0").unwrap().bounds,
                    row0,
                    "the viewport stays still while the cursor fits"
                );
            }
            if cursor == 8 {
                use crate::g2::op3::shell::{AppRow, Block};
                let card = open.card.card(&shell.locale.catalog);
                let rows = card
                    .body
                    .iter()
                    .find_map(|block| match block {
                        Block::Apps(rows, _) => Some(rows),
                        _ => None,
                    })
                    .expect("the actual producer renders applications");
                let slot = rows
                    .iter()
                    .filter(|row| matches!(row, AppRow::App { .. }))
                    .position(|row| {
                        matches!(row, AppRow::App { at, label, .. }
                        if *at == cursor && label == &format!("App {cursor:02}"))
                    })
                    .expect("selected application8 must have a rendered row");
                let list = open.frame.part("list").unwrap().bounds;
                let row = open.frame.part(&format!("row{slot}")).unwrap().bounds;
                assert!(list.contains(&row.origin) && row.bottom() <= list.bottom());
            }
        });
    }
    assert!(sent().is_empty(), "wheel never launches or closes the card");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_wheel_touchpad_accumulates_travel_and_resets_on_gesture_start(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = prepared(cx, &applications());
    let inside = shell.read_with(cx, |shell, _| {
        shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("list")
            .unwrap()
            .bounds
            .center()
    });
    for (pixels, phase, cursor) in [
        (-12., TouchPhase::Started, 0),
        (-12., TouchPhase::Moved, 0),
        (-24., TouchPhase::Moved, 1),
        (-36., TouchPhase::Moved, 1),
        (-12., TouchPhase::Started, 1),
        (-36., TouchPhase::Moved, 2),
        (48., TouchPhase::Moved, 1),
        (0., TouchPhase::Cancelled, 1),
        (-30., TouchPhase::Moved, 2),
    ] {
        cx.simulate_event(ScrollWheelEvent {
            position: inside,
            delta: ScrollDelta::Pixels(point(px(0.), px(pixels))),
            touch_phase: phase,
            modifiers: Default::default(),
        });
        settle(cx);
        shell.read_with(cx, |shell, _| {
            let open = shell.open_with.as_ref().unwrap();
            assert_eq!(
                open.card.cursor, cursor,
                "gained touchpad travel folds by 37px rows; phaseless travel uses raw notches"
            );
            assert_eq!(open.card.focus, Stop::Search);
        });
    }
    assert!(sent().is_empty());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_wheel_pointer_takes_the_row_before_notches_without_taking_focus(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut sent) = prepared(cx, &applications());
    let position = shell.read_with(cx, |shell, _| {
        shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("row3")
            .unwrap()
            .bounds
            .center()
    });
    cx.simulate_mouse_move(position, None, gpui::Modifiers::none());
    settle(cx);
    shell.read_with(cx, |shell, _| {
        let open = &shell.open_with.as_ref().unwrap().card;
        assert_eq!(
            open.cursor, 3,
            "the actual pointer takes the application under it"
        );
        assert_eq!(open.focus, Stop::Search);
    });
    for _ in 0..2 {
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(point(0., -3.)),
            touch_phase: TouchPhase::Moved,
            modifiers: Default::default(),
        });
        settle(cx);
    }
    shell.read_with(cx, |shell, _| {
        let open = &shell.open_with.as_ref().unwrap().card;
        assert_eq!(
            open.cursor, 5,
            "two notches follow the pointer's row, as EY-480's shipped arm does"
        );
        assert_eq!(open.focus, Stop::Search);
    });
    assert!(sent().is_empty());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
