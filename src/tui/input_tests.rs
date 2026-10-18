use super::*;

#[test]
fn adv_d_r1_keypad_keys_keep_legacy_actions_and_filter_text() {
    let map = crate::tui::keymap::Map::load();
    let mut editor = crate::tui::editor::Editor::new("filter", String::new(), "/".into());
    for (code, c) in (57399..=57408).zip('0'..='9').chain([
        (57409, '.'), (57410, '/'), (57411, '*'), (57412, '-'),
        (57413, '+'), (57415, '='), (57416, ','),
    ]) {
        for modifiers in [1, 129] {
            let encoded = format!("\x1b[{code};{modifiers}u");
            let keys = Decoder::default().feed(encoded.as_bytes(), false);
            assert_eq!(keys, [Key::character(c, "")], "keypad code {code}");
            assert_eq!(map.action(&keys[0], "flea"), map.action(&Key::character(c, ""), "flea"));
            if modifiers == 1 { editor.update(&keys[0]); }
        }
    }
    assert_eq!(editor.value, "0123456789./*-+=,");
    let keys = Decoder::default().feed(b"\x1b[57409;1;44u\x1b[57414u\x1b[57414;5u", false);
    assert_eq!(keys, [Key::character(',', ""), Key::named("Return", ""), Key::named("Return", "ctrl")]);
    editor.update(&keys[0]);
    assert_eq!(editor.value, "0123456789./*-+=,,");
    editor.update(&keys[1]);
    assert_eq!(editor.value, "0123456789./*-+=,,", "keypad Enter does not insert a private-use character");
    for preset in ["default", "flea", "mac", "windows", "vim"] {
        assert_eq!(map.action(&keys[1], preset), map.action(&Key::named("Return", ""), preset));
    }
}

#[test]
fn adv_d_r1_shifted_printables_match_legacy_keys_in_the_real_keymap() {
    let map = crate::tui::keymap::Map::load();
    for (base, shifted) in ('a'..='z').zip('A'..='Z').chain([
        ('/', '?'), ('1', '!'), ('2', '@'), ('3', '#'), ('4', '$'), ('5', '%'),
        ('6', '^'), ('7', '&'), ('8', '*'), ('9', '('), ('0', ')'), ('-', '_'),
        ('=', '+'), ('[', '{'), (']', '}'), (';', ':'), ('\'', '"'), (',', '<'), ('.', '>'), (' ', ' '),
    ]) {
        for variant in 0..3 {
            let encoded = match variant {
                0 => format!("\x1b[{}:{};2u", base as u32, shifted as u32),
                1 => format!("\x1b[{}:{};2;{}u", base as u32, shifted as u32, shifted as u32),
                _ => format!("\x1b[{};2;{}u", base as u32, shifted as u32),
            };
            let keys = Decoder::default().feed(encoded.as_bytes(), false);
            let legacy = Key::character(shifted, "");
            assert_eq!(keys.as_slice(), std::slice::from_ref(&legacy), "{encoded:?}");
            for preset in ["default", "flea", "mac", "windows", "vim"] {
                for context in ["listing", "editor", "pdf", "media"] {
                    assert_eq!(map.in_context(&keys[0].for_action(), preset, context), map.in_context(&legacy, preset, context), "{preset} {context} {shifted}");
                }
            }
        }
    }
    let keys = Decoder::default().feed(b"\x1b[122:90;6u\x1b[3;2~\x1b[1;2D", false);
    assert_eq!(keys, [Key::character('Z', "ctrlshift"), Key::named("Delete", "shift"), Key::named("Left", "shift")]);
    assert_eq!(map.action(&keys[0], "default"), "redo");
}
#[test]
fn adv_d_r2_shift_without_shifted_text_keeps_its_modifier() {
    let map = crate::tui::keymap::Map::load();
    for (encoded, printed) in [
        (b"\x1b[97;2u".as_slice(), 'a'),
        (b"\x1b[97;130u".as_slice(), 'a'),
        (b"\x1b[32;2u".as_slice(), ' '),
        (b"\x1b[32;130u".as_slice(), ' '),
    ] {
        let keys = Decoder::default().feed(encoded, false);
        let expected = Key::character(printed, "shift");
        assert_eq!(keys.as_slice(), std::slice::from_ref(&expected), "{encoded:?}");
        assert_eq!(map.action(&keys[0], "flea"), map.action(&expected, "flea"));
        assert_ne!(
            map.action(&keys[0], "flea"),
            map.action(&Key::character(printed, ""), "flea")
        );
        let mut editor = crate::tui::editor::Editor::new("filter", String::new(), "/".into());
        editor.update(&keys[0]);
        assert_eq!(editor.value, printed.to_string());
    }
}

