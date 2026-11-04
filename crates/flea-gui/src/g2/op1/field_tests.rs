//! Copy To is exercised through the mounted root and GPUI's text dispatch.
use super::{
    atomic_tests::open_with,
    host::Overlay,
    model::{CopyFocus, Dialog, Operations},
};
use crate::{backend::Backend, window::root::Root};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowHandle};

pub(super) fn open(
    cx: &mut TestAppContext,
    path: &str,
) -> (
    WindowHandle<Root>,
    Entity<Overlay>,
    impl FnMut() -> Option<String>,
) {
    let (backend, received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    let overlay = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.copy_to_operation(0, 71, false, path, window, cx);
                shell.operation_view(0).unwrap()
            })
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    (handle, overlay, received)
}

pub(super) fn state(view: &Entity<Overlay>, cx: &VisualTestContext) -> (String, CopyFocus, bool) {
    view.read_with(cx, |view, _| match &view.model.dialog {
        Some(Dialog::CopyTo {
            path, focus, busy, ..
        }) => (path.clone(), *focus, *busy),
        other => panic!("expected mounted Copy To, got {other:?}"),
    })
}

#[gpui::test]
fn u13_fix3_mounted_typing_replaces_the_initial_selection(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_input("/new");
    assert_eq!(
        state(&view, &visual).0,
        "/new",
        "the mounted field must receive platform text"
    );
}

#[test]
fn u13_fix3_relative_copy_is_available_but_submission_still_refuses() {
    assert!(
        Operations::copy_enabled("work"),
        "availability follows nonempty text"
    );
    assert!(!Operations::copy_enabled(""));
    let catalog = crate::test_catalogs::english();
    let mut model = Operations::new("default");
    model.copy_to(71, false, "work");
    model.key(&catalog, "enter", false);
    assert!(model.open() && model.outbox.is_empty());
    assert!(matches!(
        &model.dialog,
        Some(Dialog::CopyTo { error, .. }) if error == "Enter a full folder path, starting with /."
    ));
    assert!(model.said.is_empty(), "the refusal stays on the card");
}

#[gpui::test]
fn u13_fix3_empty_tab_and_backtab_skip_disabled_copy(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("tab");
    assert_eq!(state(&view, &visual).1, CopyFocus::Cancel);
    visual.simulate_keystrokes("tab");
    assert_eq!(
        state(&view, &visual).1,
        CopyFocus::Field,
        "disabled Copy must not be a keyboard stop"
    );
    visual.simulate_keystrokes("shift-tab");
    assert_eq!(state(&view, &visual).1, CopyFocus::Cancel);
    visual.simulate_keystrokes("shift-tab");
    assert_eq!(state(&view, &visual).1, CopyFocus::Field);
}

#[gpui::test]
fn u13_fix3_selection_replacement_and_literal_hjkl(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("end shift-left shift-left");
    visual.simulate_input("hjkl");
    assert_eq!(state(&view, &visual).0, "/filhjkl");
    visual.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(state(&view, &visual).0, "");
    visual.simulate_input("relative");
    visual.simulate_keystrokes("tab tab");
    assert_eq!(state(&view, &visual).1, CopyFocus::Confirm);
    visual.simulate_keystrokes("enter");
    assert_eq!(state(&view, &visual).0, "relative");
    assert!(view.read_with(&visual, |v, _| matches!(
        &v.model.dialog,
        Some(Dialog::CopyTo { error, .. }) if error == "Enter a full folder path, starting with /."
    )));
    assert!(view.read_with(&visual, |v, _| v.model.said.is_empty()));
}

#[gpui::test]
fn u13_fix3_copy_acquires_and_cancel_restores_window_focus(cx: &mut TestAppContext) {
    let (handle, view, mut received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let shell_focus = handle
        .update(&mut visual, |root, _, cx| {
            Entity::read(&root.shell, cx).focus.clone()
        })
        .unwrap();
    assert!(
        !visual.update(|window, _| shell_focus.is_focused(window)),
        "Copy To must own real window focus"
    );
    visual.simulate_keystrokes("escape");
    assert!(!view.read_with(&visual, |v, _| v.model.open()));
    assert!(visual.update(|window, _| shell_focus.is_focused(window)));
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":71}"#)
    );
    assert!(received().is_none());
}

#[gpui::test]
fn u13_fix3_absent_prior_focus_never_restores_the_closed_editor(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    handle
        .update(cx, |root, window, cx| {
            window.blur(cx);
            root.shell.update(cx, |shell, cx| {
                shell.copy_to_operation(0, 71, false, "", window, cx)
            });
        })
        .unwrap();
    // A second draw must not mistake the editor's own acquired focus for its restoration target.
    for _ in 0..2 {
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    assert!(
        visual.update(|window, cx| window.focused(cx).is_none()),
        "a closed editor must release focus when no prior target existed"
    );
}
