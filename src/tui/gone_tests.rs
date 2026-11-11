use super::{
    action_key_tests::drain,
    model::{Model, Tab},
    wire::{self, text},
};
use crate::jsondoc::{self, Json};
use std::path::{Path, PathBuf};

fn tab(path: &str, cursor: usize) -> Tab {
    Tab {
        path: PathBuf::from(path),
        cursor,
        back: vec![PathBuf::from("/w/back")],
        forward: Vec::new(),
    }
}

// A pane listed on /w/p/open-me with a history, a hidden tab inside /w/p and one outside it.
fn open_on(wire: &mut wire::Wire) -> Model {
    let mut m = Model::new(PathBuf::from("/w/p/open-me"), &Json::Null);
    m.open(m.path.clone(), wire).unwrap();
    drain(wire);
    m.receive(jsondoc::parse(r#"{"t":"listed","n":1}"#).unwrap(), wire)
        .unwrap();
    m.receive(
        jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"a.txt"}]}"#).unwrap(),
        wire,
    )
    .unwrap();
    drain(wire);
    m.back = vec![PathBuf::from("/w/earlier")];
    m.tabs.push(tab("/w/p/other", 4));
    m.tabs.push(tab("/w/q", 2));
    m
}

fn lists(sent: &[Json]) -> Vec<String> {
    sent.iter()
        .filter(|v| text(v, "c") == "list")
        .map(|v| text(v, "path").to_owned())
        .collect()
}

fn receive(m: &mut Model, wire: &mut wire::Wire, line: &str) -> Vec<Json> {
    m.receive(jsondoc::parse(line).unwrap(), wire).unwrap();
    drain(wire)
}

// S3-7: the TUI dropped the line, so a deleted or unmounted open folder stayed drawn from rows that no longer exist.
#[test]
fn a_gone_open_folder_lands_the_pane_and_its_tabs_on_the_nearest_folder_still_there() {
    for (line, said) in [
        (
            r#"{"t":"gone","path":"/w/p/open-me","parent":"/w"}"#,
            "That folder is no longer there.",
        ),
        (
            r#"{"t":"unmounted","path":"/w/p/open-me","parent":"/w"}"#,
            "That drive or share was disconnected, so this folder is no longer available",
        ),
    ] {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = open_on(&mut wire);
        let sent = receive(&mut m, &mut wire, line);
        assert_eq!(lists(&sent), vec!["/w".to_owned()], "{line}");
        assert_eq!(m.pending.as_deref(), Some(Path::new("/w")));
        assert_eq!(m.message, said);
        assert_eq!(
            m.back,
            vec![PathBuf::from("/w/earlier")],
            "the history stays, as the GUI's does"
        );
        assert_eq!(
            (m.tabs[1].path.as_path(), m.tabs[1].cursor),
            (Path::new("/w"), 0),
            "a hidden tab inside the gone folder moves"
        );
        assert_eq!(m.tabs[1].back, vec![PathBuf::from("/w/back")]);
        assert_eq!(
            (m.tabs[2].path.as_path(), m.tabs[2].cursor),
            (Path::new("/w/q"), 2),
            "a tab outside it stays"
        );
        m.receive(
            jsondoc::parse(r#"{"t":"listed","n":0}"#).unwrap(),
            &mut wire,
        )
        .unwrap();
        assert_eq!(m.path, Path::new("/w"));
        assert_eq!(
            m.back,
            vec![PathBuf::from("/w/earlier")],
            "landing adds no step"
        );
        wire::finish(wire, reader);
    }
}

// The TUI keeps one list out, so a folder it was entering inside the one that went, or going up into, is dropped for the parent, history and all.
#[test]
fn a_gone_line_while_a_list_inside_the_folder_that_went_is_out_moves_the_pane_on() {
    for target in ["/w/p/open-me/sub", "/w/p"] {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = open_on(&mut wire);
        super::actions::navigate(&mut m, PathBuf::from(target), &mut wire).unwrap();
        drain(&mut wire);
        let line = r#"{"t":"gone","path":"/w/p/open-me","parent":"/w"}"#;
        let sent = receive(&mut m, &mut wire, line);
        assert_eq!(lists(&sent), vec!["/w".to_owned()], "{target}");
        assert_eq!(m.pending.as_deref(), Some(Path::new("/w")));
        assert_eq!(m.back, vec![PathBuf::from("/w/earlier")]);
        assert_eq!(m.message, "That folder is no longer there.");
        let sent = receive(&mut m, &mut wire, line);
        assert!(
            lists(&sent).is_empty(),
            "the line again, answering the dropped list, moves nothing"
        );
        assert_eq!(m.pending.as_deref(), Some(Path::new("/w")));
        wire::finish(wire, reader);
    }
}

// The backend answers the re-list a changed line starts with this line, so the list it waited for is settled by it.
#[test]
fn a_gone_line_answering_the_re_list_moves_the_pane_on() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = open_on(&mut wire);
    let sent = receive(
        &mut m,
        &mut wire,
        r#"{"t":"changed","path":"/w/p/open-me"}"#,
    );
    assert_eq!(lists(&sent), vec!["/w/p/open-me".to_owned()]);
    let sent = receive(
        &mut m,
        &mut wire,
        r#"{"t":"gone","path":"/w/p/open-me","parent":"/w/p"}"#,
    );
    assert_eq!(lists(&sent), vec!["/w/p".to_owned()]);
    assert_eq!(m.pending.as_deref(), Some(Path::new("/w/p")));
    assert_eq!(m.back, vec![PathBuf::from("/w/earlier")]);
    assert_eq!(m.message, "That folder is no longer there.");
    wire::finish(wire, reader);
}

