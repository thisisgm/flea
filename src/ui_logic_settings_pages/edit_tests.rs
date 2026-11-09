// The writers ui/SettingsPanel.qml calls through ui/ViewState.qml, each as the patch it sends.
use super::super::pages::{self, State};
use super::*;
use crate::jsondoc::{parse, render};
use crate::ui_logic_settings_pages::test_support::say;

fn rows(section: &str, text: &str) -> (Json, Vec<Row>) {
    let data = parse(text).expect("fixture parses");
    let rows = pages::rows(section, &State::new(&data), Vec::new(), Vec::new);
    (data, rows)
}

fn find<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("no row {id}"))
}

fn patch(edit: Edit) -> String {
    match edit {
        Edit::Patch(json) => render(&json).replace(['\n', ' '], ""),
        other => panic!("not a patch: {other:?}"),
    }
}

#[test]
fn a_check_writes_its_dotted_leaf() {
    let (data, view) = rows("view", "{}");
    assert_eq!(patch(activate("view", find(&view, "foldersFirst"), &data, 14)), r#"{"foldersFirst":false}"#);
    assert_eq!(patch(activate("view", find(&view, "showGitStatus"), &data, 14)), r#"{"showGitStatus":true}"#);
    let (data, places) = rows("places", "{}");
    assert_eq!(patch(activate("places", find(&places, "places.driveSize"), &data, 14)), r#"{"places":{"driveSize":true}}"#);
    assert_eq!(patch(activate("places", find(&places, "places.rail"), &data, 14)), r#"{"places":{"rail":"hidden"}}"#);
    let (data, hidden) = rows("places", r#"{"places":{"rail":"hidden"}}"#);
    assert_eq!(patch(activate("places", find(&hidden, "places.rail"), &data, 14)), r#"{"places":{"rail":"shown"}}"#);
}

#[test]
fn a_greyed_row_and_a_hint_do_nothing() {
    let (data, view) = rows("view", "{}");
    assert_eq!(activate("view", find(&view, "hiddenLast"), &data, 14), Edit::None);
    let hint = view.iter().find(|row| row.kind == Kind::Hint).unwrap();
    assert_eq!(activate("view", hint, &data, 14), Edit::None);
    assert_eq!(step(find(&view, "foldersFirst"), &data, 14, 1), Edit::None);
}

#[test]
fn a_choice_walks_with_wrap_and_numbers_stay_numbers() {
    let (data, view) = rows("view", r#"{"density":"comfortable"}"#);
    assert_eq!(patch(activate("view", find(&view, "density"), &data, 14)), r#"{"density":"tight"}"#);
    assert_eq!(patch(step(find(&view, "density"), &data, 14, -1)), r#"{"density":"normal"}"#);
    assert_eq!(patch(step(find(&view, "columnsLimit"), &data, 14, 1)), r#"{"columnsLimit":4}"#);
    assert_eq!(patch(step(find(&view, "sort.key"), &data, 14, -1)), r#"{"sort":{"key":"kind"}}"#);
    let (data, places) = rows("places", "{}");
    assert_eq!(patch(step(find(&places, "places.sidebarWidth"), &data, 14, 1)), r#"{"places":{"sidebarWidth":224}}"#);
}

#[test]
fn an_unknown_stored_value_walks_from_before_the_first() {
    let (data, view) = rows("view", r#"{"density":"huge"}"#);
    assert_eq!(patch(step(find(&view, "density"), &data, 14, 1)), r#"{"density":"tight"}"#);
    assert_eq!(patch(step(find(&view, "density"), &data, 14, -1)), r#"{"density":"normal"}"#);
}

#[test]
fn the_preset_walks_the_four_presets() {
    let (data, keys) = rows("keys", r#"{"keys":"windows"}"#);
    assert_eq!(patch(step(find(&keys, "preset"), &data, 14, 1)), r#"{"keys":"default"}"#);
    assert_eq!(patch(step(find(&keys, "preset"), &data, 14, -1)), r#"{"keys":"mac"}"#);
}

#[test]
fn text_mode_pins_and_follows_and_the_ruler_steps() {
    let (data, display) = rows("display", "{}");
    assert_eq!(patch(activate("display", find(&display, "textMode"), &data, 14)), r#"{"display":{"textSize":{"mode":14}}}"#);
    let (data, display) = rows("display", r#"{"display":{"textSize":{"mode":14}}}"#);
    assert_eq!(patch(activate("display", find(&display, "textMode"), &data, 14)), r#"{"display":{"textSize":{"mode":"system"}}}"#);
    assert_eq!(patch(step(find(&display, "textStop"), &data, 14, 1)), r#"{"display":{"textSize":{"mode":16}}}"#);
    assert_eq!(patch(step(find(&display, "textStop"), &data, 14, -1)), r#"{"display":{"textSize":{"mode":12}}}"#);
}

#[test]
fn pages_and_host_rows_leave_the_patch_to_others() {
    let (data, view) = rows("view", "{}");
    assert_eq!(activate("view", find(&view, "columns"), &data, 14), Edit::Open("columns"));
    assert_eq!(activate("view", find(&view, "startFolder"), &data, 14), Edit::Host("startFolder".into()));
    let (data, columns) = rows("columns", "{}");
    assert_eq!(activate("columns", find(&columns, "backView"), &data, 14), Edit::Open("view"));
    assert_eq!(patch(activate("columns", find(&columns, "column:size"), &data, 14)), r#"{"columns":["name","date"]}"#);
    assert_eq!(patch(activate("columns", find(&columns, "column:kind"), &data, 14)), r#"{"columns":["name","size","date","kind"]}"#);
    let (data, places) = rows("places", "{}");
    assert_eq!(activate("places", &places[0], &data, 14), Edit::Host("addFavourite".into()));
}

#[test]
fn menu_checks_and_masters_write_the_hidden_set() {
    let (data, menus) = rows("menus", r#"{"menu":{"hidden":["delete"]}}"#);
    assert_eq!(patch(activate("menus", find(&menus, "delete"), &data, 14)), r#"{"menu":{"hidden":[]}}"#);
    assert_eq!(patch(activate("menus", find(&menus, "copyto"), &data, 14)), r#"{"menu":{"hidden":["delete","copyto"]}}"#);
    assert_eq!(patch(activate("menus", find(&menus, "keyHints"), &data, 14)), r#"{"keyHints":true}"#);
    let (data, menus) = rows("menus", r#"{"menu":{"hidden":[]},"shelf":{"enabled":true}}"#);
    assert_eq!(patch(activate("menus", find(&menus, "shelf"), &data, 14)), r#"{"shelf":{"enabled":false}}"#);
    let extras = menus.iter().find(|row| row.kind == Kind::Group && row.ids.contains(&"shelf")).unwrap();
    let written = patch(activate("menus", extras, &data, 14));
    assert!(written.starts_with(r#"{"shelf":{"enabled":false},"menu":{"hidden":["#), "{written}");
    assert!(!written.contains(r#""shelf"]"#) && !written.contains(r#""shelf","#), "{written}");
}

// pickRowStop: a cell or a tick addressed by index lands in the writer its key would reach.
#[test]
fn a_pointer_pick_writes_the_cell_or_the_stop_it_names() {
    let (data, view) = rows("view", "{}");
    assert_eq!(patch(pick(find(&view, "density"), &data, 14, 3)), r#"{"density":"comfortable"}"#);
    assert_eq!(patch(pick(find(&view, "columnsLimit"), &data, 14, 0)), r#"{"columnsLimit":2}"#);
    assert_eq!(pick(find(&view, "density"), &data, 14, 9), Edit::None, "a cell past the last does nothing");
    let (data, keys) = rows("keys", "{}");
    assert_eq!(patch(pick(find(&keys, "preset"), &data, 14, 1)), r#"{"keys":"vim"}"#);
    let (data, display) = rows("display", "{}");
    assert_eq!(patch(pick(find(&display, "textMode"), &data, 14, 1)), r#"{"display":{"textSize":{"mode":14}}}"#);
    assert_eq!(pick(find(&display, "textStop"), &data, 14, 0), Edit::None, "the ruler only reports while Omarchy owns the size");
    let (data, display) = rows("display", r#"{"display":{"textSize":{"mode":14}}}"#);
    assert_eq!(patch(pick(find(&display, "textStop"), &data, 14, 6)), r#"{"display":{"textSize":{"mode":20}}}"#);
    let (data, view) = rows("view", "{}");
    assert_eq!(pick(find(&view, "hiddenLast"), &data, 14, 0), Edit::None, "a greyed row takes no pick");
}

// A saved menu document keeps Actions off; both it and an unset document keep the shelf off.
const FRESH: &str = r#"{"shelf":{"enabled":false},"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent","actions"]}}"#;

fn head<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|row| row.kind == Kind::Group && row.id == id).unwrap_or_else(|| panic!("no head {id}"))
}

#[test]
fn a_fresh_install_counts_the_shelf_as_off() {
    for text in [FRESH, "{}"] {
        let (_, menus) = rows("menus", text);
        let extras = head(&menus, "extras");
        assert_eq!(say(&extras.value), if text == "{}" { "11 of 18" } else { "12 of 18" }, "{text}");
        assert_eq!(extras.state, Master::Some, "{text}");
        assert!(!find(&menus, "shelf").on, "{text}: Enable shelf is off");
        // A held list is read as saved here; with none, Properties and Permissions ship shown.
        assert_eq!(say(&head(&menus, "openInspect").value), if text == "{}" { "7 of 14" } else { "5 of 14" }, "{text}: Open in Agent ships off");
    }
}

// The HIG Actions switch writes its own key; Run Script keeps its saved hidden value.
#[test]
fn actions_default_on_and_saved_off_are_independent_of_run_script() {
    let (_, menus) = rows("menus", FRESH);
    assert!(!find(&menus, "actions").on);
    let (data, menus) = rows("menus", &FRESH.replace(r#","actions""#, ""));
    assert!(find(&menus, "actions").on);
    assert_eq!(say(&head(&menus, "extras").value), "13 of 18");
    let changed = patch(activate("menus", find(&menus, "actions"), &data, 14));
    assert!(changed.contains("runScript") && changed.contains("actions"));
}

#[test]
fn the_extras_master_with_only_the_shelf_off_turns_the_shelf_on_and_clears_the_list() {
    let (data, menus) = rows("menus", r#"{"shelf":{"enabled":false},"menu":{"hidden":[]}}"#);
    let extras = head(&menus, "extras");
    assert_eq!(extras.state, Master::Some);
    assert_eq!(patch(activate("menus", extras, &data, 14)), r#"{"shelf":{"enabled":true},"menu":{"hidden":[]}}"#);
}

#[test]
fn an_absent_shelf_switch_is_off_and_its_row_turns_it_on() {
    let (data, menus) = rows("menus", r#"{"menu":{"hidden":[]}}"#);
    assert!(!find(&menus, "shelf").on);
    assert_eq!(patch(activate("menus", find(&menus, "shelf"), &data, 14)), r#"{"shelf":{"enabled":true}}"#);
}

#[test]
fn chosen_folder_writes_the_folder_mode_and_its_path_together() {
    assert_eq!(patch(Edit::Patch(start_folder("/home/gm/Work"))), r#"{"startIn":"folder","startFolder":"/home/gm/Work"}"#);
}
