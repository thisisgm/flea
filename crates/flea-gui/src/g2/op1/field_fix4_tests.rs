//! Queued modal focus and stale drawn hits are exercised through the mounted root.
use super::{field_tests::open, model::Dialog};
use crate::{backend::Backend, window::pane::PaneHost};
use flea::ui_logic_op1::{
    confirm::Source,
    trash::{Review, Scope},
};
use gpui::{Entity, EntityInputHandler, Modifiers, TestAppContext, VisualTestContext};

#[gpui::test]
fn u13_fix4_back_to_back_copy_confirm_cancel_returns_listing_keys(cx: &mut TestAppContext) {
    let (handle, copy, mut first_lines) = open(cx, "/files");
    let (backend, mut second_lines) = Backend::test_operation_channel();
    let confirm = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
                host.backend = Some(backend);
                shell.panes[1] = Some(host);
                shell.lifecycle.attach(1);
                shell.confirm_operation(
                    1,
                    Review {
                        count: Some(1),
                        token: "73".into(),
                        ..Default::default()
                    },
                    Scope::Items,
                    Source::Menu(73),
                    window,
                    cx,
                );
                shell.operation_view(1).unwrap()
            })
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let before = handle
        .update(&mut visual, |root, _, cx| {
            Entity::read(&root.shell, cx).panes[0]
                .as_ref()
                .unwrap()
                .pane
                .cursor
        })
        .unwrap();
    visual.simulate_keystrokes("escape enter j");
    assert!(!copy.read_with(&visual, |view, _| view.model.open()));
    assert!(!confirm.read_with(&visual, |view, _| view.model.open()));
    assert_eq!(
        first_lines().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":71}"#)
    );
    assert_eq!(
        second_lines().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":73}"#)
    );
    assert!(first_lines().is_none() && second_lines().is_none());
    handle
        .update(&mut visual, |root, window, cx| {
            let shell = Entity::read(&root.shell, cx);
            assert!(shell.focus.is_focused(window));
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, before + 1);
        })
        .unwrap();
}

#[gpui::test]
fn u13_fix4_pending_collision_escape_releases_focus_and_sends_no_transfer(cx: &mut TestAppContext) {
    let (backend, mut first_lines) = Backend::test_operation_channel();
    let handle = super::atomic_tests::open_with(cx, backend);
    let (backend, mut second_lines) = Backend::test_operation_channel();
    let collision = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
                host.backend = Some(backend);
                shell.panes[1] = Some(host);
                shell.lifecycle.attach(1);
                assert!(shell.ask_operation(1, &super::tests::request(), &[], false, window, cx));
                shell.copy_to_operation(0, 71, false, "/files", window, cx);
                shell.operation_view(1).unwrap()
            })
        })
        .unwrap();
    assert!(second_lines().unwrap().contains("collisions"));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    handle.update(&mut visual, |root, window, cx| root.shell.update(cx, |shell, cx| {
        shell.receive(1, crate::backend::Event::Operations(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":false}"#.into()), window, cx);
    })).unwrap();
    visual.run_until_parked();
    visual.simulate_keystrokes("escape");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(collision.read_with(&visual, |view, _| matches!(
        view.drawn_dialog(),
        Some(Dialog::Collide { .. })
    )));
    let focus = collision.read_with(&visual, |view, _| view.editor.focus.clone().unwrap());
    assert!(visual.update(|window, _| focus.is_focused(window)));
    visual.simulate_keystrokes("escape");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(collision.read_with(&visual, |view, _| !view.model.open()
        && view.model.collide.state() == "idle"));
    assert_eq!(
        first_lines().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":71}"#)
    );
    assert!(second_lines().is_none(), "preflight Cancel clears the pending request without sending a transfer or cancellation command");
    let before = handle
        .update(&mut visual, |root, window, cx| {
            let shell = Entity::read(&root.shell, cx);
            assert!(shell.focus.is_focused(window));
            shell.panes[0].as_ref().unwrap().pane.cursor
        })
        .unwrap();
    visual.simulate_keystrokes("j");
    assert_eq!(
        handle
            .update(&mut visual, |root, _, cx| Entity::read(&root.shell, cx)
                .panes[0]
                .as_ref()
                .unwrap()
                .pane
                .cursor)
            .unwrap(),
        before + 1
    );
}

fn stale_hit(
    cx: &mut TestAppContext,
    initial: &str,
    replacement: &str,
    composing: bool,
    interior: bool,
) {
    let (handle, view, _received) = open(cx, initial);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| {
        let position = {
            let view = Entity::read(&view, cx);
            let drawn = view.editor.drawn.borrow();
            let bounds = drawn.bounds.unwrap();
            gpui::point(
                bounds.left() + drawn.line.x_for(view.editor.entry.text.len()),
                bounds.center().y,
            )
        };
        view.update(cx, |view, cx| {
            if composing {
                view.replace_and_mark_text_in_range(None, replacement, Some(1..1), window, cx);
            } else {
                view.replace_text_in_range(None, replacement, window, cx);
            }
        });
        {
            let view = Entity::read(&view, cx);
            let at = view.copy_index(position.x);
            assert_ne!(
                view.editor.drawn.borrow().line.revision,
                view.editor.entry.revision
            );
            if interior {
                assert!(
                    at < view.editor.entry.text.len()
                        && !view.editor.entry.text.is_char_boundary(at),
                    "the old hit must fall inside the new UTF-8 character"
                );
            } else {
                assert!(
                    at > view.editor.entry.text.len(),
                    "the old hit must exceed the shorter current text"
                );
            }
        }
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position,
                modifiers: Modifiers::none(),
                click_count: 2,
                first_mouse: false,
            }),
            cx,
        );
        let view = Entity::read(&view, cx);
        let entry = &view.editor.entry;
        assert_eq!(entry.text, replacement);
        assert!(entry.selected.end <= entry.text.len());
        assert!(entry.text.is_char_boundary(entry.selected.start));
        assert!(entry.text.is_char_boundary(entry.selected.end));
        assert_eq!(entry.marked.is_some(), composing);
    });
}

