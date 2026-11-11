use super::test_projection::{find, projection};
use super::*;
use crate::keys::{lookup, Context, Frontend, Input, Match, PRESETS};

const CHECKSUMS: &str = "example.checksums:sha256";

fn enabled(id: &str) -> bool {
    id == CHECKSUMS
}

fn none(_: &str) -> bool {
    false
}

#[test]
fn shared_and_preset_blocks_load() {
    let keys = parse(
        "# mine\n[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\" # verify\n\n[[preset]]\nname = \"vim\"\nmods = \"text\"\nkey = \"Q\"\naction = \"toggleHidden\"\n",
    );
    assert_eq!(keys.problems, Vec::new());
    assert_eq!(keys.bindings.len(), 2);
    let first = &keys.bindings[0];
    assert_eq!(
        (
            first.preset.as_str(),
            first.mods,
            first.mask,
            first.key.as_str(),
            first.line
        ),
        ("all", "ctrlshift", 3, "K", 2)
    );
    assert!(first.is_plugin());
    assert_eq!(first.keys(), "ctrl-shift-k");
    assert_eq!(keys.bindings[1].preset, "vim");
    assert_eq!(keys.bindings[1].keys(), "Q");
}

#[test]
fn malformed_blocks_are_named_and_the_rest_still_load() {
    let keys = parse(
        "[[ctrl]]\nkey = \"Nope\"\naction = \"copy\"\n[[ctrlshift]]\nkey = \"K\"\n[[hyper]]\nkey = \"K\"\naction = \"copy\"\nloose = 1\n[[ctrlshift]]\nkey = \"J\"\naction = \"a:b:c\"\n[[preset]]\nname = \"emacs\"\nmods = \"ctrl\"\nkey = \"J\"\naction = \"copy\"\n[[ctrlshift]]\nkey = \"H\"\naction = \"example.checksums:sha256\"\nlabel = \"Mine\"\n[[ctrlshift]]\nkey = \"Y\"\naction = \"example.checksums:sha256\"\n",
    );
    let lines: Vec<_> = keys.problems.iter().map(|p| p.line).collect();
    assert_eq!(lines, vec![1, 4, 6, 9, 10, 13, 18]);
    assert!(keys.problems[0].detail.contains("unknown key Nope"));
    assert!(keys.problems[1].detail.contains("missing action"));
    assert!(keys.problems[2].detail.contains("unknown table hyper"));
    assert!(keys.problems[4].detail.contains("unreadable action"));
    assert!(keys.problems[5].detail.contains("unknown preset emacs"));
    assert!(keys.problems[6].detail.contains("unknown field label"));
    assert_eq!(keys.bindings.len(), 1);
    assert_eq!(keys.bindings[0].key, "Y");
}

#[test]
fn an_oversized_file_loads_nothing() {
    let keys = parse(&"#".repeat(MAX_BYTES + 1));
    assert!(keys.bindings.is_empty());
    assert_eq!(keys.problems.len(), 1);
    let facts = crate::ui_logic_settings_pages::pages::UserKeyFacts {
        keys,
        plugins: Vec::new(),
        catalog: None,
        resolved: None,
    };
    assert_eq!(
        crate::ui_logic_settings_pages::pages::user_key_refusals(&facts, "default"),
        vec![crate::ui_logic_settings_pages::Text::Key(
            "settings.hint.key_file"
        )]
    );
}

#[test]
fn a_free_chord_is_live_in_every_preset() {
    let keys = parse("[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n");
    for preset in PRESETS {
        assert_eq!(
            keys.verdicts(preset, &enabled, &projection()),
            vec![Verdict::Live],
            "{preset}"
        );
        let input = Input {
            key: "k",
            text: "K",
            modifiers: crate::keys::CONTROL | crate::keys::SHIFT,
        };
        assert_eq!(
            lookup(preset, Context::Listing, Frontend::Gui, input),
            Match::Unbound,
            "{preset}"
        );
        assert_eq!(
            find(&keys, preset, input, &enabled).map(|b| b.action.as_str()),
            Some(CHECKSUMS)
        );
    }
}

#[test]
fn a_chord_the_preset_binds_is_refused_and_never_found() {
    let keys = parse(
        "[[ctrl]]\nkey = \"C\"\naction = \"example.checksums:sha256\"\n[[text]]\nkey = \"j\"\naction = \"toggleHidden\"\n[[ctrl]]\nkey = \"Q\"\naction = \"copy\"\n[[preset]]\nname = \"mac\"\nmods = \"ctrl\"\nkey = \"K\"\naction = \"example.checksums:sha256\"\n",
    );
    let verdicts = keys.verdicts("mac", &enabled, &projection());
    assert_eq!(verdicts[0], Verdict::Taken("copy"));
    assert_eq!(verdicts[1], Verdict::Taken("cursorDown"));
    assert_eq!(verdicts[2], Verdict::Taken(""));
    assert_eq!(verdicts[3], Verdict::Taken("addNetwork"));
    let input = Input {
        key: "c",
        text: "",
        modifiers: crate::keys::CONTROL,
    };
    assert!(find(&keys, "mac", input, &enabled).is_none());
    // The same mac row is another preset's free chord.
    assert_eq!(
        keys.verdicts("default", &enabled, &projection())[3],
        Verdict::Inactive
    );
}

