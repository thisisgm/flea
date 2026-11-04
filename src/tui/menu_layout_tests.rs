use super::*;
use crate::tui::render::{cards::tests::{at, role, screen, PALETTE}, frame_tests::{entry, model}};
use crate::tui::{input::Key, taildrop::Peer};

fn peer(name: &str) -> Peer { Peer { id: name.into(), label: name.into(), address: format!("100.64.0.{}", name.len()) } }
// The board's menu: opened on notes.txt with its facts answered, Taildrop up with two machines, Dropbox signed out.
pub(crate) fn opened() -> Model {
    let mut m = model();
    m.menu = true;
    m.menu_ready = true;
    m.menu_count = 1;
    m.menu_path = m.path.join("notes.txt");
    m.menu_view.paths = vec![m.menu_path.to_string_lossy().into_owned()];
    m.menu_view.row = 7;
    m.menu_view.formats = vec!["zip".into(), "tar.gz".into()];
    m.menu_view.taildrop = true;
    m.menu_view.dropbox = Some("menu.state.signedOut".into());
    m.menu_view.apps = Some(vec![("org.gnome.TextEditor.desktop".into(), "Text Editor".into(), true)]);
    m.taildrop.peers = vec![peer("east"), peer("macbookair")];
    m
}
fn ids(m: &Model) -> Vec<String> {
    entries(m, &Map::load()).into_iter().map(|row| row.map_or_else(|| "-".into(), |row| row.id)).collect()
}
fn row<'a>(rows: &'a [Option<Entry>], id: &str) -> Option<&'a Entry> { rows.iter().flatten().find(|row| row.id == id) }
// Rows enough that the cursor can sit on the body's last row.
pub(super) fn long(m: &mut Model, count: usize) {
    for index in m.total..count { m.rows.insert(index, entry(&format!("file{index:02}.txt"), false, 10)); }
    m.total = count;
}

#[test]
fn menu_matches_its_board_rows_marks_and_inks() {
    let theme = Theme::from_text(PALETTE);
    let m = opened();
    let (rows, ink, _) = screen(&m, &theme);
    // GM 2026-10-29: the signed-out Dropbox row carries no state word, so the menu is as wide as its longest row, Send with Taildrop.
    let right = 29 + 25 + 1;
    assert_eq!(at(&rows, 10, 29, 27), format!("┌{}┐", "─".repeat(25)));
    assert_eq!(at(&rows, 22, 29, 27), format!("└{}┘", "─".repeat(25)));
    for (y, mark, label, trail) in [(11, OPEN, "Open", ""), (12, WINDOW, "Open With", CHEVRON), (13, PRINTER, "Print…", ""),
        (15, ARCHIVE, "Compress", CHEVRON), (17, TAILSCALE, "Send with Taildrop", CHEVRON), (18, DROPBOX, "Move to Dropbox", ""),
        (20, EYE, "Show Hidden Files", ""), (21, TRASH, "Open Trash", "")] {
        assert_eq!((at(&rows, y, 29, 1), at(&rows, y, right, 1)), ("│".into(), "│".into()), "row {y}");
        assert_eq!(at(&rows, y, 30, 3), format!(" {mark} "), "row {y}");
        assert_eq!(at(&rows, y, 33, label.chars().count()), label);
        let n = trail.chars().count();
        assert_eq!(at(&rows, y, right - 1 - n, n + 1), format!("{trail} "), "row {y} ends one cell inside the frame");
    }
    for y in [14, 16, 19] {
        assert_eq!(at(&rows, y, 30, 25), format!(" {} ", "─".repeat(23)));
        assert_eq!(ink[y][31].0, role(&theme.rule));
    }
    assert_eq!(ink[10][29].0, role(&theme.frame));
    assert!((30..right).all(|x| ink[11][x].1 == role(&theme.tab)), "the cursor washes the row's interior");
    assert_eq!((ink[11][29].1.clone(), ink[11][right].1.clone(), ink[12][40].1.clone()), (role(&theme.background), role(&theme.background), role(&theme.background)), "and draws no edge");
    assert_eq!((ink[11][31].0.clone(), ink[11][33].0.clone(), ink[12][right - 2].0.clone()), (role(&theme.frame), role(&theme.foreground), role(&theme.muted)));
    assert_eq!((ink[18][31].0.clone(), ink[18][33].0.clone()), (role(&theme.unavailable), role(&theme.unavailable)), "an unavailable row is red alone");
}

