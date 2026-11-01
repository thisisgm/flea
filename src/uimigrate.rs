// The one-time changes a stored ui.json is owed, applied on read and recorded by its stateVersion stamp.
use crate::jsondoc::Json;
use crate::uischema::STATE_VERSION;

// Each change by the stateVersion it arrived in, oldest first; a file stamped below one has not had it.
type Step = fn(Json) -> Json;
const STEPS: &[(f64, Step)] = &[
    (1.0, show_unmounted_drives),
    (2.0, hide_new_menu_rows),
    (3.0, hide_run_rows),
    (4.0, hide_0_4_0_rows),
];

// Automatic settlement must leave a newer schema for the version that understands it.
pub fn newer_than_known(found: &Json) -> bool {
    STEPS.last().is_some_and(|(known, _)| stored_version(found) > *known)
}

// The file as found carries the stamp: the merged document already holds the shipped one in its place.
pub fn migrated(found: &Json, merged: Json) -> Json {
    let stored = stored_version(found);
    // Only a stored list the merge kept is a saved list; one that fell back is the shipped default.
    let hidden = |doc: &Json| doc.get("menu").and_then(|menu| menu.get("hidden")).cloned();
    let stored_menu = hidden(found).is_some() && hidden(found) == hidden(&merged);
    let mut state = merged;
    for (version, step) in STEPS {
        if stored < *version {
            // Menu migrations apply to a saved list, never the current unset default.
            let next = if *version >= 2.0 && !stored_menu { state } else { step(state) };
            state = with_key(next, STATE_VERSION, Json::Num(format!("{}", version)));
        }
    }
    state
}

// Sample input: 0.3.2's {"places":{"showUnmounted":false},...} has no stamp and reads as 0; 0.3.3 writes {...,"stateVersion":1}.
fn stored_version(found: &Json) -> f64 {
    found.get(STATE_VERSION).and_then(Json::as_f64).unwrap_or(0.0)
}

// Decided in 0.3.3: turned on once for every file written before it, including one switched off in 0.3.2.
fn show_unmounted_drives(state: Json) -> Json {
    let places = state.get("places").cloned().unwrap_or(Json::Obj(Vec::new()));
    with_key(state, "places", with_key(places, "showUnmounted", Json::Bool(true)))
}

// A 0.3.7 file that hid "copypath" keeps hiding its replacement "copyAs" where it stood.
fn hide_new_menu_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    // copypath becomes copyAs where it stands; a file that held both keeps the first.
    let mut kept: Vec<String> = Vec::with_capacity(items.len() + 2);
    for item in items.iter().filter_map(Json::as_str) {
        let name = if item == "copypath" { "copyAs" } else { item };
        if name == "copyAs" && kept.iter().any(|held| held == "copyAs") {
            continue;
        }
        kept.push(name.to_string());
    }
    for owned in ["pasteAs", "invertSelection"] {
        if !kept.iter().any(|name| name == owned) {
            kept.push(owned.to_string());
        }
    }
    let hidden = Json::Arr(kept.into_iter().map(Json::Str).collect());
    with_key(state, "menu", with_key(menu, "hidden", hidden))
}

// The run card's own rows ship hidden, so a 0.3.8 file that never named them hides them once.
fn hide_run_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    let mut kept: Vec<String> = items.iter().filter_map(Json::as_str).map(str::to_string).collect();
    for owned in ["run", "openAsText"] {
        if !kept.iter().any(|name| name == owned) {
            kept.push(owned.to_string());
        }
    }
    let hidden = Json::Arr(kept.into_iter().map(Json::Str).collect());
    with_key(state, "menu", with_key(menu, "hidden", hidden))
}

// Open in Agent ships off, so a stored list that never named it hides it once; Actions ships with Run Script's place hidden, so only a list that hid Run Script hides Actions too, and nothing beyond those ids is removed, reordered or changed.
fn hide_0_4_0_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    let mut kept: Vec<String> = items.iter().filter_map(Json::as_str).map(str::to_string).collect();
    let mut owned = vec!["openAgent"];
    if kept.iter().any(|name| name == "runScript") {
        owned.push("actions");
    }
    for name in owned {
        if !kept.iter().any(|held| held == name) {
            kept.push(name.to_string());
        }
    }
    let hidden = Json::Arr(kept.into_iter().map(Json::Str).collect());
    with_key(state, "menu", with_key(menu, "hidden", hidden))
}

// Replaced where it stands, so the key order a rewrite renders is the merge's and not this file's.
fn with_key(object: Json, key: &str, value: Json) -> Json {
    let Json::Obj(mut pairs) = object else { return object };
    match pairs.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => pairs.push((key.to_string(), value)),
    }
    Json::Obj(pairs)
}

