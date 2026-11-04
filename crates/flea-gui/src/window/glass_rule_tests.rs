//! Flea follows Nautilus under Omarchy at each Window Transparency level: the tag, the Glassy rule and solid automatic grounds.
use super::glass::Inputs;
use super::glass_tag::run::{Sources, Tagger};
use super::glass_tag::{
    lua_exit_rule, lua_owned_rule, lua_rule, nautilus_opacity, parse_props, rule_step, Mode, Props,
};
use std::ffi::OsString;
use std::path::Path;

#[path = "glassy_rule_lifecycle_tests.rs"]
mod lifecycle;

// The Glassy toggle Omarchy 4.5.0rc2 installs, /usr/share/omarchy/default/hypr/toggles/window-glassy.lua lines 5 to 8.
const GLASSY: &str = "require(\"default.hypr.glass\")()\n\n-- GTK apps fade as a whole, since GTK draws their backgrounds solid, and get\n-- their blur from the backdrop-filter Glassy adds to the user's gtk.css.\no.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"0.75 0.75\" })\n";
// The Opaque toggle it installs, toggles/window-opaque.lua.
const OPAQUE: &str = "-- Opaque windows, even those Omarchy makes transparent by default.\no.window({ tag = \"transparent-window\" }, { opacity = \"1 1\" })\n";

fn props(opacity: f64, inactive: f64) -> Props {
    parse_props(&format!("0x1a {opacity} {inactive}")).unwrap()
}

#[test]
fn glass_rule_reads_nautilus_opacity_from_the_glassy_toggle() {
    assert_eq!(nautilus_opacity(GLASSY), Some((0.75, 0.75)));
    assert_eq!(
        nautilus_opacity("o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"0.8 0.6\" })"),
        Some((0.8, 0.6)),
        "Flea follows a changed number, active then inactive"
    );
    assert_eq!(
        nautilus_opacity("o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"0.7\" })"),
        Some((0.7, 0.7)),
        "one value sets both, as in Hyprland"
    );
    for text in [
        OPAQUE,
        "",
        "-- o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"0.75 0.75\" })",
        "o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"1.5 0.75\" })",
        "o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"0.75 0.75 0.5\" })",
        "o.window(\"org\\\\.gnome\\\\.Nautilus\", { opacity = \"high\" })",
    ] {
        assert_eq!(nautilus_opacity(text), None, "no opacity in {text:?}");
    }
}

#[test]
fn glass_rule_lua_names_flea_class_and_disables_by_name() {
    assert_eq!(
        lua_rule(Some((0.75, 0.75))),
        "hl.window_rule({ name = \"flea-glassy\", enabled = true, match = { class = \"^com\\\\.thisisgm\\\\.flea$\" }, opacity = \"0.75 0.75\" })"
    );
    assert_eq!(
        lua_rule(None),
        "hl.window_rule({ name = \"flea-glassy\", enabled = false })"
    );
}

#[test]
fn glass_rule_evaluates_only_where_hyprland_draws_otherwise() {
    let glassy = Some((0.75, 0.75));
    let on = Some(true);
    assert_eq!(rule_step(glassy, &[props(1., 1.)], None), Some(glassy));
    assert_eq!(
        rule_step(glassy, &[props(1., 1.)], on),
        Some(glassy),
        "a config reload dropped the rule, so it is added again"
    );
    assert_eq!(rule_step(glassy, &[props(0.75, 1.)], on), Some(glassy));
    assert_eq!(rule_step(glassy, &[props(0.75, 0.75)], on), None);
    assert_eq!(
        rule_step(glassy, &[props(0.75, 0.75)], None),
        Some(glassy),
        "a fresh process confirms the shared rule so it can clean up as the last survivor"
    );
    assert_eq!(
        rule_step(glassy, &[props(f64::NAN, f64::NAN)], None),
        None,
        "a failed probe never appends the rule again"
    );
    assert_eq!(rule_step(None, &[props(0.75, 0.75)], on), Some(None));
    assert_eq!(rule_step(None, &[props(1., 1.)], None), Some(None));
    assert_eq!(
        rule_step(None, &[props(0.75, 0.75)], None),
        Some(None),
        "a rule an earlier Flea process left on is disabled"
    );
    assert_eq!(
        rule_step(None, &[props(0.75, 0.75)], Some(false)),
        None,
        "a process that disabled the rule does not disable it again"
    );
}

