//! U08 reveal and viewport bounds through the mounted host.
use super::*;
use std::time::Duration;

fn editor_point(
    producer: &Entity<Producer>,
    byte: usize,
    cx: &mut VisualTestContext,
) -> gpui::Point<gpui::Pixels> {
    producer.read_with(cx, |producer, _| {
        let model = producer.model.borrow();
        let laid = producer.laid.borrow();
        let rect = laid.row(model.renaming.unwrap()).unwrap().editor.unwrap();
        point(
            px(rect.x + 4. + model.editor_x(byte) - model.editor_scroll),
            px(rect.y + rect.height / 2.),
        )
    })
}

fn replacement_before_paint(cx: &mut TestAppContext, old_byte: usize, text: &str, marked: bool) {
    use gpui::EntityInputHandler;
    let (shell, cx) = open(false, cx);
    rows(
        &shell,
        vec![RailEntry {
            group: Group::Network,
            kind: Kind::Share,
            label: "NAS share".into(),
            uri: "smb://nas/share".into(),
            glyph: "server",
            ..RailEntry::default()
        }],
        cx,
    );
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("f2");
    frame(cx);
    let producer = producer(&shell, cx);
    let position = editor_point(&producer, old_byte, cx);
    cx.update(|window, cx| {
        producer.update(cx, |p, cx| {
            if marked {
                p.replace_and_mark_text_in_range(None, text, None, window, cx);
            } else {
                p.replace_text_in_range(None, text, window, cx);
            }
            let model = p.model.borrow();
            let laid = p.laid.borrow();
            let rect = laid.row(0).unwrap().editor.unwrap();
            let hit = model.editor_index(f32::from(position.x) - rect.x - 4. + model.editor_scroll);
            assert_eq!(
                hit, old_byte,
                "the pointer must still use the old shaped line"
            );
            assert!(hit > text.len() || !text.is_char_boundary(hit));
            assert_eq!(model.editor.text, text);
        });
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                position,
                button: MouseButton::Left,
                modifiers: Modifiers::none(),
                click_count: 2,
                first_mouse: false,
            }),
            cx,
        );
    });
    frame(cx);
    producer.read_with(cx, |p, _| {
        let model = p.model.borrow();
        assert_eq!(model.editor.text, text);
        let end = text.find('/').unwrap_or(text.len());
        assert_eq!(model.editor.selected, 0..end);
        assert_eq!(model.editor.marked.is_some(), marked);
    });
}

#[gpui::test]
fn u08_fix2_utf8_replacement_before_paint(cx: &mut TestAppContext) {
    replacement_before_paint(cx, 1, "é/files", false);
}

#[gpui::test]
fn u08_fix2_supplementary_ime_before_paint(cx: &mut TestAppContext) {
    replacement_before_paint(cx, 3, "𝄞/files", true);
}

#[gpui::test]
fn u08_fix2_shorter_ime_before_paint(cx: &mut TestAppContext) {
    replacement_before_paint(cx, 5, "é", true);
}

#[gpui::test]
fn u08_fix1_editor_owns_buttons_unicode_caret_shift_and_drag_selection(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    let label = "Nás share";
    rows(
        &shell,
        vec![RailEntry {
            group: Group::Network,
            kind: Kind::Share,
            label: label.into(),
            uri: "smb://nas/share".into(),
            glyph: "server",
            ..RailEntry::default()
        }],
        cx,
    );
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("f2");
    frame(cx);
    let producer = producer(&shell, cx);
    let at = editor_point(&producer, 3, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx);
    assert_eq!(
        producer.read_with(cx, |p, _| p.model.borrow().editor.selected.clone()),
        3..3
    );
    let end = editor_point(&producer, label.len(), cx);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::none()
    };
    cx.simulate_mouse_down(end, MouseButton::Left, shift);
    cx.simulate_mouse_up(end, MouseButton::Left, shift);
    frame(cx);
    assert_eq!(
        producer.read_with(cx, |p, _| p.model.borrow().editor.selected.clone()),
        3..label.len()
    );
    cx.simulate_mouse_down(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx);
    producer.read_with(cx, |p, _| {
        let model = p.model.borrow();
        assert_eq!(model.editor.selected, 3..label.len());
        assert!(model.editor.reversed);
        assert!(
            !model.pointer_focus,
            "the editor keeps the keyboard row highlight"
        );
    });
    for button in [MouseButton::Right, MouseButton::Middle] {
        cx.simulate_mouse_down(at, button, Modifiers::none());
        cx.simulate_mouse_up(at, button, Modifiers::none());
    }
    frame(cx);
    assert!(
        heard.borrow().is_empty(),
        "editing never emits activation, menu or tab requests"
    );
    assert_eq!(producer.read_with(cx, |p, _| p.rename_index()), Some(0));
}

