// The status-row prompts, receipts and paste question against the boards' cells, through the real key and receive paths.
use super::*;
use super::super::input::Key;
#[path = "prompts_find_tests.rs"]
mod find_tests;
#[path = "prompts_collision_tests.rs"]
mod collision_tests;
use crate::backend::testdir::TestDir;
use crate::jsondoc::parse;
use crate::tui::{actions, keymap::{Hints, Map}, render::{self, frame_tests}, wire::{echo_wire, finish}};
use crate::tui::wire::sort_tests::{drain, requests};

// Sample input: "\x1b[38;2;1;2;3mA\x1b[0mB" gives [('A', "\x1b[38;2;1;2;3m", ""), ('B', "", "")].
fn inks(text: &str) -> Vec<(char, String, String)> {
    let (mut fg, mut bg, mut out) = (String::new(), String::new(), Vec::new());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            let (mut args, mut end) = (String::new(), ' ');
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) { end = code; break; }
                args.push(code);
            }
            if end == 'm' && args == "0" { fg.clear(); bg.clear(); }
            else if end == 'm' && args.starts_with("38;") { fg = format!("\x1b[{args}m"); }
            else if end == 'm' && (args.starts_with("48;") || args == "49") { bg = format!("\x1b[{args}m"); }
        } else { out.push((c, fg.clone(), bg.clone())); }
    }
    out
}
// The text at a cell offset, which must keep one foreground and one background across its cells.
fn span(cells: &[(char, String, String)], at: usize, text: &str) -> (String, String) {
    let n = text.chars().count();
    assert_eq!(cells[at..at + n].iter().map(|c| c.0).collect::<String>(), text, "at {at}");
    let (fg, bg) = (cells[at].1.clone(), cells[at].2.clone());
    assert!(cells[at..at + n].iter().all(|c| c.1 == fg && c.2 == bg), "{text:?} keeps one ink");
    (fg, bg)
}
fn row(m: &Model) -> (Vec<(char, String, String)>, Option<usize>) {
    let theme = Theme::from_text("");
    // The frame sets its base ink before the row, as render does.
    let (out, caret) = status(m, &theme, 128, Duration::ZERO);
    let cells = inks(&format!("{}{out}", theme.base()));
    assert_eq!(cells.len(), 128, "the status row fills its columns");
    (cells, caret)
}
fn press(m: &mut Model, w: &mut Wire, key: Key) { actions::key(m, &key, &Map::load(), w).unwrap(); }
fn event(m: &mut Model, w: &mut Wire, line: &str) { m.receive(parse(line).unwrap(), w).unwrap(); }

