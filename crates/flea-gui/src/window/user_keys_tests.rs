//! The user key file: load, reload, refusal, dispatch and plugin removal, on a real Shell with a sandboxed file.
use super::user_keys::plugin_actions;
use crate::{backend::Backend, ipc::server::Event, state::Shell};
use flea::{
    ipc::wire::{Reply, Request},
    keys::user::Verdict,
    ui_logic_plugin_settings::wire::{Plugin, Source},
};
use gpui::{TestAppContext, VisualTestContext};
use std::{path::PathBuf, process::Command, time::Duration};

fn scratch() -> PathBuf {
    let made = Command::new("mktemp")
        .args(["-d", "/tmp/flea-user-keys.XXXXXX"])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with("/tmp") && root.as_os_str().len() > 5);
    std::fs::write(root.join(".flea-test-sandbox"), "user keys fixture\n").unwrap();
    root
}

fn cleanup(root: &PathBuf) {
    assert!(
        root.is_absolute() && root.starts_with("/tmp") && root.join(".flea-test-sandbox").is_file()
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn checksums() -> Plugin {
    let text = include_str!("../../../../examples/plugins/checksums/manifest.json");
    Plugin {
        manifest: flea::ui_logic_plugin_settings::manifest::parse(text, false).unwrap(),
        source: Source::Builtin,
        enabled: true,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    }
}

fn settings_open(
    shell: &mut Shell,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: "settingsOpen".into(),
        arguments: vec![],
    };
    shell.ipc(Event::Call { request, answer }, window, cx);
    receive.try_recv().unwrap().reply
}

fn mount(cx: &mut TestAppContext, file: PathBuf) -> (gpui::Entity<Shell>, &mut VisualTestContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.launch.settings.data = flea::uistate::from_file("{}");
        shell.user_keys.path = Some(file);
        super::user_keys::test_fixture::seed(&mut shell, cx);
        shell.follow_user_keys(window, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    (shell, cx)
}

fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
}

#[gpui::test]
fn user_keys_load_reload_and_vanish_with_their_file(cx: &mut TestAppContext) {
    let root = scratch();
    let file = root.join("keys.toml");
    std::fs::write(&file, "[[ctrlshift]]\nkey = \"Y\"\naction = \"settings\"\n").unwrap();
    let (shell, cx) = mount(cx, file.clone());
    shell.update(cx, |shell, _| {
        assert_eq!(shell.user_keys.facts.keys.bindings.len(), 1)
    });
    std::fs::write(&file, "[[ctrlshift]]\nkey = \"Y\"\naction = \"settings\"\n[[ctrlshift]]\nkey = \"U\"\naction = \"toggleHidden\"\n").unwrap();
    settle(cx);
    shell.update(cx, |shell, _| {
        assert_eq!(shell.user_keys.facts.keys.bindings.len(), 2)
    });
    // An editor's atomic save is reloaded as well as an in-place write.
    std::fs::write(
        root.join("next.toml"),
        "[[ctrl]]\nkey = \"Y\"\naction = \"settings\"\n",
    )
    .unwrap();
    std::fs::rename(root.join("next.toml"), &file).unwrap();
    settle(cx);
    shell.update(cx, |shell, _| {
        assert_eq!(shell.user_keys.facts.keys.bindings[0].mods, "ctrl")
    });
    assert!(file.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_file(&file).unwrap();
    settle(cx);
    shell.update(cx, |shell, _| {
        assert!(shell.user_keys.facts.keys.bindings.is_empty())
    });
    cleanup(&root);
}

#[gpui::test]
fn a_bound_builtin_runs_through_the_key_path_and_a_taken_chord_never_does(cx: &mut TestAppContext) {
    let root = scratch();
    let file = root.join("keys.toml");
    std::fs::write(&file, "[[ctrlshift]]\nkey = \"Y\"\naction = \"settings\"\n[[ctrl]]\nkey = \"C\"\naction = \"settings\"\n").unwrap();
    let (shell, cx) = mount(cx, file);
    shell.update(cx, |shell, _| {
        let verdicts = shell.user_keys.facts.current_verdicts("default").unwrap();
        assert_eq!(verdicts, &[Verdict::Live, Verdict::Taken("copy")]);
        assert_eq!(
            shell.user_keys.said.len(),
            1,
            "the refusal is said on stderr once"
        );
    });
    cx.simulate_keystrokes("ctrl-c");
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(settings_open(shell, window, cx), Reply::value("false"));
    });
    cx.simulate_keystrokes("ctrl-shift-y");
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(settings_open(shell, window, cx), Reply::value("true"))
    });
    cleanup(&root);
}

