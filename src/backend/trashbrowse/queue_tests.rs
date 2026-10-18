use super::*;
use crate::backend::opsdispatch::Ops;

fn browser() -> (
    Ops,
    std::sync::mpsc::Receiver<(
        String,
        Cancellation,
        Vec<PathBuf>,
        String,
        Option<job::Progress>,
    )>,
) {
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
    ops.trashbrowser.as_ref().unwrap().request(
        r#"{"c":"trashbrowse","op":"check","id":70,"token":1}"#.into(),
        Vec::new(),
        String::new(),
    );
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
    ops.trashbrowser.as_ref().unwrap().request(
        r#"{"c":"trashbrowse","op":"check","id":72,"token":1}"#.into(),
        Vec::new(),
        String::new(),
    );
    assert!(
        deleting.check().is_ok(),
        "later browser requests must not replace the operation's own cancel flag"
    );
    ops.live.cancel(ops.live.running().unwrap());
    assert!(deleting.check().is_err());
}
