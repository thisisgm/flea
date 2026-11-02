//! Tests for the transparent tag, covering the decision table, the PROBE parser, address checks and the hyprctl-absent path.
use super::glass_tag::events::{read_events, socket2_path};
use super::glass_tag::run::{Sources, Tagger};
use super::glass_tag::{
    decide, dispatch_ran, lua_tag, own_addresses, parse_props, plan, retags, uses_default,
    valid_address, Action, Mode, TAG,
};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "glass_eval_tests.rs"]
mod eval;

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn glass_tag_decision_table_matches_the_0_3_12_rows() {
    use Action::{Add, Keep, Remove};
    use Mode::{Glassy, Opaque, Transparent};
    // Mode, uses default, opacity, inactive, tagged by Flea, expected action.
    let rows = [
        (Transparent, true, 1., 1., false, Add),
        (Transparent, true, 1., 1., true, Add),
        (Transparent, true, 0.985, 1., false, Keep),
        (Transparent, true, 0.985, 1., true, Keep),
        (Transparent, true, 1., 0.96, false, Keep),
        (Transparent, false, 1., 1., false, Keep),
        (Transparent, false, 1., 1., true, Remove),
        (Glassy, true, 1., 1., false, Keep),
        (Glassy, true, 1., 1., true, Remove),
        (Opaque, true, 1., 1., false, Keep),
        (Opaque, true, 0.985, 1., true, Remove),
        (Glassy, false, 0.985, 0.96, true, Remove),
        (Transparent, false, 0.985, 0.96, false, Keep),
    ];
    for (mode, uses, opacity, inactive, tagged, want) in rows {
        assert_eq!(
            decide(mode, uses, opacity, inactive, tagged),
            want,
            "{mode:?} default={uses} opacity={opacity} inactive={inactive} tagged={tagged}"
        );
    }
}

#[test]
fn glass_tag_adds_and_removes_only_where_the_rules_allow() {
    for mode in [Mode::Opaque, Mode::Glassy, Mode::Transparent] {
        for uses in [false, true] {
            for (opacity, inactive) in [(1., 1.), (0.985, 1.), (1., 0.96), (f64::NAN, 1.)] {
                for tagged in [false, true] {
                    let action = decide(mode, uses, opacity, inactive, tagged);
                    let wanted = mode == Mode::Transparent && uses;
                    if action == Action::Add {
                        assert!(wanted && opacity == 1. && inactive == 1.);
                    }
                    if action == Action::Remove {
                        assert!(tagged && !wanted);
                    }
                }
            }
        }
    }
}

#[test]
fn glass_tag_probe_line_takes_three_fields_and_a_lowercase_address() {
    let props = parse_props("0x55bacb0a1f80 0.985 0.96").unwrap();
    assert_eq!(props.address, "0x55bacb0a1f80");
    assert_eq!(props.opacity, 0.985);
    assert_eq!(props.inactive, 0.96);
    assert_eq!(
        parse_props("  0x1 1 1  ").map(|p| p.address),
        Some("0x1".to_owned())
    );
    for line in [
        "",
        "0x1 1",
        "0x1 1 1 1",
        "0x1G 1 1",
        "0X1 1 1",
        "0xABC 1 1",
        "0x 1 1",
        "55bacb 1 1",
    ] {
        assert!(parse_props(line).is_none(), "{line:?} must not parse");
    }
    assert!(parse_props("0x1 abc 1").is_some_and(|p| p.opacity.is_nan()));
}

#[test]
fn glass_tag_nan_opacity_never_adds_a_tag() {
    let text = "0x1 abc 1\n";
    let outcome = plan(text, Mode::Transparent, true, &BTreeSet::new());
    assert!(outcome.changes.is_empty());
}

#[test]
fn glass_tag_only_lowercase_hex_addresses_reach_a_dispatch() {
    for good in ["0x1", "0x55bacb0a1f80", "0xdeadbeef"] {
        assert!(valid_address(good), "{good} must be valid");
    }
    for bad in [
        "", "0x", "0X1", "0xG", "0x1 ", " 0x1", "0x1;rm", "55bacb", "0xABC", "0x1\n",
    ] {
        assert!(!valid_address(bad), "{bad:?} must be refused");
    }
}

#[test]
fn glass_tag_takes_only_this_process_windows_with_valid_addresses() {
    let json = r#"[{"address":"0x55bacb0a1f80","pid":4211,"class":"com.thisisgm.flea"},{"address":"0x2","pid":99},{"address":"0xBAD","pid":4211},{"address":"0x3","pid":"4211"},{"address":"0x4"}]"#;
    assert_eq!(own_addresses(json, 4211), vec!["0x55bacb0a1f80".to_owned()]);
    assert!(own_addresses("{}", 4211).is_empty());
    assert!(own_addresses("not json", 4211).is_empty());
    assert!(own_addresses("", 4211).is_empty());
}