#[test]
fn plugin_rows_join_the_compress_group_and_each_group_takes_its_rule() {
    let theme = Theme::from_text(PALETTE);
    let mut m = opened();
    m.menu_view.actions = vec![("verify".into(), "Verify Checksum…".into())];
    let (rows, _, _) = screen(&m, &theme);
    assert_eq!(at(&rows, 16, 30, 19), format!(" {HASH} Verify Checksum…"));
    assert_eq!(ids(&m), ["open", "openwith", "print", "-", "compress", "action:verify", "-", "taildrop", "dropbox", "-", "toggleHidden", "openTrash"]);
    m.menu_view.hidden.clear();
    let rows = entries(&m, &Map::load());
    assert_eq!(ids(&m)[9..12], ["-", "delete", "-"], "Delete Permanently has a group of its own when Settings shows it");
    assert_eq!(row(&rows, "delete").unwrap().ink, Ink::Destructive);
    let (_, ink, _) = screen(&m, &theme);
    assert_eq!(ink[21][33].0, role(&theme.error));
    m.menu_view.hidden = vec!["open".into(), "toggleHidden".into(), "print".into(), "compress".into()];
    assert_eq!(ids(&m), ["open", "openwith", "-", "action:verify", "-", "taildrop", "dropbox", "-", "delete", "-", "toggleHidden", "openTrash"]);
}

#[test]
fn taildrop_submenu_titles_its_machines_at_the_parent_width_and_escape_returns() {
    let theme = Theme::from_text(PALETTE);
    let (mut w, reader) = crate::tui::wire::echo_wire();
    let map = Map::load();
    let mut m = opened();
    m.menu_cursor = ids(&m).iter().position(|id| id == "taildrop").unwrap();
    crate::tui::actions::key(&mut m, &Key::named("Right", ""), &map, &mut w).unwrap();
    assert_eq!(m.menu_view.sub, Sub::Taildrop);
    let (rows, ink, _) = screen(&m, &theme);
    assert_eq!(at(&rows, 10, 29, 27), format!("┌{}┐", "─".repeat(25)), "as wide as the menu it came from");
    assert_eq!((at(&rows, 11, 31, 8), ink[11][31].0.clone()), ("TAILDROP".into(), role(&theme.muted)));
    assert_eq!(at(&rows, 12, 30, 7), format!(" {MACHINE} east"));
    assert_eq!(at(&rows, 13, 30, 13), format!(" {MACHINE} macbookair"));
    assert_eq!(at(&rows, 14, 29, 1), "└");
    assert!((30..=54).all(|x| ink[12][x].1 == role(&theme.tab)));
    assert_eq!(hit(&m, &map, 40, 12), Some(None), "the title is no row");
    assert_eq!(hit(&m, &map, 40, 14), Some(Some(1)));
    crate::tui::actions::key(&mut m, &Key::named("Escape", ""), &map, &mut w).unwrap();
    assert!(m.menu && m.menu_view.sub == Sub::Root);
    assert_eq!(ids(&m)[m.menu_cursor], "taildrop", "Escape returns to the row it opened from");
    assert!(crate::tui::action_key_tests::drain(&mut w).is_empty());
    crate::tui::wire::finish(w, reader);
}

#[test]
fn disabled_rows_never_act_and_steps_skip_them() {
    let map = Map::load();
    let (mut w, reader) = crate::tui::wire::echo_wire();
    let mut m = opened();
    m.menu_ready = false;
    let rows = entries(&m, &map);
    for id in ["open", "openwith", "print", "compress", "taildrop", "dropbox"] { assert!(!row(&rows, id).unwrap().enabled(), "{id} waits for the snapshot"); }
    crate::tui::actions::key(&mut m, &Key::named("Return", ""), &map, &mut w).unwrap();
    assert!(m.menu && crate::tui::action_key_tests::drain(&mut w).is_empty(), "Enter on a waiting row does nothing");
    crate::tui::actions::key(&mut m, &Key::named("Down", ""), &map, &mut w).unwrap();
    assert_eq!(ids(&m)[m.menu_cursor], "toggleHidden", "Down skips every waiting row and each rule");
    m.menu_ready = true;
    m.menu_cursor = ids(&m).iter().position(|id| id == "dropbox").unwrap();
    for key in [Key::named("Return", ""), Key::named("Right", "")] { crate::tui::actions::key(&mut m, &key, &map, &mut w).unwrap(); }
    let p = place(&m, &entries(&m, &map));
    let y = p.y + 1 + m.menu_cursor;
    crate::tui::actions::key(&mut m, &crate::tui::render::cards::tests::pointer(0, p.x + 5, y + 1, false), &map, &mut w).unwrap();
    assert!(m.menu && crate::tui::action_key_tests::drain(&mut w).is_empty(), "the signed-out Dropbox row cannot act");
    m.menu_cursor = 0;
    crate::tui::actions::key(&mut m, &crate::tui::render::cards::tests::pointer(0, p.x + 5, y + 1, true), &map, &mut w).unwrap();
    assert_eq!(m.menu_cursor, 0, "hovering an unavailable row leaves the cursor");
    crate::tui::actions::key(&mut m, &crate::tui::render::cards::tests::pointer(0, p.x + 5, p.y + 2, false), &map, &mut w).unwrap();
    let sent = crate::tui::action_key_tests::drain(&mut w);
    assert_eq!(crate::tui::render::cards::tests::sent(&sent, "menuaction", "validate").len(), 1, "a click on Open opens");
    assert!(!m.menu);
    crate::tui::wire::finish(w, reader);
}