#[gpui::test]
fn u13_fix4_replacement_before_double_click_clamps_a_stale_utf8_offset(cx: &mut TestAppContext) {
    stale_hit(cx, "/abc", "/\u{e9}\u{e9}z", false, true);
}

#[gpui::test]
fn u13_fix4_ime_before_double_click_clamps_a_stale_supplementary_offset(cx: &mut TestAppContext) {
    stale_hit(cx, "/a", "/\u{1d11e}z", true, true);
}

#[gpui::test]
fn u13_fix4_shorter_ime_before_double_click_keeps_valid_selection(cx: &mut TestAppContext) {
    stale_hit(cx, "/files/a-long-directory", "/\u{e9}", true, false);
}

#[gpui::test]
fn u13_fix4_copy_confirm_copy_queue_keeps_keyboard_and_originating_backends(
    cx: &mut TestAppContext,
) {
    let (handle, first, mut first_lines) = open(cx, "/files");
    let (backend, mut second_lines) = Backend::test_operation_channel();
    let second = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
                host.backend = Some(backend);
                shell.panes[1] = Some(host);
                shell.lifecycle.attach(1);
                shell.confirm_operation(
                    1,
                    Review {
                        count: Some(1),
                        token: "73".into(),
                        ..Default::default()
                    },
                    Scope::Items,
                    Source::Menu(73),
                    window,
                    cx,
                );
                shell.operation_view(1).unwrap()
            })
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(second.read_with(&visual, |view, _| matches!(
        view.drawn_dialog(),
        Some(Dialog::Delete {
            focus: flea::ui_logic_op1::trash::Button::Cancel,
            ..
        })
    )));
    handle
        .update(&mut visual, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.copy_to_operation(0, 74, false, "/again", window, cx)
            })
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!second.read_with(&visual, |view, _| view.model.open()));
    assert!(first.read_with(&visual, |view, _| matches!(
        view.drawn_dialog(),
        Some(Dialog::CopyTo { menu: 74, .. })
    )));
    visual.simulate_input("/after");
    assert_eq!(super::field_tests::state(&first, &visual).0, "/after");
    visual.simulate_keystrokes("escape");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        first_lines().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":71}"#)
    );
    assert_eq!(
        first_lines().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":74}"#)
    );
    assert_eq!(
        second_lines().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":73}"#)
    );
    assert!(first_lines().is_none() && second_lines().is_none());
    let before = handle
        .update(&mut visual, |root, window, cx| {
            let shell = Entity::read(&root.shell, cx);
            assert!(shell.focus.is_focused(window));
            shell.panes[0].as_ref().unwrap().pane.cursor
        })
        .unwrap();
    visual.simulate_keystrokes("j");
    assert_eq!(
        handle
            .update(&mut visual, |root, _, cx| Entity::read(&root.shell, cx)
                .panes[0]
                .as_ref()
                .unwrap()
                .pane
                .cursor)
            .unwrap(),
        before + 1
    );
}

#[gpui::test]
fn i50_ime_blur_commits_the_composition(cx: &mut TestAppContext) {
    let (handle, view, _lines) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.replace_text_in_range(None, "/dir", window, cx);
            view.replace_and_mark_text_in_range(None, "\u{3081}", None, window, cx);
        })
    });
    // A pointer submit blurs the field mid-composition; the preedit commits as shown.
    let copy = visual.debug_bounds("op1-button-1").unwrap();
    visual.simulate_click(copy.center(), Modifiers::none());
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(view.read_with(&visual, |v, _| v.editor.entry.marked.is_none()));
    let (path, _, busy) = super::field_tests::state(&view, &visual);
    assert_eq!((path.as_str(), busy), ("/dir\u{3081}", true));
}
