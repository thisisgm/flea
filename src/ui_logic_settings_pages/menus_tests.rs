// tests/js/settingsmenus.js and the Menus parts of tests/js/settings.js.
use super::super::rows::{first_row, focusable, step_row, Kind, Master, Role};
use super::super::test_support::say;
use super::*;

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

fn find<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|row| row.id == id).expect(id)
}

#[test]
fn every_switch_carries_its_own_wording_and_mark() {
    for group in GROUPS {
        for id in group.ids.iter().chain(group.features).chain(LOCKED) {
            assert!(label(id).key().is_some(), "{id} has no wording");
            assert!(!glyph(id).is_empty() || !mark(id).is_empty(), "{id} has no mark");
        }
    }
    assert_eq!(say(&label("nothing")), "nothing");
}

#[test]
fn master_counts_enabled_ids_and_reads_all_some_none() {
    assert_eq!(enabled(&[], BASIC), 6);
    assert_eq!(master_state(&[], BASIC), Master::All);
    assert_eq!(enabled(&ids(&["paste"]), BASIC), 5);
    assert_eq!(master_state(&ids(&["paste"]), BASIC), Master::Some);
    assert_eq!(enabled(&ids(BASIC), BASIC), 0);
    assert_eq!(master_state(&ids(BASIC), BASIC), Master::None);
    assert_eq!(enabled(&ids(&["showOriginal", "compress"]), BASIC), 6);
}

#[test]
fn master_toggles_its_group_and_keeps_unrelated_ids() {
    let mut off = toggle_master(&[], BASIC);
    off.sort();
    let mut all = ids(BASIC);
    all.sort();
    assert_eq!(off, all);
    assert!(toggle_master(&ids(&["paste"]), BASIC).is_empty());
    assert!(toggle_master(&ids(BASIC), BASIC).is_empty());
    assert_eq!(toggle_master(&ids(&["paste", "showOriginal"]), BASIC), ids(&["showOriginal"]));
    assert!(toggle_master(&ids(&["showOriginal"]), BASIC).contains(&"showOriginal".to_owned()));
}

#[test]
fn toggle_id_adds_or_removes_only_that_id() {
    assert_eq!(toggle_id(&[], "paste"), ids(&["paste"]));
    assert_eq!(toggle_id(&ids(&["paste", "showOriginal"]), "paste"), ids(&["showOriginal"]));
    let cut = toggle_id(&[], "cut");
    assert_eq!((master_state(&cut, BASIC), enabled(&cut, BASIC)), (Master::Some, 5));
    assert_eq!(master_state(&toggle_master(&toggle_master(&[], BASIC), BASIC), BASIC), Master::All);
}

#[test]
fn every_heading_reports_its_own_group() {
    let menus = rows(&ids(&["paste"]), false);
    assert_eq!(menus[0].state, Master::Some);
    let rows = rows(&ids(&["moveto", "copyto", "properties", "permissions"]), false);
    let groups: Vec<_> = rows.iter().filter(|row| row.kind == Kind::Group).collect();
    let drawn: Vec<String> = groups
        .iter()
        .map(|row| {
            let state = match row.state { Master::All => "all", Master::Some => "some", Master::None => "none", Master::Absent => "" };
            if row.master { format!("{}|{}|{}", say(&row.label), say(&row.value), state) } else { format!("{}|no master", say(&row.label)) }
        })
        .collect();
    assert_eq!(drawn.join(", "), "Basic File Actions|6 of 6|all, Destructive|no master, Open and Inspect|10 of 14|some, \
        Extras|18 of 18|all, Shortcuts|no master, Always Shown|no master");
    let inspect = groups[2];
    assert_eq!((focusable(inspect), focusable(groups[1])), (true, false));
    let mut kept = toggle_master(&ids(&["moveto", "copyto", "properties", "permissions", "paste"]), &inspect.ids);
    kept.sort();
    assert_eq!(kept, ids(&["paste"]));
}

#[test]
fn menus_rows_lead_with_the_master_and_draw_counts() {
    let menus = rows(&ids(&["paste"]), false);
    assert_eq!([menus[0].kind, menus[1].kind, menus[2].kind], [Kind::Group, Kind::Check, Kind::Check]);
    assert_eq!(say(&menus[0].value), "5 of 6");
    assert_eq!(say(&rows(&ids(&["cut", "copy", "paste", "duplicate", "rename"]), false)[0].value), "1 of 6");
    let urgent: Vec<String> = menus.iter().filter(|r| r.role == Role::Error).map(|r| format!("{}|{}|{}", r.id, say(&r.value), r.on)).collect();
    assert_eq!(urgent, ["delete|off by default|true"]);
    assert!(!find(&menus, "paste").on);
    assert!(find(&menus, "copy").on);
    let checks: Vec<&str> = menus.iter().filter(|r| r.kind == Kind::Check).map(|r| r.id.as_str()).collect();
    assert_eq!(checks.join(","), "cut,copy,paste,duplicate,rename,trash,delete,openwith,print,run,openAsText,openTerminal,openAgent,\
moveto,copyto,properties,permissions,makeExecutable,copyAs,showOriginal,tags,shelf,compress,extract,convert,flashDrive,taildrop,\
localsend,rclone,dropbox,sharelink,actions,placeMenu,updateFlea,toggleHidden,toggleLangMarks,extThumbs,pasteAs,invertSelection,keyHints");
}

