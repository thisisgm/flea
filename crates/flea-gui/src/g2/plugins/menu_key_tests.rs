//! A key bound in the user's keys.toml runs a plugin menu action through the menu's own route, end to end.
use super::*;

fn requests(fixture: &mut Fixture, cx: &mut TestAppContext, op: &str) -> Json {
    let mut found = None;
    // The seam polls its readiness on executor timers, which a test clock moves only when told.
    wait(cx, |cx| {
        cx.executor().advance_clock(Duration::from_millis(25));
        found = found.take().or_else(|| {
            fixture
                .drain()
                .into_iter()
                .find(|r| r.get("op").and_then(Json::as_str) == Some(op))
        });
        found.is_some()
    });
    found.unwrap()
}

#[gpui::test]
fn plugin_key_binding_runs_the_menu_action_with_its_validation_and_input_card(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    drop(runtime);
    let mut fixture = open(cx, root.clone(), plugin.clone());
    let folder = attach(cx, &mut fixture, &root, "checksums", &plugin);
    let file = root.join("config/flea/keys.toml");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        "[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n",
    )
    .unwrap();
    update(cx, fixture.handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().selected = vec![0];
        shell.user_keys.path = Some(file.clone());
        crate::window::user_keys::test_fixture::seed(shell, cx);
        shell.follow_user_keys(window, cx);
        shell
            .plugins
            .host
            .clone()
            .unwrap()
            .update(cx, |host, cx| host.prepare("menu-action", cx));
    });
    wait(cx, |cx| {
        update(cx, fixture.handle, |shell, _, cx| {
            shell.user_keys.facts.declares("example.checksums:sha256")
                && shell.user_keys.facts.keys.bindings.len() == 1
                && shell.menu_mimes_loaded()
                && shell.plugins.host.as_ref().is_some_and(|host| {
                    let host = host.read(cx);
                    host.plugins
                        .iter()
                        .any(|p| p.manifest.id == "example.checksums" && host.healthy(p))
                })
        })
    });
    fixture.drain();
    update(cx, fixture.handle, |shell, window, cx| {
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: true,
            prefer_character_input: false,
        };
        shell.key(&event, window, cx);
    });
    cx.run_until_parked();
    assert!(
        fixture
            .drain()
            .iter()
            .all(|r| r.get("op").and_then(Json::as_str) != Some("snapshot")),
        "autorepeat must not start an action"
    );
    update(cx, fixture.handle, |shell, window, cx| {
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        shell.key(&event, window, cx);
    });
    let snapshot = requests(&mut fixture, cx, "snapshot");
    fixture.feed(
        cx,
        Json::Obj(vec![
            ("t".into(), Json::Str("menuaction".into())),
            ("op".into(), Json::Str("snapshot".into())),
            ("id".into(), snapshot.get("id").unwrap().clone()),
            ("ok".into(), Json::Bool(true)),
            ("kinds".into(), Json::Arr(vec![])),
        ]),
    );
    let validate = requests(&mut fixture, cx, "validate");
    assert_eq!(
        validate.get("action").and_then(Json::as_str),
        Some("example.checksums:sha256")
    );
    update(cx, fixture.handle, |shell, _, _| {
        assert!(!shell.menu_opened(), "the key draws no menu")
    });
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert!(
        receipt(&folder, "run").is_empty(),
        "the input card comes first, as from the row"
    );
    let revalidate = fixture.submit_input(cx, CHECKSUM).unwrap();
    fixture.validated(cx, &revalidate, &[root.join("notes.txt")]);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::write(folder.join("release-terminal"), "release\n").unwrap();
    wait(cx, |_| !receipt(&folder, "run").is_empty());
    assert_eq!(
        receipt(&folder, "run")[0].get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );
    update(cx, fixture.handle, |shell, window, cx| {
        assert!(!shell.user_keys.facts.plugins.is_empty());
        let off =
            flea::uistate::from_file(r#"{"plugins":{"example.checksums":{"enabled":false}}}"#);
        shell.apply_settings(off, window, cx);
        assert!(shell.user_keys.facts.plugins.is_empty());
        assert!(flea::ui_logic_settings_pages::pages::user_key_rows(
            &shell.user_keys.facts,
            "default"
        )
        .is_empty());
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(
            shell.user_key("default", &event, gpui::Modifiers::default()),
            None
        );
    });
    stop(cx, &fixture);
}
