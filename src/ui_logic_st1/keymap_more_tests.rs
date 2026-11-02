// Hint, sheet, key-name and pointer checks ported from the keymap oracle.
use super::test_support::english;
use crate::keys::presentation::{hint_for, hinted_all, key_for, row_for, sheet, spelled, SheetRow};
use crate::keys::{
    action_group, binding_rows, lookup, Context, Frontend, Input, CONTROL, POINTER, PRESETS,
    PRESET_KEYS, SHARED_KEYS, SHEET_GROUPS, SHEET_MIRRORS, SHIFT,
};

fn key(preset: &str, name: &str, text: &str, mods: u8, expected: &str, context: Context) {
    let input = Input {
        key: name,
        text,
        modifiers: mods,
    };
    let got = lookup(preset, context, Frontend::Gui, input).action();
    assert_eq!(got, expected, "{preset} {} {name} {text:?} {mods}", context.name());
}

fn action_of(preset: &str, name: &str, context: Context) -> &'static str {
    let input = Input {
        key: name,
        text: name,
        modifiers: 0,
    };
    lookup(preset, context, Frontend::Gui, input).action()
}

#[test]
fn mac_hints_name_the_native_keys() {
    assert_eq!(hint_for("mac", "rename"), "enter", "Mac menu advertises its actual Return action");
    assert_eq!(hint_for("mac", "open"), "right", "Mac Open hint uses native Right");
}

#[test]
fn vim_rename_hint_is_its_own() {
    assert_eq!(hint_for("vim", "rename"), "r", "vim rename hint is the key vim binds");
    assert_ne!(hint_for("mac", "rename"), hint_for("vim", "rename"), "mac and vim hint rename differently, so that check can go red");
}

#[test]
fn vim_and_default_hints_name_the_key_they_start_with() {
    assert_eq!(hint_for("vim", "copy"), "y", "Vim Copy hints the key it starts with");
    assert_eq!(hint_for("vim", "cut"), "d", "Vim Cut hints the key it starts with");
    assert_eq!(crate::keys::preset("unknown"), "default", "unknown stored preset resolves to Default");
    assert_eq!(hint_for("default", "copy"), "y", "Default Copy hint remains y");
    assert_eq!(hint_for("default", "trash"), "d", "Default Trash hints the key it starts with");
    assert_eq!(hint_for("default", "emptyTrash"), "", "menu-only actions invent no shortcut");
}

#[test]
fn default_sheet_never_advertises_a_lone_d() {
    let rows = sheet("default", Frontend::Gui, &english());
    let trash = rows.iter().find(|row| row.action == "trash").expect("trash row");
    assert!(!trash.keys.split(" / ").any(|spelling| spelling == "d"), "Default sheet never advertises a lone destructive d");
}

#[test]
fn menu_leaves_answer_their_own_letters_in_every_preset() {
    for preset in PRESETS {
        key(preset, "C", "c", 0, "copyAs", Context::Listing);
        key(preset, "P", "P", SHIFT, "pasteAs", Context::Listing);
        key(preset, "V", "V", SHIFT, "invertSelection", Context::Listing);
        key(preset, "C", "", CONTROL | SHIFT, "copyPath", Context::Listing);
        key(preset, "P", "p", 0, "copyPath", Context::Menu);
        key(preset, "N", "n", 0, "copyName", Context::Menu);
        key(preset, "E", "e", 0, "copyStem", Context::Menu);
        key(preset, "F", "f", 0, "copydirpath", Context::Menu);
        key(preset, "U", "u", 0, "copyUri", Context::Menu);
        key(preset, "S", "s", 0, "copyQuoted", Context::Menu);
        key(preset, "L", "L", SHIFT, "pasteLink", Context::Menu);
        key(preset, "A", "a", 0, "pasteAbsoluteLink", Context::Menu);
        key(preset, "H", "H", SHIFT, "pasteHardLink", Context::Menu);
    }
}

#[test]
fn copy_as_hint_and_shift_delete_hint() {
    assert_eq!(hint_for("default", "copyAs"), "c", "Copy as hints its opener");
    assert_eq!(hint_for("default", "deletePermanently"), "shift-delete", "a shift chord fills an action no plain key names");
}

