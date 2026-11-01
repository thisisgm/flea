//! A network place's own menu hides its plate without discarding the rail's input state.
use super::*;
use crate::{list, palette::Theme, window::u07_tests::read};
use gpui::{MouseButton, MouseDownEvent, MouseUpEvent};

fn grounds(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<gpui::Hsla> {
    let rect = shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        let index = producer.cursor().unwrap();
        producer.laid.borrow().row(index).unwrap().rect
    });
    cx.update(|window, _| {
        let scale = window.scale_factor();
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                let bounds = quad.bounds;
                (bounds.origin.x.0 / scale - rect.x).abs() < 0.1
                    && (bounds.origin.y.0 / scale - rect.y).abs() < 0.1
                    && (bounds.size.height.0 / scale - rect.height).abs() < 0.1
                    && ((bounds.size.width.0 / scale - rect.width).abs() < 0.1
                        || (bounds.size.width.0 / scale - 2.).abs() < 0.1)
            })
            .filter_map(|quad| quad.background.as_solid())
            .collect()
    })
}

fn own_menu(cx: &mut TestAppContext, pointer: bool) {
    let theme = Theme::default();
    cx.update(|cx| cx.set_global(theme));
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"keys":"default","places":{}}"#, recorder.clone(), cx);
    recorder.answer(recorder.listings()[0], SHARE_OUTPUT);
    frame(cx);
    shell.update(cx, |shell, cx| {
        let mut row = shell.network().unwrap().rows[0].clone();
        row.path = "/fixture/nas".into();
        row.editable = Some(true);
        shell.panes[shell.focus_side].as_mut().unwrap().path = row.path.clone();
        shell.network_publish(Some(vec![row]), cx);
    });
    frame(cx);
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    let index = producer.read_with(cx, |producer, _| {
        producer
            .entries()
            .iter()
            .position(|entry| entry.group == Group::Network)
            .unwrap()
    });
    shell.update_in(cx, |shell, window, cx| {
        shell.rail_focus(true, cx);
        producer.update(cx, |producer, cx| {
            producer.set_cursor(index, cx);
        });
        window.focus(&shell.dual_focus.rail, cx);
    });
    frame(cx);
    let accent = theme.accent.opacity(list::CURSOR_WASH);
    assert!(
        grounds(&shell, cx).contains(&accent),
        "real rail cursor paints"
    );
    if pointer {
        let at = producer.read_with(cx, |producer, _| {
            let rect = producer.laid.borrow().row(index).unwrap().rect;
            point(px(rect.x + 50.), px(rect.y + rect.height / 2.))
        });
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_event(MouseDownEvent {
            position: at,
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: at,
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
    } else {
        cx.simulate_keystrokes("m");
    }
    frame(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert_eq!(read(&shell, cx, "contextMenuCursor", &[]), "0");
    assert!(read(&shell, cx, "contextMenuEntries", &[]).starts_with("Open|"));
    let state = producer.read_with(cx, |producer, _| {
        let model = producer.model.borrow();
        assert_eq!(model.cursor, Some(index));
        assert_eq!(model.lit, Some(index));
        assert!(model.focused, "menu input does not reroute the rail focus");
        (
            model.cursor,
            model.lit,
            model.pointer_focus,
            model.mark_focused,
        )
    });
    assert!(
        grounds(&shell, cx).is_empty(),
        "NAS has no hover, grey or accent plate under its own menu"
    );
    let entries = read(&shell, cx, "contextMenuEntries", &[]);
    let rename = entries
        .split('|')
        .position(|label| label == "Rename")
        .unwrap();
    for _ in 0..entries.split('|').count() {
        if read(&shell, cx, "contextMenuCursor", &[]) == rename.to_string() {
            break;
        }
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    assert_eq!(
        read(&shell, cx, "contextMenuCursor", &[]),
        rename.to_string()
    );
    assert!(
        grounds(&shell, cx).is_empty(),
        "moving the menu cursor keeps NAS unmarked"
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    producer.read_with(cx, |producer, _| {
        let model = producer.model.borrow();
        assert_eq!(
            (
                model.cursor,
                model.lit,
                model.pointer_focus,
                model.mark_focused
            ),
            state
        );
        assert!(model.focused);
    });
    cx.update(|window, cx| {
        assert!(shell.read(cx).dual_focus.rail.contains_focused(window, cx));
    });
    let restored = grounds(&shell, cx);
    assert!(
        restored.contains(&if pointer {
            theme.foreground.opacity(list::GREY_CURSOR)
        } else {
            accent
        }),
        "dismissal restores the existing row presentation"
    );
    assert_eq!(
        recorder.listings().len(),
        1,
        "no network operation is repeated"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn f5_keyboard_network_menu_has_no_plate_and_escape_restores_cursor(cx: &mut TestAppContext) {
    own_menu(cx, false);
}

#[gpui::test]
fn f5_pointer_network_menu_has_no_plate_and_escape_restores_place(cx: &mut TestAppContext) {
    own_menu(cx, true);
}