#[test]
fn adv_d_r3_equal_base_shift_metadata_matches_true_legacy_input() {
    let map = crate::tui::keymap::Map::load();
    for printed in [' ', 'a'] {
        let code = printed as u32;
        let legacy = Key::character(printed, "");
        assert_eq!(Decoder::default().feed(printed.to_string().as_bytes(), false).as_slice(), std::slice::from_ref(&legacy));
        for encoded in [
            format!("\x1b[{code}:{code};2u"),
            format!("\x1b[{code};2;{code}u"),
            format!("\x1b[{code}:{code};2;{code}u"),
        ] {
            let keys = Decoder::default().feed(encoded.as_bytes(), false);
            assert_eq!(keys.as_slice(), std::slice::from_ref(&legacy), "{encoded:?}");
            for preset in ["default", "flea", "mac", "windows", "vim"] {
                for context in ["listing", "editor", "pdf", "media"] {
                    assert_eq!(map.in_context(&keys[0], preset, context), map.in_context(&legacy, preset, context));
                }
            }
        }
    }
}
    #[test]
    fn terminal_replies_never_become_file_actions() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b_Gi=42;error: ddd", false).is_empty());
        assert!(decoder.feed(&vec![b'd'; 4096], false).is_empty());
        assert!(decoder.feed(b"\x1b", false).is_empty());
        assert_eq!(decoder.feed(b"\\j", false), vec![Key::character('j', "")]);
        assert!(decoder
            .feed(b"\x1b]52;c;ddd\x07\x1bPddd\x1b\\", false)
            .is_empty());
    }

    #[test]
    fn split_sequences_and_utf8_are_retained() {
        let mut d = Decoder::default();
        assert!(d.feed(b"\x1b[1;", false).is_empty());
        assert_eq!(d.feed(b"5D", false), vec![Key::named("Left", "ctrl")]);
        assert!(d.feed(&[0xc3], false).is_empty());
        assert_eq!(d.feed(&[0xa9], false)[0].text, "é");
    }
    #[test]
    fn legacy_and_kitty_forms_coexist() {
        let mut d = Decoder::default();
        assert_eq!(
            d.feed(b" \x1b[32;5u\x1b[2;2~", false),
            vec![
                Key::character(' ', ""),
                Key::character(' ', "ctrl"),
                Key::named("Insert", "shift")
            ]
        );
        assert!(d.feed(b"\x1b", false).is_empty());
        assert_eq!(d.feed(b"", true), vec![Key::named("Escape", "")]);
    }
    #[test]
    fn mouse_modifiers_paste_and_release_keep_separate_meanings() {
        let mut decoder = Decoder::default();
        let mouse = decoder.feed(b"\x1b[<4;12;3M", false);
        assert_eq!(mouse[0].mods, "shift");
        assert_eq!(
            mouse[0].pointer,
            Some(Pointer {
                button: 0,
                x: 12,
                y: 3,
                released: false,
                motion: false
            })
        );
        assert!(decoder.feed(b"\x1b[200~ddq", false).is_empty());
        let pasted = decoder.feed(b"\x1b[201~", false);
        assert_eq!(pasted.len(), 1);
        assert_eq!(pasted[0].name, "Paste");
        assert_eq!(pasted[0].text, "ddq");
        assert!(decoder.feed(b"\x1b[113;1:3u", false).is_empty());
        assert_eq!(
            decoder.feed(b"\x1b[57365u\x1b[1;11D", false),
            vec![Key::named("F2", ""), Key::named("Left", "superalt")]
        );
        assert_eq!(decoder.feed(b"\x1b[1;7D", false)[0].mods, "unsupported");
    }
    #[test]
    fn native_menu_keys_keep_their_context_modifiers() {
        let mut decoder = Decoder::default();
        // Kitty with NumLock enabled sends CSI 1;129Q for physical F2.
        assert_eq!(decoder.feed(b"\x1b[1;129Q", false), vec![Key::named("F2", "")]);
        assert_eq!(
            decoder.feed(b"\x1b[21;2~\x1b[57373;2u\x1b[57363u", false),
            vec![
                Key::named("F10", "shift"),
                Key::named("F10", "shift"),
                Key::named("Menu", ""),
            ]
        );
    }

