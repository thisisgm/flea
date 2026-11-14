use super::*;
use flea::ui_logic_plugin_settings::wire;
use std::time::Instant;

#[path = "menu_notice_tests.rs"]
mod notice_tests;

#[path = "menu_backend.rs"]
mod backend;

#[path = "menu_shelf_plugin_tests.rs"]
mod shelf_plugin_tests;

#[track_caller]
pub(super) fn wait(cx: &mut TestAppContext, mut ready: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if ready(cx) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "real plugin host did not deliver its result"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn attach(
    cx: &mut TestAppContext,
    fixture: &mut Fixture,
    root: &Path,
    example: &str,
    plugin: &Plugin,
) -> PathBuf {
    let folder = root.join("config/flea/plugins").join(&plugin.manifest.id);
    std::fs::create_dir_all(&folder).unwrap();
    let script = std::fs::read_to_string(root.join(example).join("plugin.py")).unwrap().replace(
        "        operation = request.get(\"op\")",
        "        operation = request.get(\"op\")\n        if operation in ('run', 'send') and os.path.exists('malformed-response'):\n            print('not-json', flush=True)\n            continue",
    );
    let script = if example == "checksums" {
        script.replace("            answer({\"op\": \"done\", \"message\": \"Checksum matches.\"})",
            "            while not os.path.exists('release-terminal'):\n                import time\n                time.sleep(0.01)\n            answer({\"op\": \"done\", \"message\": \"Checksum matches.\"})")
    } else {
        script
    };
    flea::executable_file::write(&folder.join("plugin.py"), script.as_bytes(), 0o755).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins")
        .join(example)
        .join("manifest.json");
    std::fs::copy(source, folder.join("manifest.json")).unwrap();
    std::fs::create_dir_all(root.join("state/flea")).unwrap();
    std::fs::write(
        root.join("state/flea/ui.json"),
        format!(
            r#"{{"plugins":{{"{}":{{"enabled":true}}}},"shelf":{{"enabled":{}}}}}"#,
            plugin.manifest.id,
            fixture
                .plugins
                .iter()
                .any(|plugin| plugin.manifest.id == "flea.shelf" && plugin.enabled)
        ),
    )
    .unwrap();
    let mut command = Command::new(backend::binary());
    command
        .arg("--plugin-host")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .current_dir(root);
    let backend =
        Backend::start_with(command, wire::request_line(0, "list", "", Vec::new())).unwrap();
    let _ = backend.test_own(cx);
    fixture.drained = Some(backend.drained());
    update(cx, fixture.handle, |shell, window, cx| {
        shell.follow_plugins(window, cx);
        shell
            .plugins
            .host
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |host, cx| host.test_attach(backend, cx));
    });
    fixture.real_host = true;
    wait(cx, |cx| {
        update(cx, fixture.handle, |shell, _, cx| {
            shell
                .plugins
                .host
                .as_ref()
                .unwrap()
                .read(cx)
                .plugins
                .iter()
                .any(|p| p.manifest.id == plugin.manifest.id)
        })
    });
    folder
}

pub(super) fn receipt(folder: &Path, op: &str) -> Vec<Json> {
    std::fs::read_to_string(folder.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| jsondoc::parse(line).ok())
        .filter(|r| r.get("op").and_then(Json::as_str) == Some(op))
        .collect()
}

pub(super) fn stop(cx: &mut TestAppContext, fixture: &Fixture) {
    update(cx, fixture.handle, |shell, window, cx| {
        shell.retire_menu(window, cx);
        let (replacement, _lines) = Backend::test_lines();
        shell
            .plugins
            .host
            .clone()
            .unwrap()
            .update(cx, |host, cx| host.test_attach(replacement, cx));
    });
    wait(cx, |_| fixture.drained.as_ref().unwrap().try_recv().is_ok());
}

