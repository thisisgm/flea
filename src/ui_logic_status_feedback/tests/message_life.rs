//! Every footer message leaves on its own clock, and a newer message replaces an older one.
use super::english;
use crate::ui_logic_status_feedback::messages::{Escape, Messages, ERROR_MS, MESSAGE_MS};
use crate::ui_logic_status_feedback::status::Hint;

/// S8-8: an error clears on its own; the next queued error then gets its own whole time, and esc still clears one at once.
#[test]
fn s8_8_every_error_leaves_on_its_own_clock() {
    let mut bar = Messages::new("default");
    bar.say("First failed.", true, "", Hint::None, 100);
    bar.say("Second failed.", true, "", Hint::None, 2000);
    assert_eq!(bar.next_deadline(), Some(100 + ERROR_MS));
    bar.tick(100 + ERROR_MS - 1);
    assert_eq!(bar.shown().text, "First failed.");
    bar.tick(100 + ERROR_MS);
    assert_eq!(bar.shown().text, "Second failed.");
    assert_eq!(bar.next_deadline(), Some(100 + 2 * ERROR_MS));
    bar.tick(100 + 2 * ERROR_MS);
    assert!(bar.errors().is_empty());
    assert_eq!((bar.shown().text.as_str(), bar.next_deadline()), ("", None));
    bar.say("Third failed.", true, "", Hint::None, 50_000);
    bar.say("Third failed.", true, "", Hint::None, 50_000);
    assert_eq!(bar.escape(false, 51_000), Escape::Dismissed);
    assert_eq!(
        bar.next_deadline(),
        Some(51_000 + ERROR_MS),
        "the same words queued behind get their own whole time"
    );
}

/// S8-8: a refusal said again where it stands, as a repeated Ctrl+E is, takes its whole time again.
#[test]
fn s8_8_a_placed_refusal_said_again_restarts_its_clock() {
    let mut bar = Messages::new("default");
    bar.set_path("/run/user/1000/gvfs/mtp:host=x", 0);
    bar.say_placed("This location has no Trash", Hint::Deletes, 0);
    bar.say_placed("This location has no Trash", Hint::Deletes, 5000);
    assert_eq!(bar.errors().len(), 1);
    assert_eq!(bar.next_deadline(), Some(5000 + ERROR_MS));
}

/// S8-8: a live prompt hides the error and stops its clock; the error returns with its whole time.
#[test]
fn s8_8_a_prompt_over_an_error_stops_its_clock() {
    let mut bar = Messages::new("default");
    bar.say("Older failed.", true, "", Hint::None, 0);
    bar.set_arm_owner(7, 1000, 1000);
    bar.say_arm("Press d again to move it to Trash.", 4000, 1000);
    assert_eq!(bar.next_deadline(), Some(5000), "only the arm's own end");
    bar.tick(5000);
    assert_eq!(bar.shown().text, "Older failed.");
    assert_eq!(bar.next_deadline(), Some(5000 + ERROR_MS));
}

/// S1B-2: a later receipt replaces a standing error instead of waiting behind it.
#[test]
fn s1b_2_a_newer_notice_replaces_a_standing_error() {
    let catalog = english();
    let mut bar = Messages::new("default");
    bar.say("notes.txt could not be restored.", true, "", Hint::None, 0);
    bar.set_path("/run/user/1000/gvfs/mtp:host=x", 0);
    bar.say_placed("This location has no Trash", Hint::Deletes, 0);
    bar.say("Deleted 1 of 1 item", false, "", Hint::None, 10);
    assert!(bar.errors().is_empty());
    let line = bar.line(&catalog, "");
    assert_eq!(
        (line.primary.as_str(), line.error),
        ("Deleted 1 of 1 item", false)
    );
    assert_eq!(bar.next_deadline(), Some(10 + MESSAGE_MS));
    bar.say("Copy failed.", true, "", Hint::None, 20);
    bar.explain("Nothing is selected.", Hint::None, 30);
    assert!(bar.errors().is_empty(), "an explanation is newer too");
    assert_eq!(bar.shown().text, "Nothing is selected.");
}

/// A failed operation's own receipt waits behind that operation's error, then shows for its whole time.
#[test]
fn a_failed_operations_receipt_waits_behind_its_own_error() {
    let catalog = english();
    let mut bar = Messages::new("default");
    bar.say(
        "Move failed: a.txt · permission denied",
        true,
        "",
        Hint::None,
        0,
    );
    bar.say_receipt_behind(&catalog, "Moved 0 of 1 item", 10);
    assert_eq!(bar.errors().len(), 1, "the receipt replaces nothing");
    assert!(bar.shown().is_error);
    assert_eq!(
        bar.next_deadline(),
        Some(ERROR_MS),
        "only the error's clock runs"
    );
    bar.tick(ERROR_MS);
    assert_eq!(
        (bar.shown().text.as_str(), bar.shown().is_error),
        ("Moved 0 of 1 item", false)
    );
    assert_eq!(bar.next_deadline(), Some(ERROR_MS + MESSAGE_MS));
    bar.say("Copied 1 item", false, "", Hint::None, ERROR_MS + 10);
    assert_eq!(bar.shown().text, "Copied 1 item");
}
