use super::*;
use crate::tui::{action_key_tests::drain, actions, render::{cards::tests::sent, frame_tests::model}, wire};

fn press(m: &mut Model, key: Key, w: &mut Wire) { actions::key(m, &key, &Map::load(), w).unwrap(); }
fn answer(m: &mut Model, w: &mut Wire, line: &str) {
    let line = line.replace("ID", &m.action_id.to_string());
    m.receive(crate::jsondoc::parse(&line).unwrap(), w).unwrap();
}
fn select(m: &mut Model, id: &str) {
    m.menu_cursor = entries(m, &Map::load()).iter().position(|row| row.as_ref().is_some_and(|row| row.id == id)).unwrap_or_else(|| panic!("no {id} row"));
}
// Opens the menu on notes.txt and answers its snapshot and formats; Open With stays out of the way unless a test asks for it.
fn opened(w: &mut Wire, registry: bool) -> Model {
    let mut m = model();
    if !registry { m.menu_view.hidden.push("openwith".into()); }
    press(&mut m, Key::character('m', ""), w);
    drain(w);
    answer(&mut m, w, r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#);
    answer(&mut m, w, r#"{"t":"formats","id":ID,"archive":["zip"],"providers":{"taildrop":{"installed":false},"dropbox":{"installed":true},"dropboxInfo":"{\"personal\":{\"path\":\"/home/gm/Dropbox\"}}","dropboxError":""}}"#);
    m
}

#[test]
fn opening_asks_for_the_rows_facts_and_one_item_asks_for_its_registry() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    press(&mut m, Key::character('m', ""), &mut w);
    assert!(m.menu && !m.menu_ready);
    let values = drain(&mut w);
    assert_eq!(count(sent(&values, "formats", "")[0], "id"), m.action_id);
    let list = sent(&values, "actions", "list");
    assert_eq!(
        list[0].get("paths").and_then(Json::as_array).unwrap()[0].as_str(),
        Some("/fx/v/work/notes.txt")
    );
    assert_eq!(sent(&values, "menuaction", "snapshot").len(), 1);
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    assert!(m.menu_ready);
    assert_eq!(sent(&drain(&mut w), "menuaction", "applications").len(), 1);
    press(&mut m, Key::named("Return", ""), &mut w);
    assert!(drain(&mut w).is_empty() && m.menu_view.queued.as_deref() == Some("open"), "Open waits for the registry answer");
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"applications","id":ID,"ok":true,"applications":[{"id":"editor.desktop","label":"Editor","icon":"","default":true}]}"#);
    let validate = drain(&mut w);
    assert_eq!(text(sent(&validate, "menuaction", "validate")[0], "action"), "open", "then goes out");
    assert!(!m.menu);
    let mut m = model();
    m.selected = [7, 8].into_iter().collect();
    press(&mut m, Key::character('m', ""), &mut w);
    drain(&mut w);
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    assert!(
        sent(&drain(&mut w), "menuaction", "applications").is_empty(),
        "two items have no registry"
    );
    wire::finish(w, reader);
}

#[test]
fn open_with_launches_the_chosen_application_and_closes_the_slot() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, true);
    drain(&mut w);
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"applications","id":ID,"ok":true,"applications":[{"id":"editor.desktop","label":"Editor","icon":"","default":true}]}"#);
    select(&mut m, "openwith");
    press(&mut m, Key::named("Right", ""), &mut w);
    assert_eq!(m.menu_view.sub, Sub::OpenWith);
    press(&mut m, Key::named("Return", ""), &mut w);
    let values = drain(&mut w);
    assert_eq!(text(sent(&values, "menuaction", "openWith")[0], "application"), "editor.desktop");
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"openWith","id":ID,"ok":true,"path":"/fx/v/work/notes.txt"}"#);
    assert_eq!(sent(&drain(&mut w), "menuaction", "close").len(), 1);
    assert!(m.menu_action.is_empty());
    wire::finish(w, reader);
}

#[test]
fn compress_names_the_archive_after_its_item_and_reports_the_format() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, false);
    select(&mut m, "compress");
    press(&mut m, Key::named("Return", ""), &mut w);
    assert_eq!(m.menu_view.sub, Sub::Compress);
    press(&mut m, Key::named("Return", ""), &mut w);
    let values = drain(&mut w);
    assert_eq!(text(sent(&values, "menuaction", "activate")[0], "action"), "compress:zip");
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"activate","id":ID,"ok":true,"action":"compress:zip","paths":["/fx/v/work/notes.txt"],"dest":""}"#);
    let values = drain(&mut w);
    let archive = sent(&values, "archive", "compress")[0];
    assert_eq!((text(archive, "dest"), text(archive, "format"), count(archive, "menuId")), ("/fx/v/work/notes.zip", "zip", m.action_id));
    assert_eq!(m.message, "Compressing 1 item to .zip");
    wire::finish(w, reader);
}

