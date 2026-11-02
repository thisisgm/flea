//! The file cue and the rail share one status owner, so each side retires only its own line.
use super::*;
use crate::g2::dg2::mounted_tests::{open, Root};
use gpui::TestAppContext;

fn shown(shell: &mut Shell, cx: &mut Context<Shell>) -> Option<String> {
    shell.status_messages(cx, |messages, _| {
        messages
            .activities()
            .iter()
            .find(|entry| entry.owner == super::rail_drop::DRAG_LINE)
            .map(|entry| entry.text.clone())
    })
}

#[gpui::test]
fn vf_look_review_empty_drag_hint_clears_a_rail_line(cx: &mut TestAppContext) {
    let root = Root::new("hint-clear");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            let key = window_key(window);
            shell.show_rail_cue(
                key,
                None,
                Some((String::new(), "Move 1 item to Downloads".into())),
                cx,
            );
            assert_eq!(
                shown(shell, cx).as_deref(),
                Some("Move 1 item to Downloads")
            );
            assert!(!cx.global::<Drags>().said.contains_key(&key));
            shell.publish_file_line(window, String::new(), cx);
            assert_eq!(
                shown(shell, cx),
                None,
                "an empty line clears an uncached rail line"
            );
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_review_drag_hint_retries_after_running_activity(cx: &mut TestAppContext) {
    let root = Root::new("hint-retry");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.set_activity(9, "Copying file", 9, true, false, now);
            });
            let hint = "Move 1 item to alpha";
            shell.publish_file_line(window, hint.into(), cx);
            assert!(!shell.status_messages(cx, |messages, _| {
                messages.activities().iter().any(|entry| entry.text == hint)
            }));
            assert!(
                !cx.global::<Drags>().said.contains_key(&window_key(window)),
                "a hint held back by a transfer is not cached"
            );
            shell.status_messages(cx, |messages, now| {
                messages.set_activity(9, "", 9, false, false, now);
            });
            shell.publish_file_line(window, hint.into(), cx);
            assert!(
                shell.status_messages(cx, |messages, _| {
                    messages.activities().iter().any(|entry| entry.text == hint)
                }),
                "the next refresh publishes a hint a transfer held back"
            );
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_review_window_teardown_removes_cached_drag_hint(cx: &mut TestAppContext) {
    let root = Root::new("hint-teardown");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            let key = window_key(window);
            shell.publish_file_line(window, "Move 1 item to alpha".into(), cx);
            assert!(cx.global::<Drags>().said.contains_key(&key));
            shell.forget_file_drags(window, cx);
            assert!(!cx.global::<Drags>().said.contains_key(&key));
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_review_file_retire_keeps_the_rail_line(cx: &mut TestAppContext) {
    let root = Root::new("retire-rail");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            let key = window_key(window);
            // A rail folder row can say its line in the very words the file cue said.
            for rail in ["Move 1 item to Trash", "Move 1 item to alpha"] {
                shell.publish_file_line(window, "Move 1 item to alpha".into(), cx);
                cx.default_global::<Drags>().cue_lines.insert(key);
                shell.show_rail_cue(key, None, Some((String::new(), rail.into())), cx);
                shell.settle_file_feedback(window, cx);
                assert_eq!(shown(shell, cx).as_deref(), Some(rail));
                assert!(!cx.global::<Drags>().said.contains_key(&key));
            }
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_review_file_retire_clears_its_own_line(cx: &mut TestAppContext) {
    let root = Root::new("retire-own");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            let key = window_key(window);
            shell.publish_file_line(window, "Move 1 item to alpha".into(), cx);
            cx.default_global::<Drags>().cue_lines.insert(key);
            shell.settle_file_feedback(window, cx);
            assert_eq!(shown(shell, cx), None);
            assert!(!cx.global::<Drags>().said.contains_key(&key));
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_review_rail_clear_lets_the_same_file_line_return(cx: &mut TestAppContext) {
    let root = Root::new("rail-clear");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            let hint = "Move 1 item to alpha";
            shell.publish_file_line(window, hint.into(), cx);
            shell.status_messages(cx, |messages, now| {
                messages.set_activity(super::rail_drop::DRAG_LINE, "", 0, false, false, now);
            });
            assert_eq!(shown(shell, cx), None, "the rail clears the shared owner");
            shell.publish_file_line(window, hint.into(), cx);
            assert_eq!(shown(shell, cx).as_deref(), Some(hint));
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_review_suppressed_rail_cue_leaves_the_file_line_its_own(cx: &mut TestAppContext) {
    let root = Root::new("rail-suppressed");
    let (handle, _) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| {
            let key = window_key(window);
            let hint = "Move 1 item to alpha";
            shell.publish_file_line(window, hint.into(), cx);
            cx.default_global::<Drags>().cue_lines.insert(key);
            shell.status_messages(cx, |messages, now| {
                messages.set_activity(9, "Copying file", 9, true, false, now);
            });
            let trash = Some((String::new(), "Move 1 item to Trash".into()));
            shell.show_rail_cue(key, None, trash, cx);
            assert_eq!(
                shown(shell, cx).as_deref(),
                Some(hint),
                "a running transfer holds the rail line back"
            );
            shell.settle_file_feedback(window, cx);
            assert_eq!(
                shown(shell, cx),
                None,
                "the file cue still retires its own line"
            );
        })
        .unwrap();
}
