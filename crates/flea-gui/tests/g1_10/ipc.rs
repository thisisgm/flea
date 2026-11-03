use super::*;
use flea::ipc::{
    registry::validate,
    wire::{Reply, Request},
};
use flea_gui::ipc::{
    tabs::{answer, send_taken, taken, TabsAdapter},
    Adapter,
};
use flea_gui::tabs::bar::{HEIGHT, PAD_X};

fn three() -> Side {
    let mut side = Side::new(Fake::at("/home/gm"));
    side.state
        .open_new(&mut side.pane, Some("/home/gm/Work".into()));
    side.state.open_new(&mut side.pane, Some("/".into()));
    side
}

fn adapter<'a>(side: &'a Side, bar: &'a Bar, visible: bool) -> TabsAdapter<'a> {
    TabsAdapter {
        state: &side.state,
        bar,
        path: &side.pane.path,
        home: "/home/gm",
        home_label: "Home",
        visible,
        top: 40.,
    }
}

fn read(adapter: &TabsAdapter, function: &str, arguments: &[&str]) -> String {
    let arguments: Vec<String> = arguments
        .iter()
        .map(|argument| argument.to_string())
        .collect();
    let reply = adapter
        .call(function, &arguments)
        .expect("a strip reader answers");
    assert_eq!(reply.code, 0, "{function} answers as a value");
    reply
        .stdout
        .strip_suffix('\n')
        .expect("a value ends its line")
        .to_owned()
}

#[test]
fn the_strip_readers_answer_count_index_labels_and_visibility() {
    let side = three();
    let bar = bar();
    let shown = adapter(&side, &bar, true);
    assert_eq!(read(&shown, "tabCount", &[]), "3");
    assert_eq!(
        read(&shown, "tabIndex", &[]),
        "2",
        "the tab opened last is current"
    );
    assert_eq!(
        read(&shown, "tabLabels", &[]),
        "Home|Work|/",
        "labels join with a bar, Home and the root in their own words"
    );
    assert_eq!(read(&shown, "tabBarVisible", &[]), "true");
    assert_eq!(
        read(&adapter(&side, &bar, false), "tabBarVisible", &[]),
        "false"
    );
    assert!(
        shown.call("rowCentre", &["0".into()]).is_none(),
        "a reader the strip does not own is left to the next adapter"
    );
}

#[test]
fn a_tab_centre_is_its_slot_centre_and_empty_off_the_strip() {
    let side = three();
    let mut bar = bar();
    let width = bar.tab_width(3);
    let centre = |slot: f32| {
        format!(
            "{} {}",
            (PAD_X + slot * width + width / 2. + 0.5).floor(),
            (40. + HEIGHT / 2. + 0.5).floor()
        )
    };
    assert_eq!(
        read(&adapter(&side, &bar, true), "tabCentre", &["1"]),
        centre(1.),
        "the centre rounds half up, as the shipped reader does"
    );
    assert_eq!(
        read(&adapter(&side, &bar, true), "tabCentre", &[" 0 "]),
        centre(0.),
        "an integer argument is read trimmed, as the registry accepts it"
    );
    bar.drag_started(Some(0));
    bar.drop_at = Some(3);
    assert_eq!(
        read(&adapter(&side, &bar, true), "tabCentre", &["0"]),
        centre(2.),
        "a held tab answers the slot it draws in"
    );
    assert_eq!(
        read(&adapter(&side, &bar, true), "tabCentre", &["1"]),
        centre(0.),
        "and a crossed tab its shifted slot"
    );
    for argument in ["3", "-1", "x"] {
        assert_eq!(
            read(&adapter(&side, &bar, true), "tabCentre", &[argument]),
            "",
            "{argument} names no tab"
        );
    }
    assert_eq!(
        read(&adapter(&side, &bar, false), "tabCentre", &["0"]),
        "",
        "a strip not drawn has no centre to press"
    );
}

#[test]
fn the_taken_call_is_registered_and_read_only_from_its_own_target() {
    let function = validate("fleatab", "taken", &["token-1".into()])
        .expect("fleatab.taken is a registered call");
    assert_eq!(
        (function.result, function.parameters),
        ("bool", &[("token", "string")][..])
    );
    assert!(
        validate("fleatab", "taken", &[]).is_err(),
        "the token is required"
    );
    assert!(
        validate("fleatab", "tabCount", &[]).is_err(),
        "the acknowledgment target carries only its one call"
    );
    let call = |target: &str, function: &str, arguments: &[&str]| Request::Call {
        target: target.into(),
        function: function.into(),
        arguments: arguments
            .iter()
            .map(|argument| argument.to_string())
            .collect(),
    };
    assert_eq!(
        taken(&call("fleatab", "taken", &["token-1"])).as_deref(),
        Some("token-1")
    );
    assert_eq!(
        taken(&call("flea", "taken", &["token-1"])),
        None,
        "the main target never acknowledges a lift"
    );
    assert_eq!(taken(&call("fleatab", "taken", &[])), None);
    assert_eq!(taken(&Request::Show), None);
    assert_eq!(answer(true).stdout, "true\n");
    assert_eq!(
        answer(false).stdout,
        "false\n",
        "a refused ack is a plain false, never an error"
    );
}

#[test]
fn an_acknowledgment_to_a_pid_that_is_not_a_number_never_leaves_the_window() {
    let Reply { code, stdout, .. } = send_taken("12x", "token");
    assert_eq!(
        (code, stdout.as_str()),
        (2, ""),
        "a forged source pid is refused before any socket is opened"
    );
}