#[gpui::test]
fn u08_fix1_click_away_cancels_without_committing_and_other_row_still_activates(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    rows(
        &shell,
        vec![
            RailEntry {
                group: Group::Network,
                kind: Kind::Share,
                label: "NAS".into(),
                uri: "smb://nas/share".into(),
                glyph: "server",
                ..RailEntry::default()
            },
            flea::ui_logic_rail_places::order::trash_entry(),
        ],
        cx,
    );
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    let producer = producer(&shell, cx);
    for outside in [false, true] {
        producer.update(cx, |p, cx| {
            assert!(p.start_rename(0, cx));
            p.rename_push("Unsaved", cx);
        });
        frame(cx);
        let away = producer.read_with(cx, |p, _| {
            let laid = p.laid.borrow();
            point(
                px(if outside { laid.width + 50. } else { 20. }),
                px(laid.height - 10.),
            )
        });
        cx.simulate_mouse_move(away, None, Modifiers::none());
        cx.simulate_mouse_down(away, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(away, MouseButton::Left, Modifiers::none());
        frame(cx);
        assert_eq!(producer.read_with(cx, |p, _| p.rename_index()), None);
        assert!(
            heard.borrow().is_empty(),
            "click-away must not commit a rename or activate a share"
        );
    }
    producer.update(cx, |p, cx| {
        assert!(p.start_rename(0, cx));
        p.rename_push("Unsaved", cx);
    });
    frame(cx);
    let trash = producer.read_with(cx, |p, _| {
        let laid = p.laid.borrow();
        let rect = laid.row(1).unwrap().rect;
        point(px(rect.x + rect.width / 2.), px(rect.y + rect.height / 2.))
    });
    cx.simulate_mouse_move(trash, None, Modifiers::none());
    cx.simulate_mouse_down(trash, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(trash, MouseButton::Left, Modifiers::none());
    frame(cx);
    assert_eq!(producer.read_with(cx, |p, _| p.rename_index()), None);
    assert_eq!(heard.borrow().len(), 1);
    assert_eq!(heard.borrow()[0].event, Event::Trash);
}

#[gpui::test]
fn u08_overlay_reveal_does_not_reflow_and_withdrawn_rail_stops_work(cx: &mut TestAppContext) {
    let (shell, cx) = open(true, cx);
    cx.simulate_mouse_move(point(px(500.), px(100.)), None, Modifiers::none());
    shell.update(cx, |shell, cx| {
        shell.rail_state(
            &jsondoc::parse(r#"{"places":{"autoHide":true}}"#).unwrap(),
            cx,
        )
    });
    frame(cx);
    assert_eq!(ipc(&shell, "railCount", &[], cx).stdout.trim(), "0");
    cx.simulate_mouse_move(point(px(5.), px(100.)), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        crate::rail::DWELL_MS + crate::rail::OPEN_MS,
    ));
    frame(cx);
    let state = jsondoc::parse(&ipc(&shell, "railState", &[], cx).stdout).unwrap();
    assert_eq!(
        state.get("hidden").and_then(jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(state.get("inset").and_then(jsondoc::Json::as_f64), Some(0.));
    assert!(
        ipc(&shell, "railCount", &[], cx)
            .stdout
            .trim()
            .parse::<usize>()
            .unwrap()
            > 0
    );
    let producer = producer(&shell, cx);
    let reads = producer.read_with(cx, |producer, _| producer.directory_reads);
    cx.simulate_mouse_move(point(px(500.), px(100.)), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        crate::rail::SETTLE_MS + crate::rail::CLOSE_MS,
    ));
    frame(cx);
    assert_eq!(ipc(&shell, "railCount", &[], cx).stdout.trim(), "0");
    producer.read_with(cx, |producer, _| {
        assert!(!producer.polling);
        assert_eq!(producer.directory_reads, reads);
    });
}

#[gpui::test]
fn u08_last_row_scrolls_into_view_and_only_visible_rows_are_prepared(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(
        &shell,
        (0..1000)
            .map(|index| row(&format!("Place {index}"), &format!("/place/{index}")))
            .collect(),
        cx,
    );
    let producer = producer(&shell, cx);
    producer.read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        assert!(laid.rows.len() <= crate::rail::geometry::drawn_rows(laid.height));
        assert!(producer.model.borrow().shape_misses() < 200);
    });
    assert_eq!(ipc(&shell, "railRowCentre", &["999"], cx).stdout.trim(), "");
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("end");
    frame(cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "999");
    assert!(!ipc(&shell, "railRowCentre", &["999"], cx)
        .stdout
        .trim()
        .is_empty());
    assert_eq!(ipc(&shell, "railRowCentre", &["0"], cx).stdout.trim(), "");
}

#[gpui::test]
fn u08_fix1_relocated_cursor_is_revealed_and_missing_or_ambiguous_identity_clears(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("down");
    frame(cx);
    let mut entries: Vec<_> = (0..150)
        .map(|i| row(&format!("New {i}"), &format!("/new/{i}")))
        .collect();
    entries.push(row("B", "/b"));
    rows(&shell, entries, cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "150");
    assert!(!ipc(&shell, "railRowCentre", &["150"], cx)
        .stdout
        .trim()
        .is_empty());
    rows(&shell, vec![row("A", "/a")], cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "-1");
    let before = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().path.clone());
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().path.clone()),
        before
    );
    cx.simulate_keystrokes("down");
    frame(cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "0");
    rows(&shell, vec![row("A", "/a"), row("A", "/a")], cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "-1");
    cx.simulate_keystrokes("end");
    frame(cx);
    rows(&shell, vec![row("A", "/a")], cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "-1");
}
