use super::{evaluated, fake_hyprctl, lua_exit_rule, lua_owned_rule, Sources, Tagger, GLASSY};

#[test]
fn glassy_rule_clean_exit_disables_only_its_own_enabled_rule() {
    let root = crate::compositor::tests::Fixture::new();
    let (home, state) = (root.0.join("home"), root.0.join("state"));
    let toggles = state.join("omarchy/toggles/hypr");
    std::fs::create_dir_all(&toggles).unwrap();
    std::fs::write(toggles.join("window-glassy.lua"), GLASSY).unwrap();
    std::fs::write(root.0.join("props"), "1 1\n").unwrap();
    let tagger = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &state),
    );
    tagger.pass();
    assert_eq!(tagger.lock().ruled, Some(true));
    tagger.stop();
    assert_eq!(tagger.lock().ruled, Some(false));
    assert_eq!(
        std::fs::read_to_string(root.0.join("rule")).unwrap(),
        "disabled\n"
    );
    // Neither a queued rerun nor a second stop may reinstall or disable the rule again.
    tagger.pass();
    tagger.retag();
    tagger.stop();
    assert!(!tagger.lock().busy);
    assert_eq!(
        std::fs::read_to_string(root.0.join("evals")).unwrap(),
        format!(
            "{}\n{}\n",
            evaluated(Some((0.75, 0.75))),
            lua_exit_rule(std::process::id()).replace('\n', " ")
        )
    );
    let fresh = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &state),
    );
    fresh.stop();
    assert_eq!(
        std::fs::read_to_string(root.0.join("evals"))
            .unwrap()
            .lines()
            .count(),
        2,
        "a tagger that never used the rule leaves it alone on exit"
    );
}

#[test]
fn glassy_rule_emits_lua_for_the_atomic_ownership_recheck() {
    for (name, script) in [
        ("enable11", lua_owned_rule(11, Some((0.75, 0.75)))),
        ("enable22", lua_owned_rule(22, Some((0.75, 0.75)))),
        ("exit11", lua_exit_rule(11)),
        ("exit22", lua_exit_rule(22)),
        ("disable", lua_owned_rule(11, None)),
    ] {
        assert!(script.contains("hl.window_rule("));
        println!("GLASSY_LUA_BEGIN {name}\n{script}\nGLASSY_LUA_END {name}");
    }
}

#[test]
fn glassy_rule_exit_preserves_a_survivor_then_last_process_cleans_up() {
    exit_pair(false);
}

#[test]
fn glassy_rule_overlapping_exits_clear_even_if_both_clients_remain_listed() {
    exit_pair(true);
}

#[test]
fn glassy_rule_fake_clients_change_without_rewriting_the_executable() {
    let root = crate::compositor::tests::Fixture::new();
    let hyprctl = fake_hyprctl(&root.0);
    let script = std::fs::read(&hyprctl).unwrap();
    let mut permissions = std::fs::metadata(&hyprctl).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&hyprctl, permissions).unwrap();
    let next_pid = std::process::id() + 1;
    std::fs::write(
        root.0.join("clients"),
        format!("[{{\"address\":\"0x2b\",\"pid\":{next_pid},\"class\":\"com.thisisgm.flea\"}}]"),
    )
    .unwrap();
    let output = std::process::Command::new(&hyprctl)
        .args(["clients", "-j"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        crate::window::glass_tag::own_addresses(
            &String::from_utf8(output.stdout).unwrap(),
            next_pid,
        ),
        ["0x2b"]
    );
    assert_eq!(std::fs::read(&hyprctl).unwrap(), script);
}

fn exit_pair(retain_closed: bool) {
    let root = crate::compositor::tests::Fixture::new();
    let (home, state) = (root.0.join("home"), root.0.join("state"));
    let toggles = state.join("omarchy/toggles/hypr");
    std::fs::create_dir_all(&toggles).unwrap();
    std::fs::write(toggles.join("window-glassy.lua"), GLASSY).unwrap();
    std::fs::write(root.0.join("props"), "1 1\n").unwrap();
    let hyprctl = fake_hyprctl(&root.0);
    let first = Tagger::new(
        std::process::id(),
        hyprctl.clone(),
        Sources::under(&home, &state),
    );
    first.pass();
    assert_eq!(first.lock().ruled, Some(true));
    let next_pid = std::process::id() + 1;
    let clients =
        format!("[{{\"address\":\"0x2b\",\"pid\":{next_pid},\"class\":\"com.thisisgm.flea\"}}]");
    std::fs::write(root.0.join("clients"), clients).unwrap();
    let survivor = Tagger::new(next_pid, hyprctl, Sources::under(&home, &state));
    survivor.pass();
    assert_eq!(survivor.lock().ruled, Some(true));
    if retain_closed {
        std::fs::write(root.0.join("clients"), format!(
            "[{{\"address\":\"0x1a\",\"pid\":{},\"class\":\"com.thisisgm.flea\"}},{{\"address\":\"0x2b\",\"pid\":{next_pid},\"class\":\"com.thisisgm.flea\"}}]",
            std::process::id()
        )).unwrap();
    }
    first.stop();
    assert_eq!(
        std::fs::read_to_string(root.0.join("props")).unwrap(),
        "0.75 0.75\n",
        "closing the enabling process must preserve a surviving Flea window's look"
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("evals"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    if !retain_closed {
        std::fs::write(root.0.join("clients"), "[]").unwrap();
    }
    survivor.stop();
    assert_eq!(
        std::fs::read_to_string(root.0.join("rule")).unwrap(),
        "disabled\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("evals"))
            .unwrap()
            .lines()
            .count(),
        4
    );
}
