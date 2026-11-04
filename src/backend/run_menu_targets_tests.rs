// Path-addressed menu requests use the production loop and keep the active listing intact.
use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn ey379_wire_path_snapshot_tags_and_replacement_never_alias_the_active_row() {
    let _tags = super::super::tags::SEAM
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let d = TestDir::new("menu-neighbour-wire");
    let active = d.dir("active");
    let active_file = d.file("active/active.txt", "active");
    let neighbour = d.file("other.txt", "neighbour");
    let tb = Tables::load();
    let (events, _) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    st.base = active.clone();
    st.listing.push("active.txt", false);
    let (results, _) = channel();
    let cache_root = d.join("cache");
    let pool = Pool::new(
        1,
        results,
        cache_root.clone(),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    let cache = Cache::at(cache_root);
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let (flash_tx, _) = channel();
    let mut flash = flashslot::Ctx::new(flash_tx);
    let mut watch = Watch::start(events.clone());
    let mut peeks = PeekWatch::start(events.clone());
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut gate = Gate::new(events.clone());
    let path = crate::json::escape(&neighbour.to_string_lossy());
    let mut out = Vec::new();
    let mut request = |line: &str, st: &mut State, ops: &mut Ops, out: &mut Vec<u8>| {
        assert!(
            handle_line(
                line,
                out,
                st,
                &tb,
                &pool,
                &cache,
                ops,
                &mut flash,
                &mut watch,
                &mut peeks,
                &mut fsinfo,
                &poller,
                &events,
                &mut gate
            ) == Control::Continue
        );
    };
    request(
        &format!(
            r#"{{"c":"menuaction","op":"snapshot","id":7,"paths":["{path}"],"cursorPath":"{path}"}}"#
        ),
        &mut st,
        &mut ops,
        &mut out,
    );
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("menu snapshot reply")
    };
    assert!(line.contains(r#""ok":true"#), "{line}");
    assert_eq!(crate::json::field_usize_array(&line, "kinds"), [0o100000]);
    assert_eq!(crate::json::field_usize_array(&line, "modes").len(), 1);
    request(
        r#"{"c":"menuaction","op":"validate","id":7,"action":"copyAs:copyPath"}"#,
        &mut st,
        &mut ops,
        &mut out,
    );
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("menu validation reply")
    };
    assert_eq!(
        crate::json::field_str_array(&line, "paths"),
        [neighbour.to_string_lossy().as_ref()]
    );
    request(
        r#"{"c":"menuaction","op":"settags","id":7,"change":"add","tag":"Red"}"#,
        &mut st,
        &mut ops,
        &mut out,
    );
    assert_eq!(super::super::tags::read(&neighbour).unwrap(), ["Red"]);
    assert!(super::super::tags::read(&active_file).unwrap().is_empty());
    assert_eq!(ops.journal.len(), 1);
    request(
        r#"{"c":"menuaction","op":"tagstates","id":7,"tagId":9}"#,
        &mut st,
        &mut ops,
        &mut out,
    );
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("tag states reply")
    };
    assert_eq!(crate::json::field_usize(&line, "id"), Some(9));
    assert_eq!(crate::json::field_str_array(&line, "applied"), ["Red"]);

    std::fs::rename(&neighbour, d.join("old.txt")).unwrap();
    d.file("other.txt", "replacement");
    out.clear();
    request(
        r#"{"c":"menuaction","op":"settags","id":7,"change":"add","tag":"Blue"}"#,
        &mut st,
        &mut ops,
        &mut out,
    );
    assert!(String::from_utf8_lossy(&out).contains("Selected item changed"));
    assert!(super::super::tags::read(&neighbour).unwrap().is_empty());
    assert!(super::super::tags::read(&active_file).unwrap().is_empty());
    assert_eq!(st.base, active);
    assert_eq!(st.listing.name(0), "active.txt");
    assert_eq!(st.listing.len(), 1);
}
