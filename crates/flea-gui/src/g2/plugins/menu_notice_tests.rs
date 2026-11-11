use super::*;

#[path = "plugin_action_recovery_tests.rs"]
mod plugin_action_recovery_tests;
#[path = "plugin_footer_error_tests.rs"]
mod plugin_footer_error_tests;

#[derive(Debug)]
struct Notice {
    pane: String,
    shown: String,
    error: bool,
    queued: Vec<String>,
    notice: String,
    pending: usize,
}

fn notice(fixture: &Fixture, cx: &mut TestAppContext) -> Notice {
    update(cx, fixture.handle, |shell, _, cx| {
        let line = shell.footer(cx).read(cx).message_line();
        let line = line.read(cx);
        Notice {
            pane: shell.panes[0].as_ref().unwrap().status.to_string(),
            shown: line.drawn().primary.to_string(),
            error: line.drawn().error,
            queued: line
                .messages()
                .errors()
                .iter()
                .map(|error| error.text.clone())
                .collect(),
            notice: line.messages().notice().into(),
            pending: shell.menu.plugin_runtime.pending.len(),
        }
    })
}

fn fixture(cx: &mut TestAppContext) -> (Fixture, PathBuf) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    drop(runtime);
    let mut fixture = open(cx, root.clone(), plugin.clone());
    let folder = attach(cx, &mut fixture, &root, "checksums", &plugin);
    (fixture, folder)
}

fn run(fixture: &mut Fixture, digest: &str, cx: &mut TestAppContext) {
    fixture.menu(cx, 0, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|row| row.action == "example.checksums:sha256")
    });
    let validate = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    let paths = [fixture._scratch.0.join("notes.txt")];
    fixture.validated(cx, &validate, &paths);
    let revalidate = fixture.submit_input(cx, digest).unwrap();
    fixture.validated(cx, &revalidate, &paths);
}

fn release(fixture: &Fixture, folder: &Path) {
    let root = &fixture._scratch.0;
    assert!(
        root.is_absolute()
            && folder.starts_with(root)
            && std::fs::read(root.join(".flea-test-sandbox")).unwrap() == SANDBOX_MARK
    );
    std::fs::write(folder.join("release-terminal"), "release\n").unwrap();
}

#[gpui::test]
fn plugin_menu_runtime_notice_success_replaces_progress_without_escape(cx: &mut TestAppContext) {
    let (mut fixture, folder) = fixture(cx);
    run(&mut fixture, CHECKSUM, cx);
    wait(cx, |cx| notice(&fixture, cx).pane == "Verifying checksum.");
    let progress = notice(&fixture, cx);
    release(&fixture, &folder);
    wait(cx, |cx| notice(&fixture, cx).pending == 0);
    let completed = notice(&fixture, cx);
    assert_eq!(progress.pending, 1, "progress retains its terminal slot");
    assert_eq!(
        completed.pane, "Checksum matches.",
        "real terminal reached producer: {completed:?}"
    );
    assert_eq!(
        completed.shown, "Checksum matches.",
        "terminal must replace progress without dismissal: {progress:?} -> {completed:?}"
    );
    assert!(
        !progress.error && !completed.error,
        "successful notices: {progress:?} -> {completed:?}"
    );
    assert_eq!(progress.shown, "Verifying checksum.");
    assert_eq!(progress.notice, "Verifying checksum.");
    assert_eq!(completed.notice, "Checksum matches.");
    assert!(progress.queued.is_empty() && completed.queued.is_empty());
    assert_eq!(receipt(&folder, "run").len(), 1);
    stop(cx, &fixture);
}

#[gpui::test]
fn plugin_footer_error_started_failure_preserves_sentence(cx: &mut TestAppContext) {
    let (mut fixture, _folder) = fixture(cx);
    let expected = "Checksum does not match.";
    run(&mut fixture, &"0".repeat(64), cx);
    wait(cx, |cx| {
        let failed = notice(&fixture, cx);
        failed.pending == 0 && failed.error
    });
    let failed = notice(&fixture, cx);
    assert_eq!(failed.pending, 0, "failure also releases its terminal slot");
    assert!(
        failed.error,
        "real plugin failure must remain acknowledged: {failed:?}"
    );
    assert_eq!(failed.shown, expected);
    assert_eq!(failed.pane, expected);
    assert_eq!(failed.queued, vec![expected.to_owned()]);
    stop(cx, &fixture);
}

#[gpui::test]
fn plugin_menu_runtime_notice_success_replaces_older_errors(cx: &mut TestAppContext) {
    // S1B-2: a newer message replaces an older one, so the plugin's own notices are never hidden behind stale errors.
    let (mut fixture, folder) = fixture(cx);
    update(cx, fixture.handle, |shell, _, cx| {
        shell.menu_notice(0, shell.locale.catalog.tr("window.backend_stopped"), cx);
        shell.menu_refuse("menu.refusal.unavailable", cx);
    });
    let before = notice(&fixture, cx);
    assert_eq!(
        before.queued.len(),
        2,
        "the menu failure and refusal both queue"
    );
    run(&mut fixture, CHECKSUM, cx);
    wait(cx, |cx| notice(&fixture, cx).pane == "Verifying checksum.");
    let progress = notice(&fixture, cx);
    release(&fixture, &folder);
    wait(cx, |cx| notice(&fixture, cx).pending == 0);
    let completed = notice(&fixture, cx);
    for (seen, words) in [
        (&progress, "Verifying checksum."),
        (&completed, "Checksum matches."),
    ] {
        assert!(!seen.error, "the newer notice is drawn as a notice");
        assert_eq!(seen.shown, words);
        assert!(
            seen.queued.is_empty(),
            "the older errors are replaced: {:?}",
            seen.queued
        );
    }
    assert_eq!(completed.pane, "Checksum matches.");
    stop(cx, &fixture);
}
