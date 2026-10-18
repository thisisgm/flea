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
    assert_eq!(list[0].get("paths").and_then(Json::as_array).unwrap()[0].as_str(), Some("/fx/v/work/notes.txt"));
    assert_eq!(sent(&values, "menuaction", "snapshot").len(), 1);
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#);
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
    answer(&mut m, &mut w, r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#);
    assert!(sent(&drain(&mut w), "menuaction", "applications").is_empty(), "two items have no registry");
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
