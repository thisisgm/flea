use super::*;
use crate::backend::opsdispatch::Ops;

fn browser() -> (Ops, std::sync::mpsc::Receiver<BrowserRequest>) {
    let (tx, requests) = channel();
    let mut ops = Ops::new(channel().0);
    ops.trashbrowser = Some(TrashBrowser {
        tx,
        cancellation: Cancellation::default(),
    });
    (ops, requests)
}

#[test]
fn queued_delete_supersedes_a_check_and_accepts_cancel_before_starting() {
    let (mut ops, requests) = browser();
    ops.trashbrowser.as_ref().unwrap().enqueue(
        r#"{"c":"trashbrowse","op":"check","id":70,"token":1}"#.into(),
        Vec::new(),
        String::new(),
        None,
    ).unwrap();
    let (_, checking, _, _, _) = requests.recv().unwrap();
    assert!(checking.check().is_ok());
    let mut output = Vec::new();
    job::request(
        &mut output,
        &mut ops,
        r#"{"c":"trashbrowse","op":"delete","id":71,"token":1}"#.into(),
    );
    assert!(
        checking.check().is_err(),
        "accepting delete must interrupt the running confirmation check"
    );
    let id = ops.live.running().unwrap();
    ops.live.cancel(id);
    let (_, deleting, _, _, progress) = requests.try_recv().unwrap();
    assert!(progress.is_some());
    assert!(
        deleting.check().is_err(),
        "cancel reaches the flag before the worker receives the queued delete"
    );
}

#[test]
fn a_later_browser_read_does_not_cancel_the_delete_operation() {
    let (mut ops, requests) = browser();
    job::request(
        &mut Vec::new(),
        &mut ops,
        r#"{"c":"trashbrowse","op":"delete","id":71,"token":1}"#.into(),
    );
    let (_, deleting, _, _, _) = requests.recv().unwrap();
    ops.trashbrowser.as_ref().unwrap().enqueue(
        r#"{"c":"trashbrowse","op":"check","id":72,"token":1}"#.into(),
        Vec::new(),
        String::new(),
        None,
    ).unwrap();
    assert!(
        deleting.check().is_ok(),
        "later browser requests must not replace the operation's own cancel flag"
    );
    ops.live.cancel(ops.live.running().unwrap());
    assert!(deleting.check().is_err());
}

#[test]
fn busy_refusals_do_not_adopt_or_enqueue_a_delete() {
    for settling in [false, true] {
        let (mut ops, requests) = browser();
        let running = if settling {
            ops.settling.push((99, "announced Trash batch".into()));
            None
        } else {
            Some(ops.claim_transfer().0)
        };
        let next = ops.next_id;
        let mut out = Vec::new();
        job::request(&mut out, &mut ops, r#"{"c":"trashbrowse","op":"delete","id":73,"token":1}"#.into());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("An operation is already running."), "{text}");
        assert!(ops.jobs.rows.is_empty());
        assert_eq!(ops.next_id, next);
        assert_eq!(ops.live.running(), running);
        assert!(requests.try_recv().is_err());
    }
}

#[test]
fn worker_refusal_releases_slot_and_finishes_adopted_job() {
    let (mut ops, requests) = browser();
    drop(requests);
    for request in [74, 75] {
        let (tx, requests) = channel();
        drop(requests);
        ops.trashbrowser = Some(TrashBrowser { tx, cancellation: Cancellation::default() });
        let mut out = Vec::new();
        job::request(&mut out, &mut ops, format!(r#"{{"c":"trashbrowse","op":"delete","id":{request},"token":1}}"#));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Trash worker unavailable."), "{text}");
        assert!(!text.contains("An operation is already running."), "{text}");
        assert!(ops.live.running().is_none());
        assert_eq!(ops.jobs.unfinished(), 0);
        assert!(ops.jobs.rows.last().unwrap().state == crate::backend::jobs::State::Failed);
    }
}

#[test]
fn next_read_receives_a_reply_after_worker_disconnect() {
    let (tx, replies) = channel();
    let mut ops = Ops::new(tx);
    let (tx, requests) = channel();
    ops.trashbrowser = Some(TrashBrowser { tx, cancellation: Cancellation::default() });
    drop(requests);
    job::request(&mut Vec::new(), &mut ops, r#"{"c":"trashbrowse","op":"delete","id":76,"token":1}"#.into());
    job::request(&mut Vec::new(), &mut ops, r#"{"c":"trashbrowse","op":"check","id":77,"token":0}"#.into());
    match replies.recv_timeout(Duration::from_secs(1)) {
        Ok(OpMsg::Meta { line }) => {
            assert_eq!(field_usize(&line, "id"), Some(77));
            assert!(field_bool(&line, "ok"), "{line}");
            assert!(!field_bool(&line, "valid"), "{line}");
        }
        _ => panic!("a disconnected worker must not swallow every later browser read"),
    }
    assert!(ops.live.running().is_none());
    assert_eq!(ops.jobs.rows.len(), 1, "a failed delete must not be replayed");
}

#[test]
fn disconnected_read_is_refused_and_the_next_read_recovers() {
    let (tx, replies) = channel();
    let mut ops = Ops::new(tx);
    let (tx, requests) = channel();
    drop(requests);
    ops.trashbrowser = Some(TrashBrowser { tx, cancellation: Cancellation::default() });
    let mut out = Vec::new();
    job::request(&mut out, &mut ops, r#"{"c":"trashbrowse","op":"check","id":78,"token":0}"#.into());
    let refusal = String::from_utf8(out).unwrap();
    assert_eq!(field_usize(&refusal, "id"), Some(78), "{refusal}");
    assert_eq!(field_str(&refusal, "op").as_deref(), Some("check"));
    assert!(!field_bool(&refusal, "ok"));
    assert!(refusal.contains("Trash worker unavailable."));
    assert!(ops.trashbrowser.is_none());
    job::request(&mut Vec::new(), &mut ops, r#"{"c":"trashbrowse","op":"check","id":79,"token":0}"#.into());
    match replies.recv_timeout(Duration::from_secs(1)) {
        Ok(OpMsg::Meta { line }) => {
            assert_eq!(field_usize(&line, "id"), Some(79));
            assert!(field_bool(&line, "ok"), "{line}");
            assert!(!field_bool(&line, "valid"), "{line}");
        }
        _ => panic!("a plain request failure must also allow lazy worker recovery"),
    }
    assert!(ops.jobs.rows.is_empty());
    assert!(ops.live.running().is_none());
}
