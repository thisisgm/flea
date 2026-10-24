//! Busy confirmation swaps retain both their snapshot and their session identity.
use super::atomic_tests::{advance, drawn, line, open_with};
use crate::backend::Backend;
use crate::window::root::Root;
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

fn refresh(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.confirm_operation(
                    0,
                    flea::ui_logic_op1::trash::Review {
                        count: Some(1),
                        token: "6".into(),
                        ..Default::default()
                    },
                    flea::ui_logic_op1::trash::Scope::Items,
                    flea::ui_logic_op1::confirm::Source::Menu(6),
                    window,
                    cx,
                );
            })
        })
        .unwrap();
    drawn(cx, handle);
    advance(cx, handle, 2500);
    line(
        cx,
        handle,
        r#"{"t":"menuaction","id":6,"op":"checkDelete","ok":true,"valid":false}"#,
    );
}

fn fresh(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    line(
        cx,
        handle,
        r#"{"t":"menuaction","id":6,"op":"refreshDelete","ok":true,"token":7,"count":1,"bytes":4}"#,
    );
}

fn review(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Option<(bool, String)> {
    drawn(cx, handle);
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let overlay = shell.operation_view(0).unwrap();
            match overlay.read(cx).drawn_dialog() {
                Some(super::model::Dialog::Delete { busy, review, .. }) => {
                    Some((*busy, review.token.clone()))
                }
                _ => None,
            }
        })
        .unwrap()
}

#[gpui::test]
fn u13_fix1_fast_review_never_draws_working(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    refresh(cx, handle);
    assert!(received().unwrap().contains("checkDelete"));
    assert!(received().unwrap().contains("refreshDelete"));
    assert_eq!(review(cx, handle), Some((false, "6".into())));
    advance(cx, handle, 149);
    fresh(cx, handle);
    assert_eq!(review(cx, handle), Some((false, "7".into())));
    advance(cx, handle, 101);
    assert_eq!(review(cx, handle), Some((false, "7".into())));
}

#[gpui::test]
fn u13_fix1_slow_review_holds_working_until_its_drawn_minimum(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    refresh(cx, handle);
    advance(cx, handle, 149);
    assert_eq!(review(cx, handle), Some((false, "6".into())));
    advance(cx, handle, 1);
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 20);
    fresh(cx, handle);
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 79);
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 1);
    assert_eq!(review(cx, handle), Some((false, "7".into())));
}

#[gpui::test]
fn u13_fix1_cancel_stays_live_during_a_held_review(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    refresh(cx, handle);
    assert!(received().unwrap().contains("checkDelete"));
    assert!(received().unwrap().contains("refreshDelete"));
    advance(cx, handle, 150);
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 20);
    fresh(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":6}"#)
    );
    assert!(received().is_none());
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 80);
    assert_eq!(review(cx, handle), None);
}

fn replace_review(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.confirm_operation(
                    0,
                    flea::ui_logic_op1::trash::Review {
                        count: Some(2),
                        token: "8".into(),
                        ..Default::default()
                    },
                    flea::ui_logic_op1::trash::Scope::Items,
                    flea::ui_logic_op1::confirm::Source::Menu(7),
                    window,
                    cx,
                );
            });
        })
        .unwrap();
}

fn cancel(cx: &mut TestAppContext, handle: WindowHandle<Root>, input: &str) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    if input == "scrim" || input == "pointer" {
        let point = if input == "pointer" {
            visual.debug_bounds("op1-button-0").unwrap().center()
        } else {
            gpui::point(gpui::px(2.), gpui::px(2.))
        };
        visual.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
        visual.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    } else {
        visual.simulate_keystrokes(input);
    }
    visual.run_until_parked();
}

#[gpui::test]
fn u13_fix1_held_review_never_cancels_a_replacement_review(cx: &mut TestAppContext) {
    for input in ["enter", "escape", "pointer", "scrim"] {
        let (backend, mut received) = Backend::test_operation_channel();
        let handle = open_with(cx, backend);
        refresh(cx, handle);
        while received().is_some() {}
        advance(cx, handle, 150);
        assert_eq!(review(cx, handle), Some((true, "6".into())));
        advance(cx, handle, 20);
        replace_review(cx, handle);
        assert_eq!(
            received().as_deref(),
            Some(r#"{"c":"menuaction","op":"close","id":6}"#)
        );
        assert_eq!(review(cx, handle), Some((true, "6".into())));
        cancel(cx, handle, input);
        assert!(
            received().is_none(),
            "{input} must not cancel an unseen session"
        );
        advance(cx, handle, 80);
        assert_eq!(review(cx, handle), Some((false, "8".into())));
    }
}

#[gpui::test]
fn u13_fix1_held_review_never_cancels_an_unseen_collision(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    refresh(cx, handle);
    while received().is_some() {}
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(shell.ask_operation(0, &super::tests::request(), &[], false, window, cx));
            });
        })
        .unwrap();
    assert!(received().unwrap().contains("collisions"));
    advance(cx, handle, 150);
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 20);
    line(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes","d":false,"i":""}],"mergeable":false}"#,
    );
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":6}"#)
    );
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    for input in ["enter", "escape", "pointer", "scrim"] {
        cancel(cx, handle, input);
        assert!(
            received().is_none(),
            "{input} must not dismiss an unseen collision"
        );
    }
    advance(cx, handle, 80);
    drawn(cx, handle);
    handle
        .update(cx, |root, _, cx| {
            let view = root.shell.read(cx).operation_view(0).unwrap();
            assert!(matches!(
                view.read(cx).drawn_dialog(),
                Some(super::model::Dialog::Collide { .. })
            ));
        })
        .unwrap();
}

#[gpui::test]
fn u13_fix1_held_review_pointer_keeps_its_identity_across_expiry(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    refresh(cx, handle);
    while received().is_some() {}
    advance(cx, handle, 150);
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    advance(cx, handle, 20);
    replace_review(cx, handle);
    assert!(received().unwrap().contains("close"));
    assert_eq!(review(cx, handle), Some((true, "6".into())));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let point = visual.debug_bounds("op1-button-0").unwrap().center();
    visual.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    advance(cx, handle, 80);
    assert_eq!(review(cx, handle), Some((false, "8".into())));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    assert!(
        received().is_none(),
        "a gesture belongs to the session pressed"
    );
    assert_eq!(review(cx, handle), Some((false, "8".into())));
}
