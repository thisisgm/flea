use super::*;
use crate::keys::physical::Sample;
use crate::keys::{CONTROL, SHIFT};

fn sample(key: &str, us: Option<&str>, text: &str, shifted: &str) -> Sample {
    Sample {
        key: key.into(),
        us: us.map(String::from),
        text: (!text.is_empty()).then(|| text.into()),
        shifted: (!shifted.is_empty()).then(|| shifted.into()),
    }
}

fn samples() -> Vec<Sample> {
    vec![
        sample("c", Some("c"), "c", "C"),
        sample("d", Some("d"), "d", "D"),
        sample("g", Some("g"), "g", "G"),
        sample("<", None, "<", ">"),
        sample("f2", None, "", ""),
        sample("enter", None, "", ""),
    ]
}

fn found(action: &str, label: &str) -> Option<Found> {
    Some(Found {
        action: action.into(),
        label: label.into(),
    })
}

/// A small table in the default preset's shape: plain, Shift, Ctrl and Ctrl+Shift bindings, and one pair.
fn keyboard() -> SheetKeyboard {
    SheetKeyboard::new(&samples(), |cap, mask| match (cap.us, cap.key, mask) {
        ("c", _, 0) => found("copyAs", "Copy as"),
        ("c", _, CONTROL) => found("copy", "Copy"),
        ("c", _, m) if m == CONTROL | SHIFT => found("copyPath", "Copy path"),
        ("d", _, 0) => found("trashArm", "Move to Trash"),
        ("d", _, CONTROL) => found("pageDown", "Page down"),
        ("g", _, SHIFT) => found("cursorLast", "Last item"),
        (_, "f2", 0) => found("rename", "Rename"),
        (_, "enter", 0) => found("open", "Open"),
        (_, "enter", CONTROL) => found("openTab", "Open in new tab"),
        _ => None,
    })
}

fn at(keyboard: &SheetKeyboard, id: &str) -> usize {
    keyboard
        .faces
        .iter()
        .position(|face| face.cap.id == id)
        .unwrap()
}

#[test]
fn a_modifier_is_asked_about_once_per_cap_and_never_bound_itself() {
    let keyboard = keyboard();
    assert!(!keyboard
        .bound
        .iter()
        .any(|bound| keyboard.faces[bound.cap].cap.id == "LCTL"));
    // ISO Enter's two pieces are one key: its bindings are found once.
    let enter = keyboard
        .bound
        .iter()
        .filter(|bound| bound.action == "open")
        .count();
    assert_eq!(enter, 1);
    assert_eq!(keyboard.bound.len(), 9);
}

#[test]
fn at_rest_every_key_with_a_binding_is_bound_and_the_rest_faint() {
    let keyboard = keyboard();
    let looks = keyboard.looks(0, None);
    assert_eq!(looks[at(&keyboard, "AB03")], Look::Bound);
    assert_eq!(looks[at(&keyboard, "FK02")], Look::Bound);
    assert_eq!(
        looks[at(&keyboard, "AC05")],
        Look::Bound,
        "g binds with Shift"
    );
    assert_eq!(looks[at(&keyboard, "LSGT")], Look::Faint, "< binds nothing");
    // A modifier some binding uses is bound; Alt, which none uses, stays faint.
    assert_eq!(looks[at(&keyboard, "LCTL")], Look::Bound);
    assert_eq!(looks[at(&keyboard, "LFSH")], Look::Bound);
    assert_eq!(looks[at(&keyboard, "LALT")], Look::Faint);
    let pieces = keyboard
        .faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.cap.id == "RTRN");
    assert!(pieces
        .into_iter()
        .all(|(index, _)| looks[index] == Look::Bound));
}

#[test]
fn holding_ctrl_lights_both_ctrls_and_brightens_only_ctrl_bindings() {
    let keyboard = keyboard();
    let looks = keyboard.looks(CONTROL, None);
    assert_eq!(looks[at(&keyboard, "LCTL")], Look::Lit);
    assert_eq!(looks[at(&keyboard, "RCTL")], Look::Lit);
    assert_eq!(looks[at(&keyboard, "AB03")], Look::Bound);
    assert_eq!(looks[at(&keyboard, "AC03")], Look::Bound);
    assert_eq!(looks[at(&keyboard, "FK02")], Look::Faint);
    assert_eq!(
        looks[at(&keyboard, "AC05")],
        Look::Faint,
        "g binds only with Shift"
    );
    assert_eq!(looks[at(&keyboard, "LFSH")], Look::Bound, "Ctrl+Shift+C");
    assert_eq!(looks[at(&keyboard, "LALT")], Look::Faint);
}

#[test]
fn right_alt_lights_with_alt_only_where_the_group_makes_it_alt() {
    let altgr = keyboard();
    assert_eq!(altgr.looks(ALT, None)[at(&altgr, "RALT")], Look::Faint);
    assert_eq!(altgr.faces[at(&altgr, "RALT")].legend, "ALTGR");
    let mut us = samples();
    us.push(sample("alt_r", None, "", ""));
    let alt = SheetKeyboard::new(&us, |_, _| None);
    assert_eq!(alt.looks(ALT, None)[at(&alt, "RALT")], Look::Lit);
    assert_eq!(alt.faces[at(&alt, "RALT")].legend, "ALT");
}

