//! Flea follows Nautilus under Omarchy at each Window Transparency level: the tag, the Glassy rule and solid automatic grounds.
use super::glass::Inputs;
use super::glass_tag::run::{Sources, Tagger};
use super::glass_tag::{lua_rule, nautilus_opacity, parse_props, rule_step, Mode, Props};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

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
    assert_eq!(
        rule_step(glassy, &[props(1., 1.)], None, true),
        Some(glassy)
    );
    assert_eq!(
        rule_step(glassy, &[props(1., 1.)], on, true),
        Some(glassy),
        "a config reload dropped the rule, so it is added again"
    );
    assert_eq!(
        rule_step(glassy, &[props(0.75, 1.)], on, true),
        Some(glassy)
    );
    assert_eq!(rule_step(glassy, &[props(0.75, 0.75)], on, true), None);
    assert_eq!(
        rule_step(glassy, &[props(f64::NAN, f64::NAN)], None, true),
        None,
        "a failed probe never appends the rule again"
    );
    assert_eq!(rule_step(None, &[props(0.75, 0.75)], on, false), Some(None));
    assert_eq!(rule_step(None, &[props(1., 1.)], None, false), None);
    assert_eq!(
        rule_step(None, &[props(0.75, 0.75)], None, true),
        Some(None),
        "under Glassy a rule an earlier Flea process left on is disabled"
    );
    assert_eq!(
        rule_step(None, &[props(0.75, 0.75)], Some(false), true),
        None,
        "a process that disabled the rule does not disable it again"
    );
}

#[test]
fn glass_rule_automatic_levels_paint_solid_and_an_explicit_alpha_wins() {
    let alpha = "[flea]\nbackground-alpha = 0.6\n";
    for (opaque, glassy, mode) in [
        (false, false, Mode::Transparent),
        (true, false, Mode::Opaque),
        (false, true, Mode::Glassy),
    ] {
        let automatic = Inputs::resolve("", "", opaque, glassy);
        assert_eq!((automatic.mode, automatic.automatic), (mode, true));
        assert_eq!(automatic.wanted_alpha, 255, "{mode:?} grounds stay solid");
        assert!(!automatic.pins_opaque);
        let user = Inputs::resolve("", alpha, opaque, glassy);
        let theme = Inputs::resolve(alpha, "", opaque, glassy);
        let explicit = if mode == Mode::Opaque { 255 } else { 153 };
        assert_eq!(theme.wanted_alpha, explicit, "{mode:?} theme alpha");
        assert_eq!(user.wanted_alpha, 153, "{mode:?} user alpha");
        assert!(!user.automatic && !theme.automatic);
    }
}

// A hyprctl that lists one window of this process at the opacities in `props`, answers ok and logs each eval.
fn fake_hyprctl(root: &Path) -> OsString {
    let path = root.join("hyprctl");
    let script = format!(
        "#!/bin/sh\nd=\"${{0%/*}}\"\ncase \"$1\" in\nclients) printf '[{{\"address\":\"0x1a\",\"pid\":{}}}]' ;;\ngetprop) if [ \"$3\" = opacity ]; then cut -d' ' -f1 \"$d/props\"; else cut -d' ' -f2 \"$d/props\"; fi ;;\neval) printf '%s\\n' \"$2\" >> \"$d/evals\"; echo ok ;;\ndispatch) echo ok ;;\nesac\n",
        std::process::id()
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path.into_os_string()
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

    // Transparent: the tag carries Omarchy's 0.985 0.96, and no rule is evaluated.
    drawn("1", "1");
    tagger.pass();
    assert_eq!(tagger.lock().tagged.len(), 1);
    assert_eq!(evals(), "");

    // Glassy, the only toggle file as Omarchy leaves it: the tag comes off and Nautilus's 0.75 0.75 reaches every Flea window, once.
    std::fs::write(&glassy, GLASSY).unwrap();
    tagger.pass();
    assert!(tagger.lock().tagged.is_empty());
    assert_eq!(evals(), format!("{}\n", lua_rule(Some((0.75, 0.75)))));
    assert_eq!(tagger.lock().ruled, Some(true));
    drawn("0.75", "0.75");
    tagger.pass();
    assert_eq!(
        evals().lines().count(),
        1,
        "a drawn rule is not appended again"
    );

    // Back to Transparent: the rule is disabled once, and the tag returns when Hyprland draws 1 1 again.
    std::fs::remove_file(&glassy).unwrap();
    tagger.pass();
    assert_eq!(evals().lines().last(), Some(lua_rule(None).as_str()));
    assert_eq!(tagger.lock().ruled, Some(false));
    drawn("1", "1");
    tagger.pass();
    assert_eq!(tagger.lock().tagged.len(), 1);
    assert_eq!(evals().lines().count(), 2);

    // Opaque: Omarchy's tag rule draws 1 1 and the tag comes off; still no rule.
    std::fs::write(&opaque, OPAQUE).unwrap();
    tagger.pass();
    assert!(tagger.lock().tagged.is_empty());
    assert_eq!(evals().lines().count(), 2);
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
    assert_eq!(evals().lines().count(), 3);
    assert_eq!(evals().lines().last(), Some(lua_rule(None).as_str()));
    fresh.pass();
    assert_eq!(
        evals().lines().count(),
        3,
        "the disable runs once per process"
    );
}
