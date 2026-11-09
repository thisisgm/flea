//! Closing-review regression for consecutive mounted editors on different panes.
use super::{field_tests::open, model::Dialog};
use crate::{backend::Backend, window::pane::PaneHost};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[gpui::test]
fn u13_review_r2_queued_copy_editors_restore_the_live_shell(cx: &mut TestAppContext) {
    let (handle, first, mut first_lines) = open(cx, "/files");
    let (backend, mut second_lines) = Backend::test_operation_channel();
    let second = handle.update(cx, |root, window, cx| {
        root.shell.update(cx, |shell, cx| {
            let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
            host.backend = Some(backend);
            shell.panes[1] = Some(host);
            shell.lifecycle.attach(1);
            shell.copy_to_operation(1, 72, false, "/other", window, cx);
            shell.operation_view(1).unwrap()
        })
    }).unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!first.read_with(&visual, |v, _| v.model.open()));
    assert!(second.read_with(&visual, |v, _| matches!(v.model.dialog, Some(Dialog::CopyTo {menu: 72, ..}))));
    let second_focus = second.read_with(&visual, |v, _| v.editor.focus.clone().unwrap());
    assert!(visual.update(|window, _| second_focus.is_focused(window)));
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!second.read_with(&visual, |v, _| v.model.open()));
    assert_eq!(first_lines().as_deref(), Some(r#"{"c":"menuaction","op":"close","id":71}"#));
    assert_eq!(second_lines().as_deref(), Some(r#"{"c":"menuaction","op":"close","id":72}"#));
    let (shell_focus, cursor) = handle.update(&mut visual, |root, _, cx| {
        let shell = Entity::read(&root.shell, cx);
        (shell.focus.clone(), shell.panes[0].as_ref().unwrap().pane.cursor)
    }).unwrap();
    let focused = visual.update(|window, _| shell_focus.is_focused(window));
    println!("both editors closed; shell focused={focused}");
    visual.simulate_keystrokes("j");
    let after = handle.update(&mut visual, |root, _, cx| {
        Entity::read(&root.shell, cx).panes[0].as_ref().unwrap().pane.cursor
    }).unwrap();
    assert_eq!(after, cursor + 1, "closing queued editors must return keyboard input to the listing");
    assert!(focused, "closing queued editors must restore the live shell, never a hidden editor");
}

#[gpui::test]
fn u13_review_r2_pending_collision_keeps_keyboard_after_editor_closes(cx: &mut TestAppContext) {
    let (backend, _first_lines) = Backend::test_operation_channel();
    let handle = super::atomic_tests::open_with(cx, backend);
    let (second_backend, mut second_lines) = Backend::test_operation_channel();
    let second = handle.update(cx, |root, window, cx| {
        root.shell.update(cx, |shell, cx| {
            let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
            host.backend = Some(second_backend);
            shell.panes[1] = Some(host);
            shell.lifecycle.attach(1);
            assert!(shell.ask_operation(1, &super::tests::request(), &[], false, window, cx));
            shell.copy_to_operation(0, 71, false, "/files", window, cx);
            shell.operation_view(1).unwrap()
        })
    }).unwrap();
    assert!(second_lines().unwrap().contains("collisions"));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    handle.update(&mut visual, |root, window, cx| {
        root.shell.update(cx, |shell, cx| {
            shell.receive(1, crate::backend::Event::Operations(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":false}"#.into()), window, cx);
        })
    }).unwrap();
    visual.run_until_parked();
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(second.read_with(&visual, |v, _| matches!(v.drawn_dialog(), Some(Dialog::Collide {..}))), "the pending collision is drawn after the editor closes");
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(!second.read_with(&visual, |v, _| v.model.open()), "Escape must reach the collision that is currently drawn");
    // Registered corrected-tail successor; the literal raw probe is archived in ignored proof.
    assert!(second_lines().is_none(), "preflight Cancel clears the pending request without sending a transfer or cancellation command");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(second.read_with(&visual, |view, _| !view.model.open()
        && view.model.collide.state() == "idle"));
    let before = handle.update(&mut visual, |root, window, cx| {
        let shell = Entity::read(&root.shell, cx);
        assert!(shell.focus.is_focused(window));
        shell.panes[0].as_ref().unwrap().pane.cursor
    }).unwrap();
    visual.simulate_keystrokes("j");
    let after = handle.update(&mut visual, |root, _, cx| {
        Entity::read(&root.shell, cx).panes[0].as_ref().unwrap().pane.cursor
    }).unwrap();
    assert_eq!(after, before + 1, "closing preflight collision must return keyboard input to the listing");
}

#[gpui::test]
fn u13_review_r2_double_click_after_shorter_text_before_draw_is_safe(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files/a-long-directory");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| {
        let position = {
            let view = Entity::read(&view, cx);
            let drawn = view.editor.drawn.borrow();
            let bounds = drawn.bounds.unwrap();
            gpui::point(bounds.left() + drawn.line.x_for(view.editor.entry.text.len()), bounds.center().y)
        };
        assert!(window.dispatch_keystroke(gpui::Keystroke::parse("/").unwrap(), cx));
        {
            let view = Entity::read(&view, cx);
            assert_eq!(view.editor.entry.text, "/");
            assert!(view.copy_index(position.x) > view.editor.entry.text.len(), "the next frame has not repainted the shorter text");
        }
        window.dispatch_event(gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position,
            modifiers: gpui::Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        }), cx);
    });
}
