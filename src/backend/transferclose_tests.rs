// The closing progress line names the transfer's last item, driven against real files in a sandbox.
use crate::backend::collide::tests::{asked, chosen, owned};
use crate::backend::collide::Policy;
use crate::backend::opsreq::{run_transfer_checked, CancelAtGuard, OpMsg};
use crate::backend::testdir::TestDir;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;

// The progress lines as (index, name, bytes, total), and the indexes of the item lines.
type Wire = (Vec<(usize, String, u64, u64)>, Vec<usize>);

// Every progress line and item line of one transfer, in the order the wire carries them.
fn wire(paths: &[&Path], to: &Path, policy: Policy) -> Wire {
    let (tx, rx) = channel();
    run_transfer_checked(1, false, owned(paths), to.to_path_buf(), Arc::new(AtomicBool::new(false)), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: policy });
    let (mut lines, mut items) = (Vec::new(), Vec::new());
    for msg in rx.iter() {
        match msg {
            OpMsg::Progress { index, name, bytes, total, .. } => lines.push((index, name, bytes, total)),
            OpMsg::Item { index, .. } => items.push(index),
            _ => {}
        }
    }
    (lines, items)
}

#[test]
fn the_closing_progress_line_names_the_last_item_that_ended_in_the_pace_not_the_last_path() {
    // A frozen clock keeps b.txt inside a.txt's window whatever the host's speed.
    let _clock = crate::backend::opsreq::opspace::FrozenClock::hold();
    let d = TestDir::new("closing-names-last");
    let to = d.dir("to");
    let (a, b) = (d.file("a.txt", "first"), d.file("b.txt", "second one"));
    let gone = d.join("gone.txt");
    let (lines, items) = wire(&[&a, &b, &gone], &to, Policy::default());
    assert_eq!(items, vec![0, 1, 2], "the missing source still sends its item line");
    assert_eq!(lines.last(), Some(&(1, "b.txt".to_string(), 10, 0)), "the second item rode inside the first window, so its bytes close under it, the last item to end in the pace: {:?}", lines);
}

#[test]
fn the_closing_progress_line_names_an_item_the_client_closed_when_the_last_path_was_skipped() {
    let _clock = crate::backend::opsreq::opspace::FrozenClock::hold();
    let d = TestDir::new("closing-skip-last");
    let to = d.dir("to");
    let (a, b, c) = (d.file("a.txt", "first"), d.file("b.txt", "second one"), d.file("c.txt", "third"));
    d.file("to/c.txt", "there");
    let policy = chosen("skip", asked(1, &[&a, &b, &c], &to), &to);
    let (lines, items) = wire(&[&a, &b, &c], &to, policy);
    assert_eq!(items, vec![0, 1], "the skipped path sends no item line");
    assert_eq!(lines.last(), Some(&(1, "b.txt".to_string(), 10, 0)), "the bytes close under b.txt, the last item the client saw end: {:?}", lines);
}

#[test]
fn the_closing_progress_line_names_an_item_the_client_closed_when_a_cancel_skipped_the_rest() {
    let _clock = crate::backend::opsreq::opspace::FrozenClock::hold();
    let _cancel = CancelAtGuard::hold(2);
    let d = TestDir::new("closing-cancel");
    let to = d.dir("to");
    let (a, b, c) = (d.file("a.txt", "first"), d.file("b.txt", "second one"), d.file("c.txt", "third"));
    let later = d.file("d.txt", "fourth");
    let (lines, items) = wire(&[&a, &b, &c, &later], &to, Policy::default());
    assert_eq!(items, vec![0, 1, 2], "the path the cancel landed in ends, and the one after it sends no item line");
    assert_eq!(lines.last(), Some(&(2, "c.txt".to_string(), 10, 0)), "the bytes close under c.txt, not under d.txt, which the cancel skipped: {:?}", lines);
}
