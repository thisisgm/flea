//! The shared paths pipe stays owned until its answer drains, including cancellation.
use super::m9_g2rows_25_rig::{answer, edit, errors, notice, open, riding};
use gpui::TestAppContext;

#[gpui::test]
fn m9_g2rows_25_pending_cancel_does_not_loan_its_reply_to_copy(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["old.txt", "new.txt"]);
    lines();
    edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx));
    assert_eq!(lines(), [r#"{"c":"paths","rows":[0]}"#]);
    edit(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1;
        shell.clipboard_action("copy", window, cx)
    });
    assert!(
        lines().iter().all(|line| !line.contains(r#""c":"paths""#)),
        "COPY_MUST_WAIT_FOR_CARRY_PATHS"
    );
    edit(cx, handle, |shell, window, cx| {
        shell.carry_escape(window, cx)
    });
    assert!(riding(cx).is_none());
    edit(cx, handle, |shell, window, cx| {
        shell.clipboard_action("copy", window, cx)
    });
    assert!(
        lines().iter().all(|line| !line.contains(r#""c":"paths""#)),
        "CANCELLED_ASK_MUST_DRAIN_BEFORE_COPY"
    );
    answer(cx, handle, &["/files/old.txt"]);
    assert!(riding(cx).is_none());
    assert!(
        lines().is_empty(),
        "CANCELLED_CARRY_ANSWER_MUST_NOT_PUBLISH_CLIPBOARD"
    );
    edit(cx, handle, |shell, window, cx| {
        shell.clipboard_action("copy", window, cx)
    });
    assert!(lines()
        .iter()
        .any(|line| line == r#"{"c":"paths","rows":[1]}"#));
    answer(cx, handle, &["/files/new.txt"]);
    let published = lines();
    assert!(published.iter().any(|line| line.contains("/files/new.txt")));
    assert!(published
        .iter()
        .all(|line| !line.contains("/files/old.txt")));
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_failed_send_leaves_no_phantom_resolver(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["a.txt"]);
    lines();
    let backend = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend.take()
    });
    edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx));
    assert!(lines().is_empty());
    assert!(!edit(cx, handle, |shell, window, cx| shell
        .carry_pending(0, window, cx)));
    assert!(riding(cx).is_none());
    assert_eq!(
        errors(cx, handle),
        ["Flea stopped responding. Reopen Flea and try again."],
        "A_FAILED_CARRY_SEND_IS_SAID"
    );
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = backend
    });
    edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx));
    assert_eq!(lines(), [r#"{"c":"paths","rows":[0]}"#]);
    answer(cx, handle, &["/files/a.txt"]);
    assert!(riding(cx).is_some());
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_destination_snapshot_changed_during_facts_keeps_the_payload(
    cx: &mut TestAppContext,
) {
    let (from, mut from_lines) = open(cx, "/source", &["a.txt"]);
    let (to, mut to_lines) = open(cx, "/dest", &["folder"]);
    from_lines();
    to_lines();
    edit(cx, from, |shell, window, cx| shell.carry_lift(window, cx));
    from_lines();
    answer(cx, from, &["/source/a.txt"]);
    edit(cx, to, |shell, window, cx| {
        assert!(shell.carry_land(0, "/dest/folder", true, false, window, cx));
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing + 1,
            ..Default::default()
        });
    });
    cx.run_until_parked();
    assert!(
        to_lines().is_empty(),
        "STALE_DESTINATION_MUST_NOT_ASK_AN_OPERATION"
    );
    assert!(
        riding(cx).is_some(),
        "STALE_CLICK_MUST_KEEP_CAPTURED_PAYLOAD"
    );
    edit(cx, from, |shell, _, _| shell.stop_for_app_quit());
    edit(cx, to, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_new_lift_in_another_window_withdraws_the_old_notice(cx: &mut TestAppContext) {
    let (old, mut old_lines) = open(cx, "/old", &["a.txt"]);
    let (new, mut new_lines) = open(cx, "/new", &["b.txt"]);
    old_lines();
    new_lines();
    edit(cx, old, |shell, window, cx| shell.carry_lift(window, cx));
    old_lines();
    answer(cx, old, &["/old/a.txt"]);
    assert_eq!(notice(cx, old), "Carrying 1 item");
    edit(cx, new, |shell, window, cx| shell.carry_lift(window, cx));
    new_lines();
    answer(cx, new, &["/new/b.txt"]);
    super::m9_g2rows_25_rig::pending_frame(cx, old);
    assert_eq!(
        notice(cx, old),
        "",
        "REPLACED_CARRY_MUST_WITHDRAW_ITS_SOURCE_NOTICE"
    );
    assert_eq!(notice(cx, new), "Carrying 1 item");
    edit(cx, old, |shell, _, _| shell.stop_for_app_quit());
    edit(cx, new, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_window_end_keeps_the_slot_until_the_late_answer(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["a.txt"]);
    lines();
    edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx));
    assert_eq!(lines(), [r#"{"c":"paths","rows":[0]}"#]);
    let gate = |cx: &mut TestAppContext| {
        edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().nav.gate.clip_pending
        })
    };
    assert!(gate(cx), "A_LIFT_OWNS_THE_SLOT");
    assert!(edit(cx, handle, |shell, window, cx| shell
        .carry_window_ended(window, cx)));
    assert!(gate(cx), "A_WINDOW_END_KEEPS_THE_SLOT_UNTIL_THE_ANSWER");
    edit(cx, handle, |shell, window, cx| {
        shell.clipboard_action("copy", window, cx)
    });
    assert!(
        lines().iter().all(|line| !line.contains(r#""c":"paths""#)),
        "COPY_WAITS_FOR_THE_OWED_CARRY_ANSWER"
    );
    assert_eq!(
        notice(cx, handle),
        "Still resolving the last selection; try again.",
        "COPY_IS_REFUSED_ALOUD_WHILE_THE_CARRY_ANSWER_IS_OWED"
    );
    answer(cx, handle, &["/files/a.txt"]);
    assert!(riding(cx).is_none(), "THE_OWED_ANSWER_RIDES_NOWHERE");
    assert!(
        lines().is_empty(),
        "THE_OWED_ANSWER_IS_NOT_PUBLISHED_AS_A_COPY"
    );
    assert!(!gate(cx), "THE_LATE_ANSWER_FREES_THE_SLOT");
    edit(cx, handle, |shell, window, cx| {
        shell.clipboard_action("copy", window, cx)
    });
    assert!(
        lines()
            .iter()
            .any(|line| line == r#"{"c":"paths","rows":[0]}"#),
        "COPY_WORKS_ONCE_THE_SLOT_IS_FREE"
    );
    answer(cx, handle, &["/files/a.txt"]);
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_window_end_then_copy_never_takes_the_carry_answer(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["old.txt", "new.txt"]);
    lines();
    edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx));
    assert_eq!(lines(), [r#"{"c":"paths","rows":[0]}"#]);
    edit(cx, handle, |shell, window, cx| {
        shell.carry_window_ended(window, cx)
    });
    edit(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1;
        shell.clipboard_action("copy", window, cx)
    });
    answer(cx, handle, &["/files/old.txt"]);
    let published = lines();
    assert!(
        published
            .iter()
            .all(|line| !line.contains("/files/old.txt")),
        "COPY_MUST_NOT_TAKE_THE_CARRY_ANSWER"
    );
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_failed_image_paste_is_said(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["a.txt"]);
    lines();
    edit(cx, handle, |shell, _, cx| {
        shell.panes[0].as_mut().unwrap().backend = None;
        let ask = flea::ui_logic_dg1::PasteAsk::Image {
            dest: "/files".into(),
            name: "image.png".into(),
        };
        shell.clipboard_image(0, &ask, cx);
    });
    assert_eq!(
        errors(cx, handle),
        ["Flea stopped responding. Reopen Flea and try again."],
        "A_FAILED_IMAGE_PASTE_IS_SAID"
    );
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_closed_source_drains_and_frees_the_carry(cx: &mut TestAppContext) {
    let (old, mut old_lines) = open(cx, "/old", &["a.txt"]);
    let (new, mut new_lines) = open(cx, "/new", &["b.txt"]);
    old_lines();
    new_lines();
    edit(cx, old, |shell, window, cx| shell.carry_lift(window, cx));
    assert_eq!(old_lines(), [r#"{"c":"paths","rows":[0]}"#]);
    edit(cx, old, |shell, window, cx| {
        shell.carry_window_ended(window, cx);
        shell.close(window, cx);
    });
    edit(cx, old, |shell, window, cx| {
        shell.receive(0, crate::backend::Event::Drained, window, cx)
    });
    let active = cx.update(|cx| {
        cx.try_global::<super::carry::Carrying>()
            .is_some_and(|carrying| carrying.carry.active())
    });
    assert!(!active, "CLOSED_SOURCE_DRAIN_MUST_FREE_THE_CARRY");
    edit(cx, new, |shell, window, cx| shell.carry_lift(window, cx));
    assert_eq!(new_lines(), [r#"{"c":"paths","rows":[0]}"#]);
    edit(cx, new, |shell, _, _| shell.stop_for_app_quit());
}
