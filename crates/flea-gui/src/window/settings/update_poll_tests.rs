//! The production window starts update polling even before Settings is opened.
use crate::{
    backend::Backend,
    presentation::fixture::Fixture,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use gpui::TestAppContext;
use std::{process::Command, sync::Arc, time::Duration};

const CHILD: &str = "window::settings::update_poll_tests::owner_parity_update_poll_child";

#[test]
fn owner_parity_update_poll_is_armed_by_production_window() {
    for automatic in [false, true] {
        run_case(automatic);
    }
}

fn run_case(automatic: bool) {
    let fixture = Fixture::new("other3-update-startup");
    for name in ["config", "data", "cache", "state", "run", "files"] {
        fixture.mkdir(&fixture.path().join(name));
    }
    fixture.write(
        &fixture.path().join("state/flea/ui.json"),
        format!(r#"{{"updates":{{"autoCheck":{automatic}}}}}"#),
    );
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("OWNER_POLL_FIXTURE", fixture.path())
        .env("FLEA_BIN", fixture.path().join("no-cli"));
    for (key, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_RUNTIME_DIR", "run"),
    ] {
        command.env(key, fixture.path().join(name));
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("OWNER-POLL-STARTED"),
        "production window poll did not start: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fixture.cleanup();
}

#[gpui::test]
fn owner_parity_update_poll_child(cx: &mut TestAppContext) {
    let Some(root) = std::env::var_os("OWNER_POLL_FIXTURE") else {
        return;
    };
    let fixture = Fixture::open(root.into()).unwrap();
    let document = flea::jsondoc::parse(
        &std::fs::read_to_string(fixture.path().join("state/flea/ui.json")).unwrap(),
    )
    .unwrap();
    let automatic = document
        .get("updates")
        .unwrap()
        .get("autoCheck")
        .unwrap()
        .as_bool()
        .unwrap();
    let launch = Launch::resolve(
        Settings::from_json(document),
        Arguments {
            path: Some(fixture.path().join("files").to_str().unwrap().into()),
            select: None,
        },
        fixture.path().to_str().unwrap(),
    );
    let (_sender, events) = async_channel::unbounded();
    let handle = cx.update(|cx| {
        // The poll does not need the real flea-settings thread, whose wakes the test scheduler rejects.
        cx.set_global(crate::state::constructor_fixture::NoSettingsWorker);
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        });
        crate::window::open(
            launch,
            Snapshot {
                revision: 1,
                catalog: Arc::new(crate::test_catalogs::english()),
            },
            events,
            cx,
        )
        .unwrap()
    });
    handle
        .update(cx, |root, _, cx| {
            assert!(
                root.shell.read(cx).settings.poll.is_some(),
                "window startup never armed its poll"
            );
        })
        .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(59));
    cx.run_until_parked();
    handle
        .update(cx, |root, _, cx| {
            assert_eq!(
                root.shell.read(cx).settings.update.state,
                flea::ui_logic_settings_pages::update::State::Idle
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            assert!(!shell.settings.updating);
            assert_eq!(
                shell.settings.update.state,
                if automatic {
                    flea::ui_logic_settings_pages::update::State::Failed
                } else {
                    flea::ui_logic_settings_pages::update::State::Idle
                }
            );
        })
        .unwrap();
    println!("OWNER-POLL-STARTED");
}