#[test]
fn dropbox_checks_the_account_folder_then_moves_and_a_failure_closes_the_slot() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, false);
    select(&mut m, "dropbox");
    press(&mut m, Key::named("Return", ""), &mut w);
    let values = drain(&mut w);
    assert_eq!(text(sent(&values, "menuaction", "providerDestination")[0], "dest"), "/home/gm/Dropbox");
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"providerDestination","id":ID,"ok":true}"#);
    let values = drain(&mut w);
    let activate = sent(&values, "menuaction", "activate")[0];
    assert_eq!((text(activate, "action"), text(activate, "dest")), ("dropbox", "/home/gm/Dropbox"));
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"activate","id":ID,"ok":true,"action":"dropbox","paths":[],"dest":"/home/gm/Dropbox"}"#);
    let values = drain(&mut w);
    let transfer = sent(&values, "transfer", "move")[0];
    assert_eq!((text(transfer, "dest"), count(transfer, "menuId")), ("/home/gm/Dropbox", m.action_id));
    let mut m = opened(&mut w, false);
    select(&mut m, "dropbox");
    press(&mut m, Key::named("Return", ""), &mut w);
    drain(&mut w);
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"providerDestination","id":ID,"ok":false,"error":"The Dropbox account path is not a folder."}"#);
    assert_eq!(m.error, "The Dropbox account path is not a folder.");
    assert_eq!(sent(&drain(&mut w), "menuaction", "close").len(), 1);
    assert!(m.menu_action.is_empty());
    wire::finish(w, reader);
}

#[test]
fn plugin_actions_run_on_the_menu_items_and_delete_opens_its_card() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, false);
    answer(&mut m, &mut w, r#"{"t":"actions","id":ID,"actions":[{"id":"verify","name":"Verify Checksum…"}],"errors":[],"error":""}"#);
    select(&mut m, "action:verify");
    press(&mut m, Key::named("Return", ""), &mut w);
    let values = drain(&mut w);
    let run = sent(&values, "actions", "run")[0];
    assert_eq!(text(run, "action"), "verify");
    assert_eq!(run.get("paths").and_then(Json::as_array).unwrap()[0].as_str(), Some("/fx/v/work/notes.txt"));
    let mut m = opened(&mut w, false);
    answer(&mut m, &mut w, r#"{"t":"actions","id":ID,"actions":[],"errors":[],"error":"Could not read the actions folder."}"#);
    assert_eq!(m.error, "Could not read the actions folder.");
    m.menu_view.hidden.clear();
    m.menu_view.apps = Some(Vec::new());
    select(&mut m, "delete");
    press(&mut m, Key::named("Return", ""), &mut w);
    assert!(!m.menu && m.deletion.is_some());
    assert_eq!(sent(&drain(&mut w), "menuaction", "snapshot").len(), 1);
    wire::finish(w, reader);
}

// A folder selected under a file cursor is the target, so Print is not offered for it.
#[test]
fn a_selected_folder_is_the_target_not_the_cursor_file() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    m.selected = [1].into_iter().collect();
    press(&mut m, Key::character('m', ""), &mut w);
    drain(&mut w);
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    assert!(
        !entries(&m, &Map::load())
            .iter()
            .flatten()
            .any(|row| row.id == "print"),
        "a folder target has no Print"
    );
    wire::finish(w, reader);
}
// The same folder target leaves Taildrop unavailable, red with no state word (GM 2026-10-29), because a folder cannot be sent.
#[test]
fn a_selected_folder_makes_taildrop_unavailable() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    m.selected = [1].into_iter().collect();
    press(&mut m, Key::character('m', ""), &mut w);
    drain(&mut w);
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    answer(
        &mut m,
        &mut w,
        r#"{"t":"formats","id":ID,"archive":["zip"],"providers":{"taildrop":{"installed":true}}}"#,
    );
    let rows = entries(&m, &Map::load());
    let taildrop = rows
        .iter()
        .flatten()
        .find(|row| row.id == "taildrop")
        .expect("Taildrop is offered");
    assert_eq!(
        (taildrop.ink, taildrop.trail.as_str(), taildrop.chevron),
        (crate::tui::render::menu_layout::Ink::Unavailable, "", false)
    );
    wire::finish(w, reader);
}
// The Dropbox move is judged on the target: a file outside the root stays movable under a cursor on the root folder.
#[test]
fn the_dropbox_move_judges_the_target_not_a_cursor_on_the_root() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    m.path = "/home/gm".into();
    m.rows.get_mut(&0).unwrap().name = "Dropbox".into();
    m.cursor = 0;
    m.selected = [7].into_iter().collect();
    press(&mut m, Key::character('m', ""), &mut w);
    drain(&mut w);
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    answer(
        &mut m,
        &mut w,
        r#"{"t":"formats","id":ID,"archive":["zip"],"providers":{"taildrop":{"installed":false},"dropbox":{"installed":true},"dropboxInfo":"{\"personal\":{\"path\":\"/home/gm/Dropbox\"}}","dropboxError":""}}"#,
    );
    assert!(
        entries(&m, &Map::load())
            .iter()
            .flatten()
            .any(|row| row.id == "dropbox"),
        "notes.txt is outside the root"
    );
    wire::finish(w, reader);
}
// The archive lands beside the folder the menu opened on, even after the user navigates before the answer.
#[test]
fn compress_lands_beside_the_folder_the_menu_opened_on() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, false);
    select(&mut m, "compress");
    press(&mut m, Key::named("Return", ""), &mut w);
    press(&mut m, Key::named("Return", ""), &mut w);
    drain(&mut w);
    m.path = "/fx/v/other".into();
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"activate","id":ID,"ok":true,"action":"compress:zip","paths":["/fx/v/work/notes.txt"],"dest":""}"#);
    let values = drain(&mut w);
    assert_eq!(text(sent(&values, "archive", "compress")[0], "dest"), "/fx/v/work/notes.zip");
    wire::finish(w, reader);
}

