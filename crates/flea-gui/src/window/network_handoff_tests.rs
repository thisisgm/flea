//! Completed remote opens and overlapping card/browser closes preserve their real focus owner.
use super::*;
use gpui::VisualContext;

pub(super) fn focus_rail(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.rail_focus(true, cx);
            shell.keyboard.context = flea::keys::Context::Rail;
            window.focus(&shell.dual_focus.rail, cx);
        });
    });
    frame(cx);
}

#[gpui::test]
fn uish_misc_card_close_preserves_browser_focus_and_return_origin(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    focus_rail(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    let card = cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .clone()
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = card.read(cx).model.origin;
            shell.network_service(
                &Effect::Shares {
                    uri: "smb://bare.test/".into(),
                    label: "Bare".into(),
                    names: vec!["share".into()],
                    origin,
                },
                window,
                cx,
            );
        });
    });
    frame(cx);
    cx.update(|window, cx| {
        let browser = &shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares;
        assert!(
            browser.read(cx).inside(window, cx),
            "the browser took actual focus"
        );
    });
    cx.update(|_, cx| card.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    cx.update(|window, cx| {
        let browser = &shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares;
        assert!(browser.read(cx).model.open);
        assert!(
            browser.read(cx).inside(window, cx),
            "closing the completed card must not steal browser focus"
        );
    });
    cx.simulate_keystrokes("escape");
    frame(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
        assert!(
            shell.dual_focus.rail.contains_focused(window, cx),
            "browser Escape returns to its original rail"
        );
    });
}

#[gpui::test]
fn uish_misc_corrected_saved_location_retry_hands_focus_to_opened_list(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    cx.replace_root_view(|_, _| crate::window::root::Root {
        shell: shell.clone(),
    });
    focus_rail(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            shell.network_service(
                &Effect::Retry {
                    uri: "smb://tester@retry.test/share".into(),
                    label: "Share".into(),
                    origin,
                    error: "network.password_needed",
                    args: vec![],
                    connect_failed: false,
                    password: Secret::new(),
                },
                window,
                cx,
            );
        });
    });
    frame(cx);
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert!(
        recorder.find("35").is_empty(),
        "empty Retry starts no helper"
    );
    cx.simulate_keystrokes("tab tab tab tab tab");
    frame(cx);
    cx.simulate_input("synthetic");
    cx.simulate_keystrokes("enter");
    frame(cx);
    let auth = *recorder
        .find("35")
        .last()
        .expect("retry starts the real auth job");
    recorder.answer(auth, 0, "");
    cx.run_until_parked();
    let info = *recorder.find("info").last().unwrap();
    recorder.answer(info, 0, "local path: /fixture/mounted\n");
    frame(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(
            !shell
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .card
                .read(cx)
                .model
                .opened
        );
        assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
        assert!(
            shell.focus.contains_focused(window, cx),
            "completed saved-location retry focuses its opened list"
        );
        assert!(
            !shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .read(cx)
                .model
                .borrow()
                .focused,
            "the live rail producer releases focus after the open"
        );
    });
}

#[gpui::test]
fn uish_misc_late_network_open_preserves_current_card_or_other_pane_focus(cx: &mut TestAppContext) {
    for other_pane in [false, true] {
        let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
        focus_rail(&shell, cx);
        if !other_pane {
            cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
            frame(cx);
        }
        let focused = cx.update(|window, cx| window.focused(cx));
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let origin = shell.network_origin(usize::from(other_pane));
                shell.network_service(
                    &Effect::Open {
                        path: "/fixture/late".into(),
                        uri: "smb://late.test/share".into(),
                        label: "Late".into(),
                        origin,
                    },
                    window,
                    cx,
                );
            });
        });
        frame(cx);
        cx.update(|window, cx| {
            assert_eq!(window.focused(cx), focused, "other_pane={other_pane}");
            assert_eq!(
                shell.read(cx).keyboard.context,
                flea::keys::Context::Rail,
                "a delayed open must keep the current focus owner"
            );
        });
    }
}
