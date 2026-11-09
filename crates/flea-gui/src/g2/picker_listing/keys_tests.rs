//! Picker.js key cases on G1.04 table; Escape clears, then cancels.
use super::listing::Listing;
use flea::keys::{lookup, Context, Frontend, Input, Match};
use flea::protocol::RowFacts;
use flea::ui_logic_picker_listing::activates;

const RET: i32 = 0x01000004;
const ENTER: i32 = 0x01000005;
const ESC: i32 = 0x01000000;
const F2: i32 = 0x01000032;

fn key(name: &str) -> Input<'_> {
    Input {
        key: name,
        text: "",
        modifiers: 0,
    }
}

fn text(name: &str) -> Input<'_> {
    Input {
        key: "",
        text: name,
        modifiers: 0,
    }
}

fn looked(preset: &str, input: Input<'_>) -> &'static str {
    match lookup(preset, Context::Listing, Frontend::Gui, input) {
        Match::Action(action) => action,
        _ => "unbound",
    }
}

fn landed() -> Listing {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 2));
    assert!(list.held_rows(
        gen,
        0,
        vec![
            RowFacts {
                depth: 0,
                open: false,
                name: "Docs".into(),
                dir: true,
                size: 0,
                mtime: 0,
                mode: 0o040755,
                icon: String::new(),
                link: None,
                real_modes: true,
                thumbless: false,
                kind: String::new(),
                inline: None,
                tags: None,
            },
            RowFacts {
                depth: 0,
                open: false,
                name: "a.txt".into(),
                dir: false,
                size: 0,
                mtime: 0,
                mode: 0o100644,
                icon: String::new(),
                link: None,
                real_modes: true,
                thumbless: false,
                kind: String::new(),
                inline: None,
                tags: None,
            },
        ]
    ));
    list
}

#[test]
fn default_return_and_enter_look_up_as_open_and_activate() {
    assert_eq!(looked("default", key("Return")), "open");
    assert_eq!(looked("default", key("Enter")), "open");
    assert!(activates(looked("default", key("Return")), RET, RET, ENTER));
    assert!(activates(
        looked("default", key("Enter")),
        ENTER,
        RET,
        ENTER
    ));
}

#[test]
fn default_r_and_f2_look_up_as_rename_and_do_not_activate() {
    assert_eq!(looked("default", text("r")), "rename");
    assert!(!activates("rename", 0, RET, ENTER));
    assert_eq!(looked("default", key("F2")), "rename");
    assert!(!activates("rename", F2, RET, ENTER));
}

#[test]
fn mac_return_and_enter_look_up_as_rename_yet_still_activate() {
    assert_eq!(looked("mac", key("Return")), "rename");
    assert_eq!(looked("mac", key("Enter")), "rename");
    assert!(activates(looked("mac", key("Return")), RET, RET, ENTER));
    assert!(activates(looked("mac", key("Enter")), ENTER, RET, ENTER));
}

#[test]
fn mac_r_and_f2_look_up_as_rename_and_do_not_activate() {
    assert_eq!(looked("mac", text("r")), "rename");
    assert!(!activates("rename", 0, RET, ENTER));
    assert_eq!(looked("mac", key("F2")), "rename");
    assert!(!activates("rename", F2, RET, ENTER));
}

#[test]
fn open_and_page_forward_activate_parent_and_escape_never_do() {
    assert!(activates("open", RET, RET, ENTER));
    assert!(activates("pageForward", 0x01000013, RET, ENTER));
    assert!(!activates("parent", 0x01000012, RET, ENTER));
    assert!(!activates("escape", ESC, RET, ENTER));
    assert_eq!(looked("default", key("Escape")), "escape");
}

#[test]
fn esc_clears_a_set_filter_first_then_cancels() {
    let mut list = landed();
    assert_eq!(
        looked("default", key("Escape")),
        "escape",
        "Escape reaches the listing through the G1.04 table"
    );
    assert_eq!(
        list.escape(),
        super::listing::Escape::Cancel,
        "without a filter the window cancels at once"
    );
    list.filter_line("notes");
    assert_eq!(
        list.escape(),
        super::listing::Escape::ClearedFilter,
        "a set filter clears first"
    );
    assert!(list.facts().filtered.is_none());
    assert_eq!(
        list.escape(),
        super::listing::Escape::Cancel,
        "the second esc cancels"
    );
}
