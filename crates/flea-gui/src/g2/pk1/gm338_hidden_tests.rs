// GitHub #338: the chooser follows Show Hidden Files; a portal flag or the window's own toggle wins.
use super::process::Line;
use super::settings::HiddenSeed;
use super::tests::fixture::{fixture, Fixture};
use crate::path::view::Out;
use gpui::TestAppContext;

fn last_hidden(f: &Fixture, command: &str) -> Option<bool> {
    let command = format!(r#""c":"{command}""#);
    let sent = f.listing.sent();
    let line = sent.iter().rev().find(|line| line.contains(&command))?;
    Some(line.contains(r#""hidden":true"#))
}

fn lists(f: &Fixture) -> usize {
    let sent = f.listing.sent();
    sent.iter()
        .filter(|line| line.contains(r#""c":"list""#))
        .count()
}

fn apply(f: &mut Fixture, text: &str) {
    let data = flea::uistate::from_file(text);
    f.update(|picker, cx| picker.apply_settings(&data, cx));
    f.cx.run_until_parked();
}

fn listed(f: &mut Fixture) {
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":1,"read":0.000,"sort":0.000,"v":1,"w":true,"path":"/d"}"#.into(),
    ));
    f.listing.say(Line::Reply(
        r#"{"t":"rows","start":0,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}],"listing":1}"#.into(),
    ));
    f.cx.run_until_parked();
}

#[gpui::test]
fn gm338_the_first_listing_follows_show_hidden_files(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(HiddenSeed(true)));
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    apply(&mut f, r#"{"hidden":true}"#);
    assert_eq!(
        lists(&f),
        1,
        "the seed matches the settings launch applies next, so the folder is listed once"
    );
    assert_eq!(last_hidden(&f, "list"), Some(true));
}

#[gpui::test]
fn gm338_a_changed_setting_relists_the_chooser(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    assert_eq!(last_hidden(&f, "list"), Some(false));
    apply(&mut f, r#"{"hidden":true}"#);
    assert_eq!(
        last_hidden(&f, "list"),
        Some(true),
        "Show Hidden Files reaches the chooser"
    );
    let count = lists(&f);
    apply(&mut f, r#"{"hidden":true}"#);
    assert_eq!(lists(&f), count, "an unchanged setting lists nothing");
}

#[gpui::test]
fn gm338_a_portal_flag_outranks_the_seed(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(HiddenSeed(true)));
    let mut f = fixture(cx, r#"{"folder":"/d","hidden":false}"#, None);
    f.cx.run_until_parked();
    assert_eq!(
        last_hidden(&f, "list"),
        Some(false),
        "the caller's flag wins"
    );
    apply(&mut f, r#"{"hidden":false}"#);
    apply(&mut f, r#"{"hidden":true}"#);
    assert_eq!(lists(&f), 1, "a changed Show Hidden Files relists nothing");
    assert_eq!(
        last_hidden(&f, "list"),
        Some(false),
        "the caller's flag still wins over Show Hidden Files"
    );
}

#[gpui::test]
fn gm338_the_key_toggle_flips_the_chooser_only(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f);
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("ctrl-shift-.");
    f.cx.run_until_parked();
    assert_eq!(
        last_hidden(&f, "list"),
        Some(true),
        "the keys.toml toggle shows hidden files"
    );
    assert!(
        !f.read(|picker| picker.preferences.followed.hidden),
        "the app's setting is not changed by the toggle"
    );
    let count = lists(&f);
    apply(&mut f, r#"{"hidden":true}"#);
    apply(&mut f, r#"{"hidden":false}"#);
    assert_eq!(
        lists(&f),
        count,
        "a changed setting leaves the toggled window alone"
    );
    assert_eq!(
        last_hidden(&f, "list"),
        Some(true),
        "the toggle outranks the setting in this window"
    );
    f.cx.simulate_keystrokes("ctrl-shift-.");
    f.cx.run_until_parked();
    assert_eq!(last_hidden(&f, "list"), Some(false));
}

#[gpui::test]
fn gm338_a_dot_typed_in_filename_is_text_not_the_toggle(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","name":"untitled.txt","folder":"/d"}"#,
        None,
    );
    f.cx.run_until_parked();
    listed(&mut f);
    let field = f
        .read(|picker| picker.browse.filename.clone())
        .expect("a save has Filename");
    f.cx.update(|window, cx| {
        let focus = field.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    f.cx.run_until_parked();
    let asked = lists(&f);
    f.cx.simulate_input("a.b");
    f.cx.run_until_parked();
    assert!(
        f.cx.update(|_, cx| field.read(cx).text().contains("a.b")),
        "the dot lands in the name"
    );
    assert_eq!(lists(&f), asked, "the dot never relists");
    assert_eq!(last_hidden(&f, "list"), Some(false));
}

#[gpui::test]
fn gm338_a_typed_path_peek_follows_show_hidden_files(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    apply(&mut f, r#"{"hidden":true}"#);
    f.update(|picker, cx| {
        picker.path_out(
            Out::Open {
                side: 0,
                path: "/d/sub".into(),
            },
            cx,
        )
    });
    f.cx.run_until_parked();
    assert_eq!(
        last_hidden(&f, "peek"),
        Some(true),
        "a typed path is peeked with the hidden files the listing shows"
    );
}
