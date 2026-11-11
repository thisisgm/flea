//! Native completion receipts expose the painted Undo target; a standing error retains first Escape.
use super::native11_tests::{feedback_read, shares_on_other_pane};
use super::*;
use gpui::VisualContext;

fn activity(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    let value = feedback_read(shell, cx, "statusActivityState");
    // {"activities":[],"errors":0,"notice":"Moved 1 item","undoAvailable":true,"transferCard":{}}
    flea::jsondoc::parse(value.trim()).unwrap()
}

fn receive(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx);
        })
    });
    cx.run_until_parked();
    frame(cx);
}

#[gpui::test]
fn uish_misc_native12_transfer_receipt_has_a_real_transient_undo_target(cx: &mut TestAppContext) {
    for (n, done, failed_item, receipt) in [
        (
            1,
            r#"{"t":"transferdone","id":40,"ok":1,"failed":0}"#,
            false,
            Some("Moved 1 item"),
        ),
        (
            1,
            r#"{"t":"transferdone","id":40,"ok":1,"failed":0,"into":"/target","merged":1}"#,
            false,
            Some("Merged 1 item into /target"),
        ),
        (
            1,
            r#"{"t":"transferdone","id":40,"ok":0,"failed":1}"#,
            true,
            None,
        ),
        (
            2,
            r#"{"t":"transferdone","id":40,"ok":1,"failed":1}"#,
            true,
            Some("Moved 1 of 2 · 1 failed"),
        ),
    ] {
        let can_undo = receipt.is_some();
        let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
        cx.replace_root_view(|_, _| crate::window::root::Root {
            shell: shell.clone(),
        });
        frame(cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                assert!(shell.operation_overlay(0, window, cx).is_some());
            })
        });
        receive(
            &shell,
            cx,
            &format!(r#"{{"t":"transferstarted","id":40,"n":{n},"moving":true}}"#),
        );
        if failed_item {
            receive(
                &shell,
                cx,
                r#"{"t":"transferitem","id":40,"index":0,"name":"blocked.txt","ok":false,"err":"permission denied"}"#,
            );
        }
        receive(&shell, cx, done);
        if failed_item {
            assert_eq!(
                activity(&shell, cx).get("undoAvailable"),
                Some(&Json::Bool(false))
            );
            cx.simulate_keystrokes("escape");
            frame(cx);
        }
        let state = activity(&shell, cx);
        assert_eq!(state.get("errors"), Some(&Json::Num("0".into())));
        assert_eq!(
            state.get("undoAvailable"),
            Some(&Json::Bool(can_undo)),
            "actual completion {done} must own its footer target"
        );
        if let Some(receipt) = receipt {
            // v0.3.12 statusActivityState notice is bar.notice, the receipt with its own advice.
            let notice = format!("{receipt} · z undoes");
            assert_eq!(
                state.get("notice").and_then(Json::as_str),
                Some(notice.as_str())
            );
            assert!(!feedback_read(&shell, cx, "statusPrimary").contains("z undoes"));
            assert_eq!(
                feedback_read(&shell, cx, "lastMessage")
                    .matches("z undoes")
                    .count(),
                1
            );
        }
        cx.executor().advance_clock(Duration::from_millis(
            flea::ui_logic_status_feedback::messages::MESSAGE_MS + 1,
        ));
        cx.run_until_parked();
        frame(cx);
        assert_eq!(
            activity(&shell, cx).get("undoAvailable"),
            Some(&Json::Bool(false))
        );
        assert_eq!(
            activity(&shell, cx).get("notice").and_then(Json::as_str),
            Some("")
        );
    }
}

#[gpui::test]
fn uish_misc_native12_old_error_is_acknowledged_before_hidden_browser_selection_escape(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::List, window, cx);
            shell.network_say(0, "Earlier mount failed".into(), true, cx);
        })
    });
    frame(cx);
    cx.simulate_keystrokes("v");
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        [0]
    );
    assert_eq!(
        activity(&shell, cx).get("errors"),
        Some(&Json::Num("1".into()))
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(
        activity(&shell, cx).get("errors"),
        Some(&Json::Num("0".into()))
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        [0]
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .selected
        .is_empty()));
    let browser = super::native11_tests::share_state(&shell, cx);
    assert_eq!(browser.get("active"), Some(&Json::Bool(true)));
    assert_eq!(browser.get("owner"), Some(&Json::Num("1".into())));
}

#[gpui::test]
fn uish_misc_native12_card_closed_refocuses_past_other_pane_shares(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.focus_side, 0);
            shell.network_remember_focus();
            // The dismissed card held the keyboard, so nothing in the window is focused when Closed lands.
            window.blur(cx);
            shell.network_card(
                &crate::g2::network_shares::dialog::Effect::Closed,
                window,
                cx,
            );
        })
    });
    frame(cx);
    cx.update(|window, cx| {
        assert!(
            shell.read(cx).focus.is_focused(window),
            "a closed card returns the keyboard to the focused list past the other pane's shares"
        );
    });
    let browser = super::native11_tests::share_state(&shell, cx);
    assert_eq!(browser.get("active"), Some(&Json::Bool(true)));
}

#[gpui::test]
fn uish_misc_native12_card_closed_leaves_same_pane_shares_the_keyboard(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.focus_pane(1, cx);
            shell.network_remember_focus();
            window.blur(cx);
            shell.network_card(
                &crate::g2::network_shares::dialog::Effect::Closed,
                window,
                cx,
            );
        })
    });
    frame(cx);
    cx.update(|window, cx| {
        assert!(
            !shell.read(cx).focus.is_focused(window),
            "a closed card must not pull the keyboard under the focused pane's own shares"
        );
    });
}
