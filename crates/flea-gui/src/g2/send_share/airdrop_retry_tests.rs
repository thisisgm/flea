use super::super::airdrop::Act;
use super::super::airdrop_omdrop::Omdrop;
use super::super::airdrop_run::Origin;
use super::*;
use gpui::Entity;
use std::path::Path;

// A stand-in omdrop logging each argv; its status prints `status.json` beside it, and fails while there is none.
fn omdrop_with_status(tag: &str) -> (PathBuf, String) {
    let root = std::env::temp_dir().join(format!("flea-omdrop-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    let script = "#!/bin/sh\nd=$(dirname \"$0\")\necho \"$*\" >> \"$d/argv.log\"\ncase \"$1\" in\n  status) cat \"$d/status.json\" 2>/dev/null || exit 1 ;;\n  dir) echo \"$2\" > \"$d/folder\" ;;\nesac\n";
    let omdrop = root.join("omdrop");
    flea::executable_file::write(&omdrop, script.as_bytes(), 0o755).unwrap();
    (root, omdrop.to_string_lossy().into_owned())
}

const OFF: &str = r#"{"receiving": false, "visible": false, "name": "Omarchy", "dir": "/tmp", "visibility": "everyone"}"#;
const ON: &str = r#"{"receiving": true, "visible": true, "name": "Omarchy", "dir": "/tmp", "visibility": "everyone"}"#;

fn ran(root: &Path, verb: &str) -> usize {
    let log = std::fs::read_to_string(root.join("argv.log")).unwrap_or_default();
    log.lines()
        .filter(|l| l.split(' ').next() == Some(verb))
        .count()
}

fn window(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    cx.executor().allow_parking();
    let (backend, _lines) = Backend::test_lines();
    cx.add_window_view(|_, cx| shell_with(backend, cx))
}

fn acts_from_omdrop(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    ask: impl FnOnce(&mut Omdrop) -> Vec<Act>,
) {
    shell.update_in(cx, |shell, window, cx| {
        let acts = ask(&mut shell.menu.providers.omdrop);
        shell.stack_acts(Origin::omdrop(0), acts, window, cx);
    });
}

// Omdrop reads off; Receiving set on and then Off while `omdrop on` runs; once on returns, its status fails.
fn off_waiting_on_a_failed_status(
    tag: &str,
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> PathBuf {
    let (root, omdrop) = omdrop_with_status(tag);
    std::fs::write(root.join("status.json"), OFF).unwrap();
    acts_from_omdrop(shell, cx, |om| {
        let _ = om.facts(&omdrop);
        om.ask_status()
    });
    settle(cx, 300);
    std::fs::remove_file(root.join("status.json")).unwrap();
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(true, "/tmp", window, cx);
        shell.airdrop_receiving(false, "/tmp", window, cx);
    });
    settle(cx, 500);
    assert_eq!(
        (ran(&root, "on"), ran(&root, "status")),
        (1, 2),
        "on returned and its status failed"
    );
    root
}

fn wait_ms(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    settle(cx, 300);
}

#[gpui::test]
fn the_off_waiting_on_a_failed_status_runs_when_the_delayed_retry_answers(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let root = off_waiting_on_a_failed_status("retry-off", &shell, cx);
    wait_ms(cx, 1_900);
    assert_eq!(
        ran(&root, "status"),
        2,
        "nothing is asked before the delay passes"
    );
    std::fs::write(root.join("status.json"), ON).unwrap();
    wait_ms(cx, 200);
    let (status, off) = (ran(&root, "status"), ran(&root, "off"));
    let _ = std::fs::remove_dir_all(&root);
    // The retry is the third status; the fourth confirms the Off it ran.
    assert_eq!(
        (status, off),
        (4, 1),
        "the retry confirmed on and the Off ran"
    );
}

#[gpui::test]
fn a_status_that_keeps_failing_is_retried_five_times_and_never_turns_omdrop_off(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = window(cx);
    let root = off_waiting_on_a_failed_status("retry-bound", &shell, cx);
    for _ in 0..10 {
        wait_ms(cx, 2_000);
    }
    let (status, off) = (ran(&root, "status"), ran(&root, "off"));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!((status, off), (2 + 5, 0));
}

#[gpui::test]
fn on_chosen_again_asks_one_status_at_once_and_the_armed_retry_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let root = off_waiting_on_a_failed_status("retry-on", &shell, cx);
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(true, "/tmp", window, cx)
    });
    std::fs::write(root.join("status.json"), OFF).unwrap();
    settle(cx, 300);
    assert_eq!(ran(&root, "status"), 3, "On asked a status at once");
    wait_ms(cx, 4_000);
    let (status, off) = (ran(&root, "status"), ran(&root, "off"));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!((status, off), (3, 0), "On withdrew the Off and its retry");
}

