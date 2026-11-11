//! The first Root submission must mount and focus the session selected by a real producer.
use super::*;
use crate::g2::file_ops::model::Dialog;
use flea::ui_logic_file_ops::{
    confirm::Source,
    trash::{Review, Scope},
};
use gpui::{App, Entity, EntityId, Global, VisualTestContext};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug)]
struct Submission {
    captured: Option<EntityId>,
    mounted: Option<EntityId>,
    focus_owner_matches_mount: bool,
    mount_focused: bool,
}

#[derive(Default)]
struct FirstSubmission {
    captured: Cell<Option<Option<EntityId>>>,
    submitted: Cell<Option<Submission>>,
}

struct Probe(Rc<FirstSubmission>);
impl Global for Probe {}

pub(in crate::window) fn capture(overlay: Option<EntityId>, cx: &App) {
    if let Some(probe) = cx
        .try_global::<Probe>()
        .filter(|probe| probe.0.captured.get().is_none())
    {
        probe.0.captured.set(Some(overlay));
    }
}

pub(in crate::window) fn submit(shell: &Entity<Shell>, window: &mut Window, cx: &mut App) {
    let Some(probe) = cx.try_global::<Probe>().map(|probe| Rc::clone(&probe.0)) else {
        return;
    };
    if probe.submitted.get().is_some() {
        return;
    }
    let captured = probe.captured.get().expect("Root captured this submission");
    let submission = shell.update(cx, |shell, cx| {
        let mounted = shell.overlay.as_ref().map(Entity::entity_id);
        let focus = window.focused(cx);
        let mount_focused = shell
            .overlay
            .as_ref()
            .and_then(|overlay| overlay.read(cx).focus_handle())
            .is_some_and(|focus| focus.is_focused(window));
        // A no-op after moving focus proves the private focus_owner already equals the modal mount.
        window.focus(&shell.focus, cx);
        shell.focus_operation_mount(window, cx);
        let focus_owner_matches_mount = shell.focus.is_focused(window);
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
        Submission {
            captured,
            mounted,
            focus_owner_matches_mount,
            mount_focused,
        }
    });
    probe.submitted.set(Some(submission));
}

#[test]
fn gcomp_operations_first_submission_settles_queued_modal_after_backend_failure() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    let (first_backend, first_lines) = Backend::test_operation_channel();
    let (second_backend, _second_lines) = Backend::test_operation_channel();
    let first = edit(&mut cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().backend = Some(first_backend);
        shell.panes[1].as_mut().unwrap().backend = Some(second_backend);
        shell.copy_to_operation(0, 71, false, "/files", window, cx);
        shell.operation_view(0).unwrap()
    });
    frame(&mut cx, handle);
    let second = edit(&mut cx, handle, |shell, window, cx| {
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
        let second = shell.operation_view(1).unwrap();
        assert!(second.read(cx).modal(cx));
        assert_eq!(shell.overlay.as_ref(), Some(&first));
        second
    });
    frame(&mut cx, handle);
    assert!(first.read_with(&cx, |overlay, _| matches!(
        overlay.drawn_dialog(),
        Some(Dialog::CopyTo { menu: 71, .. })
    )));
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_input("/destination");
    let probe = Rc::new(FirstSubmission::default());
    cx.update(|cx| cx.set_global(Probe(Rc::clone(&probe))));
    drop(first_lines);
    visual.simulate_keystrokes("enter");
    assert_eq!(
        call(&mut cx, handle, "lastMessage"),
        Reply::value("The backend stopped accepting requests")
    );
    assert_eq!(call(&mut cx, handle, "statusError"), Reply::value("true"));
    assert_eq!(
        json(&mut cx, handle, "statusActivityState")
            .get("errors")
            .and_then(Json::as_f64),
        Some(1.)
    );

    let first_submission = probe.submitted.get().expect("the producer submitted Root");
    assert_eq!(first_submission.captured, Some(second.entity_id()));
    assert_eq!(first_submission.mounted, Some(second.entity_id()));
    assert!(
        first_submission.focus_owner_matches_mount,
        "operations.focus_owner must equal the captured modal on the first submission"
    );
    assert!(first_submission.mount_focused);
    assert!(!first.read_with(&visual, |overlay, _| overlay.model.open()));
    assert!(second.read_with(&visual, |overlay, _| matches!(
        overlay.drawn_dialog(),
        Some(Dialog::Delete { .. })
    )));
    edit(&mut cx, handle, |shell, _, cx| {
        assert_eq!(shell.overlay.as_ref(), Some(&second));
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let line = footer.line.read(cx);
        let primary = line
            .painted
            .get("primary")
            .expect("a painted error sentence");
        assert_eq!(
            primary.text.as_ref(),
            "The backend stopped accepting requests"
        );
        assert!(primary.bounds.size.width > gpui::px(0.));
        assert!(primary.bounds.size.height > gpui::px(0.));
    });
    println!("first Root submission: {first_submission:?}");
}
