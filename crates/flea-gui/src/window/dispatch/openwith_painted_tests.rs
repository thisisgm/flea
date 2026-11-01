//! EY-454: the row menu's tail opens the Open With card, and openWithState reports the parts it painted, so the design CI asserts the openwith board's layout.
use super::menudeliver_tests::{open, select, wire, REG};
use super::menus_tests::{choose, deliver, home, reply, settle, validated};
use super::test_shell::land;
use crate::{g2::op3::keys::Key, window::u07_tests::read};
use flea::jsondoc::{self, Json};

fn app(id: &str, label: &str, default: bool) -> String {
    format!(r#"{{"id":"{id}","label":"{label}","icon":"","default":{default}}}"#)
}

fn numbers(state: &Json, key: &str) -> Vec<f64> {
    let text = state.get(key).and_then(Json::as_str).unwrap_or_default();
    text.split(' ').filter_map(|n| n.parse().ok()).collect()
}

fn number(state: &Json, key: &str) -> f64 {
    state.get(key).and_then(Json::as_f64).unwrap_or(f64::NAN)
}

fn rects(state: &Json, key: &str) -> Vec<Vec<f64>> {
    state
        .get(key)
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(Json::as_str)
        .map(|text| text.split(' ').filter_map(|n| n.parse().ok()).collect())
        .collect()
}

/// Within one logical pixel, since every rect is rounded to whole pixels.
fn near(left: f64, right: f64, what: &str) {
    assert!((left - right).abs() <= 1., "{what}: {left} against {right}");
}

#[gpui::test]
fn the_tail_row_opens_the_card_and_its_state_reports_the_boards_painted_layout(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("shift-f10");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    let registered = app("nvim.desktop", "Neovim", true);
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[{registered}]}}"#
        ),
    );
    choose(&shell, cx, "openWith");
    cx.simulate_keystrokes("down enter");
    settle(cx);
    validated(
        &shell,
        cx,
        &format!("openWith:{}", flea::ui_logic_mn1::rows::OPEN_WITH_OTHER),
        &["/files/notes.txt"],
    );
    let card = shell.read_with(cx, |shell, _| shell.open_with.as_ref().map(|open| open.id));
    let card = card.expect("the tail row opens the Open With card");
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{card},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    let installed = (0..8)
        .map(|at| app(&format!("app{at}.desktop"), &format!("App {at}"), false))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{card},"op":"applications","ok":true,"path":"/files/notes.txt","kind":"Plain text document","mime":"text/plain","applications":[{registered}],"installed":[{installed}]}}"#
        ),
    );
    sent();
    settle(cx);
    let state = jsondoc::parse(&read(&shell, cx, "openWithState", &[])).unwrap();
    let (title, search, list) = (
        numbers(&state, "titleRuleRect"),
        numbers(&state, "searchRect"),
        numbers(&state, "listRect"),
    );
    let (gap, row, eyebrow) = (
        number(&state, "gap"),
        number(&state, "rowHeight"),
        number(&state, "eyebrowHeight"),
    );
    assert_eq!(
        (title.len(), search.len(), list.len()),
        (4, 4, 4),
        "{state:?}"
    );
    near(
        search[1] - (title[1] + title[3]),
        gap,
        "the field sits one gap under the title rule",
    );
    near(
        list[1] - (search[1] + search[3]),
        gap,
        "the list starts one gap under the field",
    );
    let air: Vec<f64> = state
        .get("eyebrowAir")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(Json::as_f64)
        .collect();
    let eyebrows = rects(&state, "eyebrowRects");
    assert_eq!(
        eyebrows.len(),
        2,
        "both groups draw their eyebrow: {state:?}"
    );
    near(eyebrows[0][1] - list[1], air[0], "the first eyebrow's air");
    let rule = numbers(&state, "groupRuleRect");
    assert_eq!(
        rule.len(),
        4,
        "a hairline closes the registered group: {state:?}"
    );
    near(
        rule[1],
        eyebrows[0][1] + eyebrow + row,
        "the hairline sits under the one registered row",
    );
    near(
        eyebrows[1][1] - (rule[1] + rule[3]),
        air[1],
        "the later eyebrow's air under the hairline",
    );
    assert_eq!(
        rects(&state, "rowRects").len(),
        7,
        "seven applications fill the viewport"
    );
    near(
        list[3],
        7. * row + 2. * eyebrow + air[0] + air[1] + 2.,
        "seven rows, two eyebrows, their air and two hairlines",
    );
    let card_before = numbers(&state, "rect");
    let bottom_before = numbers(&state, "listRuleRect");
    // HIG leaves the no-match state to v0.3.12: the viewport stays put and centres its caption.
    shell.update(cx, |shell, cx| {
        let open = shell.open_with.as_mut().unwrap();
        for c in "zzNoMatchingApplication".chars() {
            open.card.key(Key::Text(c.to_string()));
        }
        cx.notify();
    });
    settle(cx);
    let state = jsondoc::parse(&read(&shell, cx, "openWithState", &[])).unwrap();
    let note = numbers(&state, "noMatchRect");
    for (name, before) in [
        ("rect", card_before),
        ("searchRect", search),
        ("listRect", list.clone()),
        ("listRuleRect", bottom_before),
    ] {
        let after = numbers(&state, name);
        assert_eq!(after.len(), 4, "{name} remains painted: {state:?}");
        for (before, after) in before.into_iter().zip(after) {
            near(after, before, name);
        }
    }
    assert!(rects(&state, "rowRects").is_empty());
    assert_eq!(note.len(), 4, "the no-match note is painted: {state:?}");
    assert!(
        note[3] >= 36.,
        "the long no-match caption wraps within the card"
    );
    assert!(note[0] >= list[0] && note[0] + note[2] <= list[0] + list[2]);
    near(
        note[0] + note[2] / 2.,
        list[0] + list[2] / 2.,
        "the caption is centred horizontally in the viewport",
    );
    near(
        note[1] + note[3] / 2.,
        list[1] + (list[3] - 1.) / 2.,
        "the caption is centred vertically above the viewport's closing rule",
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