#[gpui::test]
fn a_plugin_binding_lives_only_while_its_plugin_contributes(cx: &mut TestAppContext) {
    let root = scratch();
    let file = root.join("keys.toml");
    std::fs::write(
        &file,
        "[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n",
    )
    .unwrap();
    let (shell, cx) = mount(cx, file);
    let pressed = |shell: &Shell| {
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        shell.user_key("default", &event, gpui::Modifiers::default())
    };
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(pressed(shell), None, "no plugin declares it yet");
        let installed = vec![checksums()];
        shell.user_keys.facts.plugins = plugin_actions(&installed, &shell.launch.settings.data);
        shell.user_keys.resolve("default");
        assert_eq!(
            pressed(shell),
            Some(super::user_keys::Bound::Plugin(
                "example.checksums:sha256".into()
            ))
        );
        let rows =
            flea::ui_logic_settings_pages::pages::user_key_rows(&shell.user_keys.facts, "default");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].items[0].keys, "ctrl-shift-k");
        // Disabling the plugin in settings drops the row and the binding.
        let off =
            flea::uistate::from_file(r#"{"plugins":{"example.checksums":{"enabled":false}}}"#);
        shell.user_keys.facts.plugins = plugin_actions(&installed, &off);
        shell.user_keys.resolve("default");
        assert_eq!(pressed(shell), None);
        assert!(flea::ui_logic_settings_pages::pages::user_key_rows(
            &shell.user_keys.facts,
            "default"
        )
        .is_empty());
        // A failed plugin offers nothing either.
        let mut failed = checksums();
        failed.failure = Some(flea::ui_logic_plugin_settings::manifest::Invalid::new(
            "startup", "test",
        ));
        shell.user_keys.facts.plugins = plugin_actions(&[failed], &shell.launch.settings.data);
        shell.user_keys.resolve("default");
        assert_eq!(pressed(shell), None);
        // Repopulate before removal so the host refresh must actually clear the old binding.
        shell.user_keys.facts.plugins = plugin_actions(&installed, &shell.launch.settings.data);
        shell.user_keys.resolve("default");
        assert!(pressed(shell).is_some());
        shell.user_keys_changed(window, cx);
        assert_eq!(pressed(shell), None);
    });
    cleanup(&root);
}

#[gpui::test]
fn settings_keys_lists_the_binding_and_states_each_refusal(cx: &mut TestAppContext) {
    let root = scratch();
    let file = root.join("keys.toml");
    std::fs::write(
        &file,
        "[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n[[ctrl]]\nkey = \"C\"\naction = \"example.checksums:sha256\"\n[[ctrl]]\nkey = \"Nope\"\naction = \"copy\"\n",
    )
    .unwrap();
    let (shell, cx) = mount(cx, file);
    shell.update(cx, |shell, _| {
        assert_eq!(
            shell.user_keys.said.len(),
            1,
            "one parse problem is one stderr line"
        );
        assert!(shell.user_keys.said[0].contains(":7:"));
    });
    cx.simulate_keystrokes(",");
    shell.update_in(cx, |shell, _, cx| {
        shell.user_keys.facts.plugins = plugin_actions(&[checksums()], &shell.launch.settings.data);
        shell.user_keys.resolve("default");
        shell.sync_settings_user_keys(cx);
        let view = shell.settings.panel.clone().expect("settings open");
        view.update(cx, |view, _| {
            view.panel.show_section("keys");
            let labels: Vec<String> = view
                .panel
                .rows
                .iter()
                .map(|row| row.label.resolve(&view.panel.catalog))
                .collect();
            let at = view
                .panel
                .rows
                .iter()
                .position(|row| row.id == flea::ui_logic_settings_pages::pages::USER_KEYS)
                .expect("Your Keys row");
            let item = &view.panel.rows[at].items[0];
            assert_eq!(
                (
                    item.keys.as_str(),
                    item.label.resolve(&view.panel.catalog).as_str(),
                    item.glyph
                ),
                ("ctrl-shift-k", "Verify Checksum…", "hash")
            );
            assert!(
                labels
                    .iter()
                    .any(|l| l
                        == "ctrl-c stays unbound because Copy already uses it in this preset."),
                "{labels:?}"
            );
            assert!(
                labels
                    .iter()
                    .any(|l| l == "Line 7 of keys.toml is not a binding."),
                "{labels:?}"
            );
        });
    });
    cleanup(&root);
}

