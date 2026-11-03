// Trash as a place: its rows, header, detail, preview and footer from the backend's list, and every changing key refused.
use super::*;
use crate::jsondoc::parse;
use crate::tui::{actions, render::{self, frame_tests}, wire::{echo_wire, finish}};
use crate::tui::wire::sort_tests::{drain, requests};
use std::time::Duration;

const LISTED: &str = r#"{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":1,"start":0,"rows":[{"uri":"trash:///old-draft.txt","original":"/fx/v/work/old-draft.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"}]}"#;
// A trashbrowse answer for rows start..start+count of a Trash holding total items, named "<prefix><index>.txt".
fn page(id: usize, op: &str, total: usize, start: usize, count: usize, prefix: &str) -> String {
    let rows: Vec<String> = (start..(start + count).min(total)).map(|index| format!(
        r#"{{"uri":"trash:///{prefix}{index}.txt","original":"/fx/v/work/{prefix}{index}.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"}}"#)).collect();
    format!(r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true,"total":{total},"start":{start},"rows":[{}]}}"#, rows.join(","))
}

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
// j moves the Trash cursor to the next row, which needs a second row to move to.
#[test]
fn trash_j_moves_the_cursor_down_to_a_second_row() {
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    drain(&mut w);
    m.receive(parse(&page(1, "list", 2, 0, 2, "a")).unwrap(), &mut w).unwrap();
    m.cursor = 0;
    press(&mut m, &mut w, Key::character('j', ""));
    assert!(m.error.is_empty(), "{:?}", m.error);
    assert_eq!(m.cursor, 1, "j moves to the second row");
    finish(w, reader);
}
// A reload of an open Trash drops the rows it held, so a row past the first window is asked for again.
#[test]
fn trash_reload_asks_again_for_rows_past_the_first_window() {
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    drain(&mut w);
    m.receive(parse(&page(1, "list", 100, 0, 27, "a")).unwrap(), &mut w).unwrap();
    m.cursor = 60;
    window(&mut m, &mut w).unwrap();
    let id = m.prompts.trash.as_ref().unwrap().id;
    m.receive(parse(&page(id, "window", 100, m.top, 27, "a")).unwrap(), &mut w).unwrap();
    assert_eq!(m.rows[&40].name, "a40.txt");
    open(&mut m, &mut w).unwrap();
    let id = m.prompts.trash.as_ref().unwrap().id;
    m.receive(parse(&page(id, "list", 100, 0, 27, "b")).unwrap(), &mut w).unwrap();
    drain(&mut w);
    m.cursor = 40;
    window(&mut m, &mut w).unwrap();
    let sent = drain(&mut w);
    assert_eq!(requests(&sent, "trashbrowse").len(), 1, "row 40 is not held after the reload, so Trash asks for the window");
    let id = m.prompts.trash.as_ref().unwrap().id;
    m.receive(parse(&page(id, "window", 100, m.top, 27, "b")).unwrap(), &mut w).unwrap();
    assert_eq!(m.rows[&40].name, "b40.txt");
    finish(w, reader);
}

// tui-trash-view: a trashed text file previews its first line under the Trash facts and a blank row, read from its backing file in the Trash store.
#[test]
fn tui_fix_1101_trashed_text_previews_its_first_line() {
    let root = crate::backend::testdir::TestDir::new("tui-fix-1101-trash");
    let backing = root.file("old-draft.txt", "first draft\n");
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    m.preview_visible = true;
    let listed = LISTED.replace(r#""icon":"text-plain""#, &format!(r#""icon":"text-plain","content":"{}""#, backing.display()));
    m.receive(parse(&listed).unwrap(), &mut w).unwrap();
    crate::tui::preview::load(&mut m, false);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while m.preview_reader.outstanding(m.preview_generation) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        crate::tui::preview::poll(&mut m);
    }
    crate::tui::preview::layout(&mut m);
    let rows = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
    assert_eq!(rows[5].chars().skip(81).take(30).collect::<String>(), "Original Location · /fx/v/work");
    assert_eq!(rows[6].chars().skip(81).collect::<String>().trim(), "", "a blank row follows the Trash facts");
    assert_eq!(rows[7].chars().skip(81).collect::<String>().trim_end(), "1 first draft");
    assert_eq!(m.current_path(), None, "the backing file never becomes a path actions reach");
    finish(w, reader);
}
// A row whose backing the backend could not review previews nothing, never the text of the row the cursor left.
#[test]
fn tui_fix_1101_unbacked_trash_row_never_shows_the_previous_text() {
    let root = crate::backend::testdir::TestDir::new("tui-fix-1101-trash-unbacked");
    let backing = root.file("old-draft.txt", "first draft\n");
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    m.preview_visible = true;
    let listed = page(1, "list", 2, 0, 2, "b").replace("b0.txt\",\"size", &format!("b0.txt\",\"content\":\"{}\",\"size", backing.display()));
    // Reads the cursor row's preview and answers its name and its first body row.
    let shown = |m: &mut Model| {
        crate::tui::preview::load(m, false);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while m.preview_reader.outstanding(m.preview_generation) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
            crate::tui::preview::poll(m);
        }
        crate::tui::preview::layout(m);
        let rows = frame_tests::grid(&render::frame(m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
        (rows[2].chars().skip(83).take(6).collect::<String>(), rows[7].chars().skip(81).collect::<String>().trim_end().to_owned())
    };
    m.receive(parse(&listed).unwrap(), &mut w).unwrap();
    assert_eq!(shown(&mut m), ("b0.txt".into(), "1 first draft".into()), "b0's text draws at row 7 in this layout");
    press(&mut m, &mut w, Key::character('j', ""));
    assert_eq!(shown(&mut m), ("b1.txt".into(), "".into()), "the text read for b0 stays off b1");
    press(&mut m, &mut w, Key::character('k', ""));
    assert_eq!(shown(&mut m), ("b0.txt".into(), "1 first draft".into()));
    // Trash listed again under an unmoved cursor, its row now unbacked, keeps the old text off too.
    open(&mut m, &mut w).unwrap();
    m.receive(parse(&page(2, "list", 2, 0, 2, "b")).unwrap(), &mut w).unwrap();
    assert!(m.preview_body.is_empty() && m.preview_path.as_os_str().is_empty(), "the relist itself drops b0's text");
    assert_eq!(shown(&mut m), ("b0.txt".into(), "".into()));
    finish(w, reader);
}
// With previews loaded on demand, the load key reads a trashed file's text instead of being refused as a change.
#[test]
fn tui_fix_1101_trash_load_preview_key_reads_the_backing() {
    let root = crate::backend::testdir::TestDir::new("tui-fix-1101-trash-manual");
    let backing = root.file("old-draft.txt", "first draft\n");
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    (m.preview_visible, m.preview_auto) = (true, false);
    let listed = LISTED.replace(r#""icon":"text-plain""#, &format!(r#""icon":"text-plain","content":"{}""#, backing.display()));
    m.receive(parse(&listed).unwrap(), &mut w).unwrap();
    crate::tui::preview::load(&mut m, false);
    assert!(!m.preview_reader.outstanding(m.preview_generation), "on demand, nothing reads before the key");
    press(&mut m, &mut w, Key::character(' ', "ctrl"));
    assert!(m.error.is_empty(), "{}", m.error);
    assert_eq!(m.preview_path, backing, "the load key reads the row's backing file");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while m.preview_reader.outstanding(m.preview_generation) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        crate::tui::preview::poll(&mut m);
    }
    assert_eq!(m.preview_body.lines().next(), Some("first draft"));
    finish(w, reader);
}
// A long trashed file scrolls to its last line, since the Trash head takes five rows where the folder preview's takes three.
#[test]
fn tui_fix_1101_p2_trashed_long_text_scrolls_to_its_last_line() {
    let root = crate::backend::testdir::TestDir::new("tui-fix-1101-trash-long");
    let text: String = (1..=60).map(|n| format!("row {n}\n")).collect();
    let backing = root.file("long.txt", &text);
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = opened(&mut w);
    m.preview_visible = true;
    let listed = LISTED.replace(r#""icon":"text-plain""#, &format!(r#""icon":"text-plain","content":"{}""#, backing.display()));
    m.receive(parse(&listed).unwrap(), &mut w).unwrap();
    crate::tui::preview::load(&mut m, false);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while m.preview_reader.outstanding(m.preview_generation) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        crate::tui::preview::poll(&mut m);
    }
    crate::tui::preview::layout(&mut m);
    m.preview_scroll = crate::tui::pane::preview_scroll_limit(&m);
    crate::tui::preview::layout(&mut m);
    let rows = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
    let shown = |text: &str| rows.iter().any(|line| line.contains(text));
    assert!(shown("60 row 60") && shown("39 row 39") && !shown("38 row 38"), "the scroll stops with the last line on the last body row:\n{}", rows.join("\n"));
    finish(w, reader);
}

// tui-trash-view: only text previews under the Trash facts, so a trashed picture, model or archive draws nothing there.
#[test]
fn tui_fix_1101_trashed_non_text_draws_nothing_under_the_facts() {
    let theme = Theme::from_text("");
    // Each kind and the one picture stage starts for it outside Trash: (placed image, 3D render, thumbnail ask).
    let kinds = [("pic.png", "image-png", (true, false, false)), ("chair.stl", "model-stl", (false, true, false)), ("pack.zip", "application-x-package", (false, false, true))];
    let started = |m: &mut Model, w: &mut Wire| {
        drain(w);
        let mut graphics = crate::tui::graphics::Graphics::new();
        crate::tui::stage::step(m, &mut graphics, &mut crate::tui::stage::Shown::default(), w, (130, 31), false, &mut String::new()).unwrap();
        let started = (m.image_file.is_some() || !graphics.bytes.is_empty(), m.model3d.active(), !requests(&drain(w), "thumb").is_empty());
        m.model3d.stop(w).unwrap();
        started
    };
    // Positive control: outside Trash the same call starts each kind's own picture, so none below is the Trash gate's doing.
    for (name, icon, picture) in kinds {
        let (mut w, reader) = echo_wire();
        let mut m = frame_tests::model();
        m.preview_visible = true;
        let row = m.rows.get_mut(&7).unwrap();
        (row.name, row.icon, row.thumbnail) = (name.into(), icon.into(), true);
        crate::tui::preview::load(&mut m, false);
        assert_eq!(started(&mut m, &mut w), picture, "{name}: outside Trash its picture is started");
        finish(w, reader);
    }
    for (name, icon, _) in kinds {
        let root = crate::backend::testdir::TestDir::new("tui-fix-1101-trash-non-text");
        let backing = root.file(name, "\u{89}PNG\r\n\u{1a}\n");
        let (mut w, reader) = echo_wire();
        let mut m = opened(&mut w);
        m.preview_visible = true;
        let listed = LISTED.replace("old-draft.txt", name).replace(r#""icon":"text-plain""#, &format!(r#""icon":"{icon}","content":"{}""#, backing.display()));
        m.receive(parse(&listed).unwrap(), &mut w).unwrap();
        assert_eq!(content(&m), Some(backing.clone()), "{name}: the row names its backing");
        crate::tui::preview::load(&mut m, false);
        assert_eq!(m.preview_path, backing, "{name}");
        assert!(!m.preview_reader.outstanding(m.preview_generation), "{name}: the kind gate reads nothing");
        assert_eq!(started(&mut m, &mut w), (false, false, false), "{name}: no picture is started in Trash");
        crate::tui::preview::layout(&mut m);
        let rows = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
        let below: Vec<String> = rows[6..29].iter().map(|row| row.chars().skip(81).collect::<String>().trim().to_owned()).collect();
        assert!(below.iter().all(String::is_empty), "{name}: {below:?}");
        finish(w, reader);
    }
}