#[test]
fn glass_tag_plan_tags_new_windows_and_drops_closed_ones() {
    let first = plan(
        "0x1 1 1\n0x2 0.985 1\n",
        Mode::Transparent,
        true,
        &BTreeSet::new(),
    );
    assert_eq!(first.changes, vec![("0x1".to_owned(), Action::Add)]);
    assert_eq!(first.tagged, set(&["0x1"]));
    // 0x1 has closed, so its record drops out, and 0x2 stays untagged.
    let later = plan("0x2 0.985 1\n", Mode::Transparent, true, &first.tagged);
    assert!(later.changes.is_empty());
    assert!(later.tagged.is_empty());
}

#[test]
fn glass_tag_glassy_takes_flea_tag_off_and_keeps_a_user_tag() {
    let glassy = plan("0x1 1 1\n0x3 1 1\n", Mode::Glassy, true, &set(&["0x1"]));
    assert_eq!(glassy.changes, vec![("0x1".to_owned(), Action::Remove)]);
    assert!(glassy.tagged.is_empty());
}

#[test]
fn glass_tag_keeps_its_record_of_a_window_whose_probe_failed() {
    let failed = plan("0x1  1\n", Mode::Glassy, true, &set(&["0x1"]));
    assert!(failed.changes.is_empty());
    assert_eq!(failed.tagged, set(&["0x1"]));
    let glassy = plan("0x1 1 1\n", Mode::Glassy, true, &failed.tagged);
    assert_eq!(glassy.changes, vec![("0x1".to_owned(), Action::Remove)]);
    // A failed probe of a window Flea never tagged adds no record.
    assert!(plan("0x2\n", Mode::Glassy, true, &set(&["0x1"]))
        .tagged
        .is_empty());
}

#[test]
fn glass_tag_dispatch_counts_only_an_ok_reply() {
    assert!(dispatch_ran("ok"));
    assert!(dispatch_ran("ok\n"));
    assert!(!dispatch_ran("Invalid dispatcher"));
    assert!(!dispatch_ran(
        "warning: =[C]:-1: hl.focus: window not found"
    ));
    assert!(!dispatch_ran(""));
}

#[test]
fn glass_tag_dispatch_strings_name_the_window_and_the_tag() {
    assert_eq!(
        lua_tag("address:0x1", "+transparent"),
        r#"hl.dsp.window.tag({ window = "address:0x1", tag = "+transparent" })"#
    );
    assert_eq!(TAG, "transparent");
}

#[test]
fn glass_tag_uses_default_unless_a_flea_alpha_is_set() {
    assert!(uses_default("", ""));
    assert!(uses_default("[font]\nbackground-alpha = 0.5\n", ""));
    assert!(uses_default("[flea]\nbackground-alpha = \"\"\n", ""));
    assert!(!uses_default("[flea]\nbackground-alpha = 0.34\n", ""));
    assert!(!uses_default(
        "",
        "[flea]\nbackground-alpha = 0.34 # theme\n"
    ));
    assert!(!uses_default("[flea]\nbackground-alpha = nope\n", ""));
}

#[test]
fn glass_tag_mode_follows_the_toggle_files() {
    assert_eq!(Mode::from_toggles(false, false), Mode::Transparent);
    assert_eq!(Mode::from_toggles(false, true), Mode::Opaque);
    assert_eq!(Mode::from_toggles(true, false), Mode::Glassy);
    assert_eq!(Mode::from_toggles(true, true), Mode::Glassy);
}

#[test]
fn glass_tag_reruns_on_config_reload_and_on_its_own_window_only() {
    assert!(retags("configreloaded>>"));
    assert!(retags(
        "openwindow>>55bacb0a1f80,1,com.thisisgm.flea,Flea, with commas"
    ));
    assert!(!retags("openwindow>>55bacb0a1f80,1,flea,Flea"));
    assert!(!retags(
        "openwindow>>55bacb0a1f80,1,com.thisisgm.flea.picker,Open File"
    ));
    assert!(!retags("activewindow>>flea,Flea"));
    assert!(!retags("configreloaded"));
}

#[test]
fn glass_tag_event_socket_sits_where_hyprland_puts_it() {
    assert_eq!(
        socket2_path(Some(OsStr::new("/run/user/1000")), OsStr::new("sig")),
        PathBuf::from("/run/user/1000/hypr/sig/.socket2.sock")
    );
    assert_eq!(
        socket2_path(None, OsStr::new("sig")),
        PathBuf::from("/tmp/hypr/sig/.socket2.sock")
    );
    assert_eq!(
        socket2_path(Some(OsStr::new("")), OsStr::new("sig")),
        PathBuf::from("/tmp/hypr/sig/.socket2.sock")
    );
}

