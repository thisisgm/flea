// The words a refused menu request shows; each pins the sentence a person reads, not the branch that raises it.
use super::*;
use super::tests::{unstarted_menu, CHANGED};
use crate::backend::testdir::TestDir;

const STOPPED: &str = "The menu stopped answering; reopen this window.";
const CONFIRM_AGAIN: &str = "The delete confirmation expired; confirm again.";

// A poisoned lock is the one way these reads fail, and a closed slot the one way a request is refused.
#[test]
fn a_menu_that_stopped_answering_says_to_reopen_the_window() {
    let (menu, rx) = unstarted_menu();
    menu.slot.close();
    assert!(!menu.request(r#"{"c":"menuaction","op":"validate","id":1}"#.into(), vec![], None));
    match rx.try_recv().expect("the refusal must answer rather than hang") {
        OpMsg::Meta { line } => assert_eq!(field_str(&line, "error").as_deref(), Some(STOPPED)),
        _ => panic!("a refusal must arrive as Meta"),
    }
    let snapshot = Arc::clone(&menu.snapshot);
    let restoration = Arc::clone(&menu.restoration);
    let poisoner = std::thread::spawn(move || {
        let _snapshot = snapshot.lock().unwrap();
        let _restoration = restoration.lock().unwrap();
        panic!("poison both locks");
    });
    assert!(poisoner.join().is_err());
    assert_eq!(menu.selection(1).err().as_deref(), Some(STOPPED));
    assert_eq!(menu.provider_destination(1, Path::new("/")).err().as_deref(), Some(STOPPED));
    assert_eq!(menu.selected_path(1, Path::new("/")).err().as_deref(), Some(STOPPED));
    assert_eq!(menu.retain_survivors(1, &mut Vec::new()).err().as_deref(), Some(STOPPED));
}

#[test]
fn a_stale_menu_selection_says_what_changed_and_what_to_do() {
    let sandbox = TestDir::new("menu-stale-wording");
    let kept = sandbox.file("kept", "kept");
    let other = sandbox.file("other", "other");
    let (menu, _rx) = unstarted_menu();
    *menu.snapshot.lock().unwrap() = Snapshot { id: 6, items: vec![Selected::inspect(kept.to_str().unwrap()).unwrap()], ..Snapshot::default() };
    assert_eq!(menu.selection(7).err().as_deref(), Some(CHANGED));
    assert_eq!(menu.provider_destination(7, &kept).err().as_deref(), Some(CHANGED));
    assert_eq!(menu.selected_path(7, &kept).err().as_deref(), Some(CHANGED));
    assert_eq!(menu.selected_path(6, &other).err().as_deref(), Some("That item is no longer selected; reopen the menu."));
    assert_eq!(menu.retain_survivors(0, &mut Vec::new()).err().as_deref(), Some("The items changed; select them again."));
    let items = menu.selection(6).unwrap();
    assert_eq!(validate_sources(Some(&items), &[other.clone()]).err().as_deref(), Some(CHANGED));
    assert_eq!(std::fs::read_to_string(other).unwrap(), "other");
}

#[test]
fn an_expired_delete_confirmation_says_to_confirm_again() {
    let sandbox = TestDir::new("menu-delete-expired");
    let path = sandbox.file("item", "preserved");
    let mut snapshot = Snapshot::default();
    snapshot.handle(r#"{"op":"snapshot","id":14}"#, vec![path.to_string_lossy().into()]);
    for op in ["checkDelete", "delete"] {
        let reply = snapshot.handle(&format!(r#"{{"op":"{}","id":14,"token":99}}"#, op), vec![]);
        assert_eq!(field_str(&reply, "error").as_deref(), Some(CONFIRM_AGAIN), "{}", op);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), "preserved");
}

#[test]
fn a_provider_destination_that_is_a_file_is_not_a_folder() {
    let sandbox = TestDir::new("menu-provider-file");
    let source = sandbox.file("source", "keep");
    let file = sandbox.file("account", "not a folder");
    let mut snapshot = Snapshot::default();
    snapshot.handle(r#"{"op":"snapshot","id":15}"#, vec![source.to_string_lossy().into()]);
    let capture = format!(r#"{{"op":"providerDestination","id":15,"dest":"{}"}}"#, escape(file.to_str().unwrap()));
    assert_eq!(field_str(&snapshot.handle(&capture, vec![]), "error").as_deref(), Some("The Dropbox account path is not a folder."));
    assert_eq!(std::fs::read_to_string(file).unwrap(), "not a folder");
}

// Lines no fixture reaches (a link replaced between two reads, a stat failing right after a create) are pinned by their source.
#[test]
fn retired_menu_wording_is_gone_from_the_live_source() {
    let actions = include_str!("menu_actions.rs").split("\n#[cfg(test)]\nmod tests").next().unwrap();
    let live = format!("{}\n{}", actions, include_str!("menuproperties.rs"));
    for retired in ["Menu selection expired", "review a fresh confirmation", "The menu service stopped", "survivor identities",
                    "This path was not in the menu selection", "captured menu selection", "is not a directory",
                    "Cursor source was not captured", "Could not read symlink", "record its identity"] {
        assert!(!live.contains(retired), "retired wording came back: {}", retired);
    }
    for plain in ["Could not read the link: {}.", "but undo cannot remove it: {}.", "Opened it, but the default was not saved: {}."] {
        assert!(live.contains(plain), "plain wording is missing: {}", plain);
    }
}
