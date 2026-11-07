// The Places parts of tests/js/settings.js.
use super::super::rows::{focusable, Kind};
use super::super::test_support::say;
use super::*;
use crate::jsondoc::parse;

fn places(text: &str, home: &str) -> Vec<Row> {
    let data = parse(text).expect("fixture parses");
    places_rows(&State { home, ..State::new(&data) })
}

fn find<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("no row {id}"))
}

#[test]
fn favorites_heading_carries_the_add_action() {
    let rows = places("{}", "");
    assert_eq!(say(&rows[0].label), "Favorites");
    assert_eq!(format!("{}|{}|{}", rows[0].action, say(&rows[0].value), focusable(&rows[0])), "addFavourite|Add This Folder|true");
    assert!(!rows.iter().any(|row| row.kind == Kind::Hero));
}

#[test]
fn favourite_paths_read_as_written_and_keep_the_stored_value() {
    let homed = places(r#"{"places":{"favourites":[{"label":"Projects","path":"/home/gm/Projects"},
        {"label":"flea","path":"~/Documents/claude/flea"},{"label":"Archive","path":"/srv/archive"},
        {"label":"Sibling","path":"/home/gmx/s"}]}}"#, "/home/gm");
    let first = find(&homed, "favourite:0");
    assert_eq!(format!("{}|{}", first.display, say(&first.value)), "~/Projects|/home/gm/Projects");
    assert_eq!(find(&homed, "favourite:1").display, "~/Documents/claude/flea");
    assert_eq!(find(&homed, "favourite:2").display, "/srv/archive");
    assert_eq!(say(&find(&homed, "favourite:2").value), "/srv/archive");
    assert_eq!(find(&homed, "favourite:3").display, "/home/gmx/s");
    let bare = places(r#"{"places":{"favourites":[{"label":"P","path":"/home/gm/P"}]}}"#, "");
    assert_eq!(find(&bare, "favourite:0").display, "/home/gm/P");
    assert_eq!(find(&bare, "favourite:0").favourite, Some(0));
}

#[test]
fn malformed_favourites_name_their_fault() {
    let rows = places(r#"{"places":{"favourites":[7,{"label":" ","path":"/x"},{"label":"a","path":"rel"},
        {"label":"n","path":"smb://nas/share"},{"label":"z","path":"smb://"}]}}"#, "");
    assert_eq!(say(&rows[1].label), "7");
    assert_eq!(say(&rows[1].error), "invalid favorite record");
    assert_eq!(say(&rows[2].error), "label needs visible text");
    assert_eq!(say(&rows[3].error), "use a full path, a path that starts with ~/, or a network address");
    assert!(rows[4].error.is_empty());
    assert_eq!(rows[4].glyph, "network");
    assert_eq!(say(&rows[5].error), "use a full path, a path that starts with ~/, or a network address");
    let missing = places(r#"{"places":{"favourites":[{"label":"q"}]}}"#, "");
    assert_eq!(say(&missing[1].error), "invalid path");
    let data = parse(r#"{"places":{"favourites":[{"label":"q","path":"/q"}]}}"#).unwrap();
    let statuses = [Text::data("not mounted")];
    let status = places_rows(&State { favourite_statuses: &statuses, ..State::new(&data) });
    assert_eq!(say(&status[1].error), "not mounted");
}

#[test]
fn rail_switches_read_their_shipped_defaults() {
    let rows = places("{}", "");
    assert_eq!((find(&rows, "places.driveSize").on, find(&rows, "places.trashCount").on), (false, false));
    assert!(find(&rows, "places.showUnmounted").on);
    assert!(!find(&places(r#"{"places":{"showUnmounted":false}}"#, ""), "places.showUnmounted").on);
    let recent = find(&rows, "places.showRecent");
    assert_eq!(format!("{}|{}", say(&recent.label), recent.on), "Recent|false");
    assert!(find(&places(r#"{"places":{"showRecent":true}}"#, ""), "places.showRecent").on);
    let tail: Vec<String> = rows[rows.len() - 8..rows.len() - 2].iter().map(|r| say(&r.label)).collect();
    assert_eq!(tail, ["Show unmounted drives", "List nearby hosts when adding a share", "Show tags", "Auto-hide sidebar", "Show sidebar", "Sidebar width"]);
    // 0401-settings-nearby: the option ships off and greys in place while Network is unticked.
    let nearby = find(&rows, "places.nearbyHosts");
    assert!(!nearby.on && nearby.available && nearby.glyph == "network");
    assert!(find(&places(r#"{"places":{"nearbyHosts":true}}"#, ""), "places.nearbyHosts").on);
    assert!(!find(&places(r#"{"places":{"showNetwork":false}}"#, ""), "places.nearbyHosts").available);
    assert!(find(&rows, "places.rail").on);
    assert!(!find(&rows, "places.autoHide").on);
    assert!(!find(&places(r#"{"places":{"rail":"hidden"}}"#, ""), "places.rail").on);
    let detailed = places(r#"{"places":{"driveSize":true,"trashCount":true}}"#, "");
    assert_eq!((find(&detailed, "places.driveSize").on, find(&detailed, "places.trashCount").on), (true, true));
}

#[test]
fn auto_hide_greys_show_sidebar() {
    let hiding = places(r#"{"places":{"autoHide":true}}"#, "");
    let rail = find(&hiding, "places.rail");
    assert_eq!((rail.available, focusable(rail)), (false, false));
    let rows = places("{}", "");
    let rail = find(&rows, "places.rail");
    assert_eq!((rail.available, focusable(rail)), (true, true));
}

#[test]
fn sidebar_width_snaps_to_its_stops() {
    let rows = places("{}", "");
    let width = find(&rows, "places.sidebarWidth");
    assert_eq!(width.selected, "192");
    assert_eq!(say(&width.value), "192 px");
    assert_eq!(sidebar_width(Some(&parse("250").unwrap())), 256);
    assert_eq!(sidebar_width(Some(&parse("176").unwrap())), 160);
    assert_eq!(sidebar_width(Some(&parse("\"200\"").unwrap())), 192);
    assert_eq!(sidebar_width(None), 192);
}

#[test]
fn places_end_with_the_trash_sweep() {
    let rows = places("{}", "");
    let tail: Vec<String> = rows[rows.len() - 2..].iter().map(|r| say(&r.label)).collect();
    assert_eq!(tail, ["Trash", "Empty after 30 days"]);
    assert!(!find(&rows, "trashAutoEmpty").on);
    assert_eq!(say(&find(&rows, "trashAutoEmpty").caption), "permanently");
    assert!(find(&places(r#"{"trashAutoEmpty":true}"#, ""), "trashAutoEmpty").on);
}

// tests/js/settings.js: the two buttons that sat under the list are gone; only the heading carries Add.
#[test]
fn no_row_under_the_favourites_carries_an_action() {
    let rows = places(r#"{"places":{"favourites":[{"label":"Docs","path":"/fx/v/work/Docs"}]}}"#, "");
    let actions: Vec<&str> = rows.iter().filter(|row| !row.action.is_empty()).map(|row| row.action).collect();
    assert_eq!(actions, ["addFavourite"]);
}