#[test]
fn delete_permanently_has_rows_of_its_own_and_answers_shift_delete() {
    let own_rows = PRESET_KEYS
        .iter()
        .chain(SHARED_KEYS)
        .filter(|row| {
            (row.preset == "all" || row.preset == "default")
                && row.applies(Context::Listing, Frontend::Gui)
                && action_group(row.action) == "deletePermanently"
        })
        .count();
    assert!(own_rows > 0, "the action asked for has rows of its own, so the count below can go red");
    assert_eq!(hint_for("default", "deletePermanently"), "shift-delete", "and still answers the shift chord");
}

#[test]
fn delete_permanently_shift_hint_in_vim() {
    assert_eq!(hint_for("vim", "deletePermanently"), "shift-delete", "deletePermanently still gets its shift hint where no plain row exists");
}

#[test]
fn sheet_is_populated_from_the_effective_bindings() {
    let rows = sheet("vim", Frontend::Gui, &english());
    assert!(rows.len() > 30, "sheet is populated from effective current bindings");
}

#[test]
fn focus_next_reads_as_scope_and_focus_on_the_sheet() {
    let rows = sheet("default", Frontend::Gui, &english());
    let tab = rows.iter().find(|row| row.action == "focusNext").expect("focusNext row");
    assert_eq!(tab.keys, "tab", "the sheet reads Tab with no pane-only wording");
    assert_eq!(tab.label, "Next area", "the sheet reads Tab in the board's sentence-case wording");
}

#[test]
fn focus_back_is_in_the_move_group() {
    let rows = sheet("default", Frontend::Gui, &english());
    let back = rows.iter().find(|row| row.action == "focusPrevious").expect("focusPrevious row");
    assert_eq!(back.label, "Previous area", "and Shift+Tab as Previous area, in the Move group");
    let in_move = SHEET_GROUPS
        .iter()
        .any(|group| group.id == "move" && group.actions.contains(&"focusPrevious"));
    assert!(in_move, "and Shift+Tab sits in the Move group");
}

#[test]
fn sheet_caps_stay_narrow_and_labels_name_no_identifier() {
    let catalog = english();
    let mut widest = 0;
    let mut offenders = Vec::new();
    for preset in PRESETS {
        for row in sheet(preset, Frontend::Gui, &catalog) {
            widest = widest.max(row.keys.len());
            let camel = row
                .label
                .as_bytes()
                .windows(2)
                .any(|pair| pair[0].is_ascii_lowercase() && pair[1].is_ascii_uppercase());
            if camel {
                offenders.push(row.label.clone());
            }
            if row.keys.split(" / ").count() > 2 {
                offenders.push(format!("too many spellings: {}", row.keys));
            }
        }
    }
    assert!(widest <= 18, "no cap in any preset outgrows its half of the card");
    assert!(offenders.is_empty(), "no row prints an action id where its wording belongs: {offenders:?}");
}

#[test]
fn mute_is_listed_once_per_preset_under_m() {
    let catalog = english();
    let mut mute_rows = 0;
    let mut mute_keys = String::new();
    let mut menu_still_m = true;
    for preset in PRESETS {
        for row in sheet(preset, Frontend::Gui, &catalog) {
            if row.action == "mute" {
                mute_rows += 1;
                mute_keys.clone_from(&row.keys);
            }
        }
        if action_of(preset, "m", Context::Listing) != "menu" {
            menu_still_m = false;
        }
        if action_of(preset, "m", Context::Media) != "mute" {
            menu_still_m = false;
        }
    }
    assert_eq!(mute_rows, PRESETS.len(), "every preset lists mute once");
    assert_eq!(mute_keys, "m", "and lists it under its own key");
    assert!(menu_still_m, "while m still opens the menu in the listing and mutes in a media preview");
    let in_look = SHEET_GROUPS
        .iter()
        .any(|group| group.id == "look" && group.actions.contains(&"mute"));
    assert!(in_look, "the sheet group that claims it is Look");
}

