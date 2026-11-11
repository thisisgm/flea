//! Network status roles preserve each in-flight connection and clear only their own terminal activity.
use super::*;

#[gpui::test]
fn ey439_a_busy_notice_keeps_the_existing_connection_activity(cx: &mut TestAppContext) {
    use crate::g2::network_shares::mount::Effect;
    use flea::ui_logic_device_rail::Note;
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            let catalog = shell.locale.catalog.clone();
            let opening = catalog.tr_args("network.bridge_starting", &["share"]);
            shell.network_service(
                &Effect::Starting {
                    note: Note::new("network.bridge_starting", &["share"]),
                    origin,
                },
                window,
                cx,
            );
            for key in ["network.another_opening", "network.unmounted"] {
                shell.network_service(
                    &Effect::Say {
                        note: Note::new(key, &[]),
                        origin,
                    },
                    window,
                    cx,
                );
                let line = shell.status_messages(cx, |messages, _| messages.line(&catalog, ""));
                assert!(line.busy, "{key} must not end the pending bridge");
                assert_eq!(line.primary, opening);
                assert!(!line.error);
            }
            shell.network_service(
                &Effect::Say {
                    note: Note::new("network.bridge_failed_timeout", &["share"]),
                    origin,
                },
                window,
                cx,
            );
            let line = shell.status_messages(cx, |messages, _| messages.line(&catalog, ""));
            assert!(!line.busy, "a terminal failure ends the bridge activity");
            assert!(line.error);
            shell.status_messages(cx, |messages, now| messages.dismiss(now));
            let stale = shell.network_origin(0);
            shell.network_service(
                &Effect::Starting {
                    note: Note::new("network.bridge_starting", &["share"]),
                    origin: stale,
                },
                window,
                cx,
            );
            shell.panes[0] = None;
            assert!(shell.network_side(stale).is_none());
            shell.network_service(
                &Effect::Say {
                    note: Note::new("network.bridge_failed_timeout", &["share"]),
                    origin: stale,
                },
                window,
                cx,
            );
            let line = shell.status_messages(cx, |messages, _| messages.line(&catalog, ""));
            assert!(
                !line.busy,
                "a stale terminal reply still ends its own activity"
            );
            assert!(
                !line.error,
                "a stale error must not become the new folder's message"
            );
            shell.stop_for_app_quit();
        });
    });
}

#[gpui::test]
fn ey439_an_unrelated_unmount_failure_keeps_the_connection_activity(cx: &mut TestAppContext) {
    use crate::g2::network_shares::mount::Effect;
    use flea::ui_logic_device_rail::Note;
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let opening = shell.network_origin(0);
            let unmount = shell.network_origin(0);
            assert_ne!(opening, unmount, "each dispatch receives its own origin");
            shell.network_service(
                &Effect::Starting {
                    note: Note::new("network.bridge_starting", &["share"]),
                    origin: opening,
                },
                window,
                cx,
            );
            shell.network_service(
                &Effect::Say {
                    note: Note::new("network.unmount_failed", &[]),
                    origin: unmount,
                },
                window,
                cx,
            );
            let catalog = shell.locale.catalog.clone();
            let failed = shell.status_messages(cx, |messages, _| messages.line(&catalog, ""));
            assert!(failed.error, "the unmount failure is still reported");
            shell.status_messages(cx, |messages, now| messages.dismiss(now));
            let resumed = shell.status_messages(cx, |messages, _| messages.line(&catalog, ""));
            assert!(resumed.busy, "the unrelated failure did not end the bridge");
            assert_eq!(
                resumed.primary,
                catalog.tr_args("network.bridge_starting", &["share"])
            );
            shell.stop_for_app_quit();
        });
    });
}

/// S2-11: the card's own failed connect says its reason in the footer; the connect that then succeeds retires it.
#[gpui::test]
fn s2_11_a_card_connect_that_succeeds_on_retry_clears_its_old_failure(cx: &mut TestAppContext) {
    use crate::g2::network_shares::mount::Effect;
    use flea::ui_logic_device_rail::Note;
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let catalog = shell.locale.catalog.clone();
            let failed = shell.network_origin(0);
            shell.network_service(
                &Effect::Say {
                    note: Note::new("feedback.connect.unopened", &[]),
                    origin: failed,
                },
                window,
                cx,
            );
            let line = shell.status_messages(cx, |messages, _| messages.line(&catalog, ""));
            assert!(line.error);
            assert_eq!(line.primary, catalog.tr("feedback.connect.unopened"));
            let unmount = shell.network_origin(0);
            shell.network_service(
                &Effect::Say {
                    note: Note::new("network.unmount_failed", &[]),
                    origin: unmount,
                },
                window,
                cx,
            );
            let retried = shell.network_origin(0);
            shell.network_service(
                &Effect::Open {
                    path: "/run/user/1000/gvfs/ftp:host=slot.test,user=tester".into(),
                    uri: "ftps://tester@slot.test/retry".into(),
                    label: "slot.test".into(),
                    origin: retried,
                },
                window,
                cx,
            );
            let errors = shell.status_messages(cx, |messages, _| {
                messages
                    .errors()
                    .iter()
                    .map(|entry| entry.text.clone())
                    .collect::<Vec<_>>()
            });
            assert_eq!(
                errors,
                [catalog.tr("network.unmount_failed")],
                "the connect retired its own failure and kept the unrelated unmount one"
            );
            shell.stop_for_app_quit();
        });
    });
}
