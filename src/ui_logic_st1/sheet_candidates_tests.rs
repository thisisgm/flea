use super::*;
use crate::keys::presentation::{sheet, SheetRow};
use crate::keys::Frontend;
use crate::ui_logic_st1::test_support::{english, say};

fn sheet_actions() -> Vec<Candidate> {
    action_candidates(&sheet("default", Frontend::Gui, &english()))
}

fn hint(action: &str) -> String {
    match action {
        "deletePermanently" => "shift-delete".into(),
        "trash" => "dd".into(),
        _ => String::new(),
    }
}

fn entry(label: &str, action: &str) -> MenuEntry {
    MenuEntry { label: Text::data(label), action: action.into(), ..Default::default() }
}

fn leaf(id: &str, label: &str) -> MenuLeaf {
    MenuLeaf { id: id.into(), label: Text::data(label), ..Default::default() }
}

fn fixture_entries() -> Vec<MenuEntry> {
    let mut compress = entry("Compress", "compress");
    compress.submenu = vec![
        leaf(".zip", "Compress to .zip"),
        MenuLeaf { separator: true, ..Default::default() },
        MenuLeaf { disabled: true, ..leaf(".7z", "Compress to .7z") },
    ];
    let mut delete = entry("Delete permanently", "deletePermanently");
    delete.danger = true;
    let mut paste = entry("Paste", "paste");
    paste.disabled = true;
    vec![entry("Move to Trash", "trash"), delete, entry("Permissions", "permissions"), paste, compress]
}

fn rail() -> Vec<RailEntry> {
    let rail = |label: &str, group: &str, kind: &str, identity: &str| RailEntry {
        label: label.into(),
        group: group.into(),
        kind: kind.into(),
        identity: identity.into(),
    };
    vec![
        rail("flea", "favourite", "favourite", "favourite:/home/gm/flea"),
        rail("Trash", "trash", "trash", "trash:///"),
        rail("NAS", "network", "share", "smb://host/data"),
        rail("Recent", "recent", "recent", "flea:recent"),
    ]
}

fn find<'a>(rows: &'a [Candidate], label: &str) -> &'a Candidate {
    rows.iter().find(|row| say(&row.label) == label).expect("row present")
}

#[test]
fn menu_candidates_list_every_non_separator_entry_and_leaf() {
    let menus = menu_candidates(&fixture_entries(), hint);
    assert_eq!(menus.len(), 7);
    assert_eq!(find(&menus, "Delete permanently").keys, "shift-delete");
    assert_eq!(find(&menus, "Permissions").keys, "");
    assert_eq!(find(&menus, "Compress to .zip").where_, Text::data("Compress"));
    assert_eq!(find(&menus, "Compress to .zip").keys, "");
    assert_eq!(menus.iter().filter(|row| say(&row.where_) == "Compress").count(), 2);
    assert_eq!(find(&menus, "Compress to .zip").menu_action, "compress:.zip");
    assert!(menus.iter().all(|row| !say(&row.label).is_empty()));
}

#[test]
fn menu_candidates_flag_disabled_rows_and_leaves() {
    let menus = menu_candidates(&fixture_entries(), hint);
    assert!(find(&menus, "Paste").disabled);
    assert!(find(&menus, "Compress to .7z").disabled);
    assert!(find(&menus, "Delete permanently").danger);
}

#[test]
fn flyout_without_leaves_keeps_its_own_row() {
    let mut compress = entry("Compress", "compress");
    compress.submenu = Vec::new();
    let rows = menu_candidates(&[compress], |_| String::new());
    let found = rank(&rows, "comp", &english());
    assert_eq!(found.len(), 1);
    assert_eq!(say(&found[0].label), "Compress");
}

#[test]
fn place_candidates_name_each_group_and_skip_recent() {
    let places = place_candidates(&rail());
    assert_eq!(say(&find(&places, "Open flea").where_), "Favorites");
    assert_eq!(say(&find(&places, "Open Trash").where_), "Places");
    assert_eq!(say(&find(&places, "Open NAS").where_), "Network");
    assert_eq!(places.len(), 3);
    assert!(places.iter().all(|row| row.name != "Recent"));
    assert_eq!(say(&find(&places, "Open Trash").label), "Open Trash");
}