#[test]
fn glass_tag_without_hyprctl_leaves_windows_alone_and_frees_the_tagger() {
    let missing = OsString::from("/nonexistent/flea-no-hyprctl");
    let tagger = Tagger::new(std::process::id(), missing, Sources::from_env());
    // A pass that wrote its plan after a failed query would drop this record.
    tagger.lock().tagged.insert("0x1".into());
    tagger.pass();
    assert_eq!(tagger.lock().tagged, set(&["0x1"]));
    tagger.retag();
    let deadline = Instant::now() + Duration::from_secs(5);
    while tagger.lock().busy && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !tagger.lock().busy,
        "a pass must clear busy when hyprctl is absent"
    );
    assert_eq!(tagger.lock().tagged, set(&["0x1"]));
}

#[test]
fn glass_tag_keeps_reading_past_a_title_that_is_not_utf8() {
    let events: &[u8] = b"activewindow>>flea,\xff\xfe\nopenwindow>>55bacb0a1f80,1,com.thisisgm.flea,Ti\xfftle\nconfigreloaded>>\n";
    let mut passes = 0;
    read_events(events, Path::new("/tmp/hypr/sig/.socket2.sock"), || {
        passes += 1;
    });
    assert_eq!(
        passes, 2,
        "a raw title byte neither stops the events nor hides this app's window opening"
    );
}

// A hyprctl that lists one window of this process at opacity 1 and answers every dispatch with the reply file.
fn fake_hyprctl(root: &Path) -> OsString {
    let path = root.join("hyprctl");
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\nclients) printf '[{{\"address\":\"0x1a\",\"pid\":{}}}]' ;;\ngetprop) echo 1 ;;\ndispatch) cat \"${{0%/*}}/reply\" ;;\nesac\n",
        std::process::id()
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path.into_os_string()
}

#[test]
fn glass_tag_records_only_the_dispatches_hyprland_ran() {
    let root = crate::compositor::tests::Fixture::new();
    let state = root.0.join("state");
    let tagger = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&root.0.join("home"), &state),
    );
    let reply = |text: &str| std::fs::write(root.0.join("reply"), text).unwrap();
    reply("Invalid dispatcher");
    tagger.pass();
    assert!(
        tagger.lock().tagged.is_empty(),
        "a refused add leaves no record, so the next pass adds again"
    );
    reply("ok");
    tagger.pass();
    assert_eq!(tagger.lock().tagged, set(&["0x1a"]));
    let toggles = state.join("omarchy/toggles/hypr");
    std::fs::create_dir_all(&toggles).unwrap();
    std::fs::write(toggles.join("window-glassy.lua"), "").unwrap();
    reply("Invalid dispatcher");
    tagger.pass();
    assert_eq!(
        tagger.lock().tagged,
        set(&["0x1a"]),
        "a refused remove keeps the record, so the next pass removes again"
    );
    reply("ok");
    tagger.pass();
    assert!(tagger.lock().tagged.is_empty());
}

// Turns a pass that never starts into a failure instead of a hang.
const PASS_GUARD: Duration = Duration::from_secs(10);

// A live [flea] background-alpha edit sends Hyprland no event, so the window's follow must take Flea's tag off (THEME-GLASS: native2 v0.3.12 at opacity 1, GPUI kept 0.985).
#[test]
fn glass_tag_follows_a_live_alpha_edit_without_a_hyprland_event() {
    let root = crate::compositor::tests::Fixture::new();
    let home = root.0.join("home");
    let tagger = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &root.0.join("state")),
    );
    std::fs::write(root.0.join("reply"), "ok").unwrap();
    let (ended, passes) = std::sync::mpsc::channel();
    tagger.lock().ended = Some(ended);
    let automatic = super::glass::Inputs::default();
    tagger.follow(automatic);
    passes
        .recv_timeout(PASS_GUARD)
        .expect("the window's first inputs start a pass");
    assert_eq!(tagger.lock().tagged, set(&["0x1a"]));
    tagger.follow(automatic);
    let running = tagger.lock().busy;
    assert!(
        !running && passes.try_recv().is_err(),
        "inputs the window already applied start no pass"
    );
    let shell = home.join(".config/omarchy/shell.toml");
    std::fs::create_dir_all(shell.parent().unwrap()).unwrap();
    std::fs::write(&shell, "[flea]\nbackground-alpha = 0.6\n").unwrap();
    tagger.follow(super::glass::Inputs {
        automatic: false,
        wanted_alpha: 153,
        ..automatic
    });
    passes
        .recv_timeout(PASS_GUARD)
        .expect("a live alpha edit starts a pass though Hyprland sent no event");
    assert!(tagger.lock().tagged.is_empty());
}
