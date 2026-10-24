//! A backend failure closes the old card without a keyboard release callback.
use super::{field_tests::open, model::Dialog};
use crate::{backend::Backend, window::pane::PaneHost};
use flea::ui_logic_op1::{
    confirm::Source,
    trash::{Review, Scope},
};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[gpui::test]
fn u13_join_backend_failure_then_queued_modal_restores_listing_focus(cx: &mut TestAppContext) {
    let (handle, first, first_lines) = open(cx, "/files");
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
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(first.read_with(&visual, |view, _| matches!(
        view.drawn_dialog(),
        Some(Dialog::CopyTo { menu: 71, .. })
    )));

    // Closing the real request receiver makes submission fail in flush_operations.
    drop(first_lines);
    visual.simulate_input("/destination");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!first.read_with(&visual, |view, _| view.model.open()));
    assert!(second.read_with(&visual, |view, _| matches!(
        view.drawn_dialog(),
        Some(Dialog::Delete { .. })
    )));
    let second_focus = second.read_with(&visual, |view, _| view.focus_handle().unwrap());
    assert!(visual.update(|window, _| second_focus.is_focused(window)));

    visual.simulate_keystrokes("escape");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!second.read_with(&visual, |view, _| view.model.open()));
    let closed = second_lines().expect("the queued card closes on its own backend");
    assert!(closed.contains("\"id\":73") && closed.contains("\"op\":\"close\""));
    let before = handle
        .update(&mut visual, |root, window, cx| {
            let shell = Entity::read(&root.shell, cx);
            assert!(
                shell.focus.is_focused(window),
                "a closed editor retained focus"
            );
            shell.panes[0].as_ref().unwrap().pane.cursor
        })
        .unwrap();
    visual.simulate_keystrokes("j");
    let after = handle
        .update(&mut visual, |root, _, cx| {
            Entity::read(&root.shell, cx).panes[0]
                .as_ref()
                .unwrap()
                .pane
                .cursor
        })
        .unwrap();
    assert_eq!(
        after,
        before + 1,
        "listing keys must work after both cards close"
    );
}
