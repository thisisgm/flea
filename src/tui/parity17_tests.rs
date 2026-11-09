use super::{
    action_key_tests::{drain, listing},
    actions,
    editor::Editor,
    input::Key,
    keymap::Map,
    wire::{self, text},
};

pub(super) fn editor_case(name: &str, mods: &str, home: bool, expected: &str) {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "yazi");
    let mut editor = Editor::new("path", "one two.three".into(), m.path.clone());
    if home {
        editor.update(&Key::named("Home", ""));
    }
    m.editor = Some(editor);
    actions::key(&mut m, &Key::named(name, mods), &Map::load(), &mut wire).unwrap();
    actions::key(&mut m, &Key::character('X', ""), &Map::load(), &mut wire).unwrap();
    assert_eq!(m.editor.as_ref().unwrap().value, expected);
    assert_eq!(m.path.to_str(), Some("/listing"));
    wire::finish(wire, reader);
}
macro_rules! editor_test {
    ($test:ident, $key:expr, $mods:expr, $home:expr, $expected:expr) => {
        #[test]
        fn $test() {
            editor_case($key, $mods, $home, $expected);
        }
    };
}
editor_test!(yazi_179, "B", "alt", false, "one two.Xthree");
editor_test!(yazi_180, "F", "alt", true, "oneX two.three");
editor_test!(yazi_181, "Left", "ctrl", false, "one two.Xthree");
editor_test!(yazi_182, "Right", "ctrl", true, "oneX two.three");
editor_test!(yazi_195, "U", "ctrl", false, "X");
editor_test!(yazi_196, "K", "ctrl", true, "X");
editor_test!(yazi_197, "W", "ctrl", false, "one two.X");
editor_test!(yazi_198, "D", "alt", true, "X two.three");
editor_test!(yazi_199, "Backspace", "ctrl", false, "one two.X");
editor_test!(yazi_200, "Delete", "ctrl", true, "X two.three");

pub(super) fn listing_case(keys: &[Key], action: &str) {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "yazi");
    let fixture = crate::backend::testdir::TestDir::new("tui17-parity");
    let source = fixture.file("source", "source");
    m.clipboard = vec![source.to_string_lossy().into_owned()];
    m.parity17.copied = crate::backend::linkheld::capture(&m.clipboard).unwrap();
    for key in keys {
        actions::key(&mut m, key, &Map::load(), &mut wire).unwrap();
    }
    let sent = drain(&mut wire);
    assert!(m.error.is_empty(), "{action}: {}", m.error);
    if action.starts_with("copy") {
        assert!(
            sent.iter().any(|v| text(v, "c") == "paths"),
            "{action}: {sent:?}"
        );
    } else if action.starts_with("paste") {
        assert!(
            sent.iter().any(|v| text(v, "c") == "collisions"),
            "{action}: {sent:?}"
        );
    } else if action.starts_with("sort") {
        assert!(
            sent.iter().any(|v| text(v, "c") == "sort"),
            "{action}: {sent:?}"
        );
    } else if action == "findPrevious" {
        assert_eq!(m.editor.as_ref().map(|e| e.kind), Some("findPrevious"));
    } else if action == "detailBirth" || action == "detailOff" {
        assert!(
            m.message.contains(if action == "detailBirth" {
                "Created"
            } else {
                "Off"
            }),
            "{}",
            m.message
        );
    }
    wire::finish(wire, reader);
}
macro_rules! listing_test {
    ($test:ident, $action:expr, $($key:expr),+) => {
        #[test] fn $test() { listing_case(&[$($key),+], $action); }
    };
}
listing_test!(yazi_006, "suspend", Key::named("Z", "ctrl"));
listing_test!(yazi_045, "pasteAbsoluteLink", Key::character('-', ""));
listing_test!(yazi_046, "pasteLink", Key::character('_', ""));
listing_test!(yazi_047, "pasteHardLink", Key::named("Minus", "ctrl"));
listing_test!(
    yazi_065,
    "detailBirth",
    Key::character('m', ""),
    Key::character('b', "")
);
listing_test!(
    yazi_068,
    "detailOff",
    Key::character('m', ""),
    Key::character('n', "")
);
listing_test!(
    yazi_069,
    "copyPath",
    Key::character('c', ""),
    Key::character('c', "")
);
listing_test!(
    yazi_070,
    "copyUri",
    Key::character('c', ""),
    Key::character('C', "")
);
listing_test!(
    yazi_071,
    "copydirpath",
    Key::character('c', ""),
    Key::character('d', "")
);
listing_test!(
    yazi_072,
    "copyDirUri",
    Key::character('c', ""),
    Key::character('D', "")
);
listing_test!(
    yazi_073,
    "copyName",
    Key::character('c', ""),
    Key::character('f', "")
);
listing_test!(
    yazi_074,
    "copyStem",
    Key::character('c', ""),
    Key::character('n', "")
);
listing_test!(yazi_077, "findPrevious", Key::character('?', ""));
listing_test!(yazi_078, "findNext", Key::character('n', ""));
listing_test!(
    yazi_082,
    "sortBirth",
    Key::character(',', ""),
    Key::character('b', "")
);
listing_test!(
    yazi_083,
    "sortBirthReverse",
    Key::character(',', ""),
    Key::character('B', "")
);
listing_test!(
    yazi_084,
    "sortExtension",
    Key::character(',', ""),
    Key::character('e', "")
);
listing_test!(
    yazi_092,
    "sortRandom",
    Key::character(',', ""),
    Key::character('r', "")
);
listing_test!(
    yazi_098,
    "followLink",
    Key::character('g', ""),
    Key::character('f', "")
);

pub(super) fn editor_action(keys: &[Key], action: &str) {
    let key = &keys[0];
    let (home, expected) = match action {
        "backward lean" => (false, "one two.Xthree"),
        "forward lean --end-of-word" => (true, "oneX two.three"),
        "kill bol" => (false, "X"),
        "kill eol" => (true, "X"),
        "kill backward" => (false, "one two.X"),
        "kill forward" => (true, "X two.three"),
        _ => panic!("unverified {action}"),
    };
    editor_case(&key.name, &key.mods, home, expected);
}

#[path = "finder_tests.rs"]
mod finder;
#[path = "parity17_flow_tests.rs"]
mod flows;
#[path = "parity17_follow_tests.rs"]
mod follow;