#[cfg(test)]
mod tests {
    use crate::backend::testdir::TestDir;
    use crate::jsondoc::{self, Json};
    use crate::uischema::STATE_VERSION;
    use crate::uistate::{from_file, patched};
    use crate::uistore::Store;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    // What 0.3.2 wrote for an operator who never touched the switch: the whole merged document, off.
    const OLD: &str = r#"{"view":"grid","density":"comfortable","places":{"showUnmounted":false,"driveSize":true}}"#;

    fn shown(state: &Json) -> Option<bool> {
        state.get("places").and_then(|p| p.get("showUnmounted")).and_then(Json::as_bool)
    }

    fn stamp(state: &Json) -> Option<f64> {
        state.get(STATE_VERSION).and_then(Json::as_f64)
    }

    // The stamp STEPS leaves on a fully migrated file.
    const LATEST: f64 = super::STEPS[super::STEPS.len() - 1].0;

    fn hidden(state: &Json) -> Vec<String> {
        state
            .get("menu")
            .and_then(|m| m.get("hidden"))
            .and_then(Json::as_array)
            .map(|items| items.iter().filter_map(Json::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    }

    fn patch(text: &str) -> Json {
        jsondoc::parse(text).expect("the patch parses")
    }

    fn store_with(d: &TestDir, text: &str) -> Store {
        let s = Store::at(&d.dir("state"), &d.dir("config"));
        d.dir("state/flea");
        fs::write(d.join("state/flea/ui.json"), text).expect("seed the state file");
        s
    }

    fn on_disk(d: &TestDir) -> Json {
        jsondoc::parse(&fs::read_to_string(d.join("state/flea/ui.json")).expect("state file")).expect("valid JSON on disk")
    }

    #[test]
    fn a_0_3_2_file_that_stored_the_switch_off_reads_on_and_a_write_records_the_stamp() {
        let read = from_file(OLD);
        assert_eq!(shown(&read), Some(true));
        assert_eq!(stamp(&read), Some(LATEST));
        assert_eq!(read.get("density").and_then(Json::as_str), Some("comfortable"), "the migration touches one leaf");
        assert_eq!(read.get("places").and_then(|p| p.get("driveSize")).and_then(Json::as_bool), Some(true));
        let d = TestDir::new("uimigrate-write");
        let s = store_with(&d, OLD);
        s.update(&patch(r#"{"hidden":true}"#)).expect("an unrelated write");
        let stored = on_disk(&d);
        assert_eq!((shown(&stored), stamp(&stored)), (Some(true), Some(LATEST)), "the write carries the migration down");
        assert_eq!(stored.get("hidden").and_then(Json::as_bool), Some(true));
    }

    #[test]
    fn a_stamped_file_keeps_the_switch_off_across_reads_writes_and_settles() {
        assert_eq!(shown(&from_file(r#"{"stateVersion":1,"places":{"showUnmounted":false}}"#)), Some(false));
        let d = TestDir::new("uimigrate-kept");
        let s = store_with(&d, OLD);
        s.update(&patch(r#"{"places":{"showUnmounted":false}}"#)).expect("switched off after the migration");
        s.update(&patch(r#"{"view":"list"}"#)).expect("a later write");
        assert_eq!(shown(&s.read()), Some(false), "the operator's own off outlives the next write");
        let ino = fs::metadata(d.join("state/flea/ui.json")).expect("meta").ino();
        s.settle().expect("the next launch");
        assert_eq!(fs::metadata(d.join("state/flea/ui.json")).expect("meta").ino(), ino, "a stamped file is not rewritten");
        assert_eq!(shown(&s.read()), Some(false));
    }

    // Launches settle the file before either front end reads it, so the first launch writes the stamp once.
    #[test]
    fn the_first_settle_writes_the_migration_down_and_the_second_leaves_it_alone() {
        let d = TestDir::new("uimigrate-settle");
        let s = store_with(&d, OLD);
        s.settle().expect("the first 0.3.3 launch");
        let stored = on_disk(&d);
        assert_eq!((shown(&stored), stamp(&stored)), (Some(true), Some(LATEST)));
        let ino = fs::metadata(d.join("state/flea/ui.json")).expect("meta").ino();
        s.settle().expect("the second launch");
        assert_eq!(fs::metadata(d.join("state/flea/ui.json")).expect("meta").ino(), ino, "one migration is one write");
    }

    // No file at all is the default document, which is already stamped, so a fresh off is not undone.
    #[test]
    fn a_fresh_install_reads_on_and_its_own_switch_off_stays_off() {
        let d = TestDir::new("uimigrate-fresh");
        let s = Store::at(&d.dir("state"), &d.dir("config"));
        assert_eq!((shown(&s.read()), stamp(&s.read())), (Some(true), Some(LATEST)));
        s.update(&patch(r#"{"places":{"showUnmounted":false}}"#)).expect("switched off on a fresh install");
        assert_eq!(stamp(&on_disk(&d)), Some(LATEST), "the first write is stamped");
        assert_eq!(shown(&s.read()), Some(false));
    }

    #[test]
    fn every_other_key_survives_the_migration_and_a_newer_stamp_is_kept() {
        let text = r#"{"fromANewerFlea":{"a":[1,"two"]},"places":{"newLeaf":7,"showUnmounted":false},"keys":"vim"}"#;
        let read = from_file(text);
        assert_eq!(jsondoc::render(read.get("fromANewerFlea").expect("top-level unknown")), "{\n  \"a\": [\n    1,\n    \"two\"\n  ]\n}\n");
        assert_eq!(read.get("places").and_then(|p| p.get("newLeaf")).and_then(Json::as_f64), Some(7.0));
        assert_eq!(read.get("keys").and_then(Json::as_str), Some("vim"));
        let d = TestDir::new("uimigrate-unknown");
        let s = store_with(&d, text);
        s.update(&patch(r#"{"hidden":true}"#)).expect("write");
        assert_eq!(on_disk(&d).get("fromANewerFlea"), read.get("fromANewerFlea"), "and it is written back as it was read");
        let newer = from_file(r#"{"stateVersion":7,"places":{"showUnmounted":false}}"#);
        assert_eq!((shown(&newer), stamp(&newer)), (Some(false), Some(7.0)), "a newer Flea's stamp is not rewound");
    }

    #[test]
    fn a_patch_onto_an_old_file_keeps_the_migration_and_cannot_move_the_stamp() {
        let next = patched(&from_file(OLD), &patch(r#"{"view":"list"}"#)).expect("patch applies");
        assert_eq!((shown(&next), stamp(&next)), (Some(true), Some(LATEST)));
        let off = patched(&from_file(OLD), &patch(r#"{"places":{"showUnmounted":false}}"#)).expect("patch applies");
        assert_eq!((shown(&off), stamp(&off)), (Some(false), Some(LATEST)), "the patch is the operator's and lands last");
        let refused = patched(&from_file(OLD), &patch(r#"{"stateVersion":0}"#)).expect_err("the stamp is not a setting");
        assert!(refused.contains(STATE_VERSION), "{}", refused);
    }

    // MenuAdditions040: Copy as replaces the hidden-by-default Copy path row, and Paste as and
    // Invert selection ship hidden too, so a 0.3.7 file is owed that hiding once.
    #[test]
    fn a_0_3_7_file_that_hid_copy_path_hides_its_replacement_at_the_same_position() {
        let read = from_file(
            r#"{"stateVersion":1,"places":{"showUnmounted":true},"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copypath","extThumbs"]}}"#,
        );
        assert_eq!(
            hidden(&read),
            ["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copyAs", "extThumbs", "pasteAs", "invertSelection", "run", "openAsText", "openAgent", "actions"],
            "copypath becomes copyAs where it stood, the other two are appended, then the run rows, then the agent row, then Actions where Run Script was hidden"
        );
        assert_eq!(stamp(&read), Some(LATEST));
        assert_eq!(shown(&read), Some(true), "the 1.0 step does not run again for a stamped file");
    }

    #[test]
    fn a_0_3_7_file_that_showed_copy_path_keeps_copy_as_visible() {
        let read = from_file(
            r#"{"stateVersion":1,"places":{"showUnmounted":true},"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","extThumbs"]}}"#,
        );
        let got = hidden(&read);
        assert!(!got.iter().any(|id| id == "copyAs" || id == "copypath"), "Copy as replaces that row: {}", got.join(","));
        assert!(got.iter().any(|id| id == "pasteAs") && got.iter().any(|id| id == "invertSelection"));
        assert!(got.iter().any(|id| id == "run") && got.iter().any(|id| id == "openAsText"));
        assert_eq!(stamp(&read), Some(LATEST));
    }

    #[test]
    fn a_file_already_at_2_keeps_a_shown_row_shown() {
        let read = from_file(
            r#"{"stateVersion":2,"menu":{"hidden":["delete","copyAs","invertSelection"]}}"#,
        );
        assert_eq!(hidden(&read), ["delete", "copyAs", "invertSelection", "run", "openAsText", "openAgent"], "pasteAs stays shown");
        assert_eq!(stamp(&read), Some(LATEST));
    }

    #[test]
    fn a_0_3_2_file_gets_every_step_in_one_read() {
        let read = from_file(r#"{"places":{"showUnmounted":false},"menu":{"hidden":["delete","copypath"]}}"#);
        assert_eq!((shown(&read), stamp(&read)), (Some(true), Some(LATEST)));
        assert_eq!(hidden(&read), ["delete", "copyAs", "pasteAs", "invertSelection", "run", "openAsText", "openAgent"]);
    }

    // The run card's own rows ship hidden, so a stored list that never named them gains them once.
    #[test]
    fn a_0_3_8_file_lacking_the_run_rows_hides_them_once() {
        let read = from_file(
            r#"{"stateVersion":2,"places":{"showUnmounted":true},"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs"]}}"#,
        );
        let got = hidden(&read);
        assert_eq!(got[got.len() - 4..], ["run".to_string(), "openAsText".to_string(), "openAgent".to_string(), "actions".to_string()]);
        assert_eq!(stamp(&read), Some(LATEST));
        let again = from_file(r#"{"stateVersion":3,"menu":{"hidden":["delete"]}}"#);
        assert_eq!(hidden(&again), ["delete", "openAgent"], "a stamped file keeps its shown run rows and still hides the agent row");
    }

    // Open in Agent ships off, so a stored list that never named it hides it once.
    #[test]
    fn a_0_4_0_file_lacking_the_agent_row_hides_it_once() {
        let read = from_file(
            r#"{"stateVersion":3,"places":{"showUnmounted":true},"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText"]}}"#,
        );
        let got = hidden(&read);
        assert_eq!(got[got.len() - 2..], ["openAgent".to_string(), "actions".to_string()]);
        assert_eq!(stamp(&read), Some(LATEST));
        let again = from_file(r#"{"stateVersion":4,"menu":{"hidden":["delete"]}}"#);
        assert_eq!(hidden(&again), ["delete"], "a stamped file that shows the agent row keeps it shown");
    }

    // A hidden list holding a non-string is not a list this step understands, so it passes through untouched.
    #[test]
    fn a_hidden_list_with_a_non_string_is_left_as_it_is() {
        let state = patch(r#"{"stateVersion":3,"menu":{"hidden":["delete",7]}}"#);
        assert_eq!(super::hide_0_4_0_rows(state.clone()), state);
        assert_eq!(
            hidden(&from_file(r#"{"stateVersion":3,"menu":{"hidden":["runScript","openAgent","actions"]}}"#)),
            ["runScript", "openAgent", "actions"],
            "a held id is not added twice"
        );
    }

    // Actions ships off with Run Script's place, so a 0.3.7 file that kept Run Script hidden hides it too.
    #[test]
    fn a_0_3_7_file_that_hid_run_script_gains_actions_hidden_once() {
        let read = from_file(r#"{"stateVersion":1,"menu":{"hidden":["delete","runScript"]}}"#);
        assert_eq!(
            hidden(&read),
            ["delete", "runScript", "pasteAs", "invertSelection", "run", "openAsText", "openAgent", "actions"],
            "the steps append their own ids in order, and Actions joins once"
        );
        assert_eq!(stamp(&read), Some(LATEST));
        let again = from_file(r#"{"stateVersion":4,"menu":{"hidden":["delete","runScript"]}}"#);
        assert_eq!(hidden(&again), ["delete", "runScript"], "a file stamped 4 is left alone");
    }

    // A 0.3.7 file that showed Run Script never hid Actions, so the migration leaves its row shown.
    #[test]
    fn a_0_3_7_file_that_showed_run_script_keeps_actions_shown() {
        let read = from_file(r#"{"stateVersion":1,"menu":{"hidden":["delete","moveto","copyAs"]}}"#);
        let got = hidden(&read);
        assert!(got.iter().any(|id| id == "openAgent"), "the agent row still joins the hidden list");
        assert!(!got.iter().any(|id| id == "actions"), "Run Script was shown, so Actions stays shown: {}", got.join(","));
        assert_eq!(stamp(&read), Some(LATEST));
    }

    // Two windows and the TUI write under one lock: whoever migrates first, the stamp rides every later write.
    #[test]
    fn concurrent_writers_onto_an_old_file_migrate_it_once() {
        let d = TestDir::new("uimigrate-race");
        let s = store_with(&d, OLD);
        let patches = [r#"{"hidden":true}"#, r#"{"places":{"showUnmounted":false}}"#, r#"{"sort":{"key":"size"}}"#,
                       r#"{"keys":"mac"}"#, r#"{"places":{"sidebarWidth":224}}"#, r#"{"view":"columns"}"#];
        std::thread::scope(|scope| {
            for text in patches {
                let s = &s;
                scope.spawn(move || s.update(&patch(text)).expect("concurrent write"));
            }
        });
        let stored = on_disk(&d);
        assert_eq!((shown(&stored), stamp(&stored)), (Some(false), Some(LATEST)));
        assert_eq!(stored.get("keys").and_then(Json::as_str), Some("mac"));
        assert_eq!(stored.get("view").and_then(Json::as_str), Some("columns"));
        assert_eq!(stored.get("density").and_then(Json::as_str), Some("comfortable"));
    }
}
