use super::super::airdrop::Act;
use super::super::airdrop_omdrop::Omdrop;
use super::super::airdrop_run::Origin;
use super::*;
use gpui::Entity;

// A stand-in omdrop logging each argv; its status prints `status.json` beside it, and fails while there is none.
fn omdrop_with_status(tag: &str) -> (StandInDir, String) {
    let root = StandInDir::fresh(tag);
    let script = "#!/bin/sh\nd=$(dirname \"$0\")\necho \"$*\" >> \"$d/argv.log\"\ncase \"$1\" in\n  status) cat \"$d/status.json\" 2>/dev/null || exit 1 ;;\n  dir) echo \"$2\" > \"$d/folder\" ;;\n  off) if [ -e \"$d/refuse\" ]; then echo 'Still sending to TestPhone. Cancel the send first.' >&2; exit 1; fi ;;\nesac\n";
    let omdrop = root.join("omdrop");
    flea::executable_file::write(&omdrop, script.as_bytes(), 0o755).unwrap();
    let omdrop = omdrop.to_string_lossy().into_owned();
    (root, omdrop)
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
) -> StandInDir {
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
fn a_status_that_keeps_failing_is_retried_five_times_then_the_queued_off_runs(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = window(cx);
    let root = off_waiting_on_a_failed_status("retry-bound", &shell, cx);
    for _ in 0..4 {
        wait_ms(cx, 2_000);
    }
    assert_eq!(ran(&root, "off"), 0, "the Off waits while retries remain");
    for _ in 0..6 {
        wait_ms(cx, 2_000);
    }
    let log = std::fs::read_to_string(root.join("argv.log")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    let before_off = log
        .lines()
        .take_while(|l| !l.starts_with("off"))
        .filter(|l| l.starts_with("status"))
        .count();
    assert_eq!(
        log.lines().filter(|l| l.starts_with("off")).count(),
        1,
        "{log}"
    );
    assert_eq!(
        before_off,
        2 + 5,
        "off ran once the fifth retry failed:\n{log}"
    );
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

// Omdrop off; on with an Off chosen meanwhile; owly appears on PATH before `on` has returned.
fn off_queued_then_owly_takes_over(
    tag: &str,
    refuse: &str,
    cx: &mut TestAppContext,
) -> (StandInDir, Vec<String>) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status(tag);
    std::fs::write(root.join("status.json"), OFF).unwrap();
    if !refuse.is_empty() {
        std::fs::write(root.join("refuse"), refuse).unwrap();
    }
    acts_from_omdrop(&shell, cx, |om| {
        let _ = om.facts(&omdrop);
        om.ask_status()
    });
    settle(cx, 300);
    shell.update_in(cx, |shell, window, cx| {
        shell.airdrop_receiving(true, "/tmp", window, cx);
        shell.airdrop_receiving(false, "/tmp", window, cx);
        shell.airdrop_tools(("/usr/bin/owly", ""), 0, window, cx);
    });
    let mut errors = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) && (refuse.is_empty() || errors.is_empty()) {
        settle(cx, 100);
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
    (root, errors)
}

#[gpui::test]
fn an_off_queued_during_on_still_runs_when_owly_takes_over(cx: &mut TestAppContext) {
    let (root, errors) = off_queued_then_owly_takes_over("retire-off", "", cx);
    let (on, off) = (ran(&root, "on"), ran(&root, "off"));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!((on, off), (1, 1), "the Off chosen during on still ran");
    assert!(errors.is_empty(), "{errors:?}");
}

#[gpui::test]
fn a_refused_off_after_owly_takes_over_says_omdrops_reason_in_the_footer(cx: &mut TestAppContext) {
    let (root, errors) = off_queued_then_owly_takes_over("retire-refused", "1", cx);
    let off = ran(&root, "off");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(off, 1);
    assert_eq!(
        errors,
        ["AirDrop · Still sending to TestPhone. Cancel the send first."]
    );
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

#[gpui::test]
fn an_omdrop_already_on_keeps_its_row_through_a_failed_status(cx: &mut TestAppContext) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status("row-on");
    std::fs::write(root.join("status.json"), ON).unwrap();
    acts_from_omdrop(&shell, cx, |om| {
        let _ = om.facts(&omdrop);
        om.ask_status()
    });
    settle(cx, 300);
    std::fs::remove_file(root.join("status.json")).unwrap();
    acts_from_omdrop(&shell, cx, |om| om.ask_status());
    settle(cx, 300);
    let (ready, row) = shell.update(cx, |shell, _| {
        let mut context = flea::ui_logic_context_menus::build::Context::default();
        shell.omdrop_context(true, &mut context);
        (
            shell.menu.providers.omdrop.adapter.ready(),
            context.airdrop_installed,
        )
    });
    let _ = std::fs::remove_dir_all(&root);
    assert!(!ready, "the second status failed");
    assert!(row, "Omdrop is still on, so its row stays");
}

#[gpui::test]
fn a_menu_closed_while_owly_ran_starts_no_omdrop_lookup_when_omdrop_returns(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = window(cx);
    let (root, omdrop) = omdrop_with_status("menu-close");
    std::fs::write(root.join("status.json"), ON).unwrap();
    acts_from_omdrop(&shell, cx, |om| {
        let _ = om.facts(&omdrop);
        om.ask_status()
    });
    settle(cx, 300);
    // Opened now, so a lookup that ends at once is not run again for the open menu.
    acts_from_omdrop(&shell, cx, |om| {
        om.menu_opened(super::super::join::now_ms())
    });
    settle(cx, 300);
    let looked = ran(&root, "peers");
    acts_from_omdrop(&shell, cx, |om| om.facts(""));
    shell.update_in(cx, |shell, _, _| shell.airdrop_menu_closed());
    acts_from_omdrop(&shell, cx, |om| {
        let mut acts = om.facts(&omdrop);
        acts.extend(om.ask_status());
        acts
    });
    settle(cx, 500);
    let after = ran(&root, "peers");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(looked, 1, "the open menu looked once");
    assert_eq!(after, 1, "Omdrop back behind a shut menu looks for nothing");
}
