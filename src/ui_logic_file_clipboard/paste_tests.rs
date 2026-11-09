//! The paste verbs and Copy As: every verb reads the same file selection, link pastes never spend the cut, and Copy As reaches the text clipboard only.
use super::harness::{english, Pane};
use super::{image_name, LinkKind, PasteAsk, PendingPaste, Stamp};
use crate::ui_logic_context_menus::copyas::Format;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

fn stamp() -> Stamp {
    Stamp { year: 2026, month: 9, day: 23, hour: 10, minute: 14, second: 32 }
}

#[test]
fn a_copy_pastes_as_copy_and_a_cut_as_move() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/a/f1"])), vec![]);
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert_eq!(pane.pasted(), "copy:/a/f1");
    pane.clip(&en, true, Some(paths(&["/a/f2"])), vec![]);
    pane.set_ok("cut-token");
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert_eq!(pane.pasted(), "move:/a/f2");
    pane.paste(&en, PendingPaste::forced_move("/dest"));
    assert_eq!(pane.pasted(), "move:/a/f2");
}

#[test]
fn a_forced_move_keeps_a_copy_clipboard() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/foreign/move"])), vec![]);
    pane.paste(&en, PendingPaste::forced_move("/dest"));
    match &pane.asked[0] {
        PasteAsk::Transfer { op, spends_cut, .. } => {
            assert_eq!(op, "move");
            assert!(!spends_cut);
        }
        other => panic!("a forced move stays a move, got {other:?}"),
    }
    assert!(!pane.current().moving);
}

#[test]
fn link_pastes_use_the_refreshed_paths_and_never_spend() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/a/f2"])), vec![]);
    pane.set_ok("cut-token");
    pane.watch_error();
    pane.paste(&en, PendingPaste::link("/dest", LinkKind::Absolute));
    pane.get_ok(&en, "cut", paths(&["/foreign/link"]), "");
    match &pane.asked[0] {
        PasteAsk::Link { kind, .. } => assert_eq!(*kind, LinkKind::Absolute),
        other => panic!("a link paste stays a link, got {other:?}"),
    }
    assert_eq!(pane.pasted(), "link:absolute:/foreign/link");
}

#[test]
fn an_empty_clipboard_pastes_an_image_but_a_link_refuses() {
    let en = english();
    let mut pane = Pane::new();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert_eq!(
        pane.asked[0],
        PasteAsk::Image {
            dest: "/dest".into(),
            name: "Pasted image 2026-09-23 at 10.14.32.png".into(),
        }
    );
    pane.paste(&en, PendingPaste::link("/dest", LinkKind::Relative));
    assert_eq!(pane.said[0].text, "There is nothing to paste; y copies and x cuts.");
    assert!(!pane.said[0].error);
}

#[test]
fn a_moved_destination_refuses_instead_of_landing_elsewhere() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/a/f1"])), vec![]);
    pane.set_ok("t");
    pane.path = "/elsewhere".into();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert!(pane.asked.is_empty());
    assert_eq!(pane.said.last().unwrap().text, "The paste destination changed; try again.");
    pane.path = "/dest".into();
    pane.list_in_flight = true;
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert!(pane.asked.is_empty());
    pane.list_in_flight = false;
    pane.recent = true;
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert!(pane.asked.is_empty());
}

#[test]
fn navigation_while_reading_cannot_redirect_a_paste() {
    let en = english();
    let mut pane = Pane::new();
    pane.watch_error();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    pane.path = "/elsewhere".into();
    pane.get_ok(&en, "copy", paths(&["/a/f1"]), "");
    assert!(pane.asked.is_empty());
    assert_eq!(pane.said.last().unwrap().text, "The paste destination changed; try again.");
}

#[test]
fn a_fallback_plain_text_pastes_no_files() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/a/f2"])), vec![]);
    pane.set_ok("cut-token");
    pane.watch_error();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    pane.get_ok(&en, "none", Vec::new(), "");
    assert_eq!(pane.pasted(), "image");
}

#[test]
fn watch_recovery_stops_fallback_reads() {
    let en = english();
    let mut pane = Pane::new();
    pane.watch_error();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    pane.get_ok(&en, "copy", paths(&["/fallback/file"]), "");
    pane.changed("copy", paths(&["/recovered/f"]), "new");
    let sent = pane.sent.len();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert_eq!(pane.sent.len(), sent);
    assert_eq!(pane.pasted(), "copy:/recovered/f");
}

#[test]
fn one_failed_watcher_leaves_the_healthy_panes_watch_usable() {
    let en = english();
    let mut pane = Pane::new();
    let mut other = pane.fork();
    other.on(2);
    pane.clip(&en, true, Some(paths(&["/dual/cut"])), vec![]);
    pane.set_ok("dual-token");
    let held = pane.current();
    other.changed("cut", paths(&["/dual/cut"]), "dual-token");
    assert_eq!(other.current(), held);
    other.watch_error();
    let sent = pane.sent.len();
    pane.paste(&en, PendingPaste::transfer("/dest"));
    assert_eq!(pane.sent.len(), sent);
    other.paste(&en, PendingPaste::transfer("/dest"));
    assert_eq!(other.sent.last().unwrap(), r#"{"c":"clipGet"}"#);
    other.get_err(&en);
    assert_eq!(other.pasted(), "move:/dual/cut");
}

#[test]
fn copy_as_publishes_text_and_never_touches_the_file_clipboard() {
    let mut pane = Pane::new();
    pane.copy_as(Format::Path, Some(paths(&["/a/f1", "/a/f2"])), vec![]);
    assert_eq!(pane.texts, vec!["/a/f1\n/a/f2"]);
    assert!(pane.sent.is_empty());
    assert!(pane.current().is_empty());
    pane.copy_as(Format::Name, Some(paths(&[])), vec![]);
    assert_eq!(pane.texts.len(), 1);
    pane.copy_as(Format::Uri, None, vec![0]);
    assert_eq!(pane.rows_asked, vec![vec![0]]);
    pane.copy_as_resolved(paths(&["/a/sp ace"]));
    assert_eq!(pane.texts[1], "file:///a/sp%20ace");
    assert!(pane.sent.is_empty());
}

#[test]
fn the_stamped_image_name_matches_the_oracle_sample() {
    assert_eq!(image_name(stamp()), "Pasted image 2026-09-23 at 10.14.32.png");
    assert_eq!(LinkKind::parse("pasteLink"), Some(LinkKind::Relative));
    assert_eq!(LinkKind::parse("pasteAbsoluteLink"), Some(LinkKind::Absolute));
    assert_eq!(LinkKind::parse("pasteHardLink"), Some(LinkKind::Hard));
    assert_eq!(LinkKind::parse("transfer"), None);
}

#[test]
fn the_local_stamp_reads_a_whole_wall_clock() {
    let stamp = super::paste::stamp_at(1_790_000_000).expect("libc places the second");
    assert!(stamp.year >= 2026 && (1..=12).contains(&stamp.month) && (1..=31).contains(&stamp.day));
    assert!(stamp.hour < 24 && stamp.minute < 60 && stamp.second < 61);
    assert!(image_name(stamp).starts_with("Pasted image 2026-"));
}
