//! AirDrop's runs against a stand-in owly: the browse loop, two windows on one radio, and what the receiver saved.
use super::airdrop::{Proc, Radio};
use super::airdrop_shared::Shared;
use crate::{backend::Backend, state::tests::shell_with};
use flea::ui_logic_send_share::airdrop::{Adapter, Readiness};
use gpui::{TestAppContext, VisualTestContext};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// A stand-in owly in a marked root: logs argv, radio comes up, browse exits 0 at once, receive saves one file then fails.
fn stand_in(tag: &str) -> (PathBuf, String, PathBuf) {
    let root = std::env::temp_dir().join(format!("flea-owly-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    let log = root.join("argv.log");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{log}'\ncase \"$1\" in\n  radio) echo 'awdl0 is up: fe80::1%awdl0 (02:00:00:00:00:01). Ctrl-C takes it down.' >&2; exec sleep 5 ;;\n  receive) echo \"$3/IMG_0001.HEIC\"; sleep 0.5; echo 'transfer failed: connection reset' >&2; exec sleep 5 ;;\n  browse) exit 0 ;;\nesac\n",
        log = log.display()
    );
    let owly = root.join("owly");
    std::fs::write(&owly, script).unwrap();
    std::fs::set_permissions(&owly, std::fs::Permissions::from_mode(0o755)).unwrap();
    (root.clone(), owly.to_string_lossy().into_owned(), log)
}

fn count(log: &PathBuf, verb: &str) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with(verb))
        .count()
}

fn ready() -> Adapter {
    Adapter {
        readiness: Readiness::Ready,
        card: String::new(),
    }
}

fn settle(cx: &mut VisualTestContext, for_ms: u64) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(for_ms) {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui::test]
fn a_browse_that_exits_at_once_does_not_respawn_in_a_tight_loop(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, owly, log) = stand_in("browse");
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    shell.update_in(cx, |shell, window, cx| {
        let air = &mut shell.menu.providers.airdrop;
        air.facts(&owly);
        air.adapter = ready();
        air.radio = Radio::On;
        let acts = air.menu_opened(super::join::now_ms());
        shell.airdrop_acts(0, acts, window, cx);
    });
    settle(cx, 2_000);
    shell.update(cx, |shell, _| {
        shell.menu.providers.airdrop.menu_closed();
        shell.menu.providers.airdrop_pids.stop(Proc::Browse);
    });
    cx.run_until_parked();
    let browses = count(&log, "browse");
    let _ = std::fs::remove_dir_all(&root);
    // owly's browse runs --seconds 30, so one start in two seconds is the budget.
    assert_eq!(
        browses, 1,
        "{browses} owly browse runs in 2 s with the menu open"
    );
}

#[gpui::test]
fn two_windows_never_run_two_radios_and_the_second_reads_the_first(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, owly, log) = stand_in("two");
    let machine = root.join("machine");
    let mut windows = Vec::new();
    for _ in 0..2 {
        let (backend, lines) = Backend::test_lines();
        windows.push((cx.add_window(|_, cx| shell_with(backend, cx)), lines));
    }
    let mut receiving = Vec::new();
    for (window, _) in &windows {
        window
            .update(cx, |shell, window, cx| {
                let air = &mut shell.menu.providers.airdrop;
                air.shared = Shared::at(machine.clone());
                air.facts(&owly);
                air.adapter = ready();
                let acts = air.turn_on(&root.to_string_lossy(), &shell.locale.catalog);
                shell.airdrop_acts(0, acts, window, cx);
                receiving.push(shell.menu.providers.airdrop.page("/tmp", "host").receiving);
            })
            .unwrap();
    }
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    let (radios, receivers) = (count(&log, "radio up"), count(&log, "receive"));
    for (window, _) in &windows {
        let _ = window.update(cx, |shell, window, cx| {
            let acts = shell.menu.providers.airdrop.turn_off();
            shell.airdrop_acts(0, acts, window, cx);
        });
    }
    cx.run_until_parked();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        (radios, receivers),
        (1, 1),
        "one radio and one receiver for the machine"
    );
    assert_eq!(receiving, [true, true], "both pages show Receiving on");
}

#[gpui::test]
fn what_the_receiver_saved_and_what_failed_after_accept_reach_the_footer(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, owly, _log) = stand_in("receive");
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    shell.update_in(cx, |shell, window, cx| {
        let air = &mut shell.menu.providers.airdrop;
        air.facts(&owly);
        air.adapter = ready();
        let acts = air.turn_on(&root.to_string_lossy(), &shell.locale.catalog);
        shell.airdrop_acts(0, acts, window, cx);
    });
    let said = |cx: &mut VisualTestContext| {
        shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, _| {
                let errors: Vec<String> =
                    messages.errors().iter().map(|e| e.text.clone()).collect();
                (messages.notice().to_string(), errors)
            })
        })
    };
    const SAVED: &str = "Received IMG_0001.HEIC with AirDrop";
    let started = Instant::now();
    let (mut saw_saved, mut errors) = (false, Vec::new());
    while started.elapsed() < Duration::from_secs(5) && !(saw_saved && !errors.is_empty()) {
        settle(cx, 20);
        let (notice, now) = said(cx);
        saw_saved |= notice == SAVED;
        errors = now;
    }
    shell.update_in(cx, |shell, window, cx| {
        let acts = shell.menu.providers.airdrop.turn_off();
        shell.airdrop_acts(0, acts, window, cx);
    });
    cx.run_until_parked();
    let _ = std::fs::remove_dir_all(&root);
    assert!(saw_saved, "the saved file was said in the footer");
    assert_eq!(
        errors,
        ["AirDrop · connection reset"],
        "the failure after Accept is said as an error"
    );
}
