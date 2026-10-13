use super::*;
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
fn contexts_share_only_declared_actions_and_sheet_matches_preset() {
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
    assert!(map.sheet("vim").iter().any(|line| line.contains("yy")));
    assert!(!map
        .sheet("default")
        .iter()
        .any(|line| line.to_lowercase().contains("grid")));
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
    assert_eq!(hints.dismiss, "Escape");
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
        for (name, chord) in [("dismiss", &hints.dismiss), ("keys", &hints.keys), ("load_preview", &hints.load_preview), ("delete_permanently", &hints.delete_permanently), ("trash", &hints.trash)] {
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