#[test]
fn a_gone_line_for_another_folder_or_while_leaving_for_one_moves_nothing() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = open_on(&mut wire);
    let sent = receive(
        &mut m,
        &mut wire,
        r#"{"t":"gone","path":"/w/elsewhere","parent":"/w"}"#,
    );
    assert!(lists(&sent).is_empty());
    assert_eq!(m.tabs[1].path, Path::new("/w/p/other"));
    m.open(PathBuf::from("/x"), &mut wire).unwrap();
    drain(&mut wire);
    let sent = receive(
        &mut m,
        &mut wire,
        r#"{"t":"gone","path":"/w/p/open-me","parent":"/w"}"#,
    );
    assert!(
        lists(&sent).is_empty(),
        "a list already out for another folder answers for itself"
    );
    assert_eq!(m.pending.as_deref(), Some(Path::new("/x")));
    wire::finish(wire, reader);
}

// A folder whose name is not UTF-8 goes by its exact text, so the pane and its tabs still find it.
#[test]
fn a_gone_folder_whose_name_is_not_utf8_lands_the_pane_and_its_tabs() {
    use std::os::unix::ffi::OsStrExt;
    let raw = Path::new("/w").join(std::ffi::OsStr::from_bytes(b"r\xff"));
    let (mut wire, reader) = wire::echo_wire();
    let mut m = Model::new(raw.clone(), &Json::Null);
    m.open(raw.clone(), &mut wire).unwrap();
    drain(&mut wire);
    m.receive(
        jsondoc::parse(r#"{"t":"listed","n":0}"#).unwrap(),
        &mut wire,
    )
    .unwrap();
    drain(&mut wire);
    m.tabs.push(Tab {
        path: raw.join("in"),
        cursor: 1,
        back: Vec::new(),
        forward: Vec::new(),
    });
    let sent = receive(
        &mut m,
        &mut wire,
        r#"{"t":"gone","path":"/w/r�\u000072ff","parent":"/w"}"#,
    );
    assert_eq!(lists(&sent), vec!["/w".to_string()]);
    assert_eq!(m.tabs[1].path, Path::new("/w"));
    wire::finish(wire, reader);
}