#[test]
fn physical_letters_preserve_cyrillic_text_in_editors_and_drive_actions() {
    let mut d = Decoder::default();
    let keys = d.feed("\x1b[1086:1054:106;1u\x1b[1075:1043:117;2u".as_bytes(), false);
    assert_eq!(keys[0].text, "о");
    assert_eq!(keys[0].name, "J");
    assert_eq!(keys[0].for_action().text, "j");
    assert_eq!(keys[1].text, "Г");
    assert_eq!(keys[1].for_action().text, "U");
    let mut editor = crate::tui::editor::Editor::new("filter", String::new(), std::path::PathBuf::from("/"));
    editor.update(&keys[0]);
    assert_eq!(editor.value, "о");
    let map = crate::tui::keymap::Map::load();
    assert_eq!(map.action(&keys[0].for_action(), "flea"), "cursorDown");
    let primary = d.feed("\x1b[1086:74;1u".as_bytes(), false);
    assert_eq!(primary[0].for_action().text, "j");
    let legacy = d.feed("о".as_bytes(), false);
    assert_eq!(legacy[0].for_action().text, "о");
}
#[test]
fn alternate_codes_shifted_text_and_associated_text_are_distinct() {
    let mut d = Decoder::default();
    let keys = d.feed(b"\x1b[103:71;2u\x1b[1086::106;1;1086u\x1b[27;5;13~", false);
    assert_eq!(keys[0], Key::character('G', ""));
    assert_eq!(keys[1].text, "о");
    assert_eq!(keys[1].for_action().text, "j");
    assert_eq!(keys[2], Key::named("Return", "ctrl"));
    assert!(d.feed(b"\x1b[1;1:3A\x1b[113;1:3u\x1b[113;0u\x1b[?0u\x1b[57344u", false).is_empty());
    assert_eq!(d.feed(b"\x1b[113;17u", false)[0].mods, "unsupported");
}
#[test]
fn malformed_utf8_and_abandoned_controls_do_not_hide_following_keys() {
    let mut d = Decoder::default();
    assert_eq!(d.feed(&[0xff, b'j'], false), [Key::character('j', "")]);
    assert!(d.feed(b"\x1b[1;", false).is_empty());
    assert!(d.feed(b"", true).is_empty());
    assert_eq!(d.feed(b"\x1b[Ak", false), [Key::named("Up", ""), Key::character('k', "")]);
    assert!(d.feed(b"\x1b_Gi=31;", false).is_empty());
    assert_eq!(d.feed(b"\x1b[A", false), [Key::named("Up", "")]);
    assert!(d.feed(b"\x1b", false).is_empty());
    assert!(d.feed(&[0xd0], false).is_empty());
    assert_eq!(d.feed(&[0xbe], false), [Key::character('о', "alt")]);
}

#[test]
fn associated_text_handles_ime_events_without_a_physical_key() {
    let mut d = Decoder::default();
    let keys = d.feed(b"\x1b[0;;229u\x1b[0;1;12363:12441u", false);
    assert_eq!(keys[0].text, "å");
    assert_eq!(keys[0].name, "Text");
    assert_eq!(keys[1].text, "か\u{3099}");
    let mut editor = crate::tui::editor::Editor::new("filter", String::new(), std::path::PathBuf::from("/"));
    for key in keys { editor.update(&key); }
    assert_eq!(editor.value, "åか\u{3099}");
    assert!(d.feed(b"\x1b[0;;0u\x1b[0;;27u", false).is_empty());
}


#[test]
fn review_oversized_fragmented_da1_never_becomes_typing() {
    let mut decoder = Decoder::default();
    let prefix = format!("\x1b[?62;4;{}", "0;".repeat(80));
    assert!(decoder.feed(prefix.as_bytes(), false).is_empty());
    let leaked = decoder.feed(b"0cj", false);
    println!("oversized fragmented DA1 tail decoded as {leaked:?}");
    assert_eq!(leaked, [Key::character('j', "")], "DA1 payload escaped into ordinary input");
}

