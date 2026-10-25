//! tests/js/clipboardraces.js: delayed owner echoes, dual panes, deferred selections and every fallback-get race; one exact row ID has one primary owner, and these are dg1's.
use super::harness::{english, Pane};

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

#[test]
fn a_token_without_paths_accepts_the_text_selection() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/missing-paths/local"])), vec![]);
    pane.changed("none", Vec::new(), "foreign-text");
    assert!(pane.current().is_empty());
    pane.set_ok("local-files");
    assert!(pane.current().is_empty());
}

#[test]
fn delayed_owner_echoes_keep_the_latest_object() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/delayed/a"])), vec![]);
    pane.set_ok("owned-a");
    pane.clip(&en, true, Some(paths(&["/delayed/b"])), vec![]);
    pane.set_ok("owned-b");
    let held = pane.current();
    pane.changed("copy", paths(&["/delayed/a"]), "owned-a");
    pane.changed("cut", paths(&["/delayed/b"]), "owned-b");
    assert_eq!(pane.current(), held);
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.pasted(), "move:/delayed/b");
    pane.changed("copy", paths(&["/foreign/c"]), "foreign-c");
    assert_eq!(pane.current().token, "foreign-c");
}

#[test]
fn dual_panes_keep_one_selection_and_its_latest_object() {
    let en = english();
    let mut pane = Pane::new();
    let mut other = pane.fork();
    other.on(2);
    pane.clip(&en, false, Some(paths(&["/dual/a"])), vec![]);
    pane.set_ok("dual-a");
    other.clip(&en, true, Some(paths(&["/dual/b"])), vec![]);
    other.set_ok("dual-b");
    let held = pane.current();
    other.changed("copy", paths(&["/dual/a"]), "dual-a");
    pane.changed("cut", paths(&["/dual/b"]), "dual-b");
    pane.changed("copy", paths(&["/dual/a"]), "dual-a");
    other.changed("cut", paths(&["/dual/b"]), "dual-b");
    assert_eq!(pane.current(), held);
    assert_eq!(other.current(), held);
    other.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(other.pasted(), "move:/dual/b");
    other.changed("copy", paths(&["/foreign/dual"]), "foreign-dual");
    assert_eq!(pane.current(), other.current());
    assert_eq!(pane.current().token, "foreign-dual");
}

#[test]
fn a_deferred_selection_loses_to_a_newer_local_take() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/deferred/a"])), vec![]);
    pane.changed("copy", paths(&["/deferred/a"]), "foreign-a");
    pane.clip(&en, true, Some(paths(&["/deferred/b"])), vec![]);
    let held = pane.current();
    pane.set_ok("local-a");
    assert_eq!(pane.current(), held);
    pane.set_ok("local-b");
    pane.changed("cut", paths(&["/deferred/b"]), "local-b");
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.pasted(), "move:/deferred/b");
}

#[test]
fn a_newer_local_take_survives_a_stale_fallback_read() {
    let en = english();
    for moving in [false, true] {
        let mut pane = Pane::new();
        pane.clip(&en, false, Some(paths(&["/get/a"])), vec![]);
        pane.set_ok("get-a");
        pane.watch_error();
        pane.paste(&en, super::PendingPaste::transfer("/dest"));
        assert!(pane.asked.is_empty());
        pane.clip(&en, moving, Some(paths(&["/get/b"])), vec![]);
        let held = pane.current();
        pane.get_ok(&en, "copy", paths(&["/get/a"]), "get-a");
        assert_eq!(pane.current(), held);
        let want = if moving { "move:/get/b" } else { "copy:/get/b" };
        assert_eq!(pane.pasted(), want);
        pane.set_ok("get-b");
        pane.changed(if moving { "cut" } else { "copy" }, paths(&["/get/b"]), "get-b");
        let asked = pane.asked.len();
        let sent = pane.sent.len();
        pane.paste(&en, super::PendingPaste::transfer("/dest"));
        assert_eq!(pane.asked.len(), asked + 1);
        assert_eq!(pane.sent.len(), sent);
        assert_eq!(pane.pasted(), want);
    }
}

#[test]
fn a_shared_generation_keeps_the_other_panes_cut() {
    let en = english();
    let mut pane = Pane::new();
    let mut other = pane.fork();
    other.on(2);
    other.watch_error();
    other.paste(&en, super::PendingPaste::transfer("/dest"));
    pane.clip(&en, true, Some(paths(&["/get/dual"])), vec![]);
    other.get_ok(&en, "copy", paths(&["/get/stale"]), "foreign-old");
    assert_eq!(other.pasted(), "move:/get/dual");
}
