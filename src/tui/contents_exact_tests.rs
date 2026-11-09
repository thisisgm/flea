// A contents hit whose name is not UTF-8: its identity and file operations act on its own bytes or refuse.
use super::{action_key_tests::listing, contents_tests::*, wire};
use flea::ui_logic_file_search::{ContentsMatch, ContentsMatches};

#[test]
fn tui_contents_raw_identity_and_operations_fail_closed() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    let hit = ContentsMatch {
        row: 0,
        path: "bad�.txt\u{0}626164ff2e747874".into(),
        path_bytes: b"bad\xff.txt".to_vec(),
        line_no: 3,
        line: "needle".into(),
        operable: false,
    };
    receive(
        &mut m,
        &mut w,
        &ContentsMatches {
            gen: gen as u64,
            listing: 51,
            start: 0,
            matches: vec![hit],
        }
        .line(),
    );
    listed(&mut m, &mut w, gen, 51, 1);
    rows(
        &mut m,
        &mut w,
        gen,
        51,
        0,
        &["bad�.txt\u{0}626164ff2e747874"],
    );
    receive(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"searched","gen":{gen},"n":1,"scanned":1,"ms":1.0}}"#),
    );
    assert!(m.current_path().is_none());
    for action in [
        "open",
        "preview",
        "copy",
        "cut",
        "rename",
        "trash",
        "reveal",
        "editFile",
        "menu",
        "properties",
    ] {
        super::actions::act(&mut m, action, &mut w).unwrap();
        assert!(
            super::action_key_tests::drain(&mut w).is_empty(),
            "unsafe {action} reached an external path"
        );
    }
    assert!(!m.find.matches[&0].operable);
    wire::finish(w, reader);
}
