use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn empty_trash_responds_before_review_validation() {
    let d = TestDir::new("empty-trash-wire");
    let tb = Tables::load();
    let (events, _events_rx) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let pool = Pool::new(
        1,
        channel().0,
        d.join("cache"),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    let (tx, replies) = channel();
    let mut ops = Ops::new(tx);
    let (mut watch, mut peeks) = (
        Watch::start(events.clone()),
        PeekWatch::start(events.clone()),
    );
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    handle_line(
        r#"{"c":"trashbrowse","op":"delete","id":321,"token":999}"#,
        &mut out,
        &mut st,
        &tb,
        &pool,
        &Cache::new(),
        &mut ops,
        &mut flashslot::Ctx::new(channel().0),
        &mut watch,
        &mut peeks,
        &mut fsinfo,
        &poller,
        &events,
        &mut Gate::new(events.clone()),
    );
    let text = String::from_utf8(out).unwrap();
    let lines = text
        .lines()
        .filter(|line| crate::json::field_str(line, "t").as_deref() != Some("jobs"))
        .collect::<Vec<_>>();
    let snapshot = text
        .lines()
        .find(|line| crate::json::field_str(line, "t").as_deref() == Some("jobs"))
        .unwrap();
    assert_eq!(crate::json::field_usize(snapshot, "unfinished"), Some(1));
    assert!(
        snapshot.contains(r#""kind":"emptyTrash","head":"Emptying Trash""#),
        "{snapshot}"
    );
    assert_eq!(ops.jobs.rows.len(), 1);
    assert_eq!(
        lines.len(),
        2,
        "acknowledge before the worker checks the confirmation: {text}"
    );
    assert_eq!(
        crate::json::field_str(lines[0], "t").as_deref(),
        Some("trashdeletestarted")
    );
    assert_eq!(crate::json::field_usize(lines[0], "requestId"), Some(321));
    assert_eq!(
        crate::json::field_str(lines[1], "t").as_deref(),
        Some("transferprogress")
    );
    let id = ops.live.running().expect("the job holds the shared slot");
    let mut terminal = Vec::new();
    while ops.live.running().is_some() {
        report_op(
            &mut terminal,
            &mut ops,
            replies.recv_timeout(Duration::from_secs(5)).unwrap(),
        );
    }
    let text = String::from_utf8(terminal).unwrap();
    assert!(
        text.contains(&format!(r#""t":"transferdone","id":{id}"#)),
        "{text}"
    );
    assert!(
        text.contains(r#""stale":true"#),
        "the original refusal stays visible: {text}"
    );
    assert_eq!(ops.jobs.unfinished(), 0);
    assert!(ops.jobs.rows[0].state == super::super::jobs::State::Failed);
    assert!(!ops.jobs.rows[0].undo);
    assert_eq!(
        ops.journal.len(),
        0,
        "permanent deletion is never journalled as a reversible copy"
    );
}