#[test]
fn review_long_ime_commit_survives_fragmentation() {
    let text = "界".repeat(30);
    let encoded = format!("\x1b[0;1;{}u", vec!["30028"; 30].join(":"));
    let mut decoder = Decoder::default();
    assert!(decoder.feed(&encoded.as_bytes()[..140], false).is_empty());
    let keys = decoder.feed(&encoded.as_bytes()[140..], false);
    let actual: String = keys.iter().map(|key| key.text.as_str()).collect();
    println!("fragmented IME commit: expected {:?}, actual {:?}", text, actual);
    assert_eq!(actual, text);
}

#[test]
fn long_ime_commits_survive_every_split_and_bytewise_delivery() {
    let text = "界".repeat(30);
    let encoded = format!("j\x1b[0;1;{}uk", vec!["30028"; 30].join(":"));
    for split in 0..=encoded.len() {
        let mut decoder = Decoder::default();
        let mut keys = decoder.feed(&encoded.as_bytes()[..split], false);
        keys.extend(decoder.feed(&encoded.as_bytes()[split..], false));
        assert_eq!(keys.len(), 3, "split {split}");
        assert_eq!(keys[1].text, text, "split {split}");
        assert_eq!(keys[0], Key::character('j', ""));
        assert_eq!(keys[2], Key::character('k', ""));
        let mut editor = crate::tui::editor::Editor::new("filter", String::new(), std::path::PathBuf::from("/"));
        editor.update(&keys[1]);
        assert_eq!(editor.value, text, "split {split}");
    }
    let mut decoder = Decoder::default();
    let keys: Vec<_> = encoded.bytes().flat_map(|byte| decoder.feed(&[byte], false)).collect();
    assert_eq!(keys[1].text, text);
}

#[test]
fn oversized_da1_is_rejected_whole_at_every_split_and_after_idle() {
    use crate::tui::{capabilities::{Negotiation, Protocol}, tmux::Transport};
    let encoded = format!("\x1b[?62;4;{}0cj", "0;".repeat(80));
    for split in 0..=encoded.len() {
        let mut decoder = Decoder::new(Negotiation::begin(Transport::Direct, (31, 32), true, std::time::Instant::now()));
        let mut keys = decoder.feed(&encoded.as_bytes()[..split], false);
        keys.extend(decoder.feed(&encoded.as_bytes()[split..], false));
        assert_eq!(keys, [Key::character('j', "")], "split {split}");
        assert_eq!(decoder.negotiation.detected.graphics, Protocol::None, "split {split}");
    }
    let mut decoder = Decoder::default();
    for byte in encoded.bytes() {
        let keys = decoder.feed(&[byte], false);
        assert_eq!(keys, if byte == b'j' { vec![Key::character('j', "")] } else { Vec::new() });
    }
    let mut decoder = Decoder::default();
    assert!(decoder.feed(&encoded.as_bytes()[..140], false).is_empty());
    assert!(decoder.feed(b"", true).is_empty());
    assert_eq!(decoder.feed(&encoded.as_bytes()[140..], false), [Key::character('j', "")]);
}

#[test]
fn associated_text_has_its_own_budget_and_excess_is_not_typing() {
    let count = (CSI_TEXT_LIMIT + 1) / 3;
    for (count, accepted) in [(count, true), (count + 1, false), (count * 2, false)] {
        let encoded = format!("\x1b[0;1;{}uj", vec!["65"; count].join(":"));
        for chunk in [encoded.len(), 127, 4096] {
            let mut decoder = Decoder::default();
            let keys: Vec<_> = encoded.as_bytes().chunks(chunk).flat_map(|bytes| decoder.feed(bytes, false)).collect();
            if accepted {
                assert_eq!(keys.len(), 2);
                assert_eq!(keys[0].text, "A".repeat(count));
                assert_eq!(keys[1], Key::character('j', ""));
            } else { assert_eq!(keys, [Key::character('j', "")]); }
        }
    }
    let mut decoder = Decoder::default();
    assert_eq!(decoder.feed(b"\x1b[3;2~", false), [Key::named("Delete", "shift")]);
}
