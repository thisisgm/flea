use super::*;
#[path = "keymap_additive_tests.rs"]
mod additive;
#[test]
fn compiled_source_drives_text_and_native_ctrl() {
    let map = Map::load();
    assert_eq!(
        map.action(
            &Key {
                name: "J".into(),
                text: "j".into(),
                mods: "".into(),
                pointer: None,
            },
            "default"
        ),
        "cursorDown"
    );
    assert_eq!(
        map.action(
            &Key {
                name: "X".into(),
                text: "X".into(),
                mods: "ctrl".into(),
                pointer: None,
            },
            "mac"
        ),
        ""
    );
    assert_eq!(
        map.action(
            &Key {
                name: "Insert".into(),
                text: "".into(),
                mods: "shift".into(),
                pointer: None,
            },
            "vim"
        ),
        "paste"
    );
}
#[test]
fn contexts_share_only_declared_actions() {
    let map = Map::load();
    assert_eq!(map.in_context(&Key::character('d', ""), "vim", "pdf"), "");
    assert_eq!(
        map.in_context(&Key::named("Space", ""), "mac", "pdf"),
        "preview"
    );
    assert_eq!(
        map.in_context(&Key::named("Tab", "shift"), "windows", "media"),
        "focusPrevious"
    );
    assert_eq!(
        map.in_context(&Key::named("Insert", "ctrl"), "default", "editor"),
        ""
    );
    assert_eq!(map.action(&Key::character('y', ""), "vim"), "copyArm");
}
#[test]
fn commented_headers_keep_shipped_mac_navigation_aliases_separate() {
    let map = Map::load();
    for (key, action) in [("Up", "parent"), ("Down", "open"), ("Delete", "trash")] {
        assert_eq!(map.action(&Key::named(key, "ctrl"), "mac"), action);
    }
    assert_eq!(map.action(&Key::character('3', "ctrl"), "mac"), "viewGrid");
}
#[test]
fn default_redo_is_control_shift_z() {
    let map = Map::load();
    assert_eq!(map.action(&Key::character('Z', "ctrlshift"), "default"), "redo");
}
#[test]
fn ctrl_equal_grows_listing_text_and_shift_chords_stay() {
    let map = Map::load();
    for (key, action) in [
        (Key::character('=', "ctrl"), "textSizeUp"),
        (Key::character('+', "ctrl"), "textSizeUp"),
        (Key::character('-', "ctrl"), "textSizeDown"),
        (Key::character('0', "ctrl"), "textSizeReset"),
        (Key::named("Plus", "ctrlshift"), "textSizeUp"),
        (Key::named("Equal", "ctrlshift"), "textSizeUp"),
        (Key::named("Minus", "ctrlshift"), "textSizeDown"),
        (Key::named("Underscore", "ctrlshift"), "textSizeDown"),
        (Key::named("0", "ctrlshift"), "textSizeReset"),
    ] {
        assert_eq!(map.action(&key, "default"), action, "{}", key.name);
    }
    assert_eq!(map.action(&Key::character('+', ""), "default"), "zoomIn");
    assert_eq!(map.action(&Key::character('-', ""), "default"), "zoomOut");
    assert_eq!(map.in_context(&Key::character('+', "ctrl"), "default", "pdf"), "");
    assert_eq!(map.in_context(&Key::character('-', "ctrl"), "default", "pdf"), "");
    assert_eq!(map.in_context(&Key::character('+', ""), "default", "pdf"), "zoomIn");
    assert_eq!(map.in_context(&Key::character('0', ""), "default", "preview"), "zoomFit");
    assert_eq!(map.in_context(&Key::character('0', "ctrl"), "default", "preview"), "");
}

