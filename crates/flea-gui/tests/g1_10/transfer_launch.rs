use super::*;

#[test]
fn a_tear_off_hands_its_window_the_lift_and_a_new_window_inherits_none_of_it() {
    let mut world = World::one(Side::pair("/tmp/duplicate"));
    world.side(0).pane.rows = Some(Box::new(|i| row(&format!("tear-{i}"), false)));
    let mut bar = bar();
    bar.tab_lift_began(&mut world, 1, NOW);
    let token = bar.out_token.clone();
    let tear = bar.tear_off_at(NOW).unwrap();
    assert_eq!(
        world.side(0).count(),
        2,
        "spawn without ack keeps source pair"
    );
    assert_eq!(tear.pid, "111", "tearoff passes source pid");
    assert!(
        !token.is_empty() && tear.token == token,
        "tearoff passes captured token"
    );
    assert_eq!(
        tear.cursor, "tear-4",
        "tearoff passes captured cursor filename"
    );
    let command = tear_off_command("/stub/flea", &tear);
    let set: BTreeMap<String, String> = command
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?.to_owned(), value?.to_str()?.to_owned())))
        .collect();
    assert_eq!(
        set.get("FLEA_TAB_SOURCE_PID").map(String::as_str),
        Some("111"),
        "the source pid reaches the new window"
    );
    assert_eq!(set.get("FLEA_TAB_TOKEN"), Some(&token), "and the token");
    assert_eq!(
        set.get("FLEA_TAB_CURSOR").map(String::as_str),
        Some("tear-4"),
        "and the cursor filename"
    );
    let names: Vec<&str> = set
        .keys()
        .map(String::as_str)
        .filter(|name| name.starts_with("FLEA_TAB_"))
        .collect();
    assert_eq!(
        names,
        [
            "FLEA_TAB_CURSOR",
            "FLEA_TAB_SORT",
            "FLEA_TAB_SOURCE_PID",
            "FLEA_TAB_TOKEN"
        ],
        "tearoff sets exactly the names a fresh launch drops"
    );
    let args: Vec<&str> = command.get_args().filter_map(|arg| arg.to_str()).collect();
    assert_eq!(
        args,
        ["--gui", "/tmp/duplicate"],
        "and opens the lifted tab's folder in the GPUI window"
    );
    assert_eq!(
        (
            bar.out_token.as_str(),
            bar.lifts[0].lifted_at,
            bar.out_active
        ),
        (token.as_str(), NOW, false),
        "and the lift now waits for the new window's acknowledgment"
    );

    let mut launch_pane =
        Side::new(Fake::at("/tmp/tear").with_rows(|i| row(&format!("tear-{i}"), false)));
    launch_pane
        .state
        .prepare_cursor(&launch_pane.pane, "tear-7");
    launch_pane.state.apply_pending(&mut launch_pane.pane);
    assert_eq!(
        launch_pane.pane.cursor, 7,
        "tearoff launch restores cursor filename after rows"
    );

    let mut fresh = std::process::Command::new("/stub/flea");
    for name in TEAR_OFF_ENV {
        fresh.env(name, "stale");
    }
    fresh_launch(&mut fresh);
    for name in TEAR_OFF_ENV {
        assert!(
            fresh
                .get_envs()
                .any(|(key, value)| key == name && value.is_none()),
            "a new window drops {name}"
        );
    }
    let core = include_str!("../../../../src/tearoff.rs");
    assert!(
        core.contains(r#"pub const ENV: [&str; 3] = ["FLEA_TAB_SOURCE_PID", "FLEA_TAB_CURSOR", "FLEA_TAB_TOKEN"];"#),
        "the hand-off is the same three names the core drops from everything else it starts"
    );
    assert_eq!(
        TEAR_OFF_ENV,
        ["FLEA_TAB_SOURCE_PID", "FLEA_TAB_CURSOR", "FLEA_TAB_TOKEN"],
        "in the core's order"
    );
}