#[test]
fn provider_rows_show_only_while_installed_and_say_why_they_cannot_act() {
    let map = Map::load();
    let mut m = opened();
    m.menu_view.taildrop = false;
    m.menu_view.dropbox = None;
    assert!(!ids(&m).iter().any(|id| id == "taildrop" || id == "dropbox"));
    m.menu_view.dropbox = Some("/fx/v".into());
    assert!(!ids(&m).iter().any(|id| id == "dropbox"), "a file already in the account folder has no Move to Dropbox");
    m.menu_view.dropbox = Some("/home/gm/Dropbox".into());
    let rows = entries(&m, &map);
    assert_eq!((row(&rows, "dropbox").unwrap().ink, row(&rows, "dropbox").unwrap().trail.as_str()), (Ink::Plain, ""));
    m.menu_view.taildrop = true;
    let state = |m: &Model| { let rows = entries(m, &map); let row = row(&rows, "taildrop").unwrap().clone(); (row.ink, row.trail, row.chevron) };
    assert_eq!(state(&m), (Ink::Plain, String::new(), true));
    m.taildrop.peers.clear();
    m.taildrop.refresh_locale(&m.i18n);
    for status in ["NeedsLogin", "Running", "Stopped"] {
        m.taildrop.status(&crate::jsondoc::parse(&format!(r#"{{"BackendState":"{status}","Self":{{"Capabilities":["https://tailscale.com/cap/file-sharing"]}}}}"#)).unwrap());
        assert_eq!(state(&m), (Ink::Unavailable, String::new(), false), "{status}");
    }
    m.menu_directory = true;
    assert_eq!(state(&m), (Ink::Unavailable, String::new(), false));
    m.menu_directory = false;
    m.taildrop.peers = vec![peer("east")];
    m.menu_ready = false;
    assert_eq!(state(&m).0, Ink::Dim);
    let mut m = opened();
    let id = m.menu_view.request;
    for (info, error, expected) in [("", "", "menu.state.signedOut"), ("", "Could not read Dropbox account: denied.", "menu.state.notRunning"),
        (r#"{"personal":{"path":"/home/gm/Dropbox/"}}"#, "", "/home/gm/Dropbox"), ("{", "", "menu.state.notRunning")] {
        let reply = format!(r#"{{"t":"formats","id":{id},"archive":["zip"],"providers":{{"taildrop":{{"installed":false}},"dropbox":{{"installed":true}},"dropboxInfo":"{}","dropboxError":"{error}"}}}}"#,
            crate::json::escape(info));
        super::super::menu_run::receive(&mut m, &crate::jsondoc::parse(&reply).unwrap());
        assert_eq!((m.menu_view.dropbox.as_deref(), m.menu_view.taildrop), (Some(expected), false), "{info} {error}");
    }
    let stale = format!(r#"{{"t":"formats","id":{},"archive":[],"providers":{{}}}}"#, id + 1);
    super::super::menu_run::receive(&mut m, &crate::jsondoc::parse(&stale).unwrap());
    assert_eq!(m.menu_view.formats, ["zip"], "an answer for another menu is ignored");
}

#[test]
fn key_hints_are_off_by_default_and_name_plain_keys_when_on() {
    let map = Map::load();
    let mut m = opened();
    assert!(entries(&m, &map).iter().flatten().filter(|row| !row.chevron).all(|row| row.id == "dropbox" || row.trail.is_empty()));
    m.menu_view = View::new(&crate::jsondoc::parse(r#"{"keyHints":true,"menu":{"hidden":[]}}"#).unwrap());
    m.menu_view.formats = vec!["zip".into()];
    let rows = entries(&m, &map);
    for (id, hint) in [("open", "enter"), ("print", ""), ("delete", "shift-delete"), ("toggleHidden", ".")] {
        assert_eq!(row(&rows, id).unwrap().trail, hint, "{id}");
    }
    assert_eq!(row(&rows, "compress").unwrap().trail, "", "a submenu row keeps its chevron");
}

#[test]
fn menu_corners_round_while_hyprland_rounds() {
    let theme = Theme::from_text(PALETTE);
    let m = opened();
    m.rounded.store(true, std::sync::atomic::Ordering::Relaxed);
    let (rows, _, _) = screen(&m, &theme);
    assert_eq!((at(&rows, 10, 29, 1), at(&rows, 10, 55, 1), at(&rows, 22, 29, 1), at(&rows, 22, 55, 1)), ("╭".into(), "╮".into(), "╰".into(), "╯".into()));
}
