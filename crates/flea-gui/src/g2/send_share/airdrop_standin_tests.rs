//! AirDrop's runs against a stand-in owly: the browse loop, two windows on one radio, and what the receiver saved.
use super::airdrop::{Proc, Radio};
use super::airdrop_shared::{Seen, Shared};
use crate::{backend::Backend, state::tests::shell_with, state::Shell};
use flea::ui_logic_send_share::airdrop::{Adapter, Readiness};
use gpui::{TestAppContext, VisualTestContext};

use std::path::PathBuf;
use std::sync::atomic::Ordering;
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
    flea::executable_file::write(&owly, &std::fs::read(&owly).unwrap(), 0o755).unwrap();
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
        let acts = air.menu_opened(super::join::now_ms(), &Seen::default());
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
    // Each Receiving On reads the machine's radio off the window's thread before it claims, so the second sees the first.
    let mut receiving = Vec::new();
    for (window, _) in &windows {
        window
            .update(cx, |shell, window, cx| {
                let air = &mut shell.menu.providers.airdrop;
                air.shared = Shared::queued(machine.clone());
                air.facts(&owly);
                air.adapter = ready();
                shell.airdrop_receiving(true, &root.to_string_lossy(), window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        let page = window.update(cx, |shell, _, _| {
            shell.menu.providers.airdrop.page("/tmp", "host")
        });
        receiving.push(page.unwrap().receiving);
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
        shell.airdrop_receiving(true, &root.to_string_lossy(), window, cx);
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

#[gpui::test]
fn receiving_off_while_the_look_runs_starts_no_radio(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, owly, log) = stand_in("offwins");
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    shell.update_in(cx, |shell, window, cx| {
        let air = &mut shell.menu.providers.airdrop;
        air.shared = Shared::queued(root.join("machine"));
        air.facts(&owly);
        air.adapter = ready();
        shell.airdrop_receiving(true, &root.to_string_lossy(), window, cx);
        shell.airdrop_receiving(false, &root.to_string_lossy(), window, cx);
    });
    settle(cx, 300);
    // A radio started and stopped at once may never log, so its run count says whether one started.
    let (radio, receiving, runs) = shell.update(cx, |shell, _| {
        let air = &shell.menu.providers.airdrop;
        let page = air.page("/tmp", "host");
        (air.radio, page.receiving, air.runs(Proc::Radio))
    });
    let radios = count(&log, "radio up");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!((radio, receiving, runs, radios), (Radio::Off, false, 0, 0));
}

// Each window with the backend lines it wrote, kept so its backend's channel stays open.
type Windows = Vec<(
    gpui::WindowHandle<Shell>,
    Box<dyn FnMut() -> Option<String>>,
)>;

// Two windows on one machine, each asked Receiving On in turn: the first owns the radio and the second reads it.
fn two_on_one_radio(cx: &mut TestAppContext, tag: &str) -> (PathBuf, Windows) {
    let (root, owly, _log) = stand_in(tag);
    let machine = root.join("machine");
    let mut windows: Windows = Vec::new();
    for _ in 0..2 {
        let (backend, lines) = Backend::test_lines();
        windows.push((
            cx.add_window(|_, cx| shell_with(backend, cx)),
            Box::new(lines),
        ));
    }
    for (window, _) in &windows {
        window
            .update(cx, |shell, window, cx| {
                let air = &mut shell.menu.providers.airdrop;
                air.shared = Shared::queued(machine.clone());
                air.facts(&owly);
                air.adapter = ready();
                shell.airdrop_receiving(true, &root.to_string_lossy(), window, cx);
            })
            .unwrap();
        cx.run_until_parked();
    }
    (root, windows)
}

fn radio_of(cx: &mut TestAppContext, window: &gpui::WindowHandle<Shell>) -> (Radio, bool) {
    window
        .update(cx, |shell, _, _| {
            let air = &shell.menu.providers.airdrop;
            (air.radio, air.page("/tmp", "host").receiving)
        })
        .unwrap()
}

#[gpui::test]
fn off_on_off_in_a_reader_still_stops_the_owners_radio(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, windows) = two_on_one_radio(cx, "offonoff");
    assert_eq!(
        (
            radio_of(cx, &windows[0].0).0 != Radio::Off,
            radio_of(cx, &windows[1].0)
        ),
        (true, (Radio::Off, true)),
        "the reader shows the owner's radio"
    );
    // The reader's choices land faster than its looks come back, and the last one is Off.
    let folder = root.to_string_lossy().into_owned();
    let _ = windows[1].0.update(cx, |shell, window, cx| {
        for on in [false, true, false] {
            shell.airdrop_receiving(on, &folder, window, cx);
        }
    });
    cx.run_until_parked();
    // The owner looks once a second, on the test executor's clock.
    cx.executor().advance_clock(Duration::from_millis(1_100));
    cx.run_until_parked();
    let owner = radio_of(cx, &windows[0].0).0;
    for (window, _) in &windows {
        let _ = window.update(cx, |shell, window, cx| {
            let acts = shell.menu.providers.airdrop.turn_off();
            shell.airdrop_acts(0, acts, window, cx);
        });
    }
    cx.run_until_parked();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(owner, Radio::Off, "the owner answered the last Off");
}

#[gpui::test]
fn a_stop_asked_as_the_reader_closes_still_reaches_the_owner(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, windows) = two_on_one_radio(cx, "offclose");
    let folder = root.to_string_lossy().into_owned();
    // The count a quit waits on, held from the Off until its stop is written.
    let (unwritten, asked) = windows[1]
        .0
        .update(cx, |shell, window, cx| {
            shell.airdrop_receiving(false, &folder, window, cx);
            window.remove_window();
            let unwritten = shell.menu.providers.airdrop.shared.unwritten();
            (unwritten, unwritten.load(Ordering::SeqCst))
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        (asked, unwritten.load(Ordering::SeqCst)),
        (1, 0),
        "a quit waits for the reader's stop until it is written"
    );
    let machine = root.join("machine");
    let state = std::fs::read_to_string(machine.join("airdrop.state")).unwrap_or_default();
    let stop = std::fs::read_to_string(machine.join("airdrop.stop")).unwrap_or_default();
    let _ = windows[0].0.update(cx, |shell, window, cx| {
        let acts = shell.menu.providers.airdrop.turn_off();
        shell.airdrop_acts(0, acts, window, cx);
    });
    cx.run_until_parked();
    let _ = std::fs::remove_dir_all(&root);
    let owner = state.lines().next().unwrap_or_default();
    assert!(!owner.is_empty(), "the owner published its token");
    assert_eq!(
        stop.trim(),
        owner,
        "the closed reader's stop names the owner"
    );
}

// A stand-in omdrop that logs each argv: status says on and receiving; off refuses as a running send makes it.
fn omdrop_stand_in(tag: &str) -> (PathBuf, String) {
    let root = std::env::temp_dir().join(format!("flea-omdrop-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    let script = "#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/argv.log\"\ncase \"$1\" in\n  status) echo '{\"receiving\": true, \"visible\": true, \"name\": \"Omarchy\", \"dir\": \"/tmp\", \"visibility\": \"everyone\"}' ;;\n  off) echo 'Still sending to TestPhone. Cancel the send first, or turn it off anyway with: omdrop off --force' >&2; exit 1 ;;\nesac\n";
    let omdrop = root.join("omdrop");
    std::fs::write(&omdrop, script).unwrap();
    flea::executable_file::write(&omdrop, script.as_bytes(), 0o755).unwrap();
    (root.clone(), omdrop.to_string_lossy().into_owned())
}

#[gpui::test]
fn a_refused_omdrop_off_from_the_page_says_omdrops_reason_in_the_footer(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (root, omdrop) = omdrop_stand_in("off");
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    shell.update_in(cx, |shell, window, cx| {
        let _ = shell.menu.providers.omdrop.facts(&omdrop);
        let acts = shell.menu.providers.omdrop.ask_status();
        shell.stack_acts(super::airdrop_run::Origin::omdrop(0), acts, window, cx);
    });
    settle(cx, 500);
    let on = shell.update(cx, |shell, _| shell.menu.providers.omdrop.on());
    assert!(on, "status said on");
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(false, "/tmp", window, cx)
    });
    let mut errors = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) && errors.is_empty() {
        settle(cx, 20);
        errors = shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .errors()
                    .iter()
                    .map(|e| e.text.clone())
                    .collect::<Vec<_>>()
            })
        });
    }
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        errors,
        ["AirDrop · Still sending to TestPhone. Cancel the send first, or turn it off anyway with: omdrop off --force"]
    );
}

#[path = "airdrop_folder_tests.rs"]
mod folder;

#[path = "airdrop_retry_tests.rs"]
mod retry;
