use super::{
    action_key_tests::drain,
    actions, contents, contents_tests,
    input::Key,
    keymap::Map,
    model::Model,
    print,
    render::frame_tests::model,
    wire::{self, count, text, Wire},
};
use flea::ui_logic_file_search::{ContentsMatch, ContentsMatches};

fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
fn start(m: &mut Model, w: &mut Wire, in_contents: bool) -> usize {
    actions::act(
        m,
        if in_contents {
            "searchContents"
        } else {
            "search"
        },
        w,
    )
    .unwrap();
    m.editor.as_mut().unwrap().replace("needle".into());
    press(m, w, Key::named("Enter", ""));
    let sent = drain(w);
    let query = sent
        .iter()
        .find(|v| {
            text(v, "c")
                == if in_contents {
                    "searchcontents"
                } else {
                    "searchquery"
                }
        })
        .expect("Find must reach its shared producer");
    assert!(contents::holding(m) && m.searching && m.editor.is_none());
    count(query, "gen")
}
fn settle(m: &mut Model, w: &mut Wire, operable: bool) {
    let gen = start(m, w, true);
    let bytes = if operable {
        b"report.pdf".as_slice()
    } else {
        b"raw-\xff.pdf".as_slice()
    };
    let name = String::from_utf8_lossy(bytes).into_owned();
    let metadata = ContentsMatches {
        gen: gen as u64,
        listing: 40,
        start: 0,
        matches: vec![ContentsMatch {
            row: 0,
            path: name.clone(),
            path_bytes: bytes.to_vec(),
            line_no: 1,
            line: "needle".into(),
            operable,
        }],
    }
    .line();
    contents_tests::receive(m, w, &metadata);
    contents_tests::receive(
        m,
        w,
        &format!(
            r#"{{"t":"listed","gen":{gen},"listing":40,"path":"{}","n":1}}"#,
            m.path.display()
        ),
    );
    contents_tests::rows(m, w, gen, 40, 0, &[&name]);
    contents_tests::receive(
        m,
        w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":1,"scanned":1,"ms":0.0,"walking":false}}"#),
    );
    assert!(!contents::holding(m) && !m.searching);
    assert_eq!(m.find.matches[&0].operable, operable);
    drain(w);
}
pub(super) fn settle_unsafe(m: &mut Model, w: &mut Wire) {
    settle(m, w, false);
    assert!(contents::guarded(m) && m.current_path().is_none());
}

#[test]
fn tui_print_held_find_refuses_every_entry_without_dispatching() {
    let root = crate::backend::testdir::TestDir::new("print-find-busy");
    root.dir("catalog/en");
    root.file(
        "catalog/en/flea.json",
        r#"{"tui.operation.search_busy":"Wait for search to settle before a file operation"}"#,
    );
    for in_contents in [false, true] {
        for cancelling in [false, true] {
            for entry in ["keyboard", "action", "menu"] {
                let (mut w, reader) = wire::echo_wire();
                let mut m = model();
                m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
                    omarchy: root.join("omarchy"),
                    language_file: root.join("language"),
                    catalogs: vec![
                        root.join("catalog"),
                        crate::i18n::test_catalogs::english_only(),
                    ],
                });
                start(&mut m, &mut w, in_contents);
                if cancelling {
                    press(&mut m, &mut w, Key::named("Escape", ""));
                    assert!(m.find.cancelling);
                    assert!(drain(&mut w).iter().any(|v| text(v, "c") == "searchcancel"));
                }
                match entry {
                    "keyboard" => press(&mut m, &mut w, Key::character('P', "ctrl")),
                    "action" => actions::act(&mut m, "print", &mut w).unwrap(),
                    _ => {
                        m.menu = true;
                        print::open(&mut m, &mut w, true).unwrap();
                    }
                }
                assert_eq!(
                    m.error, "Wait for search to settle before a file operation",
                    "{in_contents}/{cancelling}/{entry}"
                );
                assert!(m.printing.card.is_none() && m.printing.run.is_none() && !m.menu);
                actions::act(&mut m, "copy", &mut w).unwrap();
                actions::act(&mut m, "rename", &mut w).unwrap();
                assert!(!m.pending_clipboard && m.editor.is_none());
                assert!(contents::holding(&m));
                assert!(
                    drain(&mut w).is_empty(),
                    "held source cannot dispatch a snapshot or operation"
                );
                wire::finish(w, reader);
            }
        }
    }
}

#[test]
fn tui_print_settled_contents_checks_identity_and_allows_safe_rows() {
    for operable in [false, true] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = model();
        settle(&mut m, &mut w, operable);
        press(&mut m, &mut w, Key::character('P', "ctrl"));
        let sent = drain(&mut w);
        if operable {
            assert!(m.error.is_empty() && m.printing.card.is_some());
            assert!(sent
                .iter()
                .any(|v| text(v, "c") == "menuaction" && text(v, "op") == "snapshot"));
        } else {
            assert!(m.printing.card.is_none());
            assert_eq!(m.error, m.i18n.tr("search.contents.identity"));
            assert!(sent.is_empty());
        }
        wire::finish(w, reader);
    }
}