#[test]
fn tab_rows_read_the_chord_alone_and_brackets_still_step() {
    let catalog = english();
    let mut caps = Vec::new();
    for preset in PRESETS {
        let rows = sheet(preset, Frontend::Gui, &catalog);
        let next = rows
            .iter()
            .find(|row| row.action == "tabNext")
            .map_or("none", |row| row.keys.as_str());
        let previous = rows
            .iter()
            .find(|row| row.action == "tabPrevious")
            .map_or("none", |row| row.keys.as_str());
        caps.push(format!("{preset}:{next};{previous}"));
    }
    let wanted: Vec<String> = PRESETS
        .iter()
        .map(|name| format!("{name}:ctrl-pagedown;ctrl-pageup"))
        .collect();
    assert_eq!(caps.join(" "), wanted.join(" "), "next tab and prev tab read as the board draws them on every preset");
    assert_eq!(action_of("default", "]", Context::Listing), "tabNext", "] still steps to the next tab");
    assert_eq!(action_of("default", "[", Context::Listing), "tabPrevious", "[ still steps to the previous tab");
    assert_eq!(hint_for("vim", "tabNext"), "]", "and the menu hint for next tab stays ]");
}

#[test]
fn pointer_contract_is_populated() {
    assert!(POINTER.len() > 10, "pointer contract remains populated");
}

#[test]
fn mac_suppressed_command_x_is_hidden_and_effective_rows_resolve() {
    let effective: Vec<_> = binding_rows("mac", Frontend::Gui).collect();
    assert!(!effective.iter().any(|row| row.mods == "ctrl" && row.key == "X"), "suppressed Mac Ctrl+X never appears in sheet");
    let resolves = effective.iter().all(|row| {
        lookup("mac", Context::Listing, Frontend::Gui, row.input()).action() == row.action
    });
    assert!(resolves, "every effective Mac binding resolves to advertised action");
}

#[test]
fn f4_fits_columns_in_every_preset_with_a_lowercase_token() {
    for preset in PRESETS {
        key(preset, "F4", "", 0, "autofitColumns", Context::Listing);
    }
    let row = PRESET_KEYS.iter().find(|row| row.action == "autofitColumns");
    assert_eq!(row.map_or("", |row| row.keys), "f4", "and its keys token is lowercase like every other");
}

#[test]
fn open_in_new_tab_is_in_move_and_drawn_on_the_sheet() {
    let in_move = SHEET_GROUPS
        .iter()
        .any(|group| group.id == "move" && group.actions.contains(&"openTab"));
    assert!(in_move, "openTab is claimed by the move group");
    let rows = sheet("default", Frontend::Gui, &english());
    let open_tab = rows.iter().find(|row| row.action == "openTab");
    // Sentence case for sheet labels, per CHANGES.md.
    assert_eq!(open_tab.map_or("", |row| row.label.as_str()), "Open in new tab", "and the sheet draws it as open in new tab");
    assert_eq!(open_tab.map_or("", |row| row.keys.as_str()), "ctrl-enter", "and the sheet caps it as the ctrl enter chord");
}

fn spellings(rows: &[SheetRow], action: &str) -> usize {
    rows.iter()
        .find(|row| row.action == action)
        .map_or(0, |row| row.keys.split(" / ").count())
}

#[test]
fn mirrored_pairs_draw_the_same_number_of_spellings() {
    let catalog = english();
    let mut lopsided = Vec::new();
    for preset in PRESETS {
        for frontend in [Frontend::Gui, Frontend::Tui] {
            let rows = sheet(preset, frontend, &catalog);
            for pair in SHEET_MIRRORS {
                if spellings(&rows, pair[0]) != spellings(&rows, pair[1]) {
                    lopsided.push(format!("{preset}:{}:{}/{}", frontend.name(), pair[0], pair[1]));
                }
            }
        }
    }
    assert!(lopsided.is_empty(), "no mirrored pair of the generated sheet is lopsided in any preset or frontend: {lopsided:?}");
}

#[test]
fn default_tab_pair_draws_one_spelling_each() {
    let rows = sheet("default", Frontend::Gui, &english());
    let next = rows.iter().find(|row| row.action == "tabNext").map_or("", |row| row.keys.as_str());
    let previous = rows
        .iter()
        .find(|row| row.action == "tabPrevious")
        .map_or("", |row| row.keys.as_str());
    assert_eq!(format!("{next}|{previous}"), "ctrl-pagedown|ctrl-pageup", "next and prev tab draw one spelling each on Default");
    assert!(!SHEET_MIRRORS.is_empty(), "the generator names the pairs it evens");
}

const DRAWN: [&str; 8] = [
    "newFolder",
    "textSizeUp",
    "textSizeDown",
    "textSizeReset",
    "trash",
    "loadPreview",
    "keymapSheet",
    "filter",
];

