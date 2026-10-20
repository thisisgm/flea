// Trash judges whole key sequences, keeps the listing out of reach while the preview has focus, and opens from yazi's own key.
use super::*;
use crate::jsondoc::parse;
use crate::tui::input::Pointer;
use crate::tui::{actions, render::{self, frame_tests}, wire::{echo_wire, finish}};
use crate::tui::wire::sort_tests::{drain, requests};

const LISTED: &str = r#"{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":3,"start":0,"rows":[{"uri":"trash:///notes.txt","original":"/fx/v/work/notes.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"},{"uri":"trash:///old","original":"/fx/v/work/old","size":0,"deleted":"2025-06-13T09:30:00","directory":true,"icon":"folder"},{"uri":"trash:///blob","original":"/fx/v/work/blob","size":9,"deleted":"2025-06-12T09:30:00","directory":false,"icon":""}]}"#;

fn press(m: &mut Model, w: &mut Wire, key: Key) { actions::key(m, &key, &Map::load(), w).unwrap(); }
fn click(m: &mut Model, w: &mut Wire, button: u32, x: usize, y: usize) {
    let mut key = Key::named("", "");
    key.pointer = Some(Pointer { button, x, y, released: false, motion: false });
    press(m, w, key);
}
fn trash(w: &mut Wire, preset: &str) -> Model {
    let mut m = frame_tests::model();
    (m.preset, m.preview_visible) = (preset.into(), true);
    actions::act(&mut m, "openTrash", w).unwrap();
    m.receive(parse(LISTED).unwrap(), w).unwrap();
    drain(w);
    m
}
fn listed(w: &mut Wire) -> Vec<String> {
    requests(&drain(w), "list").iter().map(|v| text(v, "path").to_owned()).collect()
}

// With the preview focused, a press on the listing is refused like any other Trash press, so no menu row runs there.
#[test]
fn a_listing_press_stays_refused_while_the_preview_has_focus() {
    let (mut w, reader) = echo_wire();
    let mut m = trash(&mut w, "default");
    let (left, middle, _) = render::panes(m.columns, m.preview_visible);
    press(&mut m, &mut w, Key::named("Tab", "ctrl"));
    assert!(m.preview_focus && m.prompts.trash.is_some());
    let hidden = m.hidden;
    for (button, x) in [(2, left + 5), (0, left + 5), (2, left / 2), (0, left + middle)] {
        click(&mut m, &mut w, button, x, render::BODY_ROW);
        assert!(!m.menu, "button {button} at {x} opened the menu in Trash");
    }
    press(&mut m, &mut w, Key::named("Return", ""));
    assert_eq!(m.hidden, hidden, "Show Hidden Files ran in Trash");
    assert!(listed(&mut w).is_empty() && m.prompts.trash.is_some(), "a listing press left Trash");
    click(&mut m, &mut w, 0, left + middle + 10, render::BODY_ROW + 3);
    assert!(m.preview_focus && m.prompts.trash.is_some(), "a press in the preview pane keeps its focus");
    finish(w, reader);
}

// The second key of a sequence is judged with its prefix: yazi's g h goes Home and g d goes to Downloads, never h or d alone.
#[test]
fn trash_judges_a_sequence_whole() {
    let (mut w, reader) = echo_wire();
    let home = std::env::var("HOME").unwrap();
    let mut m = trash(&mut w, "yazi");
    press(&mut m, &mut w, Key::character('g', ""));
    press(&mut m, &mut w, Key::character('h', ""));
    assert_eq!(listed(&mut w), [home.as_str()], "g h went where h alone goes");
    assert!(m.prompts.trash.is_some(), "Trash stays up until the folder answers");
    m.receive(parse(&format!(r#"{{"t":"listed","n":1,"path":"{home}","v":1,"w":true}}"#)).unwrap(), &mut w).unwrap();
    assert!(m.prompts.trash.is_none(), "the listed reply takes Trash off");
    let mut m = trash(&mut w, "yazi");
    press(&mut m, &mut w, Key::character('g', ""));
    press(&mut m, &mut w, Key::character('d', ""));
    assert!(m.error.is_empty(), "g d was refused as d alone: {:?}", m.error);
    assert_eq!(listed(&mut w), [format!("{home}/Downloads")]);
    let mut m = trash(&mut w, "yazi");
    press(&mut m, &mut w, Key::character('d', ""));
    assert_eq!(m.error, m.prompts.i18n.tr("trash.viewOnly"), "d alone still trashes, so it is refused");
    finish(w, reader);
}

// Plain Tab only moves focus, the same move as Ctrl+Tab, so Trash lets it through.
#[test]
fn plain_tab_moves_focus_in_trash() {
    let (mut w, reader) = echo_wire();
    let mut m = trash(&mut w, "default");
    press(&mut m, &mut w, Key::named("Tab", ""));
    assert!(m.preview_focus && m.error.is_empty(), "Tab was refused: {:?}", m.error);
    finish(w, reader);
}

// yazi's g t opens the Trash browser, and its parity row says so.
#[test]
fn yazi_g_t_opens_trash() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.preset = "yazi".into();
    press(&mut m, &mut w, Key::character('g', ""));
    press(&mut m, &mut w, Key::character('t', ""));
    assert!(m.error.is_empty() && m.prompts.trash.is_some(), "g t: {:?}", m.error);
    assert_eq!(requests(&drain(&mut w), "trashbrowse").len(), 1);
    let parity = include_str!("parity.tsv");
    let row = parity.lines().find(|line| line.starts_with("mgr\t[\"g\", \"t\"]")).unwrap();
    assert!(row.contains("\topenTrash\tbound\t"), "{row}");
    finish(w, reader);
}

// A Trash row's kind word comes from the catalog: a folder, a known type's comment, and a file of no known type.
#[test]
fn trash_kind_words_come_from_the_catalog() {
    let (mut w, reader) = echo_wire();
    let m = trash(&mut w, "default");
    let i18n = &m.prompts.i18n;
    assert_ne!(i18n.tr("trash.kindFolder"), "trash.kindFolder", "the English catalog has the folder word");
    assert_ne!(i18n.tr("trash.kindFile"), "trash.kindFile", "the English catalog has the file word");
    assert_eq!(m.rows[&1].kind, i18n.tr("trash.kindFolder"));
    assert_eq!(m.rows[&2].kind, i18n.tr("trash.kindFile"));
    finish(w, reader);
}

// A wheel turn over the Trash listing moves its cursor like it does in a folder.
#[test]
fn wheel_moves_the_trash_cursor() {
    let (mut w, reader) = echo_wire();
    let mut m = trash(&mut w, "default");
    let (left, _, _) = render::panes(m.columns, m.preview_visible);
    click(&mut m, &mut w, 65, left / 2, render::BODY_ROW + 1);
    assert!(m.error.is_empty(), "{:?}", m.error);
    assert_eq!(m.cursor, 1, "wheel down on the Trash listing");
    finish(w, reader);
}
