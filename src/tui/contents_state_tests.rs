use super::{
    action_key_tests::listing,
    contents,
    contents_tests::*,
    input::Key,
    keymap::Map,
    render::{self, frame_tests::plain},
    theme::Theme,
    wire::{self, count, text},
};
use std::time::Duration;

#[test]
fn tui_contents_replacement_is_coherent_and_scope_switch_is_generation_safe() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let first = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, first, 51, &["old.txt"]);
    let before = plain(&render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        120,
        15,
        Duration::ZERO,
    ));
    let second = enter(&mut m, &mut w, "needle");
    metadata(&mut m, &mut w, second, 52, 0, &["new.txt"]);
    listed(&mut m, &mut w, second, 52, 1);
    assert_eq!(m.rows[&0].name, "old.txt");
    assert_eq!(m.find.matches[&0].path, "old.txt");
    let held = plain(&render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        120,
        15,
        Duration::ZERO,
    ));
    assert!(before.contains("first needle in old.txt") && held.contains("first needle in old.txt"));
    rows(&mut m, &mut w, first, 51, 0, &["stale.txt"]);
    assert_eq!(m.rows[&0].name, "old.txt");
    rows(&mut m, &mut w, second, 52, 0, &["new.txt"]);
    assert_eq!(m.rows[&0].name, "new.txt");
    assert_eq!(m.find.matches[&0].path, "new.txt");
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{second},"n":1,"scanned":1,"ms":1.0}}"#),
    );
    press(&mut m, &mut w, Key::character('g', "ctrl"));
    // Tab offers Home only where Home searches another folder, so this Find runs from a folder inside Home.
    let home = std::env::var("HOME").unwrap();
    m.editor.as_mut().unwrap().path = std::path::Path::new(&home).join("listing");
    press(&mut m, &mut w, Key::named("Tab", ""));
    assert_eq!(
        m.find.session.scope,
        flea::ui_logic_file_search::Scope::Home
    );
    m.editor.as_mut().unwrap().replace("other".into());
    press(&mut m, &mut w, Key::named("Enter", ""));
    let sent = super::action_key_tests::drain(&mut w);
    let ask = sent
        .iter()
        .find(|v| text(v, "c") == "searchcontents")
        .unwrap();
    assert_eq!(text(ask, "path"), home);
    assert!(count(ask, "gen") > second);
    receive(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"error","where":"searchcontents","gen":{second},"msg":"stale failure"}}"#
        ),
    );
    assert!(m.error.is_empty());
    assert!(m.find.waiting);
    let failed = count(ask, "gen");
    receive(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"error","where":"searchcontents","gen":{failed},"msg":"scope unavailable"}}"#
        ),
    );
    assert!(
        m.find.session.scope == flea::ui_logic_file_search::Scope::Folder,
        "failed Home request keeps the held Here scope"
    );
    assert_eq!(m.search_query, "needle");
    wire::finish(w, reader);
}
#[test]
fn tui_contents_window_identity_is_sent_back_for_file_operations() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 51, &["safe.txt"]);
    super::actions::act(&mut m, "copy", &mut w).unwrap();
    let sent = super::action_key_tests::drain(&mut w);
    let request = sent.iter().find(|v| text(v, "c") == "paths").unwrap();
    assert_eq!(count(request, "listing"), 51);
    wire::finish(w, reader);
}
#[test]
fn tui_contents_toggle_to_names_uses_shared_ranked_session() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    press(&mut m, &mut w, Key::character('g', "ctrl"));
    press(&mut m, &mut w, Key::character('g', "ctrl"));
    m.editor.as_mut().unwrap().replace("notes".into());
    press(&mut m, &mut w, Key::named("Down", ""));
    let sent = super::action_key_tests::drain(&mut w);
    assert!(sent.iter().any(|v| text(v, "c") == "searchopen"));
    let ask = sent.iter().find(|v| text(v, "c") == "searchquery").unwrap();
    let gen = count(ask, "gen");
    receive(
        &mut m,
        &mut w,
        &crate::backend::proto::listed_gen_line(2, 0.0, 0.0, 0, "/listing", true, gen),
    );
    rows(&mut m, &mut w, gen, 0, 0, &["recent.txt", "near.txt"]);
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":2,"scanned":7,"ms":1.0}}"#),
    );
    assert_eq!(contents::row_height(&m), 1);
    assert_eq!(
        m.rows[&0].name, "recent.txt",
        "producer ranking remains authoritative"
    );
    wire::finish(w, reader);
}
#[test]
fn tui_contents_cancel_before_rows_rejects_late_completion() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    press(&mut m, &mut w, Key::named("Escape", ""));
    metadata(&mut m, &mut w, gen, 51, 0, &["late.txt"]);
    listed(&mut m, &mut w, gen, 51, 1);
    rows(&mut m, &mut w, gen, 51, 0, &["late.txt"]);
    assert_eq!(m.rows[&0].name, "folder");
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":0,"scanned":0,"ms":0.0,"cancelled":true}}"#),
    );
    assert_eq!(m.total, 2);
    assert!(m.search.is_empty());
    wire::finish(w, reader);
}

