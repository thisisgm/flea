// The GUI controller over a recording host: actions, keys, reply lines, the exit landing and the stopwatch adapter.
use super::*;
use crate::ipc::Adapter;
use flea::i18n::{I18n, Paths};
use flea::keys::{self, Context, Frontend, Input};
use gpui::{KeyDownEvent, Keystroke, Modifiers};
use std::path::PathBuf;

#[derive(Default)]
struct Fake {
    path: String,
    home: String,
    cursor: Option<String>,
    recent: Option<String>,
    clip: Option<String>,
    now: u64,
    log: Vec<String>,
}

impl Host for Fake {
    fn pane(&self) -> HostPane<'_> {
        HostPane {
            path: &self.path,
            home: &self.home,
            hidden: false,
            count: 40,
            listing: 0,
            sizes: 0,
            cursor_name: self.cursor.as_deref(),
            recent: self.recent.as_deref(),
            now: self.now,
        }
    }
    fn send(&mut self, line: String) {
        self.log.push(line);
    }
    fn leave_recent(&mut self, path: &str) {
        self.log.push(format!("leave-recent {path}"));
    }
    fn reset_landed(&mut self) {
        self.log.push("reset".into());
    }
    fn relist(&mut self, path: &str, select: &str) {
        self.log.push(format!("relist {path} {select}"));
    }
    fn reveal(&mut self, folder: &str, select: &str) {
        self.log.push(format!("reveal {folder} {select}"));
    }
    fn restart_settle(&mut self) {
        self.log.push("settle".into());
    }
    fn clipboard_text(&self) -> Option<String> {
        self.clip.clone()
    }
    fn publish(&mut self, _: crate::pane::swap::Session) {}
}

fn host(path: &str) -> Fake {
    Fake {
        path: path.into(),
        cursor: Some("todo.txt".into()),
        now: 1000,
        ..Fake::default()
    }
}

fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

fn typed(search: &mut Search, host: &mut Fake, text: &str) {
    for ch in text.chars() {
        search.key(StripKey::Text(ch.to_string()), host);
    }
}

#[test]
fn f_opens_the_walk_and_each_key_sends_a_newer_gen() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    assert!(search.action("search", &mut host));
    assert_eq!(
        host.log,
        [
            "reset",
            r#"{"c":"searchopen","path":"/d","folder":"/d","hidden":false}"#
        ]
    );
    assert!(search.typing() && search.active());
    host.log.clear();
    typed(&mut search, &mut host, "no");
    assert_eq!(
        host.log,
        [
            r#"{"c":"searchquery","query":"n","gen":1,"count":40,"sizes":0}"#,
            r#"{"c":"searchquery","query":"no","gen":2,"count":40,"sizes":0}"#,
        ]
    );
    // Any action this unit does not own is left to the window.
    assert!(!search.action("filter", &mut host));
}

#[test]
fn paste_takes_the_clipboard_first_line_and_an_empty_clipboard_sends_nothing() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    search.action("search", &mut host);
    host.log.clear();
    search.key(StripKey::Paste, &mut host);
    assert!(host.log.is_empty(), "no clipboard, no request");
    host.clip = Some("re\tport\nsecond line".into());
    search.key(StripKey::Paste, &mut host);
    assert_eq!(
        host.log,
        [r#"{"c":"searchquery","query":"report","gen":1,"count":40,"sizes":0}"#]
    );
}

