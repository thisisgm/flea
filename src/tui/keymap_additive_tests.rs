use super::*;

#[test]
fn relative_link_accepts_legacy_and_csiu_shifted_underscore() {
    let map = Map::load();
    for encoded in [b"_".as_slice(), b"\x1b[95;2u", b"\x1b[45:95;2u"] {
        let mut decoder = crate::tui::input::Decoder::default();
        let keys = decoder.feed(encoded, false);
        assert_eq!(keys.len(), 1);
        assert_eq!(map.in_context(&keys[0], "yazi", "listing"), "pasteLink");
    }
}

#[test]
fn review_t2_keymap_lint_preserves_bound_actions_in_every_preset() {
    let map = Map::load();
    let shipped = Map {
        blocks: map.blocks.iter().filter(|(kind, _)| kind != "tui").cloned().collect(),
    };
    let mut changes = Vec::new();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        let mut checked = 0;
        for (kind, block) in &shipped.blocks {
            let Some((_, key)) = block_key(kind, block) else { continue; };
            for context in ["listing", "rail", "menu", "panel", "preview", "pdf", "media", "editor"] {
                // The new opt-in preset deliberately replaces listing keys, but inherits overlay controls.
                if preset == "yazi" && context == "listing" { continue; }
                let before = shipped.in_context(&key, preset, context);
                if before.is_empty() { continue; }
                let after = map.in_context(&key, preset, context);
                checked += 1;
                if before != after {
                    changes.push(format!("{preset} {context} {key:?}: {before} -> {after}"));
                }
            }
        }
        assert!(checked > 0);
        println!("additive keymap lint {preset}: {checked} bound resolutions checked");
    }
    assert!(changes.is_empty(), "{}", changes.join("\n"));
}

#[test]
fn every_tui_preset_has_unique_chords_with_reachable_prefixes_and_editor_commands() {
    let map = Map::load();
    let mut seen = std::collections::BTreeSet::new();
    for (kind, row) in &map.blocks {
        if kind != "tui" { continue; }
        for context in get(row, "context").split(',') {
            let signature = (get(row,"name"), context, get(row,"prefix"), get(row,"mods"), get(row,"key"));
            assert!(seen.insert(signature), "duplicate TUI chord: {signature:?}");
            if !get(row,"prefix").is_empty() {
                assert!(map.chord(&format!("prefix:{}",get(row,"prefix")),get(row,"name"),context).is_some(), "unreachable TUI prefix: {signature:?}");
            }
        }
    }
    for preset in ["default","vim","mac","windows","yazi"] {
        for (name,mods,action) in [("B","alt","wordBackward"),("F","alt","wordForward"),("Left","ctrl","wordBackward"),("Right","ctrl","wordForward"),
            ("U","ctrl","killBol"),("K","ctrl","killEol"),("W","ctrl","killBackward"),("D","alt","killForward"),("Backspace","ctrl","killBackward"),("Delete","ctrl","killForward")] {
            assert_eq!(map.in_context(&Key::named(name,mods),preset,"editor"),action,"{preset}: {mods}-{name}");
        }
    }
}