#[test]
fn hints_switch_defaults_off_and_reads_its_value() {
    let menus = rows(&[], false);
    let hints = find(&menus, "keyHints");
    assert_eq!(format!("{}|{}", say(&hints.label), hints.on), "Show keyboard hints|false");
    assert!(find(&rows(&[], true), "keyHints").on);
    let terminal = find(&menus, "openTerminal");
    assert_eq!(format!("{}|{}", say(&terminal.label), terminal.glyph), "Open in Terminal|terminal");
    let locks: Vec<&str> = menus.iter().filter(|r| r.kind == Kind::Lock).map(|r| r.id.as_str()).collect();
    assert_eq!(locks, ["open"]);
    assert_eq!(find(&menus, "toggleHidden").kind, Kind::Check, "Show Hidden Files leaves the file menu by its switch");
    assert_eq!(find(&menus, "open").kind, Kind::Lock);
    assert_eq!(say(&menus[menus.len() - 3].label), "Every key stays bound either way.");
}

#[test]
fn menus_cursor_steps_over_read_only_rows() {
    let menus = rows(&[], false);
    assert!(!focusable(&menus[7]));
    assert_eq!(first_row(&menus), 0);
    assert!(focusable(&menus[2]));
    assert!(!focusable(menus.last().unwrap()));
    let end = menus.len() - 3;
    assert_eq!(step_row(&menus, end, 1), end);
    assert_eq!(step_row(&menus, 0, -1), 0);
    assert!(focusable(&menus[step_row(&menus, 6, 1)]));
}

#[test]
fn switch_inventory_counts_and_destructive_row() {
    let menus = rows(&[], true);
    assert_eq!(menus.iter().filter(|r| r.kind == Kind::Check && r.id != "keyHints").count(), 39);
    assert_eq!(say(&find(&menus, "tags").label), "Tags");
    assert_eq!(find(&menus, "delete").role, Role::Error);
    assert_eq!(say(&find(&menus, "delete").value), "off by default");
    assert_eq!(say(&find(&menus, "delete").label), "Delete Permanently\u{2026}");
}

#[test]
fn the_shipped_hidden_list_is_the_schema_default() {
    let schema = include_str!("../uischema.rs");
    let from = schema.find(r#""menu": { "hidden": ["#).expect("DEFAULTS names menu.hidden");
    let list = &schema[from..from + schema[from..].find(']').expect("the list ends")];
    let ids: Vec<&str> = list.split('"').skip(5).step_by(2).collect();
    assert_eq!(ids, SHIPPED_HIDDEN);
}

// EY-08: a fresh ui.json leaves Open and Inspect at 7 of 14, Properties and Permissions on, Open in Agent off.
#[test]
fn a_fresh_install_counts_open_and_inspect_seven_of_fourteen() {
    let data = crate::uistate::from_file("{}");
    let hidden = crate::ui_logic_settings_pages::pages::hidden(&data);
    let group = crate::ui_logic_settings_pages::menus::GROUPS
        .iter()
        .find(|g| g.id == "openInspect")
        .unwrap();
    let on: Vec<&str> = group
        .ids
        .iter()
        .copied()
        .filter(|id| !hidden.iter().any(|h| h == id))
        .collect();
    assert_eq!(
        on,
        [
            "openwith",
            "print",
            "properties",
            "permissions",
            "makeExecutable",
            "showOriginal",
            "tags"
        ],
        "hidden {hidden:?}"
    );
}

// EY-08: the fresh install's hidden set ticks Open With, Print..., Properties and Permissions... and leaves Copy To... off.
#[test]
fn fixlogic_ey08_fresh_install_open_and_inspect_counts_seven_of_fourteen_with_copy_to_off() {
    let rows = rows(&super::super::pages::hidden(&crate::uischema::defaults()), false);
    let head = rows.iter().find(|row| row.master && row.id == "openInspect").expect("openInspect");
    assert_eq!(say(&head.value), "7 of 14");
    assert_eq!(head.state, Master::Some);
    for (id, on) in [("openwith", true), ("print", true), ("properties", true), ("permissions", true), ("copyto", false), ("openAgent", false)] {
        assert_eq!(find(&rows, id).on, on, "{id}");
    }
}
