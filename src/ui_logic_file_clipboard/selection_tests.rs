//! The queue half of tests/js/clipboard.js: orderedPicks, paths resolution, staleness and the busy refusal; each untagged reply or failure releases A before B publishes exactly one system clipboard choice.
use super::harness::{english, Pane};
use super::TakeOut;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

#[test]
fn a_newer_choice_waits_behind_an_older_paths_request() {
    let en = english();
    for failed in [false, true] {
        let mut pane = Pane::new();
        pane.clip(&en, false, None, vec![0]);
        pane.clip(&en, true, None, vec![1]);
        assert_eq!(pane.rows_asked, vec![vec![0]]);
        if failed {
            pane.clip_failed(&en);
        } else {
            pane.resolved(&en, paths(&["/dest/A"]));
        }
        assert!(pane.sent.is_empty());
        assert_eq!(pane.rows_asked, vec![vec![0], vec![1]]);
        pane.resolved(&en, paths(&["/dest/B"]));
        assert_eq!(pane.sent, vec![r#"{"c":"clipSet","op":"cut","paths":["/dest/B"]}"#]);
        assert_eq!(pane.current().paths, paths(&["/dest/B"]));
        assert!(pane.current().moving);
        assert!(!pane.queue.pending());
        assert_eq!(pane.queue.queued(), 0);
    }
}

#[test]
fn a_resolved_cut_lands_locally_and_ends_its_claim() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, None, vec![0]);
    pane.resolved(&en, paths(&["/a/f2"]));
    assert!(pane.session.borrow().current().moving);
    assert_eq!(pane.sent[0], r#"{"c":"clipSet","op":"cut","paths":["/a/f2"]}"#);
    assert!(!pane.queue.pending());
}

#[test]
fn a_paths_request_refuses_while_another_asker_is_out() {
    let en = english();
    let mut pane = Pane::new();
    pane.copy_as_pending = Some((crate::ui_logic_context_menus::copyas::Format::Path, "/dest".into()));
    pane.clip(&en, false, None, vec![0]);
    assert_eq!(pane.said.last().unwrap().text, "Still resolving the last selection; try again.");
    assert!(pane.rows_asked.is_empty());
}

#[test]
fn a_listing_moved_under_a_request_says_to_copy_again() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, None, vec![0]);
    pane.clip(&en, false, None, vec![1]);
    pane.path = "/elsewhere".into();
    pane.clip_failed(&en);
    assert!(pane.sent.is_empty());
    assert_eq!(pane.said.last().unwrap().text, "Selected items changed; copy or cut again.");
}

#[test]
fn direct_paths_publish_without_a_round_trip() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/menu/taken"])), vec![]);
    assert_eq!(pane.sent.len(), 1);
    assert!(pane.rows_asked.is_empty());
    assert_eq!(pane.said[0].text, "Copied 1 item");
    assert!(pane.said[0].paste_hint);
}

#[test]
fn take_reports_its_decision() {
    let en = english();
    let mut pane = Pane::new();
    let path = pane.path.clone();
    let view = super::View {
        path: &path,
        listing: 1,
        paths_busy: false,
        list_in_flight: false,
    };
    let out = pane.queue.take(&en, false, None, vec![2], view);
    assert_eq!(out, TakeOut::AskPaths(vec![2]));
    let out = pane.queue.take(&en, true, Some(paths(&["/x"])), vec![], view);
    assert_eq!(out, TakeOut::Publish(paths(&["/x"]), true));
}

#[test]
fn a_landing_whose_listing_moved_is_dropped_and_said_stale() {
    let en = english();
    for reload in [false, true] {
        let mut pane = Pane::new();
        pane.clip(&en, true, None, vec![0]);
        if reload {
            pane.list_in_flight = true;
        } else {
            pane.listing += 1;
        }
        pane.resolved(&en, paths(&["/dest/A"]));
        assert!(pane.sent.is_empty(), "a stale landing publishes nothing");
        assert!(pane.current().is_empty());
        assert_eq!(*pane.said.last().unwrap(), super::words::landed_stale(&en));
        assert!(!pane.queue.pending());
        assert_eq!(pane.queue.queued(), 0);
    }
}

// EY-490: a sent copy landing on a moved listing says Errors.js's stale refusal in error ink, as v0.3.12 does.
#[test]
fn s2_int94_a_sent_copy_landing_on_a_moved_listing_says_the_stale_error() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, None, vec![0]);
    pane.listing += 1;
    pane.resolved(&en, paths(&["/dest/A"]));
    let said = pane.said.last().expect("a refusal is said");
    assert_eq!(said.text, "The listing changed before that arrived, so nothing was done.");
    assert!(said.error, "error ink");
    assert!(pane.sent.is_empty() && pane.current().is_empty(), "nothing is copied");
}
