//! The mn2 rows: agent and Actions eligibility, plugin fail-closed, dispatch verbs.
use super::agent;
use super::plugins::{self, Contribution, Menu};
use super::rows::{self, FIRE};
use super::wire::{Action, Listing};

fn action(id: &str, name: &str, script: bool) -> Action {
    action_mode(id, name, script, "background")
}

fn action_mode(id: &str, name: &str, script: bool, mode: &str) -> Action {
    Action {
        id: id.into(),
        name: name.into(),
        mode: mode.into(),
        selection: "many".into(),
        script,
        mime: vec![if script { "*/*" } else { "image/*" }.into()],
    }
}

fn listing() -> Listing {
    Listing {
        id: 4,
        actions: vec![
            action("action:resize.toml", "Resize to 1600 px", false),
            action("script:ocr.sh", "ocr", true),
        ],
        errors: Vec::new(),
        error: String::new(),
    }
}

fn shown() -> Vec<String> {
    Vec::new()
}

#[test]
fn mn2_the_agent_row_joins_the_background_menu_with_its_mark() {
    let row = rows::agent_row(&shown()).unwrap();
    assert_eq!(row.id.as_ref(), agent::ID);
    assert_eq!(row.action.as_ref(), agent::ID);
    assert_eq!(row.glyph, Some(agent::GLYPH));
}

#[test]
fn mn2_the_agent_switch_takes_its_row_away() {
    let hidden = vec![agent::ID.to_string()];
    assert!(rows::agent_row(&hidden).is_none());
}

#[test]
fn mn2_the_agent_runs_omarchy_agent_in_the_folder_with_no_shell() {
    assert_eq!(agent::argv(), ["omarchy", "agent", "--pick"]);
    assert_eq!(
        agent::spawn("/fx/v/work"),
        (
            "/fx/v/work",
            vec![
                "omarchy".to_string(),
                "agent".to_string(),
                "--pick".to_string()
            ]
        )
    );
    assert_eq!(agent::KEY, "A");
}

#[test]
fn mn2_the_agent_row_ships_off_with_the_shipped_hidden_set() {
    assert!(
        crate::ui_logic_context_menus::inventory::SHIPPED_HIDDEN.contains(&agent::ID),
        "Open in Agent ships off (CHANGES 655)"
    );
    let shipped: Vec<String> = crate::ui_logic_context_menus::inventory::SHIPPED_HIDDEN
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    assert!(rows::agent_row(&shipped).is_none());
}

#[test]
fn mn2_actions_lists_matches_before_tags_with_the_lightning_mark() {
    let row = rows::actions_row(&shown(), &listing()).unwrap();
    assert_eq!(row.id.as_ref(), rows::ID);
    assert_eq!(row.action.as_ref(), FIRE);
    assert_eq!(row.glyph, Some(rows::GLYPH));
    let leaves = row.submenu.unwrap();
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[0].action.as_ref(), FIRE);
    assert_eq!(leaves[0].id.as_ref(), "action:resize.toml");
    assert_eq!(
        leaves[0].glyph,
        Some("image"),
        "a manifest action over image globs carries the image mark"
    );
    assert_eq!(
        leaves[1].glyph,
        Some(rows::FLYOUT_GLYPH),
        "a script keeps the flyout terminal mark"
    );
}

#[test]
fn mn2_a_preview_leaf_is_routed_to_preview_and_a_run_leaf_to_run() {
    // The menu fires every flyout leaf under the parent's verb, so the leaf's mode, read off the listing the row came from, is the only thing that tells the host preview from run.
    let preview = Listing {
        actions: vec![action_mode("action:inspect.toml", "Inspect", false, "preview")],
        ..listing()
    };
    let row = rows::actions_row(&shown(), &preview).unwrap();
    assert_eq!(row.submenu.unwrap().len(), 1, "the preview leaf is still drawn");
    assert_eq!(rows::leaf_previews(&preview, "action:inspect.toml"), Some(true));
    assert_eq!(rows::leaf_previews(&listing(), "action:resize.toml"), Some(false));
    assert_eq!(rows::leaf_previews(&listing(), "script:ocr.sh"), Some(false));
    assert_eq!(
        rows::leaf_previews(&listing(), "action:gone.toml"),
        None,
        "a leaf the listing no longer names routes nowhere"
    );
}

