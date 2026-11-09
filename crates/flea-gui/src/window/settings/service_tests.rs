use super::{
    store::{Update, Worker},
    SessionStore,
};
use crate::{
    backend::Backend,
    g2::settings_pages::{claim::Claim, seams::Store},
};
use flea::jsondoc::{self, Json};
use gpui::{AppContext, TestAppContext};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let parent = std::env::current_dir().unwrap().join(".superpowers/u11");
        std::fs::create_dir_all(&parent).unwrap();
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("service.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(parent));
        std::fs::write(path.join(".flea-test-sandbox"), "u11 settings service\n").unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::env::current_dir().unwrap().join(".superpowers/u11");
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(parent)
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn next(worker: &Worker) -> Update {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(update) = worker.updates.try_recv() {
            if !matches!(update, Update::Corner(_)) {
                return update;
            }
        }
        assert!(Instant::now() < until, "the settings worker never answered");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn u11_independent_workers_read_the_private_store_and_report_a_failed_writer() {
    let root = Fixture::new();
    let state = root.0.join("state");
    let store = flea::uistore::Store::at(&state, &root.0.join("config"));
    store
        .update(&jsondoc::parse(r#"{"density":"compact"}"#).unwrap())
        .unwrap();
    let path = state.join("flea/ui.json");
    let missing = root.0.join("no-such-flea");
    let a = Worker::at(path.clone(), missing.clone()).unwrap();
    let b = Worker::at(path, missing).unwrap();
    for worker in [&a, &b] {
        assert!(matches!(next(worker), Update::Loaded { saved: false, .. }));
    }
    store
        .update(&jsondoc::parse(r#"{"density":"normal","hidden":true}"#).unwrap())
        .unwrap();
    for worker in [&a, &b] {
        let Update::Loaded { document, .. } = next(worker) else {
            panic!("expected a settings read")
        };
        assert_eq!(
            document.get("density").and_then(Json::as_str),
            Some("normal")
        );
        assert_eq!(document.get("hidden").and_then(Json::as_bool), Some(true));
    }
    a.writes
        .send((1, jsondoc::parse(r#"{"hidden":false}"#).unwrap(), None))
        .unwrap();
    assert!(matches!(
        next(&a),
        Update::Failed {
            through: 1,
            keep: true
        }
    ));
    assert_eq!(
        store.read().get("hidden").and_then(Json::as_bool),
        Some(true)
    );
}

#[test]
fn u11_a_bad_patch_changes_no_session_value_and_queues_no_write() {
    let mut store = SessionStore {
        document: flea::uischema::defaults(),
        patches: Default::default(),
    };
    let before = store.document.clone();
    assert!(store
        .write(&jsondoc::parse(r#"{"hidden":true,"noSuchSetting":2}"#).unwrap())
        .is_err());
    assert_eq!(store.document, before);
    assert!(store.patches.borrow().is_empty());
}

#[test]
fn u11_an_unreadable_document_reports_once_and_recovers_after_a_valid_read() {
    let root = Fixture::new();
    let path = root.0.join("ui.json");
    std::fs::write(&path, "{bad").unwrap();
    let worker = Worker::at(path.clone(), root.0.join("no-flea")).unwrap();
    assert!(matches!(next(&worker), Update::Unreadable));
    std::fs::write(&path, "{}").unwrap();
    assert!(matches!(next(&worker), Update::Loaded { .. }));
    std::fs::write(path, "[]").unwrap();
    assert!(matches!(next(&worker), Update::Unreadable));
}

#[gpui::test]
fn u11_failed_writes_keep_session_values_but_never_hide_the_failure(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let shell = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        shell.update(cx, |shell, cx| {
            shell.open_settings(window, cx);
            let patch = jsondoc::parse(r#"{"density":"normal"}"#).unwrap();
            shell.settings.pending.push((1, patch.clone()));
            shell.apply_settings(
                flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap(),
                window,
                cx,
            );
            shell.receive_settings(
                Update::Failed {
                    through: 1,
                    keep: true,
                },
                window,
                cx,
            );
            shell.receive_settings(
                Update::Loaded {
                    document: flea::uischema::defaults(),
                    through: 1,
                    saved: false,
                },
                window,
                cx,
            );
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("density")
                    .and_then(Json::as_str),
                Some("normal")
            );
            assert!(shell
                .settings
                .panel
                .as_ref()
                .unwrap()
                .read(cx)
                .panel
                .save_error
                .is_some());
            assert_eq!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .file_metrics()
                    .row,
                37.
            );
        });
    });
}

#[gpui::test]
fn u11_refused_favourite_moves_never_show_an_optimistic_blink_back(cx: &mut TestAppContext) {
    let root = Fixture::new();
    let worker = Worker::at(root.0.join("ui.json"), root.0.join("no-flea")).unwrap();
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let document = flea::uistate::from_file(r#"{"places":{"favourites":[{"path":"/files","label":"Files"},{"path":"/other","label":"Other"}]}}"#);
            shell.apply_settings(document.clone(), window, cx);
            shell.settings.worker = Some(worker);
            shell.open_settings(window, cx);
            shell.move_settings_favourite(0, Some(1), window, cx);
            assert_eq!(shell.launch.settings.data, document);
            shell.receive_settings(Update::Failed { through: 1, keep: false }, window, cx);
            assert_eq!(shell.launch.settings.data, document);
            assert!(shell.settings_message(cx).is_some());
        });
    });
}

#[gpui::test]
fn u11_a_default_command_that_never_starts_draws_its_error_without_saving_a_setting(
    cx: &mut TestAppContext,
) {
    let mut claim = Claim {
        packaged: true,
        ..Claim::default()
    };
    assert_eq!(claim.start(), Some(true));
    let failed = super::claim::completed(claim, PathBuf::from("/nonexistent-u11-flea"));
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.open_settings(window, cx);
        shell.settings.claim = failed;
        shell.settings.about_open = true;
        shell.sync_settings_claim(cx);
        shell
    });
    shell.update(cx, |shell, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                view.panel.show_section("about");
                cx.notify();
            });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        let index = panel
            .rows
            .iter()
            .position(|r| r.id == "makeDefault")
            .unwrap();
        assert!(!panel.rows[index].on && !panel.rows[index].inert);
        assert_eq!(
            panel.rows[index + 1].role,
            flea::ui_logic_settings_pages::rows::Role::Error
        );
        assert_eq!(
            panel.rows[index + 1].label.resolve(&panel.catalog),
            "nonexistent-u11-flea could not start"
        );
        assert!(shell.launch.settings.data.get("makeDefault").is_none());
    });
}

