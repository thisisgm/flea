//! tests/js/clipboardraces.js, the fallback-get half: foreign selections that land while a read is out, two reads at once, and reads that outlive their sets.
use super::harness::{english, Pane};

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

#[test]
fn a_newer_watcher_selection_survives_an_older_read() {
    let en = english();
    let mut pane = Pane::new();
    let mut other = pane.fork();
    other.on(2);
    pane.changed("copy", paths(&["/foreign/a"]), "foreign-a");
    pane.watch_error();
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert!(pane.asked.is_empty());
    let generation = pane.session.borrow().generation();
    other.changed("cut", paths(&["/foreign/b"]), "foreign-b");
    let held = pane.current();
    assert_eq!(pane.session.borrow().generation(), generation + 1);
    pane.get_ok(&en, "copy", paths(&["/foreign/a"]), "foreign-a");
    assert_eq!(pane.current(), held);
    assert_eq!(other.current(), held);
    assert_eq!(pane.pasted(), "move:/foreign/b");
    other.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(other.pasted(), "move:/foreign/b");
}

#[test]
fn two_reads_serve_the_newer_selection_first_or_last() {
    let en = english();
    for newer_first in [false, true] {
        let mut pane = Pane::new();
        let mut other = pane.fork();
        other.on(2);
        pane.watch_error();
        other.watch_error();
        pane.paste(&en, super::PendingPaste::transfer("/dest"));
        other.paste(&en, super::PendingPaste::transfer("/dest"));
        assert!(pane.asked.is_empty() && other.asked.is_empty());
        assert_eq!(pane.sent[0], r#"{"c":"clipGet"}"#);
        assert_eq!(other.sent[0], r#"{"c":"clipGet"}"#);
        if newer_first {
            other.get_ok(&en, "cut", paths(&["/read/b"]), "read-b");
            let held = pane.current();
            pane.get_ok(&en, "copy", paths(&["/read/a"]), "read-a");
            assert_eq!(pane.current(), held);
            assert_eq!(other.current(), held);
        } else {
            pane.get_ok(&en, "copy", paths(&["/read/a"]), "read-a");
            assert_eq!(pane.pasted(), "copy:/read/a");
            other.get_ok(&en, "cut", paths(&["/read/b"]), "read-b");
            assert_eq!(other.pasted(), "move:/read/b");
        }
        assert_eq!(pane.current().token, "read-b");
        assert_eq!(pane.session.borrow().pending_gets(), 0);
        assert_eq!(other.pasted(), "move:/read/b");
        pane.paste(&en, super::PendingPaste::transfer("/dest"));
        pane.get_ok(&en, "cut", paths(&["/read/b"]), "read-b");
        assert_eq!(pane.pasted(), "move:/read/b");
    }
}

#[test]
fn a_read_from_before_a_set_never_pastes_old_files() {
    let en = english();
    let mut pane = Pane::new();
    let mut other = pane.fork();
    other.on(2);
    pane.changed("copy", paths(&["/pending/a"]), "pending-a");
    other.watch_error();
    pane.clip(&en, true, Some(paths(&["/pending/b"])), vec![]);
    let held = pane.current();
    let sent = other.sent.len();
    other.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(other.sent.len(), sent);
    assert_eq!(other.pasted(), "move:/pending/b");
    other.get_ok(&en, "copy", paths(&["/pending/a"]), "pending-a");
    assert_eq!(pane.current(), held);
    assert_eq!(other.current(), held);
    assert_eq!(other.pasted(), "move:/pending/b");
    pane.set_ok("pending-b");
    pane.changed("cut", paths(&["/pending/b"]), "pending-b");
    other.changed("cut", paths(&["/pending/b"]), "pending-b");
    let asked = other.asked.len();
    other.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(other.asked.len(), asked + 1);
    assert_eq!(other.pasted(), "move:/pending/b");
    assert_eq!(pane.current().token, "pending-b");
}

#[test]
fn an_equal_path_foreign_copy_stays_authoritative() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/shared/f"])), vec![]);
    pane.changed("copy", paths(&["/shared/f"]), "foreign-token");
    pane.set_ok("local-token");
    assert_eq!(pane.current().token, "foreign-token");
}

#[test]
fn a_later_none_retires_an_earlier_deferred_selection() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/shared/new"])), vec![]);
    pane.changed("cut", paths(&["/shared/new"]), "deferred-foreign");
    pane.changed("none", Vec::new(), "");
    pane.set_ok("new-local");
    assert!(pane.current().is_empty());
}

#[test]
fn replaying_an_early_echo_cannot_revive_a_failed_watcher() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/shared/error"])), vec![]);
    pane.changed("cut", paths(&["/shared/error"]), "deferred-own");
    pane.watch_error();
    pane.set_ok("deferred-own");
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.sent.last().unwrap(), r#"{"c":"clipGet"}"#);
}