#[test]
fn ctrl_text_aliases_keep_the_display_hints_on_shift_chords() {
    for preset in ["default", "vim", "mac", "windows"] {
        for (action, shown) in [
            ("textSizeUp", "Ctrl+Shift++"),
            ("textSizeDown", "Ctrl+Shift+-"),
            ("textSizeReset", "Ctrl+Shift+0"),
        ] {
            assert_eq!(
                flea::keys::presentation::key_for(preset, action),
                shown,
                "{preset}: {action}"
            );
        }
    }
}
#[test]
fn header_comments_do_not_strip_quoted_hashes_or_accept_other_suffixes() {
    let map = Map::parse(r##"
[[text]] # A header comment can contain = and ]].
char = "#"
action = "literal#action"
# [[preset]] is still a comment.
[[text]]# No intervening space is required.
char = "j"
action = "cursorDown"
[[code]] unexpected
"##);
    assert_eq!(map.blocks.len(), 2);
    assert_eq!(map.action(&Key::character('#', ""), "default"), "literal#action");
    assert_eq!(map.action(&Key::character('j', ""), "default"), "cursorDown");
}
#[test]
fn hints_name_the_shipped_keys_as_the_sheet_spells_them() {
    let map = Map::load();
    let hints = Hints::new(&map, "default");
    assert_eq!(hints.keys, "?");
    assert_eq!(hints.load_preview, "Ctrl+Space");
    assert_eq!(hints.delete_permanently, "Shift+Delete");
    assert_eq!(hints.trash, "Delete");
    assert_eq!(map.chord("trashArm", "default", "listing").as_deref(), Some("d"));
    assert_eq!(map.chord("noSuchAction", "default", "listing"), None);
}
#[test]
fn every_shipped_preset_names_a_key_for_every_hint() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows"] {
        let hints = Hints::new(&map, preset);
        for (name, chord) in [("keys", &hints.keys), ("load_preview", &hints.load_preview), ("delete_permanently", &hints.delete_permanently), ("trash", &hints.trash)] {
            assert!(!chord.is_empty(), "{preset} has no key for the {name} hint");
        }
    }
}
#[test]
fn a_rebound_key_reaches_its_chord_and_a_shadowed_one_does_not() {
    let map = Map::parse(r##"
[[preset]]
name = "default"
mods = "ctrl"
key = "P"
action = "loadPreview"
context = "listing"
[[code]]
key = "Backspace"
action = "escape"
[[code]]
key = "Tab"
action = "escape"
[[code]]
key = "Backspace"
action = "parent"
"##);
    assert_eq!(map.chord("loadPreview", "default", "listing").as_deref(), Some("Ctrl+P"));
    assert_eq!(map.chord("loadPreview", "vim", "listing"), None, "another preset does not read the default's chord");
    assert_eq!(map.chord("escape", "default", "listing").as_deref(), Some("Backspace"));
    assert_eq!(map.chord("parent", "default", "listing"), None, "the earlier Backspace row shadows this one");
}

#[test]
fn enhanced_pairs_and_plain_mac_alternatives_are_resolved_in_every_preset() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for (key, action) in [
            (Key::named("Return", "ctrl"), "openTab"),
            (Key::character('O', ""), "openTab"),
            (Key::named("Delete", "shift"), "deletePermanently"),
            (Key::character('Z', "ctrlshift"), "redo"),
            (Key::character('Y', "ctrl"), "redo"),
            (Key::character('N', "ctrlshift"), "newFolder"),
            (Key::character('N', ""), "newFolder"),
            (Key::character('M', ""), "movePaste"),
            (Key::character('U', ""), "duplicate"),
            (Key::character('i', ""), if preset == "yazi" { "properties" } else { "factsToggle" }),
            (Key::character('I', "shift"), "detailNext"),
            (Key::character('P', "ctrl"), "print"),
            (Key::character('G', "ctrl"), "searchContents"),
            (Key::character('A', ""), "openAgent"),
            (Key::character('E', "shift"), "editFile"),
        ] {
            assert_eq!(map.action(&key, preset), action, "{preset} {key:?}");
        }
        assert_eq!(map.action(&Key::character('X', ""), preset), "", "X is window only");
        assert_eq!(map.action(&Key::character('X', "shift"), preset), "");
        assert_ne!(map.action(&Key::character('e', ""), preset), "editFile");
        for context in ["listing", "preview", "pdf", "media"] {
            assert_eq!(map.in_context(&Key::named("Tab", ""), preset, context), "focusNext");
            assert_eq!(map.in_context(&Key::named("Tab", "ctrl"), preset, context), "focusPreview");
            assert_eq!(map.in_context(&Key::named("PageDown", "ctrl"), preset, context), "tabNext");
            assert_eq!(map.in_context(&Key::named("PageUp", "ctrl"), preset, context), "tabPrevious");
        }
        for context in ["menu", "editor", "panel"] {
            assert_eq!(map.in_context(&Key::character('E', "shift"), preset, context), "");
        }
    }
    assert_eq!(map.action(&Key::character('D', "ctrl"), "windows"), "trash");
    assert_eq!(map.action(&Key::character('D', "ctrl"), "default"), "pageDown");
    assert_eq!(map.action(&Key::character('y', ""), "vim"), "copyArm");
    assert_eq!(map.action(&Key::named("Return", ""), "mac"), "rename");
}