fn favourites_saving(
    shell: &mut crate::state::Shell,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) -> flea::ipc::wire::Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        crate::ipc::server::Event::Call {
            request: flea::ipc::wire::Request::Call {
                target: "flea".into(),
                function: "favouritesSaving".into(),
                arguments: Vec::new(),
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().unwrap().reply
}

#[gpui::test]
fn readers3_favourites_saving_reads_true_while_a_move_is_in_flight(cx: &mut TestAppContext) {
    let root = Fixture::new();
    let worker = Worker::at(root.0.join("ui.json"), root.0.join("no-flea")).unwrap();
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let document = flea::uistate::from_file(r#"{"places":{"favourites":[{"path":"/files","label":"Files"},{"path":"/other","label":"Other"}]}}"#);
            shell.apply_settings(document, window, cx);
            shell.settings.worker = Some(worker);
            shell.open_settings(window, cx);
            assert_eq!(
                favourites_saving(shell, window, cx),
                flea::ipc::wire::Reply::value("false")
            );
            shell.move_settings_favourite(0, Some(1), window, cx);
            assert_eq!(
                favourites_saving(shell, window, cx),
                flea::ipc::wire::Reply::value("true"),
                "a favourite move in flight reads saving"
            );
            shell.receive_settings(
                Update::Failed {
                    through: 1,
                    keep: false,
                },
                window,
                cx,
            );
            assert_eq!(
                favourites_saving(shell, window, cx),
                flea::ipc::wire::Reply::value("false"),
                "a move the worker answered no longer reads saving"
            );
        });
    });
}