#[test]
fn sheet_scale_chords_and_carry_follow_the_board_in_every_preset() {
    let catalog = english();
    for preset in PRESETS {
        for frontend in [Frontend::Gui, Frontend::Tui] {
            let rows = sheet(preset, frontend, &catalog);
            for (action, keys) in [
                ("textSizeUp", "ctrl-shift-plus"),
                ("textSizeDown", "ctrl-shift-minus"),
                ("textSizeReset", "ctrl-shift-0"),
            ] {
                let row = rows.iter().find(|row| row.action == action).unwrap();
                assert_eq!(row.keys, keys, "{preset} {frontend:?} {action}");
            }
            let carry = rows.iter().find(|row| row.action == "carry");
            if frontend == Frontend::Gui {
                let carry = carry.expect("GUI carry row");
                assert_eq!((&*carry.keys, &*carry.label, carry.group), ("X", "Carry to another app", "move"));
            } else {
                assert!(carry.is_none(), "carry is a GUI action");
            }
        }
    }
}

#[test]
fn key_name_draws_the_first_row_of_each_drawn_action() {
    let mut drifted = Vec::new();
    for preset in PRESETS {
        let whole: Vec<_> = binding_rows(preset, Frontend::Gui).collect();
        for action in DRAWN {
            let first = whole.iter().find(|row| row.action == action);
            let plain = hint_for(preset, action);
            let want = if plain.is_empty() {
                first.map_or_else(String::new, |row| spelled(row.keys))
            } else {
                plain.to_owned()
            };
            let row = row_for(preset, action, None);
            let first_keys = first.map_or("", |row| row.keys);
            let row_keys = row.map_or("", |row| row.keys);
            if key_for(preset, action) != want || row_keys != first_keys {
                drifted.push(format!("{preset}:{action}"));
            }
        }
    }
    assert!(drifted.is_empty(), "KeyName names the whole table's first row: {drifted:?}");
}

#[test]
fn a_chord_only_action_is_named_in_every_preset() {
    for preset in PRESETS {
        assert!(!key_for(preset, "newFolder").is_empty(), "and a chord-only action is named");
    }
}

#[test]
fn underscore_minus_and_plus_are_three_marks() {
    let marks: Vec<String> = ["ctrl-shift-underscore", "ctrl-shift-minus", "ctrl-shift-plus"]
        .iter()
        .map(|keys| spelled(keys))
        .collect();
    assert_eq!(marks.join("|"), "Ctrl+Shift+_|Ctrl+Shift+-|Ctrl+Shift++", "underscore, minus and plus are three marks");
}

#[test]
fn hinted_all_names_every_key_or_none() {
    let catalog = english();
    assert_eq!(hinted_all(&catalog, "%1 and %2 step", "default", &["textSizeUp", "textSizeDown"]), "Ctrl+Shift++ and Ctrl+Shift+- step", "a sentence with several keys names them all");
    assert_eq!(hinted_all(&catalog, "%1 and %2 step", "default", &["textSizeUp", "noSuchAction"]), "", "and is empty when the preset binds none of one of them");
}

// KeyName.js: the comment's samples, keyOf for one modifier kind, and hinted empty when the preset binds no key.
#[test]
fn noscene_key_name_spells_samples_and_names_one_modifier_kind() {
    use crate::keys::presentation::key_of;
    assert_eq!(spelled("ctrl-shift-n"), "Ctrl+Shift+N");
    assert_eq!(spelled("delete"), "Delete");
    assert_eq!(spelled("ctrl-shift-plus"), "Ctrl+Shift++");
    assert_eq!(key_of("default", "deletePermanently", "shift"), "Shift+Delete", "the shift row, spelled");
    assert_eq!(key_of("default", "deletePermanently", "super"), "", "no row of that kind names no key");
    assert_eq!(key_of("default", "noSuchAction", "none"), "", "an unbound action names no key");
}

#[test]
fn noscene_hinted_is_empty_when_the_preset_binds_no_key() {
    use crate::keys::presentation::{hinted, key_for};
    let catalog = english();
    let undo = key_for("default", "undo");
    assert!(!undo.is_empty() && undo != "undo", "undo is bound in the default preset: {undo}");
    assert_eq!(hinted(&catalog, "feedback.hint.undo", "default", "undo"), format!("{undo} undoes"), "a bound action fills %1 with its key");
    assert_eq!(hinted(&catalog, "feedback.hint.undo", "default", "noSuchAction"), "", "and an unbound one leaves no hint");
}