#[test]
fn user_key_reads_refuse_nonregular_paths_and_reload_on_permission_changes() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let root = scratch();
    let file = root.join("keys.toml");
    std::fs::create_dir(&file).unwrap();
    let (stamp, keys) = super::user_keys::read(&file, None).expect("invalid file is diagnosed");
    assert!(keys.bindings.is_empty());
    assert_eq!(keys.problems.len(), 1);
    assert!(keys.problems[0].detail.contains("regular file"));
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir(&file).unwrap();
    assert!(Command::new("mkfifo")
        .arg(&file)
        .status()
        .unwrap()
        .success());
    let (_, keys) = super::user_keys::read(&file, stamp).unwrap();
    assert_eq!(
        keys.problems.len(),
        1,
        "a FIFO returns rather than waiting for a writer"
    );
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    assert!(file.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, "[[ctrlshift]]\nkey = \"Y\"\naction = \"settings\"\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (stamp, _) = super::user_keys::read(&file, None).unwrap();
    let old = std::fs::metadata(&file).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let new = std::fs::metadata(&file).unwrap();
    assert_ne!(
        old.mode(),
        new.mode(),
        "the fixture must actually change permission mode"
    );
    assert_eq!(
        (old.ino(), old.size(), old.mtime(), old.mtime_nsec()),
        (new.ino(), new.size(), new.mtime(), new.mtime_nsec())
    );
    assert!(
        super::user_keys::read(&file, stamp).is_some(),
        "permission-only change causes another read"
    );
    let (retry, _) = super::user_keys::read(&root, None).unwrap();
    assert!(
        super::user_keys::read(&root, retry).is_some(),
        "failed reads retry without a metadata change"
    );
    let (_, missing) = super::user_keys::read(&root.join("missing"), retry).unwrap();
    assert!(
        missing.problems.is_empty(),
        "removing an unreadable path clears its diagnostic"
    );
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    cleanup(&root);
}

#[gpui::test]
fn user_keys_existing_idle_host_bootstraps_after_file_load(cx: &mut TestAppContext) {
    let root = scratch();
    let file = root.join("keys.toml");
    std::fs::write(
        &file,
        "[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n",
    )
    .unwrap();
    let host = cx.update(crate::g2::plugins::service);
    host.update(cx, |host, _| host.plugins = vec![checksums()]);
    // Hold the first Shell frame while the real read and deferred attachment run.
    let cx = cx.add_empty_window();
    let (backend, _requests) = Backend::test_channel();
    let shell = gpui::AppContext::new(cx, |cx| crate::state::tests::shell_with(backend, cx));
    shell.update_in(cx, |shell, window, cx| {
        shell.launch.settings.data = flea::uistate::from_file("{}");
        shell.user_keys.path = Some(file);
        super::user_keys::test_fixture::seed(shell, cx);
        shell.follow_user_keys(window, cx);
        shell.start_plugins(window, cx);
    });
    cx.run_until_parked();
    shell.update_in(cx, |shell, _, _| {
        assert!(
            shell.plugins.host.is_none(),
            "key file loads before attachment"
        );
        assert_eq!(shell.user_keys.facts.keys.bindings.len(), 1);
        assert!(!shell.user_keys.facts.declares("example.checksums:sha256"));
    });
    cx.executor().advance_clock(Duration::from_millis(2));
    cx.run_until_parked();
    shell.update(cx, |shell, _| {
        assert!(
            shell.plugins.host.is_some(),
            "deferred attachment completed"
        );
        assert!(
            shell.user_keys.facts.declares("example.checksums:sha256"),
            "attaching an idle existing host must initialize the key facts"
        );
        assert_eq!(
            shell.user_keys.facts.current_verdicts("default").unwrap(),
            &[Verdict::Live]
        );
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(
            shell.user_key("default", &event, gpui::Modifiers::none()),
            Some(super::user_keys::Bound::Plugin(
                "example.checksums:sha256".into()
            ))
        );
        let rows =
            flea::ui_logic_settings_pages::pages::user_key_rows(&shell.user_keys.facts, "default");
        assert_eq!(rows[1].items.len(), 1, "Settings shares the live verdict");
    });
    cleanup(&root);
}