// The window's Copy As letters copy the menu's targets; Copy Relative reads them from the listed folder.
#[test]
fn copy_as_letters_copy_the_targets_and_relative_reads_from_the_listed_folder() {
    let (mut w, reader) = wire::echo_wire();
    for (letter, copied) in [('r', "notes.txt\n../other/x.txt"), ('p', "/fx/v/work/notes.txt\n/fx/v/other/x.txt"), ('s', "'/fx/v/work/notes.txt'\n'/fx/v/other/x.txt'")] {
        let mut m = opened(&mut w, false);
        drain(&mut w);
        press(&mut m, Key::character(letter, ""), &mut w);
        assert!(!m.menu && m.error.is_empty(), "{letter}: the letter copies and closes the menu, not {:?}", m.error);
        let values = drain(&mut w);
        let ask = sent(&values, "paths", "");
        assert_eq!(ask.len(), 1, "{letter}: one paths request");
        let line = format!(r#"{{"t":"paths","id":{},"paths":["/fx/v/work/notes.txt","/fx/v/other/x.txt"]}}"#, count(ask[0], "id"));
        m.receive(crate::jsondoc::parse(&line).unwrap(), &mut w).unwrap();
        let values = drain(&mut w);
        let clip = sent(&values, "clipSet", "text");
        assert_eq!(clip[0].get("paths").and_then(Json::as_array).unwrap()[0].as_str(), Some(copied), "{letter}");
    }
    let mut m = opened(&mut w, true);
    drain(&mut w);
    press(&mut m, Key::character('r', ""), &mut w);
    assert!(sent(&drain(&mut w), "paths", "").is_empty() && m.menu_view.queued.as_deref() == Some("copyRelative"), "Copy Relative waits for the registry answer");
    wire::finish(w, reader);
}

#[test]
fn a_raw_named_folder_opens_from_the_menu_by_its_own_bytes() {
    use std::os::unix::ffi::OsStrExt;
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    m.rows.get_mut(&0).unwrap().name = flea::bytename::name_text(b"r\xff").into_owned();
    m.cursor = 0;
    m.menu_view.hidden.push("openwith".into());
    press(&mut m, Key::character('m', ""), &mut w);
    drain(&mut w);
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    let raw = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/fx/v/work/r\xff"));
    assert_eq!(
        m.menu_path, raw,
        "the single target is the raw folder, not its exact spelling"
    );
    drain(&mut w);
    select(&mut m, "open");
    press(&mut m, Key::named("Return", ""), &mut w);
    assert_eq!(
        text(sent(&drain(&mut w), "menuaction", "validate")[0], "action"),
        "open"
    );
    answer(
        &mut m,
        &mut w,
        r#"{"t":"menuaction","op":"validate","action":"open","id":ID,"ok":true,"paths":["/fx/v/work/r\ufffd\u000072ff"]}"#,
    );
    let list = drain(&mut w);
    assert_eq!(
        text(sent(&list, "list", "")[0], "path"),
        "/fx/v/work/r\u{fffd}\u{0}72ff",
        "Open lists the raw folder"
    );
    assert_eq!(m.pending.as_deref(), Some(raw));
    wire::finish(w, reader);
}
