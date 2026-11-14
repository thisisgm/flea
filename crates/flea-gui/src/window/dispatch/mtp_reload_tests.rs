//! Busy-folder Reload follows the pending request through the native shell and its focus ring.
use super::{asked, asked_on, land, lists, open, row, set_cursor};
use crate::{
    backend::Event, g2::status_feedback::state::Reload, state::Shell,
    window::startup::LISTING_WAIT_MS,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use std::time::Duration;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

pub(super) fn wait(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    draw(cx);
}

pub(super) fn notice(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.update(cx, |shell, cx| {
        let catalog = shell.locale.catalog.clone();
        shell.status_messages(cx, |messages, _| messages.notice_text(&catalog))
    })
}

fn focus(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| shell.read(cx).focus_view(window, cx).to_string())
}

fn pending(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("phone", true), row("local", true)],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("enter");
    assert_eq!(asked(&shell, cx), "/files/phone");
    wait(cx);
    assert!(cx.debug_bounds("fb1-reload").is_some());
    (shell, cx)
}

#[gpui::test]
fn mtp_reload_click_retries_the_pending_folder_once_and_restores_focus(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    let before = lists(&shell, cx);
    let at = cx.debug_bounds("fb1-reload").unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    assert_eq!(lists(&shell, cx), before + 1);
    assert_eq!(asked(&shell, cx), "/files/phone");
    assert!(cx.debug_bounds("fb1-reload").is_none());
    let commands = super::sent();
    let retry = commands
        .iter()
        .rev()
        .find(|line| flea::json::field_str(line, "c").as_deref() == Some("list"))
        .unwrap();
    assert!(
        flea::json::field_bool(retry, "wantChanged"),
        "the pending-folder retry carries the existing manual Reload flag"
    );
    assert_eq!(
        focus(&shell, cx),
        "list",
        "the removed control hands focus to its list"
    );
    cx.simulate_click(at, Modifiers::none());
    cx.simulate_keystrokes("f5 ctrl-r");
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "duplicate activation cannot queue more retries"
    );
    land(&shell, cx, "/files/phone", vec![row("photo.jpg", false)], 2);
    assert!(
        !notice(&shell, cx).starts_with("Reloaded"),
        "C1: the first rows of a folder being opened are no change"
    );
    assert!(shell.read_with(cx, |shell, _| shell.input.handle("reload-left").is_none()));
    // A click queued on the removed control reaches the shell after its state is gone.
    let control = shell.read_with(cx, |shell, _| shell.surfaces[0].notice().unwrap().clone());
    control.update(cx, |_, cx| cx.emit(Reload));
    cx.run_until_parked();
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "a stale activation never reloads the folder that replaced the waiting state"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_forward_into_a_waiting_place_retries_without_a_change_count(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, crate::window::navigate::Step::Back, window, cx),
                Ok(true)
            );
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![row("phone", true), row("local", true)],
        2,
    );
    assert_eq!(asked(&shell, cx), "/files");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, crate::window::navigate::Step::Forward, window, cx),
                Ok(true)
            );
        })
    });
    assert_eq!(asked(&shell, cx), "/files/phone");
    wait(cx);
    let before = lists(&shell, cx);
    let at = cx.debug_bounds("fb1-reload").unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    assert_eq!(lists(&shell, cx), before + 1);
    land(&shell, cx, "/files/phone", vec![row("photo.jpg", false)], 3);
    assert_eq!(
        notice(&shell, cx),
        "",
        "C1: Forward into a waiting place counts no change"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_tab_enter_space_and_keyboard_reload_share_the_same_action(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    let mut before = lists(&shell, cx);
    for key in ["enter", "space", "ctrl-r", "f5"] {
        // The button follows the chrome in the ordinary ring and precedes the listing in reverse.
        cx.simulate_keystrokes("shift-tab");
        let stop = cx.update(|window, cx| shell.read(cx).focus_view(window, cx));
        assert_eq!(stop, "reload-left");
        cx.simulate_keystrokes(key);
        draw(cx);
        before += 1;
        assert_eq!(
            lists(&shell, cx),
            before,
            "{key} runs the existing Reload action"
        );
        assert_eq!(asked(&shell, cx), "/files/phone");
        wait(cx);
    }
    cx.simulate_keystrokes("shift-tab tab");
    assert_eq!(
        cx.update(|window, cx| shell.read(cx).focus_view(window, cx)),
        "list"
    );
    land(&shell, cx, "/files/phone", vec![row("photo.jpg", false)], 2);
    assert_eq!(
        notice(&shell, cx),
        "",
        "C1: keyboard retries count no change"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_background_answer_and_navigation_remove_the_control(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    let before = lists(&shell, cx);
    cx.simulate_keystrokes("shift-tab");
    land(
        &shell,
        cx,
        "/files/phone",
        vec![row("automatic.jpg", false)],
        2,
    );
    assert_eq!(
        lists(&shell, cx),
        before,
        "automatic recovery needs no Reload"
    );
    assert_eq!(notice(&shell, cx), "");
    assert!(cx.debug_bounds("fb1-reload").is_none());
    assert_eq!(
        cx.update(|window, cx| shell.read(cx).focus_view(window, cx)),
        "list"
    );
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/files", vec![row("phone", true)], 3);
    cx.simulate_keystrokes("enter");
    wait(cx);
    assert_eq!(asked(&shell, cx), "/files/phone");
    assert!(cx.debug_bounds("fb1-reload").is_some());
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(focus(&shell, cx), "reload-left");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(
                0,
                crate::window::navigate::Step::Open("/other".into()),
                window,
                cx,
            );
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("fb1-reload").is_none());
    land(&shell, cx, "/other", vec![row("safe.txt", false)], 4);
    assert_eq!(asked(&shell, cx), "/other");
    assert!(shell.read_with(cx, |shell, _| shell.input.handle("reload-left").is_none()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_permission_and_real_errors_keep_their_original_state(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::Refused {
                    operation: "scan".into(),
                    path: "/files/phone".into(),
                    message: "Permission denied (os error 13)".into(),
                    mode: 0o40700,
                },
                window,
                cx,
            )
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("fb1-reload").is_none());
    assert!(cx.debug_bounds("fb1-state-detail").is_some());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(
                    0,
                    crate::window::navigate::Step::Open("/files/phone".into()),
                    window,
                    cx
                ),
                Ok(true)
            );
        })
    });
    wait(cx);
    assert!(cx.debug_bounds("fb1-reload").is_some());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::Refused {
                    operation: "scan".into(),
                    path: "/files/phone".into(),
                    message: "Input/output error (os error 5)".into(),
                    mode: 0,
                },
                window,
                cx,
            )
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("fb1-reload").is_none());
    assert!(cx.debug_bounds("fb1-state-detail").is_none());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_grid_uses_the_approved_waiting_recipe(cx: &mut TestAppContext) {
    let (shell, cx) = super::open_with(cx, r#""keys":"default","view":"grid""#, None);
    wait(cx);
    assert!(cx.debug_bounds("fb1-reload").is_some());
    let metrics = shell.read_with(cx, |shell, cx| {
        shell.surfaces[0].notice().unwrap().read(cx).metrics()
    });
    assert_eq!(metrics, crate::g2::status_feedback::state::LIST);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_waiting_footer_persists_and_clears_with_automatic_recovery(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    let footer = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let drawn = footer.line.read(cx).drawn();
            (
                drawn.primary.to_string(),
                drawn.error,
                drawn.hint.to_string(),
            )
        })
    };
    let expected = (
        "That folder is not responding.".to_owned(),
        true,
        String::new(),
    );
    assert_eq!(footer(cx), expected);
    cx.executor().advance_clock(Duration::from_secs(12));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(
        footer(cx),
        expected,
        "the state outlives an ordinary error notice"
    );
    land(
        &shell,
        cx,
        "/files/phone",
        vec![row("automatic.jpg", false)],
        2,
    );
    assert_ne!(footer(cx).0, expected.0);
    assert!(!footer(cx).1);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_dual_tab_ring_targets_each_pending_pane(cx: &mut TestAppContext) {
    use super::{land_on, open_with};
    let (shell, cx) = open_with(cx, r#""keys":"default","view":"dual""#, None);
    for side in 0..2 {
        land_on(&shell, cx, side, "/files", vec![row("phone", true)], 1, 1);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let _ = shell.step(
                    side,
                    crate::window::navigate::Step::Open(format!("/phone-{side}")),
                    window,
                    cx,
                );
            })
        });
    }
    wait(cx);
    for (keys, expected) in [
        ("tab", "reload-left"),
        ("tab", "list"),
        ("tab", "reload-right"),
        ("shift-tab", "list"),
        ("shift-tab", "reload-left"),
    ] {
        cx.simulate_keystrokes(keys);
        assert_eq!(
            cx.update(|window, cx| shell.read(cx).focus_view(window, cx)),
            expected
        );
    }
    cx.simulate_keystrokes("tab tab");
    assert_eq!(focus(&shell, cx), "reload-right");
    let right = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell.panes[1].as_ref().unwrap().nav.counters.lists
        })
    };
    let before = right(cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        right(cx),
        before + 1,
        "Enter on the right control retries the right pane"
    );
    assert_eq!(asked_on(&shell, cx, 1), "/phone-1");
    assert!(shell.read_with(cx, |shell, _| !shell.panes[1].as_ref().unwrap().waiting));
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().waiting));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