#[test]
fn a_capital_letter_is_probed_with_shift_held() {
    // G is bound (cursorLast) as typed text with Shift; a user binding there is taken.
    let keys = parse("[[text]]\nkey = \"G\"\naction = \"example.checksums:sha256\"\n");
    assert!(
        matches!(keys.verdicts("default", &enabled, &projection())[0], Verdict::Taken(action) if !action.is_empty())
    );
}

#[test]
fn a_second_binding_of_one_chord_is_repeated_and_the_first_wins() {
    let keys = parse(
        "[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n[[ctrlshift]]\nkey = \"k\"\naction = \"toggleHidden\"\n",
    );
    assert_eq!(
        keys.verdicts("default", &enabled, &projection()),
        vec![Verdict::Live, Verdict::Repeated]
    );
    let input = Input {
        key: "k",
        text: "",
        modifiers: crate::keys::CONTROL | crate::keys::SHIFT,
    };
    assert_eq!(
        find(&keys, "default", input, &enabled).map(|b| b.action.as_str()),
        Some(CHECKSUMS)
    );
    // With the plugin gone the first is inactive, so the second takes the chord.
    assert_eq!(
        keys.verdicts("default", &none, &projection()),
        vec![Verdict::Inactive, Verdict::Live]
    );
    assert_eq!(
        find(&keys, "default", input, &none).map(|b| b.action.as_str()),
        Some("toggleHidden")
    );
}

#[test]
fn an_unknown_builtin_is_refused_and_a_known_one_resolves_to_the_table() {
    let keys = parse("[[ctrlshift]]\nkey = \"K\"\naction = \"launchRockets\"\n[[ctrlshift]]\nkey = \"J\"\naction = \"toggleHidden\"\n");
    assert_eq!(
        keys.verdicts("default", &none, &projection()),
        vec![Verdict::Unknown, Verdict::Live]
    );
    assert_eq!(builtin("toggleHidden"), Some("toggleHidden"));
    assert!(builtin("launchRockets").is_none());
    assert!(builtin_label("toggleHidden").is_some());
}

#[test]
fn a_removed_plugin_drops_its_bindings() {
    let keys = parse("[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n");
    assert_eq!(
        keys.verdicts("default", &none, &projection()),
        vec![Verdict::Inactive]
    );
    let input = Input {
        key: "k",
        text: "",
        modifiers: crate::keys::CONTROL | crate::keys::SHIFT,
    };
    assert!(find(&keys, "default", input, &none).is_none());
}

#[test]
fn plugin_ids_are_the_menu_leaf_form() {
    assert!(is_plugin_id("example.checksums:sha256"));
    assert!(!is_plugin_id("copy"));
    assert!(!is_plugin_id(":sha256"));
    assert!(!is_plugin_id("a:"));
    assert!(!is_plugin_id("a:b:c"));
}

#[test]
fn settings_keys_lists_live_bindings_and_one_refusal_line_each() {
    use crate::ui_logic_settings_pages::{
        pages::{user_key_rows, UserKeyFacts, USER_KEYS},
        rows::Kind,
        Text,
    };
    let mut facts = UserKeyFacts {
        keys: parse("[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n[[ctrl]]\nkey = \"C\"\naction = \"toggleHidden\"\n[[ctrlshift]]\nkey = \"k\"\naction = \"toggleHidden\"\n[[ctrlshift]]\nkey = \"J\"\naction = \"nothing\"\n[[ctrl]]\nkey = \"Q\"\naction = \"copy\"\n[[bogus]]\n"),
        plugins: vec![(CHECKSUMS.into(), "Verify Checksum…".into(), "hash".into())],
        catalog: None,
        resolved: None,
    };
    facts.resolve("default", &projection());
    let rows = user_key_rows(&facts, "default");
    assert_eq!(rows[0].kind, Kind::Group);
    assert_eq!(rows[1].id, USER_KEYS);
    assert_eq!(rows[1].items.len(), 1);
    assert_eq!(rows[1].items[0].keys, "ctrl-shift-k");
    assert_eq!(rows[1].items[0].label, Text::data("Verify Checksum…"));
    assert_eq!(rows[1].items[0].glyph, "hash");
    let hints: Vec<&Text> = rows[2..].iter().map(|row| &row.label).collect();
    assert_eq!(hints.len(), 5, "{hints:?}");
    assert!(matches!(hints[0], Text::Args("settings.hint.key_taken", _)));
    assert!(matches!(
        hints[1],
        Text::Args("settings.hint.key_repeated", _)
    ));
    assert!(matches!(
        hints[2],
        Text::Args("settings.hint.key_unknown", _)
    ));
    assert!(matches!(
        hints[3],
        Text::Args("settings.hint.key_reserved", _)
    ));
    assert!(matches!(
        hints[4],
        Text::Args("settings.hint.key_malformed", _)
    ));
    // Nothing in the file, nothing in the pane.
    assert!(user_key_rows(&UserKeyFacts::default(), "default").is_empty());
    // A binding to a plugin that is gone draws no row at all.
    let gone = UserKeyFacts {
        plugins: Vec::new(),
        catalog: None,
        resolved: None,
        keys: parse("[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:sha256\"\n"),
    };
    assert!(user_key_rows(&gone, "default").is_empty());
}