#[test]
fn a_held_set_lists_every_binding_using_it_exact_set_first() {
    let keyboard = keyboard();
    let rows: Vec<_> = keyboard
        .rows(CONTROL, None)
        .iter()
        .map(|b| b.action.as_str())
        .collect();
    assert_eq!(rows, ["openTab", "pageDown", "copy", "copyPath"]);
    let shift: Vec<_> = keyboard
        .rows(SHIFT, None)
        .iter()
        .map(|b| b.action.as_str())
        .collect();
    assert_eq!(shift, ["cursorLast", "copyPath"]);
    assert_eq!(keyboard.rows(0, None).len(), keyboard.bound.len());
}

#[test]
fn a_looked_up_chord_lights_its_cap_and_narrows_the_list_to_it() {
    let keyboard = keyboard();
    let c = at(&keyboard, "AB03");
    let looks = keyboard.looks(CONTROL, Some(c));
    assert_eq!(looks[c], Look::Lit);
    assert_eq!(looks[at(&keyboard, "AC03")], Look::Bound);
    let rows: Vec<_> = keyboard
        .rows(CONTROL, Some(c))
        .iter()
        .map(|b| b.action.as_str())
        .collect();
    assert_eq!(rows, ["copy", "copyPath"]);
    assert_eq!(keyboard.chord(CONTROL, c), "CTRL + C");
    assert_eq!(keyboard.chord(CONTROL | SHIFT, c), "CTRL + SHIFT + C");
}

#[test]
fn a_key_event_finds_its_cap_by_position_then_by_name() {
    let keyboard = keyboard();
    assert_eq!(keyboard.cap_of("c", Some("c")), Some(at(&keyboard, "AB03")));
    // A non-Latin group reports its own key with the US position beside it.
    assert_eq!(keyboard.cap_of("с", Some("c")), Some(at(&keyboard, "AB03")));
    assert_eq!(keyboard.cap_of("f2", None), Some(at(&keyboard, "FK02")));
    assert_eq!(keyboard.cap_of("c", None), Some(at(&keyboard, "AB03")));
    assert_eq!(keyboard.cap_of("enter", None), Some(at(&keyboard, "RTRN")));
    assert_eq!(
        keyboard.cap_of("pagedown", None),
        Some(at(&keyboard, "PGDN"))
    );
    assert_eq!(
        keyboard.cap_of("control", None),
        None,
        "a modifier is held, never looked up"
    );
}

#[test]
fn a_pair_arm_is_marked_and_a_blank_cap_is_spelled_by_its_position() {
    let keyboard = keyboard();
    let d = keyboard
        .bound
        .iter()
        .find(|bound| bound.action == "trashArm")
        .unwrap();
    assert!(d.pair());
    assert_eq!(keyboard.chord(0, d.cap), "D");
    assert_eq!(keyboard.chord(SHIFT, at(&keyboard, "AC05")), "SHIFT + G");
    assert_eq!(keyboard.chord(CONTROL, at(&keyboard, "AE12")), "CTRL + =");
}

#[test]
fn a_searched_action_lights_the_keys_that_run_it() {
    let keyboard = keyboard();
    let looks = keyboard.looks_for("rename");
    assert_eq!(looks[at(&keyboard, "FK02")], Look::Lit);
    assert_eq!(looks[at(&keyboard, "AB03")], Look::Faint);
    // Only a Ctrl binding: Ctrl lights with its key.
    let looks = keyboard.looks_for("pageDown");
    assert_eq!(looks[at(&keyboard, "AC03")], Look::Lit);
    assert_eq!(looks[at(&keyboard, "LCTL")], Look::Lit);
    // A pair arm answers for its action, as `d d` runs trash.
    assert_eq!(
        keyboard.looks_for("trash")[at(&keyboard, "AC03")],
        Look::Lit
    );
    // An action run both plain and with Ctrl, as `,` and Ctrl+`,` open Settings, lights Ctrl too.
    let both = SheetKeyboard::new(&samples(), |cap, mask| match (cap.us, mask) {
        ("c", 0 | CONTROL) => found("settings", "Settings"),
        _ => None,
    });
    let looks = both.looks_for("settings");
    assert_eq!(looks[at(&both, "AB03")], Look::Lit);
    assert_eq!(looks[at(&both, "LCTL")], Look::Lit);
    assert_eq!(looks[at(&both, "LFSH")], Look::Faint);
    // An action no key runs, such as a menu row, leaves the resting keyboard.
    assert_eq!(keyboard.looks_for("openWith"), keyboard.looks(0, None));
}

