//! The eleven path readers a driven test asks, answered from what the surface last drew.
use super::{
    inputs,
    view::{bar, Bar},
    HOME,
};
use flea_gui::{
    ipc::{path::PathAdapter, Adapter},
    path::{paint::Size, suggestions::Unranked, view::Inputs},
};
use gpui::TestAppContext;

fn ask(bar: &mut Bar<'_>, function: &str, arguments: &[&str]) -> String {
    let arguments: Vec<String> = arguments
        .iter()
        .map(|argument| argument.to_string())
        .collect();
    let reply = bar
        .read(|view| PathAdapter { view }.call(function, &arguments))
        .expect(function);
    assert_eq!(reply.code, 0, "{function}");
    reply.stdout.trim_end_matches('\n').to_owned()
}

// JavaScript's Math.round over a centre, as the QML readers answer.
fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

#[gpui::test]
fn the_chrome_readers_answer_its_crumbs_field_and_slot(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked), inputs("/home/gm/Work/claude"));
    let cell = bar.advance(Size::Caption);
    assert_eq!(ask(&mut bar, "pathBarOpen", &[]), "false");
    assert_eq!(ask(&mut bar, "crumbCount", &[]), "3");
    assert_eq!(ask(&mut bar, "crumbText", &[]), "~/Work/claude");
    assert_eq!(ask(&mut bar, "pathCentre", &[]), "394 13");
    assert_eq!(
        ask(&mut bar, "crumbCentre", &["1"]),
        format!("{} 13", round(cell * 2. + cell * 5. / 2.))
    );
    assert_eq!(ask(&mut bar, "crumbCentre", &["9"]), "");
    assert_eq!(
        ask(&mut bar, "pathFrameRect", &[]),
        "",
        "a field never opened has no frame to report"
    );
    assert!(
        bar.read(|view| PathAdapter { view }
            .call("rowCentre", &["0".into()])
            .is_none()),
        "another surface's reader"
    );
    bar.update(|view, window, cx| view.open(window, cx));
    assert_eq!(ask(&mut bar, "pathBarOpen", &[]), "true");
    assert_eq!(ask(&mut bar, "pathBarText", &[]), "/home/gm/Work/claude/");
    assert_eq!(ask(&mut bar, "pathFrameRect", &[]), "0 2 788 23");
    assert_eq!(
        ask(&mut bar, "crumbCentre", &["1"]),
        "",
        "the open field hides the crumbs"
    );
    bar.cx.simulate_keystrokes("escape");
    assert_eq!(
        ask(&mut bar, "pathBarText", &[]),
        "/home/gm/Work/claude/",
        "the line stays after the field closes"
    );
    assert_eq!(ask(&mut bar, "paneCrumbCount", &["0"]), "0");
    assert_eq!(ask(&mut bar, "paneCrumbText", &["1"]), "");
    assert_eq!(ask(&mut bar, "panePathRect", &["0"]), "");
}

#[gpui::test]
fn the_pane_readers_answer_each_dual_strip_and_the_chrome_keeps_none(cx: &mut TestAppContext) {
    let dual = Inputs {
        dual: true,
        paths: ["/home/gm/Work".into(), "/srv/www".into()],
        ..inputs(HOME)
    };
    let mut bar = bar(cx, Box::new(Unranked), dual);
    let cell = bar.advance(Size::Caption);
    assert_eq!(
        ask(&mut bar, "crumbCentre", &["0"]),
        "",
        "dual draws no chrome crumbs"
    );
    assert_eq!(ask(&mut bar, "paneCrumbCount", &["0"]), "2");
    assert_eq!(ask(&mut bar, "paneCrumbText", &["0"]), "~/Work");
    assert_eq!(ask(&mut bar, "paneCrumbText", &["1"]), "/srv/www");
    assert_eq!(ask(&mut bar, "panePathRect", &["0"]), "0 26 400 27");
    assert_eq!(ask(&mut bar, "panePathRect", &["1"]), "401 26 387 27");
    let centre = format!(
        "{} {}",
        round(401. + 14. + cell * 1. + cell * 4. / 2.),
        round(26. + 27. / 2.)
    );
    assert_eq!(ask(&mut bar, "paneCrumbCentre", &["1", "1"]), centre);
    assert_eq!(ask(&mut bar, "paneCrumbCentre", &["1", "7"]), "");
    assert_eq!(
        ask(&mut bar, "paneCrumbCount", &["2"]),
        "0",
        "there is no third pane"
    );
}
