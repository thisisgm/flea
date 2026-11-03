use super::*;

#[test]
fn startup_opening_and_tab_policies_match_all_oracle_cases() {
    let home = "/home/test";
    for (state, named, expected) in [
        ("{}", "", home),
        ("null", "", home),
        (
            r#"{"startIn":"folder","startFolder":"/work"}"#,
            "/asked",
            "/asked",
        ),
        ("{}", "/asked", "/asked"),
        (r#"{"startIn":"folder","startFolder":"/work"}"#, "", "/work"),
        (
            r#"{"startIn":"last","lastPath":"/pictures/2026"}"#,
            "",
            "/pictures/2026",
        ),
        (r#"{"startIn":"last"}"#, "", home),
        (r#"{"startIn":"folder","startFolder":""}"#, "", home),
        (r#"{"startIn":"fromANewerFlea"}"#, "", home),
        (r#"{"startIn":"last","lastPath":null}"#, "", home),
    ] {
        assert_eq!(
            settings(state).start_path(home, Some(named)),
            expected,
            "{state} {named}"
        );
    }
    for (state, expected) in [
        ("{}", "/downloads"),
        (r#"{"newTab":"home"}"#, home),
        (
            r#"{"newTab":"start","startIn":"folder","startFolder":"/srv"}"#,
            "/srv",
        ),
        (
            r#"{"newTab":"start","startIn":"last","lastPath":"/vault"}"#,
            "/vault",
        ),
        (r#"{"newTab":"start","startIn":"folder"}"#, home),
        (r#"{"newTab":"fromANewerFlea"}"#, "/downloads"),
        (r#"{"newTab":"start"}"#, home),
    ] {
        assert_eq!(
            settings(state).new_tab_path("/downloads", home),
            expected,
            "{state}"
        );
    }
    assert_eq!(LISTING_WAIT_MS, 10_000);
    assert_eq!(INSPECT_WAIT_MS, 10_000);
}

#[test]
fn every_saved_dual_launch_precedence_case_matches_the_oracle() {
    for (dual, start, named, expected, side) in [
        (
            r#"{"paths":["/music","/work"],"focus":0}"#,
            "/home/test",
            "",
            ["/music", "/work"],
            None,
        ),
        (
            r#"{"paths":["/music","/work"],"focus":0}"#,
            "/asked",
            "/asked",
            ["/asked", "/work"],
            Some(0),
        ),
        (
            r#"{"paths":["/music","/work"],"focus":0}"#,
            "/pictures",
            "/asked",
            ["/asked", "/work"],
            Some(0),
        ),
        (
            r#"{"paths":["/music","/work"],"focus":1}"#,
            "/asked",
            "/asked",
            ["/music", "/asked"],
            Some(1),
        ),
        (
            r#"{"paths":["/music","/work"],"focus":1}"#,
            "/pictures",
            "/asked",
            ["/music", "/asked"],
            Some(1),
        ),
        (
            r#"{"paths":["/music","/work"]}"#,
            "/asked",
            "/asked",
            ["/asked", "/work"],
            Some(0),
        ),
        ("{}", "/pictures", "", ["/pictures", "/pictures"], None),
        ("null", "/pictures", "/asked", ["/asked", "/asked"], Some(0)),
        (
            r#"{"paths":["/music"],"focus":1}"#,
            "/pictures",
            "/asked",
            ["/asked", "/asked"],
            Some(0),
        ),
    ] {
        let state = settings(&format!(r#"{{"dual":{dual}}}"#));
        let (paths, launch_side) = state.dual_paths(start, Some(named));
        assert_eq!(paths, expected, "{dual} {start} {named}");
        assert_eq!(launch_side, side);
    }
}

#[test]
fn fresh_list_compact_and_saved_state_are_preserved_without_a_migration() {
    let raw = r#"{"view":"list","columnsLimit":3,"density":"compact","keys":"vim","lastTabs":{"paths":["/a","/b"],"index":1},"places":{"favourites":["/kept"]},"future":42}"#;
    let json = jsondoc::parse(raw).unwrap();
    let saved = Settings::from_json(json.clone());
    assert_eq!(saved.view, View::List);
    assert_eq!(saved.data, json);
    let fresh = Settings::from_json(Json::Null);
    assert_eq!(fresh.view, View::List);
    assert_eq!(fresh.columns, 3);
    assert_eq!(fresh.rail_width, 192.);
    let launch = Launch::resolve(
        settings(r#"{"view":"dual","dual":{"paths":["/left","/right"],"focus":1}}"#),
        Arguments {
            path: Some("/asked".into()),
            select: Some("/asked/file".into()),
        },
        "/home/test",
    );
    assert_eq!(launch.paths, ["/left", "/asked"]);
    assert_eq!(launch.select, [None, Some("/asked/file".into())]);
    assert_eq!(launch.focus, 1);
}

#[test]
fn argv_is_literal_and_unnamed_launches_stay_unnamed_until_settings_resolve() {
    let args = |values: &[&str]| Arguments::parse(values.iter().map(|v| (*v).into()));
    assert_eq!(args(&[]).unwrap(), Arguments::default());
    assert_eq!(
        args(&["--select", "/a/selected", "--", "/a"])
            .unwrap()
            .path
            .as_deref(),
        Some("/a")
    );
    assert_eq!(
        args(&["--", "-strange\nname"]).unwrap().path.as_deref(),
        Some("-strange\nname")
    );
    for invalid in [
        vec!["--select"],
        vec!["--bad"],
        vec!["/a", "/b"],
        vec!["/a", "--", "/b"],
        vec!["--", "/a", "/b"],
    ] {
        assert!(args(&invalid).is_err(), "{invalid:?}");
    }
    let mut relative = args(&["--select", "folder/item"]).unwrap();
    relative.absolutize(std::path::Path::new("/cwd"));
    assert_eq!(relative.path.as_deref(), Some("/cwd/folder"));
    assert_eq!(relative.select.as_deref(), Some("/cwd/folder/item"));
    let command = flea_gui::window::startup::new_window_command("/files/a b\n$(data)");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["--gui", "/files/a b\n$(data)"]
    );
    assert!(command
        .get_envs()
        .any(|(key, value)| key == "FLEA_FRONTEND" && value == Some(std::ffi::OsStr::new("gpui"))));
}