#[test]
fn tui_contents_zero_results_and_bad_pairs_never_show_old_snippets() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    metadata(&mut m, &mut w, gen, 51, 0, &["wrong.txt"]);
    listed(&mut m, &mut w, gen, 51, 1);
    rows(&mut m, &mut w, gen, 51, 0, &["another.txt"]);
    assert_eq!(m.rows[&0].name, "folder");
    assert!(!m.error.is_empty());
    m.error.clear();
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 52, &[]);
    let out = plain(&render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        120,
        15,
        Duration::ZERO,
    ));
    assert!(out.contains("NOTHING MATCHES needle"));
    assert!(!out.contains("first needle"));
    assert_eq!(m.total, 0);
    wire::finish(w, reader);
}
#[test]
fn tui_contents_prompt_strings_use_the_catalog_delta_overlay() {
    let root = super::contents_process_tests::Root::new();
    let catalog = root.0.join("catalog/en");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(catalog.join("flea.json"),r#"{"tui.find.scope_hint":"%1 · tab changes scope","tui.find.status":"Find “%1” %2","tui.find.invalid_response":"The search response could not be used safely. Try Find again."}"#).unwrap();
    let paths = flea::i18n::Paths {
        omarchy: root.0.join("omarchy"),
        language_file: root.0.join("language"),
        catalogs: vec![
            root.0.join("catalog"),
            crate::i18n::test_catalogs::english_only(),
        ],
    };
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    m.prompts.i18n = flea::i18n::I18n::load(paths);
    press(&mut m, &mut w, Key::character('g', "ctrl"));
    // /listing is outside Home, so only a wired Everywhere gives Tab another scope to offer.
    m.find.session.wire.everywhere = true;
    let prompt = super::prompts::status(&m, &Theme::from_text(""), 120, Duration::ZERO).0;
    assert!(plain(&prompt).contains("contents in /listing · tab changes scope"));
    m.find.session.wire.everywhere = false;
    m.editor.as_mut().unwrap().replace("needle".into());
    press(&mut m, &mut w, Key::named("Enter", ""));
    let ask = super::action_key_tests::drain(&mut w);
    let gen = count(
        ask.iter()
            .find(|v| text(v, "c") == "searchcontents")
            .unwrap(),
        "gen",
    );
    land(&mut m, &mut w, gen, 51, &["notes.txt"]);
    let footer = super::layout::footer(&m, &Theme::from_text(""), 120, Duration::ZERO);
    assert!(plain(&footer).contains("Find “needle” contents in /listing"));
    wire::finish(w, reader);
}
#[test]
fn tui_contents_snippet_uses_bold_in_cursor_and_wash_only_at_rest() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 51, &["one.txt", "two.txt"]);
    let theme = Theme::from_text("");
    let frame = render::frame(&m, &theme, &Map::load(), 120, 15, Duration::ZERO);
    assert!(frame.contains(&format!("\x1b[1mneedle\x1b[0m{}", theme.cursor)));
    assert!(frame.contains(&format!(
        "{}needle\x1b[0m{}",
        theme.match_fill, theme.background
    )));
    wire::finish(w, reader);
}