#[test]
fn reply_lines_land_only_for_the_newest_gen() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    search.action("search", &mut host);
    typed(&mut search, &mut host, "ab");
    host.log.clear();
    let line = |gen: u64, n: usize| {
        format!(
            r#"{{"t":"searched","n":{n},"scanned":90,"ms":4.5,"cancelled":false,"gen":{gen},"walking":false}}"#
        )
    };
    assert_eq!(search.line(&line(1, 7), &mut host), Ok(true));
    assert_eq!(
        (search.session.total, host.log.len()),
        (0, 0),
        "an older gen drops"
    );
    assert_eq!(search.line(&line(2, 7), &mut host), Ok(true));
    assert_eq!(
        (search.session.total, host.log.as_slice()),
        (7, ["settle".to_owned()].as_slice())
    );
    assert_eq!(
        search.line(r#"{"t":"listed","n":3,"path":"/d","gen":2}"#, &mut host),
        Ok(false)
    );
    assert!(search
        .line(r#"{"t":"searched","n":-1}"#, &mut host)
        .is_err());
}

#[test]
fn esc_cancels_a_walk_then_relists_the_folder_and_waits_for_the_landing() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    search.action("search", &mut host);
    typed(&mut search, &mut host, "nor");
    host.log.clear();
    search.key(StripKey::Escape, &mut host);
    assert_eq!(host.log, [r#"{"c":"searchcancel"}"#]);
    search.line(r#"{"t":"searched","n":12,"scanned":9,"ms":1,"cancelled":true,"gen":3,"walking":false}"#, &mut host).unwrap();
    host.log.clear();
    search.key(StripKey::Escape, &mut host);
    // searchnoblank.js:80, searchnoblank.js:82, searchnoblank.js:83, searchnoblank.js:96
    assert_eq!(
        host.log,
        [r#"{"c":"searchclose"}"#, "relist /d /d/todo.txt"]
    );
    assert!(search.relisting() && search.active() && search.session.leaving);
    host.log.clear();
    // Keys during the exit wait for the folder.
    typed(&mut search, &mut host, "x");
    assert!(host.log.is_empty());
    search.landed(&mut host);
    assert!(!search.active() && !search.relisting());
}

#[test]
fn only_the_exits_own_failed_relist_clears_the_cursor_file() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    search.action("search", &mut host);
    typed(&mut search, &mut host, "b");
    search.line(r#"{"t":"searched","n":1,"scanned":9,"ms":1,"cancelled":false,"gen":1,"walking":false}"#, &mut host).unwrap();
    search.key(StripKey::Escape, &mut host);
    search.listing_failed(&mut host);
    // searchnoblank.js:121
    assert_eq!(search.session.pending_select, "");
    assert!(!search.active(), "a dead exit never leaves the strip stuck");
    let mut later = Search::default();
    later.session.pending_select = "/d/b.txt".into();
    later.listing_failed(&mut host);
    // searchnoblank.js:125, searchnoblank.js:126
    assert_eq!(later.session.pending_select, "/d/b.txt");
    assert!(!later.relisting());
}

#[test]
fn reveal_leaves_the_walk_before_it_opens_the_results_folder() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    search.action("search", &mut host);
    typed(&mut search, &mut host, "t");
    host.cursor = Some("sub/todo.txt".into());
    host.log.clear();
    assert!(search.action("reveal", &mut host));
    assert_eq!(
        host.log,
        [r#"{"c":"searchclose"}"#, "reveal /d/sub /d/sub/todo.txt"]
    );
    host.log.clear();
    host.cursor = None;
    search.action("reveal", &mut host);
    assert!(host.log.is_empty(), "no row, no reveal");
}

#[test]
fn search_stamps_answer_the_newest_gen_and_nothing_else() {
    let (mut search, mut host) = (Search::default(), host("/d"));
    search.action("search", &mut host);
    host.now = 2000;
    typed(&mut search, &mut host, "q");
    search.rows_drawn(1, 2010);
    host.now = 2050;
    search.line(r#"{"t":"searched","n":1,"scanned":9,"ms":1,"cancelled":false,"gen":1,"walking":false}"#, &mut host).unwrap();
    let adapter = SearchAdapter { search: &search };
    let reply = adapter
        .call("searchStamps", &[])
        .expect("searchStamps answers");
    assert_eq!(
        reply.stdout,
        concat!(
            r#"{"gen":1,"keyAt":2000,"firstRowsAt":2010,"settledAt":2050,"query":"q","indexed":9,"total":1,"#,
            r#""walking":false,"cancelled":false,"partial":false,"firstRow":null,"firstName":null,"#,
            "\"presentedFirst\":null,\"presentedSettled\":null,\"coverage\":\"none\"}\n"
        )
    );
    assert!(
        adapter.call("filterStamps", &[]).is_none(),
        "filterStamps belongs to the filter"
    );
}

#[test]
fn the_strip_words_and_status_come_from_the_catalog() {
    let (en, mut search, mut host) = (english(), Search::default(), host("/home/u/d"));
    host.home = "/home/u".into();
    search.action("search", &mut host);
    let words = search.words(&en, "/home/u/d", "/home/u", false);
    assert_eq!(
        (
            words.scope.as_str(),
            words.note.as_str(),
            words.way_out.as_str(),
            words.typing
        ),
        ("in ~", "", "esc returns", true)
    );
    typed(&mut search, &mut host, "zqq");
    search.line(r#"{"t":"searched","n":0,"scanned":9,"ms":1,"cancelled":false,"gen":3,"walking":false}"#, &mut host).unwrap();
    assert_eq!(
        search.status(&en),
        (String::new(), "NOTHING MATCHES zqq".to_owned())
    );
    assert_eq!(
        search.words(&en, "/home/u/d", "/home/u", false).note,
        "0 found"
    );
}

fn event(key: &str, held: Modifiers, key_char: Option<&str>) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            modifiers: held,
            key: key.into(),
            key_char: key_char.map(Into::into),
            ..Keystroke::default()
        },
        is_held: false,
        prefer_character_input: false,
    }
}

#[test]
fn platform_keys_map_onto_the_query_line() {
    let none = Modifiers::default();
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    assert_eq!(
        StripKey::from_event(&event("v", control, None)),
        StripKey::Paste
    );
    assert_eq!(
        StripKey::from_event(&event("insert", shift, None)),
        StripKey::Paste
    );
    assert_eq!(
        StripKey::from_event(&event("g", control, None)),
        StripKey::Contents
    );
    assert_eq!(
        StripKey::from_event(&event("a", shift, Some("A"))),
        StripKey::Text("A".into())
    );
    assert_eq!(
        StripKey::from_event(&event("a", control, Some("a"))),
        StripKey::Other
    );
    assert_eq!(
        StripKey::from_event(&event("enter", none, None)),
        StripKey::Enter
    );
    assert_eq!(
        StripKey::from_event(&event("tab", shift, None)),
        StripKey::Tab
    );
    assert_eq!(
        StripKey::from_event(&event("f1", none, None)),
        StripKey::Other
    );
}

#[test]
fn the_key_table_routes_f_ctrl_f_and_o_to_this_units_actions() {
    let at = |key: &str, text: &str, modifiers: u8| {
        keys::lookup(
            "default",
            Context::Listing,
            Frontend::Gui,
            Input {
                key,
                text,
                modifiers,
            },
        )
        .action()
    };
    assert_eq!(at("f", "f", 0), "search");
    assert_eq!(at("f", "f", keys::CONTROL), "search");
    assert_eq!(at("o", "o", 0), "reveal");
    // Ctrl+G in the listing opens contents search; inside the strip the query line takes it first.
    assert_eq!(at("g", "g", keys::CONTROL), "searchContents");
}

#[test]
fn the_sheet_and_pointer_tables_name_search_and_reveal() {
    let sheet = |action: &str| {
        keys::BASE_SHEET
            .iter()
            .find(|row| row.action == action)
            .map(|row| row.keys)
    };
    assert_eq!((sheet("search"), sheet("reveal")), (Some("f"), Some("o")));
    let pointer = |place: &str, press: &str, row: &str| {
        keys::POINTER
            .iter()
            .find(|binding| binding.place == place && binding.press == press && binding.row == row)
            .map(|binding| binding.effect)
    };
    // A result reveals on one click and on a double click; a neighbour folder reveals on one.
    assert_eq!(pointer("listing", "left", "result"), Some("reveal"));
    assert_eq!(pointer("listing", "left x2", "result"), Some("reveal"));
    assert_eq!(pointer("neighbour", "left", "dir"), Some("reveal"));
    // Only a standing query turns a row activation into a reveal.
    let mut search = Search::default();
    assert!(!search.session.reveals_on_activate());
    search.session.mode = Mode::Results;
    search.session.query = "n".into();
    assert!(search.session.reveals_on_activate());
}
