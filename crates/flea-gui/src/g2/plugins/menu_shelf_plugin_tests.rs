//! EY-550: a shelf plugin someone added gets Add to Shelf while the built-in Shelf is off, and the files reach it.
use super::*;
use flea::ui_logic_plugin_settings::wire;

const MANIFEST: &str = r#"{"schemaVersion":1,"id":"example.pile","name":"Pile","version":"1.0.0","author":"Me","license":"MIT",
"description":"Switches a bar pile.","kinds":["shelf"],"entryPoints":{"run":"run"},
"shelf":{"omarchyPlugin":"pile"},"access":[]}"#;

// Answers hello, then writes each request beside it and answers done.
const RUN: &str = "#!/bin/sh\nread hello\nprintf '%s\\n' '{\"op\":\"ready\"}'\nwhile IFS= read -r line; do printf '%s\\n' \"$line\" >> requests.jsonl; printf '%s\\n' '{\"op\":\"done\"}'; done\n";

// Counts each start in the plugin's folder, then exits before hello.
const CRASH: &str = "#!/bin/sh\necho start >> starts\nexit 4\n";

fn pile(root: &Path, run: &str) -> (Plugin, PathBuf) {
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let folder = root.join("config/flea/plugins/example.pile");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), MANIFEST).unwrap();
    flea::executable_file::write(&folder.join("run"), run.as_bytes(), 0o755).unwrap();
    std::fs::create_dir_all(root.join("state/flea")).unwrap();
    std::fs::write(
        root.join("state/flea/ui.json"),
        r#"{"plugins":{"example.pile":{"enabled":true}},"shelf":{"enabled":false}}"#,
    )
    .unwrap();
    let plugin = Plugin {
        manifest: manifest::parse(MANIFEST, false).unwrap(),
        source: Source::Folder(folder.to_string_lossy().into()),
        enabled: true,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    };
    (plugin, folder)
}

// The real plugin host over the scratch homes, with the built-in Shelf off, once it lists the added pile.
fn real_host(cx: &mut TestAppContext, fixture: &mut Fixture, root: &Path) {
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
        shell.launch.settings.data = flea::uistate::from_file(r#"{"shelf":{"enabled":false}}"#);
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
            let host = shell.plugins.host.as_ref().unwrap().read(cx);
            host.plugins
                .iter()
                .any(|p| p.manifest.id == "example.pile" && p.enabled && p.failure.is_none())
        })
    });
}

#[gpui::test]
fn add_to_shelf_reaches_an_added_shelf_plugin_while_the_builtin_is_off(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, folder) = pile(&root, RUN);
    let mut fixture = open(cx, root.clone(), plugin);
    real_host(cx, &mut fixture, &root);
    fixture.menu(cx, 0, vec![]);
    // Opening a file's menu readies the added shelf, and its Add to Shelf row joins the open menu.
    wait(cx, |cx| {
        fixture.entries(cx).iter().any(|entry| entry.id == "shelf")
    });
    let validate = fixture.choose(cx, "addToShelf", "").unwrap();
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    wait(cx, |cx| {
        !receipt(&folder, "shelf").is_empty()
            && update(cx, fixture.handle, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().status.as_str() == "Added it to the shelf"
            })
    });
    let requests = receipt(&folder, "shelf");
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(
        requests[0].get("action").and_then(Json::as_str),
        Some("add")
    );
    assert_eq!(
        requests[0]
            .get("paths")
            .and_then(Json::as_array)
            .map(|paths| paths.iter().filter_map(Json::as_str).collect::<Vec<_>>()),
        Some(vec![root.join("notes.txt").to_str().unwrap()])
    );
    stop(cx, &fixture);
}

#[gpui::test]
fn add_to_shelf_follows_the_builtin_switch_even_when_the_listing_says_on(cx: &mut TestAppContext) {
    let root = scratch();
    let (plugin, _) = pile(&root, RUN);
    let fixture = open(cx, root, plugin);
    let (manifest, _) = flea::ui_logic_plugin_settings::builtin::manifests()
        .into_iter()
        .find(|(m, _)| m.id == "flea.shelf")
        .unwrap();
    update(cx, fixture.handle, |shell, _, _| {
        // Only the built-in Shelf, listed as on and healthy, so shelf.enabled alone decides the row.
        shell.menu.plugin_runtime.plugins = vec![Plugin {
            manifest,
            source: Source::Builtin,
            enabled: true,
            failure: None,
            mark_path: String::new(),
            settings: Vec::new(),
            shelf: None,
        }];
        shell.launch.settings.data = flea::uistate::from_file(r#"{"shelf":{"enabled":false}}"#);
        assert!(shell.menu_hidden().contains(&"shelf".to_owned()));
        shell.launch.settings.data = flea::uistate::from_file(r#"{"shelf":{"enabled":true}}"#);
        assert!(!shell.menu_hidden().contains(&"shelf".to_owned()));
        shell.menu.plugin_runtime.plugins.clear();
        assert!(shell.menu_hidden().contains(&"shelf".to_owned()));
    });
}

#[gpui::test]
fn a_shelf_plugin_that_cannot_start_is_started_once_across_menu_opens(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, folder) = pile(&root, CRASH);
    let mut fixture = open(cx, root.clone(), plugin);
    real_host(cx, &mut fixture, &root);
    let handle = fixture.handle;
    let failed = |cx: &mut TestAppContext| {
        update(cx, handle, |shell, _, cx| {
            let host = shell.plugins.host.as_ref().unwrap().read(cx);
            host.plugins
                .iter()
                .any(|p| p.manifest.id == "example.pile" && p.failure.is_some())
        })
    };
    for _ in 0..3 {
        fixture.menu(cx, 0, vec![]);
        wait(cx, &failed);
        assert!(!fixture.entries(cx).iter().any(|entry| entry.id == "shelf"));
        update(cx, fixture.handle, |shell, window, cx| {
            shell.retire_menu(window, cx)
        });
    }
    // The failed listing keeps prepare and the backend from starting it again, with no restart budget in the session.
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(folder.join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    stop(cx, &fixture);
}
