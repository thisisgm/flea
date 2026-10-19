// Trash as a place: its rows, header, detail, preview and footer from the backend's list, and every changing key refused.
use super::*;
use crate::jsondoc::parse;
use crate::tui::{actions, render::{self, frame_tests}, wire::{echo_wire, finish}};
use crate::tui::wire::sort_tests::{drain, requests};
use std::time::Duration;

const LISTED: &str = r#"{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":1,"start":0,"rows":[{"uri":"trash:///old-draft.txt","original":"/fx/v/work/old-draft.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"}]}"#;

fn press(m: &mut Model, w: &mut Wire, key: Key) { actions::key(m, &key, &Map::load(), w).unwrap(); }
fn opened(w: &mut Wire) -> Model {
    let mut m = frame_tests::model();
    (m.cursor, m.top) = (7, 2);
    open(&mut m, w).unwrap();
    m.prompts.trash.as_mut().unwrap().kinds = crate::backend::kind::Kinds::from_pairs(&[("text/plain", Some("Plain text document"))]);
    m
}

// tui-trash-view: the header is the place, the detail is the date it went in, the parent pane is empty and the preview states the Trash facts.
#[test]
fn trash_opens_as_a_place_from_the_backend_list() {
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    let sent = drain(&mut w);
    let asked = requests(&sent, "trashbrowse");
    assert_eq!(asked.len(), 1);
    assert_eq!((text(asked[0], "op"), count(asked[0], "id"), count(asked[0], "start"), count(asked[0], "count")), ("list", 1, 0, 27));
    assert!(requests(&sent, "list").is_empty(), "Trash is never listed through its files folder");
    let waiting = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
    assert!(!waiting[15].contains("EMPTY"), "no empty-folder notice before Trash answers");
    m.receive(parse(LISTED).unwrap(), &mut w).unwrap();
    m.preview_visible = true;
    let rows = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
    assert!(rows[0].starts_with(" Trash "));
    assert_eq!(rows[0].chars().skip(120).collect::<String>(), "Deleted ▾ ");
    let current: String = rows[2].chars().skip(31).take(48).collect();
    assert_eq!(current.trim_end(), "old-draft.txt                 2025-06-14 09:30");
    assert_eq!(rows[2].chars().take(27).collect::<String>().trim(), "", "the parent pane is empty in Trash");
    assert_eq!(rows[2].chars().skip(83).take(13).collect::<String>(), "old-draft.txt");
    assert_eq!(rows[3].chars().skip(81).take(26).collect::<String>(), "Plain text document · 13 B");
    assert_eq!(rows[4].chars().skip(81).take(26).collect::<String>(), "Deleted · 2025-06-14 09:30");
    assert_eq!(rows[5].chars().skip(81).take(30).collect::<String>(), "Original Location · /fx/v/work");
    assert!(rows[30].starts_with(" 1 item "));
    assert!(rows[30].contains("tmpfs · 64.1 MB free"), "the disk facts are the folder's Trash was opened from");
    assert_eq!(m.current_path(), None, "no path stands for a Trash row");
    finish(w, reader);
}
// Keys that would change a file are refused in one sentence and send nothing; moving and leaving still work.
#[test]
fn trash_refuses_changing_keys_and_escape_returns_to_the_folder() {
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    m.receive(parse(LISTED).unwrap(), &mut w).unwrap();
    drain(&mut w);
    for key in [Key::character('r', ""), Key::character('p', ""), Key::named("Delete", ""), Key::named("Return", ""), Key::named("F2", "")] {
        m.error.clear();
        press(&mut m, &mut w, key.clone());
        assert_eq!(m.error, "Trash is only viewed here; restore or delete its items in the window.", "{key:?}");
        assert!(m.editor.is_none());
        let sent = drain(&mut w);
        assert!(sent.is_empty(), "{key:?} sent {sent:?}");
    }
    press(&mut m, &mut w, Key::named("Escape", ""));
    assert!(m.error.is_empty(), "the first Escape clears the sentence");
    assert!(m.prompts.trash.is_some());
    press(&mut m, &mut w, Key::character('j', ""));
    assert_eq!(m.cursor, 0, "one row, so the cursor stays");
    press(&mut m, &mut w, Key::named("Escape", ""));
    assert!(m.prompts.trash.is_none());
    assert_eq!((m.path.as_path(), m.restore_cursor, m.restore_path.as_deref(), m.top),
        (std::path::Path::new("/fx/v/work"), Some(7), Some(std::path::Path::new("/fx/v/work/notes.txt")), 2));
    let sent = drain(&mut w);
    assert_eq!(requests(&sent, "list").iter().map(|v| text(v, "path")).collect::<Vec<_>>(), ["/fx/v/work"]);
    finish(w, reader);
}
// A stale Trash answer and the folder's own rows never land in Trash; a failed list says the backend's sentence.
#[test]
fn trash_drops_stale_answers_and_folder_rows() {
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    m.receive(parse(&LISTED.replace(r#""id":1"#, r#""id":9"#)).unwrap(), &mut w).unwrap();
    assert_eq!((m.total, m.rows.len()), (0, 0), "an answer to another request is dropped");
    m.receive(parse(r#"{"t":"rows","start":0,"rows":[{"n":"notes.txt","p":33188}]}"#).unwrap(), &mut w).unwrap();
    assert!(m.rows.is_empty(), "the folder's rows stay out of Trash");
    m.receive(parse(r#"{"t":"trashbrowse","id":1,"op":"list","ok":false,"error":"Trash could not be read."}"#).unwrap(), &mut w).unwrap();
    assert_eq!(m.error, "Trash could not be read.");
    finish(w, reader);
}
