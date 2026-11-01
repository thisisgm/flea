use super::{
    action_key_tests::{drain, listing},
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    render::{self, frame_tests::plain},
    theme::Theme,
    wire::{self, count, text},
};
use crate::jsondoc::{self, Json};
use flea::ui_logic_sr1::{ContentsMatch, ContentsMatches};
use std::time::Duration;

pub(super) fn press(m: &mut Model, w: &mut wire::Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
pub(super) fn enter(m: &mut Model, w: &mut wire::Wire, query: &str) -> usize {
    let keys = super::input::Decoder::default().feed(b"\x07", false);
    assert_eq!(keys.len(), 1, "terminal Ctrl+G emits one key");
    press(m, w, keys[0].clone());
    m.editor
        .as_mut()
        .expect("Ctrl+G must open Find")
        .replace(query.into());
    press(m, w, Key::named("Enter", ""));
    let sent = drain(w);
    let ask = sent
        .iter()
        .find(|v| text(v, "c") == "searchcontents")
        .expect("shared contents request");
    count(ask, "gen")
}
pub(super) fn receive(m: &mut Model, w: &mut wire::Wire, line: &str) {
    m.receive(jsondoc::parse(line).unwrap(), w).unwrap();
}
pub(super) fn metadata(
    m: &mut Model,
    w: &mut wire::Wire,
    gen: usize,
    listing: usize,
    start: usize,
    names: &[&str],
) {
    let line = ContentsMatches {
        gen: gen as u64,
        listing: listing as u64,
        start,
        matches: names
            .iter()
            .enumerate()
            .map(|(i, name)| ContentsMatch {
                row: start + i,
                path: name.to_string(),
                path_bytes: name.as_bytes().to_vec(),
                line_no: 12 + i,
                line: format!("first needle in {name}"),
                operable: true,
            })
            .collect(),
    }
    .line();
    receive(m, w, &line);
}
pub(super) fn listed(m: &mut Model, w: &mut wire::Wire, gen: usize, listing: usize, total: usize) {
    receive(
        m,
        w,
        &format!(
            r#"{{"t":"listed","gen":{gen},"listing":{listing},"path":"/listing","n":{total}}}"#
        ),
    );
}
pub(super) fn rows(
    m: &mut Model,
    w: &mut wire::Wire,
    gen: usize,
    listing: usize,
    start: usize,
    names: &[&str],
) {
    let values: Vec<_> = names
        .iter()
        .map(|name| {
            Json::Obj(vec![
                ("n".into(), wire::word(name)),
                ("p".into(), wire::number(420)),
            ])
        })
        .collect();
    m.receive(
        Json::Obj(vec![
            ("t".into(), wire::word("rows")),
            ("gen".into(), wire::number(gen)),
            ("listing".into(), wire::number(listing)),
            ("start".into(), wire::number(start)),
            ("rows".into(), Json::Arr(values)),
        ]),
        w,
    )
    .unwrap();
}
pub(super) fn land(m: &mut Model, w: &mut wire::Wire, gen: usize, listing: usize, names: &[&str]) {
    metadata(m, w, gen, listing, 0, names);
    listed(m, w, gen, listing, names.len());
    rows(m, w, gen, listing, 0, names);
    receive(
        m,
        w,
        &format!(
            r#"{{"t":"searched","gen":{gen},"n":{},"scanned":8,"ms":1.0,"walking":false}}"#,
            names.len()
        ),
    );
    drain(w);
}
#[test]
fn tui_contents_dispatches_ctrl_g_to_shared_producer() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    assert!(enter(&mut m, &mut w, "needle") > 0);
    assert_eq!(
        m.rows.get(&0).unwrap().name,
        "folder",
        "old frame stays complete while searching"
    );
    wire::finish(w, reader);
}
#[test]
fn tui_contents_two_rows_share_one_target() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 51, &["notes.txt", "Docs/code.rs"]);
    m.cursor = 0;
    let frame = render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        120,
        15,
        Duration::ZERO,
    );
    let body = plain(&frame);
    assert!(
        body.contains("first needle in notes.txt"),
        "snippet must reach real frame"
    );
    assert!(
        body.contains("12 first needle"),
        "line number occupies mark cells"
    );
    let (left, _, _) = render::panes(120, m.preview_visible);
    for y in [render::BODY_ROW, render::BODY_ROW + 1] {
        m.last_click = None;
        press(
            &mut m,
            &mut w,
            Key {
                name: "Mouse".into(),
                text: String::new(),
                mods: String::new(),
                pointer: Some(super::input::Pointer {
                    button: 0,
                    x: left + 4,
                    y,
                    released: false,
                    motion: false,
                }),
            },
        );
        assert_eq!(m.current_path(), Some("/listing/notes.txt".into()));
    }
    wire::finish(w, reader);
}
#[test]
fn tui_contents_discards_stale_query_and_scope() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    m.search = "Finding".into();
    m.search_query = "new".into();
    m.searching = true;
    listed(&mut m, &mut w, 77, 88, 999);
    assert_eq!(
        m.total, 2,
        "unsolicited generation cannot replace held listing"
    );
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 51, &["notes.txt"]);
    listed(&mut m, &mut w, gen + 1, 52, 999);
    rows(&mut m, &mut w, gen + 1, 52, 0, &["stale.txt"]);
    assert_eq!(m.total, 1);
    assert_eq!(m.current_path(), Some("/listing/notes.txt".into()));
    wire::finish(w, reader);
}
#[test]
fn tui_contents_cancel_and_missing_producer_are_truthful() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let failed_gen = enter(&mut m, &mut w, "needle");
    receive(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"error","where":"searchcontents","gen":{failed_gen},"path":"/listing","msg":"ripgrep is unavailable."}}"#
        ),
    );
    assert!(!m.searching, "producer failure must end pending state");
    m.error.clear();
    m.errors.clear();
    let gen = enter(&mut m, &mut w, "needle");
    press(&mut m, &mut w, Key::named("Escape", ""));
    assert!(drain(&mut w).iter().any(|v| text(v, "c") == "searchcancel"));
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":0,"scanned":0,"ms":0.0,"cancelled":true}}"#),
    );
    assert!(!m.searching);
    assert_eq!(
        m.rows.get(&0).unwrap().name,
        "folder",
        "cancel preserves navigation snapshot"
    );
    wire::finish(w, reader);
}