#[gpui::test]
fn plugin_menu_runtime_host_action_reaches_the_production_executable_and_disable_removes_its_row(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    drop(runtime);
    let mut fixture = open(cx, root.clone(), plugin.clone());
    let folder = attach(cx, &mut fixture, &root, "checksums", &plugin);
    fixture.menu(cx, 1, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "example.checksums:sha256")
    });
    let progress = Rc::new(RefCell::new(Vec::new()));
    let received = progress.clone();
    let _responses = update(cx, fixture.handle, |shell, window, cx| {
        let host = shell.plugins.host.clone().unwrap();
        cx.subscribe_in(
            &host,
            window,
            move |shell, _, response: &crate::g2::plugins::Response, _, _| {
                if response.plugin == "example.checksums"
                    && response.request.get("op").and_then(Json::as_str) == Some("run")
                {
                    received.borrow_mut().push((
                        response
                            .message
                            .get("op")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        shell.menu.plugin_runtime.pending.contains_key(&response.id),
                    ));
                }
            },
        )
    });
    let validate = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    assert!(receipt(&folder, "run").is_empty());
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert!(receipt(&folder, "run").is_empty());
    let revalidate = fixture.submit_input(cx, CHECKSUM).unwrap();
    assert_eq!(revalidate.get("id"), validate.get("id"));
    assert!(receipt(&folder, "run").is_empty());
    fixture.validated(cx, &revalidate, &[root.join("notes.txt")]);
    wait(cx, |_| {
        progress.borrow().iter().any(|(op, _)| op == "progress")
    });
    update(cx, fixture.handle, |shell, _, _| {
        assert_eq!(shell.menu.plugin_runtime.pending.len(), 1)
    });
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::write(folder.join("release-terminal"), "release\n").unwrap();
    wait(cx, |_| !receipt(&folder, "run").is_empty());
    assert_eq!(
        receipt(&folder, "run")[0].get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );
    let actual = receipt(&folder, "run").remove(0);
    assert!(
        actual.get("folder").is_none()
            && actual.get("path").is_none()
            && actual.get("cwd").is_none(),
        "the selection-only executable receives no unselected folder or path field"
    );
    wait(cx, |_| progress.borrow().iter().any(|(op, _)| op == "done"));
    update(cx, fixture.handle, |shell, _, _| {
        assert!(shell.menu.plugin_runtime.pending.is_empty())
    });
    assert_eq!(
        progress
            .borrow()
            .iter()
            .map(|(op, _)| op.as_str())
            .collect::<Vec<_>>(),
        vec!["progress", "done"]
    );
    fixture.menu(cx, 0, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "example.checksums:sha256")
    });
    let wrong = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    fixture.validated(cx, &wrong, &[root.join("notes.txt")]);
    let wrong = fixture.submit_input(cx, &"0".repeat(64)).unwrap();
    fixture.validated(cx, &wrong, &[root.join("notes.txt")]);
    wait(cx, |cx| {
        progress.borrow().len() >= 4
            && update(cx, fixture.handle, |shell, _, _| {
                shell.menu.plugin_runtime.pending.is_empty()
            })
    });
    update(cx, fixture.handle, |shell, _, cx| {
        let host = shell.plugins.host.as_ref().unwrap().read(cx);
        assert!(
            host.plugins
                .iter()
                .any(|p| p.manifest.id == "example.checksums" && host.healthy(p)),
            "a checksum mismatch keeps the provider healthy"
        );
    });
    fixture.menu(cx, 0, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "example.checksums:sha256")
    });
    update(cx, fixture.handle, |shell, _, cx| {
        assert!(shell
            .plugins
            .host
            .as_ref()
            .unwrap()
            .read(cx)
            .line(wire::request_line(
                500,
                "disable",
                "example.checksums",
                Vec::new()
            )))
    });
    wait(cx, |cx| {
        !fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "example.checksums:sha256")
    });
    assert_eq!(
        receipt(&folder, "hello").len(),
        1,
        "one lasting executable handles readiness and the action"
    );
    stop(cx, &fixture);
}

#[gpui::test]
fn plugin_menu_runtime_host_send_uses_real_target_discovery_and_failed_plugin_removal(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, runtime) = plugin("send-target", &root);
    runtime.shutdown();
    drop(runtime);
    let mut fixture = open(cx, root.clone(), plugin.clone());
    let folder = attach(cx, &mut fixture, &root, "send-target", &plugin);
    let completed = Rc::new(std::cell::Cell::new(false));
    let sent = completed.clone();
    let _response = update(cx, fixture.handle, |shell, _, cx| {
        cx.subscribe(
            shell.plugins.host.as_ref().unwrap(),
            move |_, _, response: &Response, _| {
                if response.request.get("op").and_then(Json::as_str) == Some("send")
                    && response.message.get("op").and_then(Json::as_str) == Some("done")
                    && response.ok
                {
                    sent.set(true);
                }
            },
        )
    });
    fixture.menu(cx, 1, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "plugin-send:example.send-target" && !r.disabled)
    });
    let validate = fixture
        .choose(cx, "plugin-send:example.send-target", "receiver")
        .unwrap();
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    wait(cx, |_| !receipt(&folder, "send").is_empty());
    // The receipt precedes the answer; only the terminal response proves the session can accept the next send.
    wait(cx, |_| completed.get());
    assert_eq!(
        receipt(&folder, "send")[0].get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );
    assert_eq!(receipt(&folder, "hello").len(), 1);
    assert!(!std::fs::read_to_string(folder.join("requests.jsonl"))
        .unwrap()
        .contains("photo.png"));
    fixture.menu(cx, 0, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "plugin-send:example.send-target" && !r.disabled)
    });
    wait(cx, |cx| {
        update(cx, fixture.handle, |shell, _, _| {
            shell.menu.plugin_runtime.target_pending.is_empty()
        })
    });
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::write(
        folder.join("malformed-response"),
        "fixture protocol failure\n",
    )
    .unwrap();
    update(cx, fixture.handle, |shell, _, cx| {
        shell
            .plugins
            .host
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |host, cx| {
                host.request(
                    "example.send-target",
                    Json::Obj(vec![
                        ("op".into(), Json::Str("send".into())),
                        ("target".into(), Json::Str("receiver".into())),
                        (
                            "paths".into(),
                            Json::Arr(vec![Json::Str(
                                root.join("notes.txt").to_string_lossy().into(),
                            )]),
                        ),
                    ]),
                    cx,
                );
            });
    });
    wait(cx, |cx| {
        !fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "plugin-send:example.send-target")
    });
    stop(cx, &fixture);
}

#[path = "menu_key_tests.rs"]
mod key_tests;