#[test]
fn shift_delete_and_uppercase_alternatives_decode_without_keyboard_negotiation() {
    use crate::tui::input::Decoder;
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for (bytes, action) in [
            (b"\x1b[3;2~".as_slice(), "deletePermanently"),
            (b"O".as_slice(), "openTab"),
            (b"\x19".as_slice(), "redo"),
            (b"N".as_slice(), "newFolder"),
        ] {
            let mut decoder = Decoder::default();
            let keys = decoder.feed(bytes, true);
            assert_eq!(keys.len(), 1);
            assert_eq!(Map::load().action(&keys[0], preset), action);
        }
    }
}

#[test]
fn candidate_contents_binding_and_sequence_hints_reach_real_actions() {
    let map = Map::load();
    assert_eq!(map.action(&Key::character('S', ""), "yazi"), "searchContents");
    assert_eq!(map.chord("tabNew", "yazi", "listing").as_deref(), Some("t t"));
    assert_eq!(map.chord("sortName", "yazi", "listing").as_deref(), Some(", a"));
    assert!(map.chord("tabNew", "yazi", "editor").is_none());
    let arm = "[[tui]]\nname = \"yazi\"\nmods = \"text\"\nkey = \"t\"\naction = \"prefix:t\"\ncontext = \"listing\"\n\n";
    let tail = "[[tui]]\nname = \"yazi\"\nmods = \"text\"\nkey = \"t\"\naction = \"tabNew\"\ncontext = \"listing\"\nprefix = \"t\"\n";
    assert_eq!(Map::parse(&format!("{arm}{tail}")).chord("tabNew", "yazi", "listing").as_deref(), Some("t t"));
    let earlier = "[[tui]]\nname = \"yazi\"\nmods = \"text\"\nkey = \"t\"\naction = \"cursorDown\"\ncontext = \"listing\"\n\n";
    assert!(Map::parse(&format!("{earlier}{arm}{tail}")).chord("tabNew", "yazi", "listing").is_none(), "a shadowed arm is not a reachable sequence");
}

#[test]
fn convert_fallback_is_queryable_and_does_not_escape_listing() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for mods in ["", "shift"] {
            assert_eq!(map.action(&Key::character('F', mods), preset), "convert");
            assert!(map.in_context(&Key::character('F', mods), preset, "editor").is_empty());
            assert!(map.in_context(&Key::character('F', mods), preset, "panel").is_empty());
        }
    }
}

// parity row none+Up -> cursorUp [panel]: Up moves the panel's cursor in every preset.
#[test]
fn tui_fix_1101_p2_up_in_the_panel_is_cursor_up() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        assert_eq!(map.in_context(&Key::named("Up", ""), preset, "panel"), "cursorUp", "{preset}");
    }
}
#[test]
fn gui_only_horizontal_extension_stays_out_of_the_terminal() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for name in ["Left", "Right"] {
            assert_eq!(map.action(&Key::named(name, "shift"), preset), "", "{preset} shift-{name}");
        }
    }
}