// Test-only catalog mirrors the unit's i18n-delta.json entries without changing shared catalogs.
pub(super) fn catalog(root: &super::contents_process_tests::Root) -> flea::i18n::I18n {
    let base = root.0.join("catalog");
    std::fs::create_dir_all(base.join("en")).unwrap();
    std::fs::write(base.join("en/flea.json"), r#"{"tui.find.scope_hint":"%1 · tab changes scope","tui.find.status":"Find “%1” %2","tui.find.invalid_response":"The search response could not be used safely. Try Find again."}"#).unwrap();
    flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: root.0.join("omarchy"),
        language_file: root.0.join("language"),
        catalogs: vec![base, crate::i18n::test_catalogs::english_only()],
    })
}

#[test]
fn tui_contents_names_failure_restores_find_and_displays_error() {
    for (where_, path) in [
        ("search", "/mount"),
        ("scan", "/listing"),
        ("sort", "/listing"),
        ("contentsidentity", "searchopen"),
        ("contentsidentity", "searchquery"),
        ("read", "stdin"),
    ] {
        for tagged in [true, false] {
            let (mut w, reader) = wire::echo_wire();
            let mut m = listing(&mut w, "default");
            actions::act(&mut m, "search", &mut w).unwrap();
            m.editor.as_mut().unwrap().replace("needle".into());
            press(&mut m, &mut w, Key::named("Enter", ""));
            let sent = drain(&mut w);
            let ask = sent.iter().find(|v| text(v, "c") == "searchquery").unwrap();
            let gen = count(ask, "gen");
            assert!(gen > 0 && m.find.waiting);
            let error = |gen| {
                Json::Obj(vec![
                    ("t".into(), wire::word("error")),
                    ("where".into(), wire::word(where_)),
                    ("path".into(), wire::word(path)),
                    ("gen".into(), wire::number(gen)),
                    ("msg".into(), wire::word("/mount is not responding.")),
                ])
            };
            let mut failure = error(gen);
            if !tagged {
                if let Json::Obj(fields) = &mut failure {
                    fields.retain(|(key, _)| key != "gen");
                }
            }
            m.receive(failure, &mut w).unwrap();
            assert!(!m.find.waiting, "{where_} must release the held frame");
            assert!(!m.searching && m.search.is_empty(), "{where_}");
            assert_eq!(m.error, "/mount is not responding.", "{where_}");
            let frame = plain(&render::frame(
                &m,
                &Theme::from_text(""),
                &Map::load(),
                120,
                15,
                Duration::ZERO,
            ));
            assert!(frame.contains("/mount is not responding."), "{where_}");
            assert_eq!(
                m.current_path(),
                Some("/listing/folder".into()),
                "navigation survives {where_}"
            );
            actions::act(&mut m, "copy", &mut w).unwrap();
            assert!(
                drain(&mut w).iter().any(|v| text(v, "c") == "paths"),
                "{where_} keeps actions guarded"
            );
            m.error.clear();
            actions::act(&mut m, "search", &mut w).unwrap();
            m.editor.as_mut().unwrap().replace("new query".into());
            press(&mut m, &mut w, Key::named("Enter", ""));
            drain(&mut w);
            m.receive(error(gen), &mut w).unwrap();
            assert!(m.find.waiting && m.error.is_empty(), "stale {where_}");
            wire::finish(w, reader);
        }
    }
}

#[test]
fn tui_contents_find_error_routing_preserves_unrelated_failures() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    for where_ in ["search", "scan", "sort", "read"] {
        receive(
            &mut m,
            &mut w,
            &format!(
                r#"{{"t":"error","where":"{where_}","path":"/listing","msg":"ordinary failure"}}"#
            ),
        );
        assert!(
            m.error.contains("ordinary failure"),
            "idle {where_} was swallowed"
        );
        m.error.clear();
    }
    actions::act(&mut m, "search", &mut w).unwrap();
    m.editor.as_mut().unwrap().replace("needle".into());
    press(&mut m, &mut w, Key::named("Enter", ""));
    receive(
        &mut m,
        &mut w,
        r#"{"t":"error","where":"contentsidentity","path":"menuaction","msg":"unrelated operation failed"}"#,
    );
    assert!(m.find.waiting, "an unrelated operation cannot cancel Find");
    assert!(m.error.contains("unrelated operation failed"));
    wire::finish(w, reader);
}