#[test]
fn place_candidates_name_devices() {
    let device = RailEntry { label: "USB".into(), group: "device".into(), kind: "device".into(), identity: "device:usb".into() };
    let places = place_candidates(&[device]);
    assert_eq!(say(&places[0].where_), "Devices");
}

#[test]
fn recent_candidates_abbreviate_home_and_keep_a_root_whole() {
    let recents = recent_candidates(&["/home/gm/Documents/claude/mix.flac".into(), "/etc/hosts".into()], "/home/gm");
    assert_eq!(say(&recents[0].label), "Open mix.flac");
    assert_eq!(say(&recents[0].where_), "~/Documents/claude");
    assert_eq!(say(&recents[1].where_), "/etc");
}

#[test]
fn dispatch_fixture_rows_run_as_their_surface_would() {
    let menus = menu_candidates(&fixture_entries(), hint);
    assert_eq!(dispatch(find(&menus, "Move to Trash")), Decision::Menu { menu_action: "trash".into() });
    assert_eq!(
        dispatch(find(&menus, "Delete permanently")),
        Decision::Confirm { menu_action: "deletePermanently".into() }
    );
    assert_eq!(dispatch(find(&menus, "Paste")), Decision::Disabled);
    let places = place_candidates(&rail());
    assert_eq!(dispatch(find(&places, "Open Trash")), Decision::Place { rail_index: Some(1), identity: "trash:///".into() });
    let recents = recent_candidates(&["/home/gm/Documents/claude/mix.flac".into()], "/home/gm");
    assert_eq!(dispatch(&recents[0]), Decision::Recent { path: "/home/gm/Documents/claude/mix.flac".into() });
}

#[test]
fn action_key_dispatches_as_its_key() {
    let rows = action_candidates(&[SheetRow {
        action: "trash",
        label: "trash".into(),
        keys: "dd".into(),
        context: "listing",
        group: "",
    }]);
    assert_eq!(dispatch(&rows[0]), Decision::Action { action: "trash".into() });
}

#[test]
fn flyout_leaves_survive_the_dedupe_as_distinct_runs() {
    let mut compress = entry("Compress", "compress");
    compress.submenu = vec![leaf("zip", ".zip"), leaf("tar", ".tar")];
    let mut open_with = entry("Open with", "openWith");
    open_with.submenu = vec![leaf("a.desktop", "Alpha"), leaf("b.desktop", "Beta")];
    let rows = unique(&action_candidates(&sheet("default", Frontend::Gui, &english())).into_iter()
        .chain(menu_candidates(&[compress, open_with], no_key))
        .collect::<Vec<_>>());
    let runs: Vec<String> = rows
        .iter()
        .filter(|row| say(&row.where_) == "Compress" || say(&row.where_) == "Open with")
        .map(|row| row.menu_action.clone())
        .collect();
    assert_eq!(runs.join("|"), "compress:zip|compress:tar|openWith:a.desktop|openWith:b.desktop");
}

fn no_key(_: &str) -> String {
    String::new()
}

#[test]
fn sheet_rows_name_no_place_for_listing_keys_and_no_raw_context_id() {
    let rows = sheet_actions();
    let rename = rows.iter().find(|row| row.action == "rename").expect("rename row");
    assert_eq!(say(&rename.where_), "");
    assert!(rows.iter().all(|row| say(&row.where_) != "media" && say(&row.where_) != "all"));
}

// tests/sheet-query.qml checkCompress: a leaf with its own sheet wording is its own row and names no parent.
#[test]
fn a_leaf_with_sheet_wording_names_no_parent() {
    let mut compress = entry("Compress", "compress");
    compress.submenu = vec![MenuLeaf { sheet_label: Text::data("Compress to .zip"), ..leaf(".zip", ".zip") }];
    let menus = menu_candidates(&[compress], hint);
    let row = find(&menus, "Compress to .zip");
    assert_eq!(row.where_, Text::Empty);
    assert_eq!(row.menu_action, "compress:.zip");
    assert_eq!(menus.iter().filter(|row| say(&row.label).contains("Compress to .")).count(), 1);
}