#[test]
fn mn2_an_empty_listing_offers_no_row_at_all() {
    let empty = Listing {
        actions: Vec::new(),
        ..listing()
    };
    assert!(rows::actions_row(&shown(), &empty).is_none());
}

#[test]
fn mn2_the_actions_row_ships_on_while_run_script_ships_off() {
    // HIG CHANGES keeps Actions (file menu) on and Run Script off, as src/uischema.rs writes.
    let hidden = crate::ui_logic_context_menus::inventory::SHIPPED_HIDDEN;
    assert!(!hidden.contains(&rows::ID), "Actions ships on (HIG CHANGES)");
    assert!(hidden.contains(&"runScript"), "Run Script ships off");
    let shipped: Vec<String> = hidden.iter().map(|id| (*id).to_owned()).collect();
    assert!(rows::actions_row(&shipped, &listing()).is_some());
}

#[test]
fn mn2_the_actions_switch_takes_it_away_as_well() {
    let hidden = vec![rows::ID.to_string()];
    assert!(rows::actions_row(&hidden, &listing()).is_none());
}

#[test]
fn mn2_an_unknown_kind_leaves_manifest_leaves_on_the_flyout_mark() {
    // The repo's native fixture lists a "*/*" runner beside two image actions; only the image ones wear the image mark.
    let mut leaves = Vec::new();
    for globs in [vec!["*/*"], vec!["video/*"], vec!["image/*", "video/*"], vec!["image/png", "image/jpeg"], vec![]] {
        let mut wide = action("action:wide.toml", "Wide", false);
        wide.mime = globs.iter().map(|glob| glob.to_string()).collect();
        let listed = Listing { actions: vec![wide], ..listing() };
        leaves.push(rows::actions_row(&shown(), &listed).unwrap().submenu.unwrap()[0].glyph);
    }
    let flyout = Some(rows::FLYOUT_GLYPH);
    assert_eq!(leaves, [flyout, flyout, flyout, Some("image"), flyout]);
}

fn checksums() -> Contribution {
    Contribution {
        plugin: "mtorvik.checksums".into(),
        action: "verify".into(),
        label: "Verify Checksum…".into(),
        glyph: "hash".into(),
        menus: vec![Menu::File],
        when: plugins::When {
            mime: Vec::new(),
            min_items: 1,
            max_items: None,
            folders: true,
        },
    }
}

fn optimize() -> Contribution {
    Contribution {
        plugin: "gm.optimize".into(),
        action: "optimize".into(),
        label: "Optimize Images".into(),
        glyph: "image".into(),
        menus: vec![Menu::File],
        when: plugins::When {
            mime: vec!["image/*".into()],
            min_items: 1,
            max_items: Some(500),
            folders: false,
        },
    }
}

fn item(mime: &str, folder: bool) -> plugins::Item {
    plugins::Item {
        mime: mime.into(),
        folder,
    }
}

#[test]
fn mn2_a_plugin_row_shows_only_on_the_kinds_it_declares() {
    let offered = [optimize()];
    let images = [item("image/png", false), item("image/jpeg", false)];
    assert_eq!(plugins::eligible(&offered, false, &images).len(), 1);
    let notes = [item("text/plain", false)];
    assert_eq!(
        plugins::eligible(&offered, false, &notes).len(),
        0,
        "an image/png-only row is not eligible on notes.txt"
    );
    let folder = [item("inode/directory", true)];
    assert_eq!(
        plugins::eligible(&offered, false, &folder).len(),
        0,
        "folders:false keeps the row off a folder selection"
    );
    let many: Vec<plugins::Item> = (0..501).map(|_| item("image/png", false)).collect();
    assert_eq!(
        plugins::eligible(&offered, false, &many).len(),
        0,
        "maxItems caps the row"
    );
    assert_eq!(plugins::eligible(&offered, false, &[]).len(), 0);
}

