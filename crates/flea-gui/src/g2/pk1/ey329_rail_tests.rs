// EY-329: the rail rows `controls` lists carry PickerPlaces.qml's focus and refusal, not only their centres.
use super::*;

// The rail rows close the `controls` list, each as (name, focused, enabled).
fn rail_rows(f: &mut Fixture) -> Vec<(String, bool, bool)> {
    let count = f.read(|picker| picker.rail.entries.len());
    let reply = endpoint(f, "controls", &[]);
    let controls = flea::jsondoc::parse(&reply.stdout).unwrap();
    let controls = controls.as_array().unwrap();
    assert!(controls.len() >= count, "{}", reply.stdout);
    controls[controls.len() - count..]
        .iter()
        .map(|control| {
            let flag = |name: &str| control.get(name).and_then(|on| on.as_bool()).unwrap();
            let name = control.get("name").and_then(|name| name.as_str()).unwrap();
            (name.to_owned(), flag("focused"), flag("enabled"))
        })
        .collect()
}

fn rows(expected: [(&str, bool, bool); 4]) -> Vec<(String, bool, bool)> {
    expected
        .iter()
        .map(|&(name, focused, enabled)| (name.to_owned(), focused, enabled))
        .collect()
}

#[gpui::test]
fn ey329_rail_rows_mark_the_focused_cursor_and_refuse_only_the_invalid_favourite(
    cx: &mut TestAppContext,
) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let state =
        flea::jsondoc::parse(r#"{"places":{"favourites":[{"label":"Docs","path":"/d"},17]}}"#)
            .unwrap();
    f.update(|picker, cx| picker.rail_favourites(&state, cx));
    f.cx.run_until_parked();
    assert_eq!(
        rail_rows(&mut f),
        rows([
            ("Recent", false, true),
            ("Docs", false, true),
            ("17", false, false),
            ("Home", false, true)
        ]),
        "an unfocused rail marks no row, and the invalid favourite alone is refused"
    );
    let rail = f.read(|picker| picker.rail.focus.clone());
    f.cx.update(|window, cx| window.focus(&rail, cx));
    f.cx.run_until_parked();
    assert_eq!(
        rail_rows(&mut f),
        rows([
            ("Recent", true, true),
            ("Docs", false, true),
            ("17", false, false),
            ("Home", false, true)
        ]),
        "the focused rail marks its cursor row"
    );
    f.cx.simulate_keystrokes("down down");
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.rail.cursor), 2);
    assert_eq!(
        rail_rows(&mut f),
        rows([
            ("Recent", false, true),
            ("Docs", false, true),
            ("17", true, false),
            ("Home", false, true)
        ]),
        "the mark follows the rail cursor onto the refused row"
    );
}

#[gpui::test]
fn ey329_rail_rows_are_refused_while_submitting_and_without_a_backend(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let enabled = |f: &mut Fixture| {
        rail_rows(f)
            .into_iter()
            .map(|(_, _, enabled)| enabled)
            .collect::<Vec<_>>()
    };
    assert_eq!(enabled(&mut f), [true, true], "idle rows are enabled");
    f.update(|picker, cx| picker.accept(cx));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| (
            picker.accept_state().submitting(),
            picker.backend_unavailable()
        )),
        (true, false),
        "the cursor file's mark is in flight and the backend is alive"
    );
    assert_eq!(
        enabled(&mut f),
        [false, false],
        "a submission refuses every rail row"
    );
    f.checks.say(Line::Failed("exited".into()));
    f.settle(|f| f.read(Picker::backend_unavailable));
    assert!(
        !f.read(|picker| picker.accept_state().submitting()),
        "a lost backend ends the submission"
    );
    assert_eq!(
        enabled(&mut f),
        [false, false],
        "a lost backend refuses every rail row"
    );
}
