use super::{
    action_key_tests::{drain, listing},
    actions, contents,
    contents_fix_tests::names,
    contents_process_tests::{find, until, Root},
    contents_tests::{enter, land, press, receive},
    input::Key,
    keymap::Map,
    model::Model,
    render,
    theme::Theme,
    wire::{self, count, text},
};
use crate::jsondoc::Json;
use flea::ui_logic_sr1::Scope;
use std::time::{Duration, Instant};

#[test]
fn tui_contents_fix2_everywhere_enter_preserves_editor_and_snapshot() {
    for pending in [false, true] {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = listing(&mut wire, "default");
        let gen = enter(&mut m, &mut wire, "needle");
        land(&mut m, &mut wire, gen, 51, &["a", "b"]);
        if pending {
            enter(&mut m, &mut wire, "replacement pending");
            assert!(m.find.waiting);
        }
        let before = m.current_path();
        let matches = m.find.matches.clone();
        let root = Root::new();
        m.prompts.i18n = super::contents_tests::catalog(&root);
        m.find.session.wire.everywhere = true;
        press(&mut m, &mut wire, Key::character('g', "ctrl"));
        press(&mut m, &mut wire, Key::named("Tab", ""));
        press(&mut m, &mut wire, Key::named("Tab", ""));
        assert_eq!(m.find.session.scope, Scope::Everywhere);
        m.editor.as_mut().unwrap().replace("new needle".into());
        drain(&mut wire);
        let result = actions::key(&mut m, &Key::named("Enter", ""), &Map::load(), &mut wire);
        assert!(
            result.is_ok(),
            "unmapped scope must keep the TUI alive: {result:?}"
        );
        assert!(drain(&mut wire).is_empty(), "no guessed Everywhere request");
        assert_eq!(
            m.editor
                .as_ref()
                .expect("Everywhere refusal must retain the editor while pending")
                .value,
            "new needle"
        );
        assert_eq!(m.find.session.scope, Scope::Folder);
        assert_eq!(m.current_path(), before);
        assert_eq!(m.find.matches, matches);
        assert_eq!(m.find.listing, 51);
        assert_eq!(m.find.waiting, pending);
        wire::finish(wire, reader);
    }
}

#[test]
fn tui_contents_fix2_filtered_capacity_scroll_and_window() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    m.resize_body(12);
    let gen = enter(&mut m, &mut wire, "needle");
    land(
        &mut m,
        &mut wire,
        gen,
        51,
        &[
            "needle-a", "needle-b", "needle-c", "needle-d", "needle-e", "needle-f",
        ],
    );
    m.resize_body(6);
    m.apply_filter("".into(), &mut wire).unwrap();
    m.apply_filter("needle".into(), &mut wire).unwrap();
    assert_eq!(m.visible().len(), 3, "pending filter uses file capacity");
    m.resize_body(12);
    let sent = drain(&mut wire);
    let id = count(
        sent.iter().find(|v| text(v, "c") == "filter").unwrap(),
        "id",
    );
    receive(
        &mut m,
        &mut wire,
        &format!(r#"{{"t":"filtered","id":{id},"query":"needle","rows":[0,1,2,3,4,5]}}"#),
    );
    assert_eq!(m.visible().len(), 6);
    m.resize_body(6);
    assert_eq!(m.visible().len(), 3, "answered filter uses file capacity");
    m.cursor = 5;
    m.window(&mut wire).unwrap();
    assert_eq!(
        m.visible(),
        vec![3, 4, 5],
        "sixth file must fit inside the body"
    );
    let frame = render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        120,
        10,
        Duration::ZERO,
    );
    assert!(render::frame_tests::plain(&frame).contains("first needle in needle-f"));
    m.rows.clear();
    drain(&mut wire);
    m.window(&mut wire).unwrap();
    let sent = drain(&mut wire);
    let page = sent.iter().find(|v| text(v, "c") == "window").unwrap();
    assert_eq!(count(page, "start"), 3);
    assert_eq!(count(page, "count"), 3);
    wire::finish(wire, reader);
}

fn settled(m: &mut Model, wire: &mut wire::Wire) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "search deadline expired; last event: {last}"
        );
        let event = wire
            .events
            .recv_timeout(remaining)
            .expect("search event before deadline");
        let value = match event {
            Ok(value) => value,
            Err(error) => {
                last = error;
                continue;
            }
        };
        last = crate::jsondoc::render(&value);
        let done = text(&value, "t") == "searched"
            && !value
                .get("walking")
                .and_then(Json::as_bool)
                .unwrap_or(false);
        m.receive(value, wire).unwrap();
        if done {
            break;
        }
    }
}

#[test]
fn tui_contents_fix2_failed_navigation_restores_real_search_paging() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    for contents in [false, true] {
        let root = Root::new();
        let search = root.0.join("search");
        std::fs::create_dir(&search).unwrap();
        for i in 0..9 {
            std::fs::write(search.join(format!("needle-{i}.txt")), "needle body\n").unwrap();
        }
        eprintln!("FIX2 process {contents}: spawn");
        let mut wire = root.backend(false);
        let mut m = Model::new(root.0.clone(), &Json::Null);
        m.resize_body(4);
        eprintln!("FIX2 process {contents}: initial list");
        m.open(root.0.clone(), &mut wire).unwrap();
        until(&mut m, &mut wire, "rows");
        eprintln!("FIX2 process {contents}: find");
        if contents {
            find(&mut m, &mut wire, "needle").unwrap();
        } else {
            names(&mut m, &mut wire);
        }
        eprintln!("FIX2 process {contents}: wait settled");
        settled(&mut m, &mut wire);
        assert_eq!(m.total, 9);
        let active_gen = m.find.active.as_ref().unwrap().gen;
        let held_path = m.current_path();
        m.open(root.0.join("missing"), &mut wire).unwrap();
        eprintln!("FIX2 process {contents}: wait scan error");
        let error = until(&mut m, &mut wire, "error");
        assert_eq!(text(&error, "where"), "scan");
        assert!(m.pending.is_none());
        assert_eq!(m.current_path(), held_path);
        assert!(
            m.find.waiting,
            "closed backend session must be re-created before paging"
        );
        assert!(contents::guarded(&m));
        m.error.clear();
        eprintln!("FIX2 process {contents}: wait settled");
        settled(&mut m, &mut wire);
        assert!(m.find.active.as_ref().unwrap().gen > active_gen);
        assert_eq!(m.find.active.as_ref().unwrap().contents, contents);
        assert_eq!(m.total, 9);
        m.cursor = 8;
        m.window(&mut wire).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last = String::new();
        while m.current_path().is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "page deadline expired; last event: {last}"
            );
            let event = wire
                .events
                .recv_timeout(remaining)
                .expect("page event before deadline");
            let value = match event {
                Ok(value) => value,
                Err(error) => {
                    last = error;
                    continue;
                }
            };
            last = crate::jsondoc::render(&value);
            m.receive(value, &mut wire).unwrap();
        }
        let path = m.current_path().expect("paged result identity");
        assert!(path.starts_with(&search) && path.is_file(), "{path:?}");
        if contents {
            assert_eq!(m.find.matches[&8].line, "needle body");
        }
        m.open(search.clone(), &mut wire).unwrap();
        until(&mut m, &mut wire, "rows");
        assert!(m.find.active.is_none(), "successful navigation leaves Find");
        assert_eq!(m.path, search);
    }
}