#[test]
fn tui_contents_cancel_racing_completed_producer_restores_navigation() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    press(&mut m, &mut w, Key::named("Escape", ""));
    metadata(&mut m, &mut w, gen, 51, 0, &["late.txt"]);
    listed(&mut m, &mut w, gen, 51, 1);
    rows(&mut m, &mut w, gen, 51, 0, &["late.txt"]);
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":1,"scanned":1,"ms":1.0}}"#),
    );
    let sent = super::action_key_tests::drain(&mut w);
    assert!(sent.iter().any(|v| text(v, "c") == "searchclose"));
    assert!(sent.iter().any(|v| text(v, "c") == "list"));
    assert_eq!(m.pending.as_deref(), Some(std::path::Path::new("/listing")));
    receive(&mut m, &mut w, r#"{"t":"listed","path":"/listing","n":2}"#);
    rows(&mut m, &mut w, 0, 52, 0, &["folder", "file"]);
    assert!(!m.find.cancelling && !m.searching);
    assert!(m.search.is_empty());
    wire::finish(w, reader);
}

#[test]
fn tui_contents_preserves_shared_producer_order() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    m.height = 4;
    let gen = enter(&mut m, &mut w, "needle");
    metadata(&mut m, &mut w, gen, 51, 0, &["A.txt", "b.txt"]);
    listed(&mut m, &mut w, gen, 51, 3);
    rows(&mut m, &mut w, gen, 51, 0, &["A.txt", "b.txt"]);
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":3,"scanned":3,"ms":1.0}}"#),
    );
    assert_eq!(
        m.rows
            .values()
            .map(|row| row.name.as_bytes())
            .collect::<Vec<_>>(),
        vec![b"A.txt".as_slice(), b"b.txt".as_slice()]
    );
    m.cursor = 2;
    m.window(&mut w).unwrap();
    metadata(&mut m, &mut w, gen, 51, 1, &["b.txt", "é.txt"]);
    rows(&mut m, &mut w, gen, 51, 1, &["b.txt", "é.txt"]);
    assert_eq!(
        m.rows
            .values()
            .map(|row| row.name.as_bytes())
            .collect::<Vec<_>>(),
        vec![b"b.txt".as_slice(), "é.txt".as_bytes()]
    );
    let replacement = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, replacement, 52, &["B.txt", "z.txt"]);
    assert_eq!(
        m.rows
            .values()
            .map(|row| row.name.as_bytes())
            .collect::<Vec<_>>(),
        vec![b"B.txt".as_slice(), b"z.txt".as_slice()]
    );
    wire::finish(w, reader);
}

#[test]
fn tui_contents_here_scope_matches_the_header_home_abbreviation() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    m.path = home.join("Docs");
    let catalog = super::contents_process_tests::Root::new();
    m.prompts.i18n = super::contents_tests::catalog(&catalog);
    m.find.session.scope = flea::ui_logic_file_search::Scope::Folder;
    m.editor = Some(super::editor::Editor::new(
        "search",
        String::new(),
        m.path.clone(),
    ));
    let prompt = plain(&super::prompts::status(&m, &Theme::from_text(""), 120, Duration::ZERO).0);
    assert!(prompt.contains(&format!("in {}", super::layout::display_path(&m))));
    assert!(prompt.contains("in ~/Docs"));
    wire::finish(w, reader);
}

#[test]
fn tui_contents_odd_height_keeps_the_full_names_page_after_toggle() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    m.resize_body(5);
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 51, &["a.txt", "b.txt"]);
    assert_eq!(m.height, 5);
    press(&mut m, &mut w, Key::character('g', "ctrl"));
    press(&mut m, &mut w, Key::character('g', "ctrl"));
    m.editor.as_mut().unwrap().replace("a".into());
    press(&mut m, &mut w, Key::named("Enter", ""));
    let sent = super::action_key_tests::drain(&mut w);
    let query = sent.iter().find(|v| text(v, "c") == "searchquery").unwrap();
    assert_eq!(count(query, "count"), 5);
    wire::finish(w, reader);
}

#[path = "contents_message_tests.rs"]
mod message;
