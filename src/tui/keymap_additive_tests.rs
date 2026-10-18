use super::*;

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