#[test]
fn mn2_mime_globs_match_kinds_the_way_manifests_do() {
    assert!(plugins::mime_match("image/*", "image/png"));
    assert!(plugins::mime_match("*/*", "text/plain"));
    assert!(!plugins::mime_match("image/*", "text/plain"));
    assert!(plugins::mime_match("text/??", "text/cs"));
}

#[test]
fn mn2_the_handed_marks_carry_the_board_cuts() {
    assert_eq!(rows::mark_path("hash"), Some("M4 9h16 M4 15h16 M10 3 8 21 M16 3l-2 18"));
    assert_eq!(rows::mark_path("rocket"), None);
    for drawn in ["zap", "bot"] {
        assert_eq!(rows::mark_path(drawn), None, "{drawn} has one source, mn1's cut table");
        assert_eq!(rows::static_glyph(drawn), Some(drawn));
    }
}

#[test]
fn mn2_a_plugin_row_is_drawn_like_a_builtin_with_its_own_mark() {
    let rows = rows::plugin_rows(&[checksums()]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action.as_ref(), "mtorvik.checksums:verify");
    assert_eq!(rows[0].id.as_ref(), "mtorvik.checksums:verify");
    assert_eq!(rows[0].glyph, Some("hash"));
}

#[test]
fn mn2_two_plugin_rows_fire_under_their_own_plugin_and_action() {
    // A top-level row fires its own action with no leaf, so the action must name the plugin.
    let mut other = checksums();
    other.plugin = "gm.optimize".into();
    let drawn = rows::plugin_rows(&[checksums(), other]);
    assert_eq!(drawn[0].action.as_ref(), "mtorvik.checksums:verify");
    assert_eq!(drawn[1].action.as_ref(), "gm.optimize:verify");
}

#[test]
fn mn2_a_plugin_glyph_mn1_knows_is_drawn() {
    assert_eq!(rows::static_glyph("image"), Some("image"));
    assert_eq!(rows::static_glyph("hash"), Some("hash"));
    assert_eq!(rows::static_glyph("rocket"), None);
}

#[test]
fn mn2_a_guide_example_row_draws_with_the_mark_mn1_knows() {
    let drawn = rows::plugin_rows(&[optimize()]);
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].glyph, Some("image"));
}

#[test]
fn mn2_a_disabled_or_failed_plugin_draws_nothing() {
    // The join only passes `contributing()` plugins here; nothing else reaches the rows.
    assert!(rows::plugin_rows(&[]).is_empty());
    let offered = [checksums()];
    let one = [item("text/plain", false)];
    let background: Vec<_> = plugins::eligible(&offered, true, &one);
    assert!(
        background.is_empty(),
        "a file row never lands in the background menu"
    );
    let none: Vec<_> = plugins::eligible(&offered, false, &[]);
    assert!(
        none.is_empty(),
        "a one-item row never shows with nothing selected"
    );
    let file: Vec<_> = plugins::eligible(&offered, false, &one);
    assert_eq!(file.len(), 1);
}

#[test]
fn mn2_hidden_plugin_rows_stay_hidden_by_the_menus_switch() {
    // Provider rows keep their v0.3.12 place and their own switch; a plugin row shows only while its plugin is enabled and the Menus switch keeps it.
    let row = &rows::plugin_rows(&[checksums()])[0];
    assert!(crate::ui_logic_context_menus::rows::apply_hidden(
        std::slice::from_ref(row),
        &["mtorvik.checksums:verify".to_string()]
    )
    .is_empty());
}