#[test]
fn glass_rule_material_floor_preserves_mode_and_explicit_precedence() {
    let alpha = "[flea]\nbackground-alpha = 0.6\n";
    for (opaque, glassy, mode) in [
        (false, false, Mode::Transparent),
        (true, false, Mode::Opaque),
        (false, true, Mode::Glassy),
    ] {
        let automatic = Inputs::resolve("", "", "", opaque, glassy);
        assert_eq!((automatic.mode, automatic.automatic), (mode, true));
        let expected = if mode == Mode::Glassy { 192 } else { 255 };
        assert_eq!(automatic.wanted_alpha, expected, "{mode:?} material floor");
        assert!(!automatic.pins_opaque);
        let user = Inputs::resolve("", "", alpha, opaque, glassy);
        let theme = Inputs::resolve("", alpha, "", opaque, glassy);
        let explicit = if mode == Mode::Opaque { 255 } else { 153 };
        assert_eq!(theme.wanted_alpha, explicit, "{mode:?} theme alpha");
        assert_eq!(user.wanted_alpha, 153, "{mode:?} user alpha");
        assert!(!user.automatic && !theme.automatic);
    }
}

// The first Lua line, for example "-- flea-glassy enable 42", identifies an ownership operation to the fake compositor.
fn fake_hyprctl(root: &Path) -> OsString {
    let path = root.join("hyprctl");
    let script = format!(
        r#"#!/bin/sh
d="${{0%/*}}"
case "$1" in
clients) printf '[{{"address":"0x1a","pid":{}}}]' ;;
getprop) if [ "$3" = opacity ]; then cut -d' ' -f1 "$d/props"; else cut -d' ' -f2 "$d/props"; fi ;;
eval)
  [ "$2" != -- ] || shift
  printf '%s' "$2" | tr '\n' ' ' >> "$d/evals"
  printf '\n' >> "$d/evals"
  pid=$(printf '%s\n' "$2" | head -n1 | cut -d' ' -f4)
  operation=$(printf '%s\n' "$2" | head -n1 | cut -d' ' -f3)
  case "$operation" in
  enable) printf '%s\n' "$pid" >> "$d/owners"; echo enabled > "$d/rule"; echo '0.75 0.75' > "$d/props" ;;
  exit)
    if [ -f "$d/owners" ]; then sed "/^$pid$/d" "$d/owners" > "$d/owners.next"; mv "$d/owners.next" "$d/owners"; fi
    if [ ! -s "$d/owners" ]; then echo disabled > "$d/rule"; echo '1 1' > "$d/props"; fi ;;
  disable)
    if [ -f "$d/owners" ]; then sed "/^$pid$/d" "$d/owners" > "$d/owners.next"; mv "$d/owners.next" "$d/owners"; fi
    echo disabled > "$d/rule"; echo '1 1' > "$d/props" ;;
  esac
  echo ok ;;
dispatch) echo ok ;;
esac
"#,
        std::process::id()
    );
    // A child writes the executable so concurrent test forks cannot inherit a writable script fd.
    let written = std::process::Command::new("/bin/sh")
        .args([
            "-c",
            "printf '%s' \"$2\" > \"$1\" && chmod 700 \"$1\"",
            "sh",
        ])
        .arg(&path)
        .arg(script)
        .status()
        .unwrap();
    assert!(written.success());
    path.into_os_string()
}

fn evaluated(opacity: Option<(f64, f64)>) -> String {
    lua_owned_rule(std::process::id(), opacity).replace('\n', " ")
}

fn glassy_rule_startup(mode: Mode, stale: bool) {
    let root = crate::compositor::tests::Fixture::new();
    let (home, state) = (root.0.join("home"), root.0.join("state"));
    if mode == Mode::Opaque {
        let toggles = state.join("omarchy/toggles/hypr");
        std::fs::create_dir_all(&toggles).unwrap();
        std::fs::write(toggles.join("window-opaque.lua"), OPAQUE).unwrap();
    }
    if stale {
        std::fs::write(root.0.join("rule"), "enabled\n").unwrap();
    }
    std::fs::write(
        root.0.join("props"),
        if stale { "0.75 0.75\n" } else { "1 1\n" },
    )
    .unwrap();
    let tagger = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &state),
    );
    tagger.pass();
    assert_eq!(
        std::fs::read_to_string(root.0.join("rule")).unwrap_or_default(),
        "disabled\n",
        "{mode:?} must disable even an unknown or absent named rule"
    );
    assert_eq!(tagger.lock().ruled, Some(false));
    assert_eq!(
        std::fs::read_to_string(root.0.join("props")).unwrap(),
        "1 1\n"
    );
    tagger.pass();
    assert_eq!(
        std::fs::read_to_string(root.0.join("evals")).unwrap(),
        format!("{}\n", evaluated(None)),
        "successful startup cleanup runs only once"
    );
}

#[test]
fn glassy_rule_transparent_start_clears_a_stale_rule() {
    glassy_rule_startup(Mode::Transparent, true);
}

#[test]
fn glassy_rule_opaque_start_clears_a_stale_rule() {
    glassy_rule_startup(Mode::Opaque, true);
}

#[test]
fn glassy_rule_disabling_an_absent_rule_succeeds_once() {
    glassy_rule_startup(Mode::Transparent, false);
    glassy_rule_startup(Mode::Opaque, false);
}