#[test]
fn code_and_shift_letters_cannot_shadow_typed_builtin_rows() {
    for (table, key, preset, action) in [
        ("shift", "G", "default", "cursorLast"),
        ("code", "J", "mac", "cursorDown"),
    ] {
        let keys = parse(&format!(
            "[[{table}]]\nkey = \"{key}\"\naction = \"{CHECKSUMS}\"\n"
        ));
        assert_eq!(
            keys.verdicts(preset, &enabled, &projection()),
            vec![Verdict::Taken(action)]
        );
    }
}

#[test]
fn text_and_code_spellings_of_one_chord_are_refused_in_either_order() {
    for (first, second) in [("text", "code"), ("code", "text")] {
        let keys = parse(&format!("[[{first}]]\nkey = \"q\"\naction = \"copy\"\n[[{second}]]\nkey = \"q\"\naction = \"paste\"\n"));
        assert_eq!(
            keys.verdicts("default", &none, &projection()),
            vec![Verdict::Live, Verdict::Repeated]
        );
        let input = Input {
            key: "q",
            text: "q",
            modifiers: 0,
        };
        assert_eq!(
            find(&keys, "default", input, &none).map(|b| b.action.as_str()),
            Some("copy")
        );
    }
}

#[test]
fn missing_plugin_actions_are_reported_only_after_the_catalog_arrives() {
    use crate::ui_logic_settings_pages::pages::{user_key_refusals, UserKeyFacts};
    let mut facts = UserKeyFacts {
        keys: parse("[[ctrlshift]]\nkey = \"K\"\naction = \"example.checksums:typo\"\n"),
        ..Default::default()
    };
    assert!(user_key_refusals(&facts, "default").is_empty());
    facts.catalog = Some(vec![CHECKSUMS.into()]);
    assert_eq!(user_key_refusals(&facts, "default").len(), 1);
    facts.keys.bindings[0].action = CHECKSUMS.into();
    assert!(
        user_key_refusals(&facts, "default").is_empty(),
        "disabled declarations stay inactive"
    );
    facts.catalog = Some(Vec::new());
    assert_eq!(
        user_key_refusals(&facts, "default").len(),
        1,
        "removed declarations no longer resolve"
    );
}

#[test]
fn malformed_and_duplicate_fields_cannot_install_a_partial_binding() {
    for extra in ["action = \"paste\"", "broken", "key = 42", "[[not-closed"] {
        let keys = parse(&format!("[[ctrlshift]]\nkey = \"K\"\naction = \"copy\"\n{extra}\n[[ctrlshift]]\nkey = \"Y\"\naction = \"settings\"\n"));
        assert!(!keys.problems.is_empty());
        assert_eq!(keys.bindings.len(), 1, "{extra}");
        assert_eq!(keys.bindings[0].key, "Y");
    }
}

#[test]
fn function_keys_require_the_names_events_send() {
    assert!(parse("[[code]]\nkey = \"F024\"\naction = \"copy\"\n")
        .bindings
        .is_empty());
    let keys = parse("[[code]]\nkey = \"F24\"\naction = \"copy\"\n");
    assert!(keys.bindings[0].matches(Input {
        key: "F24",
        text: "",
        modifiers: 0
    }));
}

#[test]
fn shifted_digits_and_their_text_symbols_overlap_in_both_orders() {
    for (first, second) in [("shift", "text"), ("text", "shift")] {
        let first_key = if first == "shift" { "1" } else { "!" };
        let second_key = if second == "shift" { "1" } else { "!" };
        let keys = parse(&format!("[[{first}]]\nkey = \"{first_key}\"\naction = \"copy\"\n[[{second}]]\nkey = \"{second_key}\"\naction = \"paste\"\n"));
        assert_eq!(
            keys.verdicts("default", &none, &projection()),
            vec![Verdict::Live, Verdict::Repeated]
        );
        let input = Input {
            key: "1",
            text: "!",
            modifiers: crate::keys::SHIFT,
        };
        assert_eq!(
            lookup("default", Context::Listing, Frontend::Gui, input),
            Match::Unbound
        );
        assert_eq!(
            find(&keys, "default", input, &none).map(|b| b.action.as_str()),
            Some("copy")
        );
    }
}