// tui-filter-typing and tui-filter: the label and count in muted ink while typing, the count in foreground once applied.
#[test]
fn filter_prompt_and_applied_filter_keep_the_board_columns() {
    let theme = Theme::from_text("");
    let mut m = frame_tests::model();
    m.filter = "o".into();
    m.editor = Some(Editor::new("filter", "o".into(), m.path.clone()));
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 0, "Filter"), (theme.muted.clone(), theme.background.clone()));
    assert_eq!(span(&cells, 7, "o"), (theme.foreground.clone(), theme.field.clone()));
    assert_eq!((cells[8].0, &cells[8].2, cells[9].0, &cells[9].2), (' ', &theme.field, ' ', &theme.background), "the field fits o and the caret's cell");
    assert_eq!(span(&cells, 10, "5 of 9 shown").0, theme.muted);
    assert_eq!(caret, Some(8));
    m.editor = None;
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 0, "Filter o").0, theme.muted);
    assert_eq!(span(&cells, 10, "5 of 9 shown").0, theme.foreground);
    assert_eq!(caret, None);
}
// tui-path: Go to with the completion's ghost after the caret in the extension ink.
#[test]
fn go_to_prompt_draws_its_completion_after_the_caret() {
    let theme = Theme::from_text("");
    let mut m = frame_tests::model();
    m.editor = Some(Editor::new("path", "Me".into(), m.path.clone()));
    m.completion = "Media/".into();
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 0, "Go to").0, theme.muted);
    assert_eq!(span(&cells, 6, "Me"), (theme.foreground.clone(), theme.field.clone()));
    assert_eq!(span(&cells, 8, "dia/ "), (theme.ext.clone(), theme.field.clone()));
    assert_eq!(caret, Some(8));
}
// tui-new-folder and tui-rename: the suggested name and the stem open selected, with no caret, and Rename's keys trail the row.
#[test]
fn new_folder_and_rename_open_selected_on_the_status_row() {
    let theme = Theme::from_text("");
    let sandbox = TestDir::new("tui-prompt-rename");
    let path = sandbox.file("notes.txt", "kept");
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.path = sandbox.path().into();
    press(&mut m, &mut w, Key::character('N', "shift"));
    assert_eq!(m.editor.as_ref().map(|e| e.kind), Some("mkdir"), "N opens New Folder");
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 0, "Folder name").0, theme.muted);
    assert_eq!(span(&cells, 12, "New Folder"), (theme.foreground.clone(), theme.stem.clone()));
    assert_eq!(span(&cells, 22, " ").1, theme.field);
    assert_eq!(caret, None);
    m.editor = Some(Editor::rename("notes.txt".into(), path.clone(), false).unwrap());
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 0, "Rename").0, theme.muted);
    assert_eq!(span(&cells, 7, "notes"), (theme.foreground.clone(), theme.stem.clone()));
    assert_eq!(span(&cells, 12, ".txt "), (theme.ext.clone(), theme.field.clone()));
    let hint = "enter saves · esc cancels · ctrl-a selects the whole name";
    assert_eq!(span(&cells, 128 - hint.chars().count(), hint).0, theme.muted, "the keys end at the row's last cell");
    assert_eq!(caret, None);
    let frame = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    assert!(frame.ends_with("\x1b[?25l"), "no caret while the stem is selected");
    let rows = frame_tests::grid(&frame, 130, 31);
    assert_eq!(rows[9].chars().skip(31).take(9).collect::<String>(), "notes.txt", "the listing row keeps its name, with no editor in it");
    assert!(rows[30].starts_with(" Rename notes.txt "));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "kept");
    finish(w, reader);
}
// tui-rename-error: a refused name keeps its field with the caret after the edit, and the one sentence follows it in the error ink.
#[test]
fn a_refused_rename_says_one_sentence_after_its_field() {
    let theme = Theme::from_text("");
    let sandbox = TestDir::new("tui-prompt-refused");
    let path = sandbox.file("notes.txt", "kept");
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.path = sandbox.path().into();
    m.editor = Some(Editor::rename("notes.txt".into(), path.clone(), false).unwrap());
    for key in [Key::named("Left", ""), Key::named("Right", ""), Key::named("Right", ""), Key::character('/', ""), Key::named("End", ""), Key::named("Return", "")] {
        press(&mut m, &mut w, key);
    }
    assert!(requests(&drain(&mut w), "rename").is_empty(), "a refused name is never sent");
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 7, "no/tes"), (theme.foreground.clone(), theme.field.clone()));
    assert_eq!(span(&cells, 13, ".txt ").0, theme.ext);
    assert_eq!(span(&cells, 19, "A name cannot contain /.").0, theme.error);
    assert_eq!(caret, Some(13));
    assert!(m.error.is_empty(), "the field's sentence is the only one");
    let frame = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    assert!(frame.ends_with("\x1b[31;15H\x1b[6 q\x1b[?25h"), "the terminal's bar caret sits after no/tes");
    // A refusal the backend sends lands in the field too, never also in the footer's error.
    m.editor.as_mut().unwrap().replace("taken.txt".into());
    m.editor.as_mut().unwrap().pending = true;
    event(&mut m, &mut w, r#"{"t":"error","where":"rename","path":"/x","msg":"An item named taken.txt already exists."}"#);
    assert_eq!(m.editor.as_ref().map(|e| (e.pending, e.error.as_str())), Some((false, "An item named taken.txt already exists.")));
    assert!(m.error.is_empty(), "one error sentence, not two");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "kept");
    finish(w, reader);
}
// tui-filter-nomatch: the search mark over NOTHING MATCHES, and no preview of a row the filter does not draw, before and after the answer.
#[test]
fn a_filter_that_matches_nothing_clears_the_preview() {
    let (mut w, reader) = echo_wire();
    let theme = Theme::from_text("");
    let mut m = frame_tests::model();
    m.preview_visible = true;
    let before = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
    assert!(before[2].contains("notes.txt") && before[2].chars().skip(81).collect::<String>().contains("notes.txt"), "the fixture previews notes.txt");
    m.apply_filter("zzz".into(), &mut w).unwrap();
    for answered in [false, true] {
        if answered { event(&mut m, &mut w, r#"{"t":"filtered","id":1,"query":"zzz","n":9,"rows":[]}"#); }
        let rows = frame_tests::grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
        assert_eq!(rows[14].chars().nth(53), Some('\u{ea6d}'), "answered {answered}");
        assert_eq!(rows[15].chars().skip(44).take(19).collect::<String>(), "NOTHING MATCHES zzz");
        for (y, row) in rows.iter().enumerate().take(29).skip(2) { assert_eq!(row.chars().skip(81).collect::<String>().trim(), "", "answered {answered}: preview row {y}"); }
        assert!(rows[30].starts_with(" Filter zzz  0 of 9 shown"));
    }
    finish(w, reader);
}
// tui-trash-arm, tui-trashed and tui-undone: the arm's sentence, the receipt with its muted undo key, the kept scroll, and the cursor on what came back.
#[test]
fn trash_keeps_the_scroll_and_its_undo_lands_on_the_restored_file() {
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = Model::new("/listing".into(), &Json::Null);
    m.hints = Hints::new(&Map::load(), "default");
    m.height = 10;
    m.open(m.path.clone(), &mut w).unwrap();
    event(&mut m, &mut w, r#"{"t":"listed","n":40,"path":"/listing"}"#);
    let rows: Vec<String> = (0..40).map(|i| format!(r#"{{"n":"f{i:02}.txt","p":33188}}"#)).collect();
    event(&mut m, &mut w, &format!(r#"{{"t":"rows","start":0,"rows":[{}]}}"#, rows.join(",")));
    (m.top, m.cursor) = (10, 15);
    drain(&mut w);
    press(&mut m, &mut w, Key::character('d', ""));
    assert_eq!(m.message, "Press d again to move it to Trash, or press Delete on its own.");
    press(&mut m, &mut w, Key::character('d', ""));
    let values = drain(&mut w);
    let sent = requests(&values, "trash");
    assert_eq!(sent.len(), 1);
    event(&mut m, &mut w, &format!(r#"{{"t":"trashed","id":{},"ok":1,"failed":0,"moved":["/listing/f15.txt"]}}"#, count(sent[0], "id")));
    assert_eq!(m.message, "Moved 1 item to Trash · z undoes");
    let footer = inks(&crate::tui::layout::footer(&m, &theme, 128, Duration::ZERO));
    assert_eq!(span(&footer, 10, "Moved 1 item to Trash").0, theme.foreground);
    assert_eq!(span(&footer, 32, "· z undoes").0, theme.muted);
    m.expire_message(m.message_at + Duration::from_secs(7));
    assert_eq!(m.message, "Moved 1 item to Trash · z undoes", "a receipt that names its undo stays for the error time");
    // The watcher's change relists the folder; the scroll and the cursor's index stay, and the gone file's locate clears.
    event(&mut m, &mut w, r#"{"t":"changed","path":"/listing"}"#);
    assert_eq!(requests(&drain(&mut w), "list").len(), 1);
    event(&mut m, &mut w, r#"{"t":"listed","n":39,"path":"/listing"}"#);
    assert_eq!((m.top, m.cursor), (10, 15), "the relist keeps the scroll");
    event(&mut m, &mut w, r#"{"t":"located","directory":"/listing","path":"/listing/f15.txt","index":-1}"#);
    assert_eq!(m.restore_path, None);
    drain(&mut w);
    event(&mut m, &mut w, r#"{"t":"undone","op":"trash","ok":true}"#);
    assert_eq!(m.message, "Undid Move to Trash");
    assert_eq!(m.restore_path.as_deref(), Some(std::path::Path::new("/listing/f15.txt")));
    event(&mut m, &mut w, r#"{"t":"listed","n":40,"path":"/listing"}"#);
    let sent = drain(&mut w);
    assert_eq!(requests(&sent, "locate").iter().map(|v| text(v, "path")).collect::<Vec<_>>(), ["/listing/f15.txt"]);
    // An undo of a step this client did not take names it and anchors nothing.
    event(&mut m, &mut w, r#"{"t":"undone","op":"rename","ok":true}"#);
    assert_eq!(m.message, "Undid Rename");
    finish(w, reader);
}
// A selection trashes as "them"; a trash that failed says so in one sentence and claims no receipt.
#[test]
fn trash_of_many_says_them_and_a_failure_is_one_sentence() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.selected.extend([7, 8]);
    assert_eq!(trash_arm(&m, "d"), "Press d again to move them to Trash, or press Delete on its own.");
    m.hints.trash.clear();
    assert_eq!(trash_arm(&m, "d"), "Press d again to move them to Trash.");
    event(&mut m, &mut w, r#"{"t":"trashed","ok":0,"failed":1,"err":"permission denied"}"#);
    assert_eq!(m.error, "1 item could not be moved to Trash: permission denied.");
    assert!(m.message.is_empty());
    finish(w, reader);
}
// tui-copied and the transfer receipts: the clipboard's count, then what a paste copied, skipped and whether it was cancelled.
#[test]
fn copy_and_paste_receipts_count_items() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.pending_clipboard = true;
    event(&mut m, &mut w, r#"{"t":"paths","paths":["/fx/v/work/notes.txt"]}"#);
    assert_eq!(m.message, "Copied 1 item");
    (m.pending_clipboard, m.cut) = (true, true);
    event(&mut m, &mut w, r#"{"t":"paths","paths":["/a","/b"]}"#);
    assert_eq!(m.message, "Cut 2 items");
    m.transfer_id = 7;
    event(&mut m, &mut w, r#"{"t":"transferdone","id":7,"ok":1204,"failed":0,"skipped":1,"cancelled":true}"#);
    assert_eq!(m.message, "Copied 1,204 items · 1 skipped · cancelled · z undoes");
    finish(w, reader);
}
// tui-progress: Copying 2 of 5, then the 20-cell bar and its percent in the metadata ink.
#[test]
fn transfer_progress_draws_the_twenty_cell_bar() {
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    event(&mut m, &mut w, r#"{"t":"transferstarted","id":3,"n":5,"moving":false}"#);
    event(&mut m, &mut w, r#"{"t":"transferprogress","id":3,"index":1,"bytes":100,"total":100}"#);
    let cells = inks(&crate::tui::layout::footer(&m, &theme, 128, Duration::ZERO));
    span(&cells, 0, "9 items");
    span(&cells, 9, "Copying 2 of 5");
    assert_eq!(span(&cells, 25, "━━━━━━━━").0, theme.accent);
    assert_eq!(span(&cells, 33, "────────────").0, theme.border);
    assert_eq!(span(&cells, 47, "40%").0, theme.meta);
    finish(w, reader);
}
// tui-collision: Paste asks first; Enter or b keeps both, r replaces, s skips and Escape sends nothing.
#[test]
fn paste_asks_and_each_answer_sends_its_choice() {
    let theme = Theme::from_text("");
    for (key, choice) in [(Key::named("Return", ""), Some("keep")), (Key::character('b', ""), Some("keep")), (Key::character('r', ""), Some("replace")),
        (Key::character('s', ""), Some("skip")), (Key::named("Escape", ""), None)] {
        let (mut w, reader) = echo_wire();
        let mut m = frame_tests::model();
        m.path = "/fx/v/work/Docs".into();
        m.clipboard = vec!["/fx/v/work/notes.txt".into()];
        press(&mut m, &mut w, Key::character('p', ""));
        let sent = drain(&mut w);
        let asked = requests(&sent, "collisions");
        assert_eq!((asked.len(), text(asked[0], "dest")), (1, "/fx/v/work/Docs"), "paste asks before it writes");
        assert!(requests(&sent, "transfer").is_empty());
        let id = count(asked[0], "id");
        event(&mut m, &mut w, &format!(r#"{{"t":"collisions","id":{id},"total":1,"names":[{{"n":"notes.txt","d":false,"i":"text-plain"}}],"mergeable":false}}"#));
        let (cells, caret) = row(&m);
        assert_eq!(span(&cells, 0, "notes.txt already exists in Docs.").0, theme.foreground);
        assert_eq!(span(&cells, 35, "enter or b keeps both · r replaces · s skips · esc cancels").0, theme.muted);
        assert_eq!(caret, None);
        assert!(!crate::tui::layout::focused(&m), "the cursor row greys behind the question");
        press(&mut m, &mut w, Key::character('x', ""));
        assert!(m.prompts.question.is_some(), "another key leaves the question standing");
        press(&mut m, &mut w, key);
        let sent = drain(&mut w);
        let transfers = requests(&sent, "transfer");
        match choice {
            Some(choice) => {
                assert_eq!(transfers.len(), 1);
                assert_eq!((text(transfers[0], "collide"), count(transfers[0], "collideId"), text(transfers[0], "op")), (choice, id, "copy"));
            }
            None => assert!(transfers.is_empty(), "Escape cancels"),
        }
        assert!(m.prompts.question.is_none());
        finish(w, reader);
    }
}
// Nothing collides: the transfer still refuses a name that appears meanwhile; a FAT drive's answer asks again under drive-safe names; a second paste waits.
#[test]
fn paste_without_collisions_refuses_late_names_and_a_second_paste_waits() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.clipboard = vec!["/elsewhere/a.txt".into()];
    press(&mut m, &mut w, Key::character('p', ""));
    let id = count(requests(&drain(&mut w), "collisions")[0], "id");
    press(&mut m, &mut w, Key::character('p', ""));
    assert_eq!(m.error, "A transfer is already waiting on its question; try again once it is answered.");
    assert!(requests(&drain(&mut w), "collisions").is_empty());
    event(&mut m, &mut w, &format!(r#"{{"t":"fatnames","id":{id},"fstype":"vfat","volume":"USB","renames":[]}}"#));
    let again = drain(&mut w);
    assert_eq!(text(requests(&again, "collisions")[0], "fat"), "rename");
    event(&mut m, &mut w, &format!(r#"{{"t":"collisions","id":{},"total":0,"names":[],"mergeable":false}}"#, id + 99));
    assert!(requests(&drain(&mut w), "transfer").is_empty(), "an answer to another question is dropped");
    event(&mut m, &mut w, &format!(r#"{{"t":"collisions","id":{id},"total":0,"names":[],"mergeable":false}}"#));
    let sent = drain(&mut w);
    let transfer = requests(&sent, "transfer")[0];
    assert_eq!((text(transfer, "collide"), text(transfer, "fat"), count(transfer, "collideId")), ("refuse", "rename", id));
    finish(w, reader);
}
// The undo key's receipt stays for the error time and then clears; any other message goes after four seconds.
#[test]
fn only_a_receipt_that_names_its_undo_outlasts_the_message_time() {
    let mut m = frame_tests::model();
    m.say("Copied 1 item".into());
    m.expire_message(m.message_at + Duration::from_secs(4));
    assert!(m.message.is_empty());
    m.say(undoable(&m, "Renamed".into()));
    m.expire_message(m.message_at + Duration::from_secs(7));
    assert_eq!(m.message, "Renamed · z undoes");
    m.expire_message(m.message_at + Duration::from_secs(8));
    assert!(m.message.is_empty(), "the receipt clears on its own");
    m.hints.undo.clear();
    m.say(undoable(&m, "Renamed".into()));
    m.expire_message(m.message_at + Duration::from_secs(4));
    assert!(m.message.is_empty(), "with no undo key the receipt is an ordinary message");
}
// Two trashes in flight, answered out of order: each receipt undoes the paths that its own request moved.
#[test]
fn two_trashes_in_flight_each_receipt_undoes_its_own_paths() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.cursor = 7;
    trash(&mut m, &mut w).unwrap();
    m.cursor = 8;
    trash(&mut m, &mut w).unwrap();
    let values = drain(&mut w);
    let sent = requests(&values, "trash");
    let (first, second) = (count(sent[0], "id"), count(sent[1], "id"));
    event(&mut m, &mut w, &format!(r#"{{"t":"trashed","id":{second},"ok":1,"failed":0,"moved":["/fx/v/work/todo.md"]}}"#));
    event(&mut m, &mut w, &format!(r#"{{"t":"trashed","id":{first},"ok":1,"failed":0,"moved":["/fx/v/work/notes.txt"]}}"#));
    event(&mut m, &mut w, r#"{"t":"undone","op":"trash","ok":true}"#);
    assert_eq!(m.restore_path.as_deref(), Some(std::path::Path::new("/fx/v/work/notes.txt")), "the answer recorded last undoes first");
    event(&mut m, &mut w, r#"{"t":"undone","op":"trash","ok":true}"#);
    assert_eq!(m.restore_path.as_deref(), Some(std::path::Path::new("/fx/v/work/todo.md")), "the other answer undoes to its own item");
    finish(w, reader);
}
// A partly failed trash: the undo anchor is an item that moved, never the first requested item that failed.
#[test]
fn undo_of_a_partly_failed_trash_lands_on_the_item_that_moved() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.selected.extend([7, 8]);
    trash(&mut m, &mut w).unwrap();
    let id = count(requests(&drain(&mut w), "trash")[0], "id");
    event(&mut m, &mut w, &format!(r#"{{"t":"trashed","id":{id},"ok":1,"failed":1,"err":"permission denied","moved":["/fx/v/work/todo.md"]}}"#));
    event(&mut m, &mut w, r#"{"t":"undone","op":"trash","ok":true}"#);
    assert_eq!(m.restore_path.as_deref(), Some(std::path::Path::new("/fx/v/work/todo.md")), "the undo lands on the item that moved");
    finish(w, reader);
}
