//! Properties dismissal through the mounted window, native pointer/key delivery and backend wire.
use super::{
    menudeliver_tests::{select, wire},
    menus_tests::{home, reply, settle},
    openwith_controls_tests::click,
    test_shell::{land, open_root},
};
use gpui::TestAppContext;

#[derive(Clone, Copy, Debug)]
enum Fixture {
    File,
    CountingFolder,
    FinishedFolder,
    Several,
    Tagged,
}

#[derive(Clone, Copy, Debug)]
enum Dismiss {
    Pointer,
    Enter,
    Space,
    Escape,
}

fn exercise(cx: &mut TestAppContext, fixture: Fixture, dismiss: Dismiss) {
    let (shell, cx) = open_root(cx, "windows");
    land(&shell, cx, "/files", home(), 1);
    let rows: &[usize] = match fixture {
        Fixture::CountingFolder | Fixture::FinishedFolder => &[0],
        Fixture::Several => &[0, 2, 3],
        Fixture::File | Fixture::Tagged => &[2],
    };
    select(&shell, cx, rows);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("alt-enter");
    settle(cx);
    let snapshot = sent();
    let id = snapshot
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .and_then(|line| flea::json::field_usize(line, "id"))
        .expect("the listing shortcut opens Properties through window dispatch");
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true}}"#),
    );
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"properties","id":{id}}}"#
        )]
    );
    let facts = match fixture {
        Fixture::CountingFolder | Fixture::FinishedFolder => {
            r#""path":"/files/alpha","directory":true,"bytes":4096"#
        }
        Fixture::Several => r#""kinds":"1 folder, 2 files","bytes":123,"done":true"#,
        Fixture::File => r#""path":"/files/notes.txt","bytes":99"#,
        Fixture::Tagged => r#""path":"/files/notes.txt","bytes":99,"tags":["Red","Blue"]"#,
    };
    let properties =
        format!(r#"{{"t":"menuaction","op":"properties","id":{id},"ok":true,{facts}}}"#);
    reply(&shell, cx, &properties);
    let contents = sent();
    if matches!(fixture, Fixture::CountingFolder | Fixture::FinishedFolder) {
        assert_eq!(
            contents,
            [format!(r#"{{"c":"menuaction","op":"contents","id":{id}}}"#)]
        );
        reply(
            &shell,
            cx,
            &format!(
                r#"{{"t":"menuaction","op":"contents","id":{id},"ok":true,"items":11,"bytes":2560562,"done":{},"partial":false}}"#,
                matches!(fixture, Fixture::FinishedFolder)
            ),
        );
    } else {
        assert!(contents.is_empty());
    }
    settle(cx);
    let close = cx.debug_bounds("op3-button-0").expect("Close is drawn");
    assert!(shell.read_with(cx, |shell, _| {
        let open = shell.properties.as_ref().expect("the card is open");
        let card = open.card().card(&shell.locale.catalog);
        assert_eq!(card.buttons.len(), 1);
        assert_eq!(card.buttons[0].label, "Close");
        assert!(card.buttons[0].focused && card.buttons[0].enabled);
        open.ready()
    }));
    sent();
    match dismiss {
        Dismiss::Pointer => click(cx, close.center()),
        Dismiss::Enter | Dismiss::Space => {
            cx.simulate_keystrokes("tab");
            settle(cx);
            assert!(
                shell.read_with(cx, |shell, _| {
                    shell
                        .properties
                        .as_ref()
                        .unwrap()
                        .card()
                        .card(&shell.locale.catalog)
                        .buttons[0]
                        .focused
                }),
                "Tab retains the card's only focus stop"
            );
            cx.simulate_keystrokes(if matches!(dismiss, Dismiss::Enter) {
                "enter"
            } else {
                "space"
            });
        }
        Dismiss::Escape => cx.simulate_keystrokes("escape"),
    }
    settle(cx);
    let dismissed = shell.read_with(cx, |shell, _| shell.properties.is_none());
    let lines = sent();
    let painted = cx.debug_bounds("op3-card");
    let selection = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    // A reply already in flight must neither reopen the card nor start another folder walk.
    reply(&shell, cx, &properties);
    let reopened = shell.read_with(cx, |shell, _| shell.properties.is_some());
    let late_lines = sent();
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    assert!(dismissed, "{fixture:?} {dismiss:?}: Properties stays open");
    assert!(painted.is_none(), "the dismissed card leaves the window");
    assert_eq!(
        lines,
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)],
        "dismissal expires its own snapshot exactly once"
    );
    assert_eq!(selection, rows, "dismissal preserves the listing selection");
    assert!(
        !reopened && late_lines.is_empty(),
        "late facts stay cancelled"
    );
}

macro_rules! cases {
    ($($name:ident: $fixture:ident, $dismiss:ident;)*) => {
        $(#[gpui::test]
        fn $name(cx: &mut TestAppContext) {
            exercise(cx, Fixture::$fixture, Dismiss::$dismiss);
        })*
    };
}

cases! {
    properties_close_file_pointer: File, Pointer;
    properties_close_file_enter: File, Enter;
    properties_close_file_space: File, Space;
    properties_close_file_escape: File, Escape;
    properties_close_counting_folder_pointer: CountingFolder, Pointer;
    properties_close_counting_folder_enter: CountingFolder, Enter;
    properties_close_counting_folder_space: CountingFolder, Space;
    properties_close_counting_folder_escape: CountingFolder, Escape;
    properties_close_finished_folder_pointer: FinishedFolder, Pointer;
    properties_close_finished_folder_enter: FinishedFolder, Enter;
    properties_close_finished_folder_space: FinishedFolder, Space;
    properties_close_finished_folder_escape: FinishedFolder, Escape;
    properties_close_several_pointer: Several, Pointer;
    properties_close_several_enter: Several, Enter;
    properties_close_several_space: Several, Space;
    properties_close_several_escape: Several, Escape;
    properties_close_tagged_pointer: Tagged, Pointer;
    properties_close_tagged_enter: Tagged, Enter;
    properties_close_tagged_space: Tagged, Space;
    properties_close_tagged_escape: Tagged, Escape;
}