#[test]
fn a_shifted_symbol_finds_its_cap_by_the_us_key_it_shifts() {
    let keyboard = keyboard();
    // Shift+7 on Swedish reports the US position as `&`, and types `/`.
    let seven = keyboard.faces.iter().position(|face| face.cap.us == "7");
    assert_eq!(keyboard.cap_of("/", Some("&")), seven);
    // The ISO key beside Shift and `.` both shift to `>` on US; the typed legend decides.
    let less = at(&keyboard, "LSGT");
    assert_eq!(keyboard.cap_of(">", Some(">")), Some(less));
}

#[test]
fn a_list_row_spells_its_keys_as_the_keyboard_does() {
    let cases = [
        ("ctrl-c", "CTRL + C"),
        ("y / ctrl-c", "Y / CTRL + C"),
        ("G / end", "SHIFT + G / END"),
        ("shift-Ö / ctrl-l", "SHIFT + Ö / CTRL + L"),
        ("dd / delete", "D D / DEL"),
        ("j / down", "J / ↓"),
        ("shift-tab", "SHIFT + TAB"),
        ("ctrl-shift-+", "CTRL + SHIFT + +"),
        ("ctrl--", "CTRL + -"),
        ("ctrl-r / f5", "CTRL + R / F5"),
        ("super-alt-v", "ALT + SUPER + V"),
        ("g space", "G SPACE"),
        ("enter", "RETURN"),
        ("?", "?"),
        ("/", "/"),
    ];
    for (keys, spelled) in cases {
        assert_eq!(spell(keys), spelled, "{keys}");
    }
}

#[test]
fn a_held_set_reads_in_the_rest_lists_groups_with_plugin_keys_last() {
    let keyboard = SheetKeyboard::new(&samples(), |cap, mask| match (cap.us, cap.key, mask) {
        ("c", _, CONTROL) => found("copy", "Copy"),
        ("c", _, m) if m == CONTROL | SHIFT => found("copyName", "Copy name"),
        ("d", _, CONTROL) => found("je93.todo", "Todo"),
        ("g", _, m) if m == CONTROL | SHIFT => found("tabNext", "Next tab"),
        (_, "enter", CONTROL) => found("openTab", "Open in new tab"),
        _ => None,
    });
    let rows: Vec<_> = keyboard
        .rows(CONTROL, None)
        .iter()
        .map(|b| b.action.as_str())
        .collect();
    // Move before Change, the exact set first within each; a plugin's key after every group.
    assert_eq!(
        rows,
        ["openTab", "tabNext", "copy", "copyName", "je93.todo"]
    );
    let group = |action| group_of(action).map(|at| crate::keys::SHEET_GROUPS[at].id);
    assert_eq!(group("cursorDown"), Some("move"));
    assert_eq!(group("tab3"), Some("move"), "a digit's tab moves");
    assert_eq!(group("zoomFit"), Some("look"));
    assert_eq!(group("tag2"), Some("change"));
    assert_eq!(group("je93.todo"), None);
}

#[test]
fn a_looked_up_key_lists_its_own_bindings_group_first() {
    // vim's D: `dd` cuts (Change), Ctrl + D pages down (Move); looking up D puts the exact `D D` row first.
    let keyboard = SheetKeyboard::new(&samples(), |cap, mask| match (cap.us, mask) {
        ("d", 0) => found("cutArm", "Cut"),
        ("d", CONTROL) => found("pageDown", "Page down"),
        _ => None,
    });
    let d = at(&keyboard, "AC03");
    let rows: Vec<_> = keyboard
        .rows(0, Some(d))
        .iter()
        .map(|b| b.action.as_str())
        .collect();
    assert_eq!(rows, ["cutArm", "pageDown"]);
}

#[test]
fn the_iso_key_without_a_us_position_is_found_by_what_it_types() {
    // UK: the key beside left Shift types `\`, while the US `\` position types `#`; GPUI reports no US key for it.
    let uk = vec![
        sample("\\", None, "\\", "|"),
        sample("#", Some("\\"), "#", "~"),
        sample("c", Some("c"), "c", "C"),
    ];
    let keyboard = SheetKeyboard::new(&uk, |cap, _| {
        (cap.us == "c").then(|| found("copy", "Copy")).flatten()
    });
    assert_eq!(keyboard.cap_of("\\", None), Some(at(&keyboard, "LSGT")));
    assert_eq!(
        keyboard.cap_of("|", None),
        Some(at(&keyboard, "LSGT")),
        "its Shift character too"
    );
    assert_eq!(
        keyboard.cap_of("#", Some("\\")),
        Some(at(&keyboard, "BKSL"))
    );
}

#[test]
fn a_named_cap_keeps_its_key_name_in_the_rest_lists_spelling() {
    let keyboard = keyboard();
    let insert = &keyboard.faces[at(&keyboard, "INS")];
    assert_eq!(canonical(insert, 0), "insert");
    assert_eq!(spell(&canonical(insert, 0)), "INS", "not a sequence I N S");
    let c = &keyboard.faces[at(&keyboard, "AB03")];
    assert_eq!(canonical(c, CONTROL | SHIFT), "ctrl-shift-c");
}