#[gpui::test]
fn an_off_chosen_after_the_confirming_status_failed_runs_without_waiting(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status("retry-late-off");
    std::fs::write(root.join("status.json"), OFF).unwrap();
    acts_from_omdrop(&shell, cx, |om| {
        let _ = om.facts(&omdrop);
        om.ask_status()
    });
    settle(cx, 300);
    std::fs::remove_file(root.join("status.json")).unwrap();
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(true, "/tmp", window, cx)
    });
    settle(cx, 500);
    assert_eq!((ran(&root, "on"), ran(&root, "status")), (1, 2));
    std::fs::write(root.join("status.json"), ON).unwrap();
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(false, "/tmp", window, cx)
    });
    settle(cx, 500);
    let (status, off) = (ran(&root, "status"), ran(&root, "off"));
    let _ = std::fs::remove_dir_all(&root);
    // Off asked the third status itself, with no clock advanced; the fourth confirms the off.
    assert_eq!((status, off), (4, 1));
}

#[gpui::test]
fn folders_chosen_while_omdrop_dir_runs_end_on_the_latest_with_one_status_after(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status("dir-queue");
    std::fs::write(root.join("status.json"), ON).unwrap();
    shell.update_in(cx, |shell, window, cx| {
        let _ = shell.menu.providers.omdrop.facts(&omdrop);
        let tool = shell.airdrop_tool();
        for folder in ["/tmp/a", "/tmp/b", "/tmp/c"] {
            shell.airdrop_folder_landed(&tool, folder, window, cx);
        }
    });
    settle(cx, 800);
    let log = std::fs::read_to_string(root.join("argv.log")).unwrap_or_default();
    let set = std::fs::read_to_string(root.join("folder")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    let runs: Vec<&str> = log.lines().collect();
    assert_eq!(runs, ["dir /tmp/a", "dir /tmp/c", "status --json"], "{log}");
    assert_eq!(set.trim(), "/tmp/c", "the latest choice is Omdrop's folder");
}

#[gpui::test]
fn a_retry_armed_before_omdrop_is_replaced_asks_neither(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let root = off_waiting_on_a_failed_status("retry-swap", &shell, cx);
    let (other, other_path) = omdrop_with_status("retry-other");
    acts_from_omdrop(&shell, cx, |om| om.facts(&other_path));
    wait_ms(cx, 4_000);
    let asked = (ran(&root, "status"), ran(&other, "status"));
    let _ = (
        std::fs::remove_dir_all(&root),
        std::fs::remove_dir_all(&other),
    );
    assert_eq!(asked, (2, 0));
}

#[gpui::test]
fn an_off_queued_during_on_still_runs_when_owly_takes_over(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status("retire-off");
    std::fs::write(root.join("status.json"), OFF).unwrap();
    acts_from_omdrop(&shell, cx, |om| {
        let _ = om.facts(&omdrop);
        om.ask_status()
    });
    settle(cx, 300);
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(true, "/tmp", window, cx);
        shell.airdrop_receiving(false, "/tmp", window, cx);
        // owly appears on PATH before `on` has returned.
        shell.airdrop_tools(("/usr/bin/owly", ""), 0, window, cx);
    });
    settle(cx, 500);
    let (on, off) = (ran(&root, "on"), ran(&root, "off"));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!((on, off), (1, 1), "the Off chosen during on still ran");
}

#[gpui::test]
fn the_omdrop_page_learns_this_machines_name_for_shown_as(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status("host");
    std::fs::write(
        root.join("status.json"),
        r#"{"receiving": false, "visible": false, "name": "", "dir": "/tmp"}"#,
    )
    .unwrap();
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_settings_for(("", &omdrop), window, cx)
    });
    settle(cx, 500);
    let (active, host) = shell.update(cx, |shell, _| {
        (
            shell.omdrop_active(),
            shell.menu.providers.airdrop_host.clone(),
        )
    });
    let _ = std::fs::remove_dir_all(&root);
    let want = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    assert!(active, "omdrop runs AirDrop");
    assert_eq!(host, want.trim(), "Shown as has a name to fall back on");
}