#[test]
fn glassy_rule_disabling_an_absent_rule_is_silent() {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "window::glass_rule_tests::glassy_rule_disabling_an_absent_rule_succeeds_once",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(child.status.success(), "{child:?}");
    assert!(child.stderr.is_empty(), "{child:?}");
    assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"));
}

#[test]
fn glassy_rule_runtime_switch_to_transparent_disables_the_rule() {
    let root = crate::compositor::tests::Fixture::new();
    let (home, state) = (root.0.join("home"), root.0.join("state"));
    let toggles = state.join("omarchy/toggles/hypr");
    std::fs::create_dir_all(&toggles).unwrap();
    let glassy = toggles.join("window-glassy.lua");
    std::fs::write(&glassy, GLASSY).unwrap();
    std::fs::write(root.0.join("props"), "1 1\n").unwrap();
    let tagger = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &state),
    );
    tagger.pass();
    assert_eq!(tagger.lock().ruled, Some(true));
    assert_eq!(
        std::fs::read_to_string(root.0.join("rule")).unwrap(),
        "enabled\n"
    );
    std::fs::remove_file(&glassy).unwrap();
    tagger.pass();
    assert_eq!(tagger.lock().ruled, Some(false));
    assert_eq!(
        std::fs::read_to_string(root.0.join("rule")).unwrap(),
        "disabled\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("evals")).unwrap(),
        format!("{}\n{}\n", evaluated(Some((0.75, 0.75))), evaluated(None))
    );
}

#[test]
fn glass_rule_pass_follows_each_level_and_leaves_an_explicit_alpha_alone() {
    let root = crate::compositor::tests::Fixture::new();
    let (home, state) = (root.0.join("home"), root.0.join("state"));
    let toggles = state.join("omarchy/toggles/hypr");
    std::fs::create_dir_all(&toggles).unwrap();
    let tagger = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &state),
    );
    let drawn = |active: &str, inactive: &str| {
        std::fs::write(root.0.join("props"), format!("{active} {inactive}\n")).unwrap();
    };
    let evals = || std::fs::read_to_string(root.0.join("evals")).unwrap_or_default();
    let glassy = toggles.join("window-glassy.lua");
    let opaque = toggles.join("window-opaque.lua");

    // Transparent: the tag carries Omarchy's 0.985 0.96, and startup disables any stale rule.
    drawn("1", "1");
    tagger.pass();
    assert_eq!(tagger.lock().tagged.len(), 1);
    assert_eq!(evals(), format!("{}\n", evaluated(None)));

    // Glassy, the only toggle file as Omarchy leaves it: the tag comes off and Nautilus's 0.75 0.75 reaches every Flea window, once.
    std::fs::write(&glassy, GLASSY).unwrap();
    tagger.pass();
    assert!(tagger.lock().tagged.is_empty());
    assert_eq!(
        evals(),
        format!("{}\n{}\n", evaluated(None), evaluated(Some((0.75, 0.75))))
    );
    assert_eq!(tagger.lock().ruled, Some(true));
    drawn("0.75", "0.75");
    tagger.pass();
    assert_eq!(
        evals().lines().count(),
        2,
        "a drawn rule is not appended again"
    );

    // Back to Transparent: the rule is disabled once, and the tag returns when Hyprland draws 1 1 again.
    std::fs::remove_file(&glassy).unwrap();
    tagger.pass();
    assert_eq!(evals().lines().last(), Some(evaluated(None).as_str()));
    assert_eq!(tagger.lock().ruled, Some(false));
    drawn("1", "1");
    tagger.pass();
    assert_eq!(tagger.lock().tagged.len(), 1);
    assert_eq!(evals().lines().count(), 3);

    // Opaque: Omarchy's tag rule draws 1 1 and the tag comes off; still no rule.
    std::fs::write(&opaque, OPAQUE).unwrap();
    tagger.pass();
    assert!(tagger.lock().tagged.is_empty());
    assert_eq!(evals().lines().count(), 3);
    std::fs::remove_file(&opaque).unwrap();

    // An explicit alpha keeps Flea's own fade: a fresh process under Glassy disables a rule an earlier one left on.
    std::fs::create_dir_all(home.join(".config/omarchy")).unwrap();
    std::fs::write(
        home.join(".config/omarchy/shell.toml"),
        "[flea]\nbackground-alpha = 0.6\n",
    )
    .unwrap();
    std::fs::write(&glassy, GLASSY).unwrap();
    drawn("0.75", "0.75");
    let fresh = Tagger::new(
        std::process::id(),
        fake_hyprctl(&root.0),
        Sources::under(&home, &state),
    );
    fresh.pass();
    assert_eq!(evals().lines().count(), 4);
    assert_eq!(evals().lines().last(), Some(evaluated(None).as_str()));
    fresh.pass();
    assert_eq!(
        evals().lines().count(),
        4,
        "the disable runs once per process"
    );
}
