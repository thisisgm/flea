use super::english;
use crate::ui_logic_status_feedback::messages::{Escape, Messages, MESSAGE_MS};
use crate::ui_logic_status_feedback::status::{Hint, Note};

#[test]
fn plugin_action_recovery_replaces_only_its_tracked_error_even_when_text_matches() {
    let mut bar = Messages::new("default");
    bar.say("Checksum does not match.", true, "", Hint::None, 0);
    let own = bar
        .say_tracked_error("Checksum does not match.", 1)
        .unwrap();
    bar.say("Checksum does not match.", true, "", Hint::None, 2);
    let other = bar.say_tracked_error("Another plugin failed.", 3).unwrap();
    bar.forget_error(own, 4);
    assert_eq!(bar.errors().len(), 3);
    assert_eq!(bar.errors()[0].serial, 0);
    assert_eq!(bar.errors()[1].serial, 0);
    assert_eq!(bar.errors()[2].serial, other);
    bar.forget_error(own, 5);
    bar.forget_error(0, 6);
    assert_eq!(
        bar.errors().len(),
        3,
        "stale or untracked tokens acknowledge nothing"
    );
}

#[test]
fn a_matching_withdraw_clears_the_notice_and_keeps_the_error() {
    let mut bar = Messages::new("default");
    bar.say("The last tab cannot be closed", false, "", Hint::None, 0);
    let own = bar.notice_serial();
    bar.say("Copy failed.", true, "", Hint::None, 0);
    bar.withdraw(own, 0);
    assert_eq!(bar.notice(), "");
    assert_eq!(bar.errors().len(), 1);
    assert_eq!(bar.shown().text, "Copy failed.");
}

#[test]
fn a_stale_withdraw_keeps_the_later_notice() {
    let mut bar = Messages::new("default");
    bar.say("The last tab cannot be closed", false, "", Hint::None, 0);
    let stale = bar.notice_serial();
    bar.say("Copy failed.", true, "", Hint::None, 0);
    bar.say("Later notice.", false, "", Hint::None, 0);
    bar.withdraw(stale, 0);
    assert_eq!(bar.notice(), "Later notice.");
    assert_eq!(bar.errors().len(), 1);
}

#[test]
fn a_notice_times_out_but_one_carrying_undo_stands() {
    let mut bar = Messages::new("default");
    bar.say("Renamed to notes.txt", false, "", Hint::None, 100);
    assert_eq!(bar.next_deadline(), Some(100 + MESSAGE_MS));
    assert!(!bar.tick(100 + MESSAGE_MS - 1).notice_cleared);
    assert!(bar.tick(100 + MESSAGE_MS).notice_cleared);
    assert_eq!((bar.notice(), bar.next_deadline()), ("", None));
    bar.say("Moved 4 items to Trash", false, "", Hint::Undo, 0);
    assert_eq!(bar.next_deadline(), None);
    bar.say(
        "Copied 5 items",
        false,
        "",
        Hint::Note(Note::Background, true),
        0,
    );
    assert_eq!(bar.next_deadline(), None);
}

#[test]
fn a_notice_hidden_behind_an_error_keeps_its_whole_time_after_acknowledgement() {
    let mut bar = Messages::new("default");
    bar.say("Copy failed.", true, "", Hint::None, 0);
    bar.say("Renamed to notes.txt", false, "", Hint::None, 10);
    assert_eq!(bar.next_deadline(), None);
    assert_eq!(bar.escape(true, 9000), Escape::Dismissed);
    assert_eq!(bar.next_deadline(), Some(9000 + MESSAGE_MS));
    assert_eq!(bar.shown().text, "Renamed to notes.txt");
}

#[test]
fn empty_say_dismisses_the_head_and_forget_names_its_own() {
    let mut bar = Messages::new("default");
    bar.say("First failed.", true, "", Hint::None, 0);
    bar.say("Second failed.", true, "", Hint::None, 0);
    bar.forget("Second failed.", 0);
    assert_eq!(bar.errors().len(), 1);
    bar.say("", false, "", Hint::None, 0);
    assert!(bar.errors().is_empty());
    bar.say("Notice", false, "", Hint::None, 0);
    bar.dismiss(0);
    assert_eq!(bar.notice(), "");
}

#[test]
fn an_arm_lives_on_its_owners_stamp_and_ends_everywhere_at_once() {
    let mut bar = Messages::new("default");
    let prompt = "Press d again to move it to Trash, or Delete on its own.";
    bar.set_arm_owner(7, 0, 0);
    bar.say_arm(prompt, 4000, 0);
    assert!(!bar.arm_live());
    assert_eq!(bar.notice(), prompt);
    bar.set_arm_owner(7, 1000, 1000);
    bar.say_arm(prompt, 4000, 1000);
    assert!(bar.arm_live());
    assert_eq!((bar.notice(), bar.shown().text.as_str()), ("", prompt));
    bar.say("Older failed.", true, "", Hint::None, 1000);
    assert_eq!(bar.shown().text, prompt);
    assert_eq!(bar.next_deadline(), Some(5000));
    let lost = bar.tick(5000).arm_lost;
    assert_eq!(lost.map(|arm| (arm.owner, arm.stamp)), Some((7, 1000)));
    assert!(!bar.arm_live());
    assert_eq!(bar.shown().text, "Older failed.");
    bar.set_arm_owner(7, 2000, 2000);
    bar.say_arm(prompt, 4000, 2000);
    bar.owner_stamp(0, 2100);
    assert!(bar.arm().is_none());
    bar.owner_stamp(3000, 3000);
    bar.say_arm(prompt, 4000, 3000);
    assert!(bar.set_path("/elsewhere", 3100).is_some());
    assert!(bar.arm().is_none());
    bar.owner_stamp(4000, 4000);
    bar.say_arm(prompt, 4000, 4000);
    assert!(bar.set_arm_owner(8, 0, 4100).is_some());
}

#[test]
fn escape_dismisses_then_cancels_a_cancellable_transfer_once() {
    let mut bar = Messages::new("default");
    bar.set_activity(1, "Copying 1 of 5", 9, true, false, 0);
    assert_eq!(bar.escape(false, 0), Escape::Ignored);
    bar.say("Copy failed.", true, "", Hint::None, 0);
    assert_eq!(bar.escape(true, 0), Escape::Dismissed);
    assert_eq!(bar.escape(true, 0), Escape::Cancel(9));
    // StatusBar.escapePressed consumes every escape while a cancellable transfer runs, a cancel already sent included.
    assert_ne!(bar.escape(true, 0), Escape::Ignored);
    assert_eq!(bar.escape(true, 0), Escape::Cancelling);
    assert_eq!(bar.escape(false, 0), Escape::Ignored);
}

/// StatusBar.qml restarts its clear timer from say, the arm's start and end and three visibility edges, never from progress.
#[test]
fn progress_lines_never_restart_a_shown_notices_time() {
    let mut bar = Messages::new("default");
    bar.say("Renamed to notes.txt", false, "", Hint::None, 0);
    let mut cleared_at = None;
    for now in (500..=MESSAGE_MS + 2000).step_by(500) {
        bar.set_activity(
            1,
            &format!("Copying {} of 99", now / 500),
            7,
            true,
            false,
            now,
        );
        bar.set_path("/a", now);
        bar.set_arm_owner(3, 0, now);
        bar.forget("nothing standing", now);
        if bar.tick(now).notice_cleared && cleared_at.is_none() {
            cleared_at = Some(now);
        }
    }
    assert_eq!(cleared_at, Some(MESSAGE_MS));
    bar.set_activity(1, "", 7, false, false, 7000);
    bar.say("Copied 2 items", false, "", Hint::None, 7000);
    bar.set_activity(2, "Waiting for NAS", 0, false, false, 7500);
    assert_eq!(
        bar.next_deadline(),
        None,
        "a sticky line hides the notice and stops its clock"
    );
    bar.set_activity(2, "", 0, false, false, 9000);
    assert_eq!(
        bar.next_deadline(),
        Some(9000 + MESSAGE_MS),
        "the sticky leaving shows it again with its whole time"
    );
}

#[test]
fn the_line_splits_the_undo_target_and_keeps_esc_ahead_of_it() {
    let catalog = english();
    let mut bar = Messages::new("default");
    bar.say("Copied 1 item", false, "", Hint::Undo, 0);
    let line = bar.line(&catalog, "");
    assert_eq!(
        (
            line.primary.as_str(),
            line.secondary.as_str(),
            line.undo.as_str()
        ),
        ("Copied 1 item", "", "z undoes")
    );
    bar.say("Renamed to notes.txt", false, "", Hint::None, 0);
    let line = bar.line(&catalog, "");
    assert_eq!((line.secondary.as_str(), line.undo.as_str()), ("", ""));
    bar.say("Copy failed.", true, "disk full", Hint::Undo, 0);
    let line = bar.line(&catalog, "");
    assert!(line.error);
    assert_eq!(
        (
            line.secondary.as_str(),
            line.undo.as_str(),
            line.detail.as_str()
        ),
        (" · esc dismisses", "z undoes", "disk full")
    );
    let same = bar.line(&catalog, "Copy failed.");
    assert_eq!(same.secondary, "");
    bar.dismiss(0);
    bar.say(
        "Copied 5 items",
        false,
        "",
        Hint::Note(Note::Background, true),
        0,
    );
    let line = bar.line(&catalog, "");
    assert_eq!(
        (line.secondary.as_str(), line.undo.as_str()),
        (" · rclone uploads them in the background", "z undoes")
    );
    bar.say("Copied 1 item", false, "", Hint::Paste, 0);
    assert_eq!(bar.line(&catalog, "").secondary, " · p pastes");
}

#[test]
fn a_no_trash_refusal_carries_its_key_and_belongs_to_its_place() {
    let catalog = english();
    let mut bar = Messages::new("default");
    bar.set_path("/run/user/1000/gvfs/mtp:host=x", 0);
    let text = catalog.tr("feedback.no_trash");
    bar.say_placed(&text, Hint::Deletes, 0);
    bar.say_placed(&text, Hint::Deletes, 0);
    assert_eq!(bar.errors().len(), 1);
    let line = bar.line(&catalog, "");
    assert_eq!(
        (line.primary.as_str(), line.secondary.as_str()),
        (
            "This location has no Trash",
            " · esc dismisses · shift-delete deletes"
        )
    );
    assert_eq!(
        (line.hint.as_str(), line.extra.as_str()),
        (" · esc dismisses · shift-delete deletes", "")
    );
    bar.set_activity(1, "Converting 1 of 3", 0, false, false, 0);
    let line = bar.line(&catalog, "");
    assert_eq!(
        (line.hint.as_str(), line.extra.as_str()),
        (" · esc dismisses", " · Converting 1 of 3")
    );
    bar.set_activity(1, "", 0, false, false, 0);
    bar.set_path("/run/user/1000/gvfs", 0);
    assert!(bar.errors().is_empty());
}

#[test]
fn activity_and_search_take_the_hint_away_and_the_busy_mark_follows_starting() {
    let catalog = english();
    let mut bar = Messages::new("default");
    bar.say("Copied 1 item", false, "", Hint::Undo, 0);
    bar.set_search("3 found", 0);
    let line = bar.line(&catalog, "");
    assert_eq!((line.primary.as_str(), line.undo.as_str()), ("3 found", ""));
    bar.set_search("", 0);
    bar.set_activity(1, "Starting Pixel 8", 0, false, true, 0);
    let line = bar.line(&catalog, "");
    assert_eq!(
        (line.primary.as_str(), line.busy),
        ("Starting Pixel 8", true)
    );
    bar.set_retry("c.txt selected for retry");
    assert_eq!(
        bar.line(&catalog, "").secondary,
        " · c.txt selected for retry"
    );
}

#[test]
fn every_notice_end_takes_its_hint_with_it() {
    let catalog = english();
    let mut bar = Messages::new("default");
    bar.say("Copied 5 items", false, "", Hint::Paste, 0);
    assert!(!bar.line(&catalog, "").hint.is_empty());
    assert!(bar.tick(MESSAGE_MS).notice_cleared);
    assert_eq!(bar.line(&catalog, "").hint, "");
    bar.say("Copied 5 items", false, "", Hint::Paste, 0);
    bar.dismiss(0);
    assert_eq!(bar.line(&catalog, "").hint, "");
}

/// EY-421: the trash receipt's "· z undoes" leaves the sentence for the muted undo target, as Status.js hintOf splits it.
#[test]
fn ey421_a_receipts_undo_tail_draws_as_the_undo_target() {
    let catalog = english();
    let said = crate::ui_logic_file_ops::lines::trashed(&catalog, "default", 1, 0, "");
    assert_eq!(
        said, "Moved 1 item to Trash · z undoes",
        "the receipt as op1 says it"
    );
    let (text, hint) =
        crate::ui_logic_status_feedback::status::split_undo(&said, &catalog, "default");
    assert_eq!(
        (text.as_str(), hint),
        ("Moved 1 item to Trash", Hint::TransientUndo)
    );
    let mut bar = Messages::new("default");
    bar.say(&text, false, "", hint, 0);
    let line = bar.line(&catalog, "");
    assert_eq!(
        line.primary, "Moved 1 item to Trash",
        "the foreground sentence"
    );
    assert_eq!(line.undo, "z undoes", "the muted undo target");
    assert_eq!(
        bar.transient_text(&catalog),
        said,
        "the wire keeps the whole receipt"
    );
    assert_eq!(bar.next_deadline(), Some(MESSAGE_MS));
    assert!(!bar.tick(MESSAGE_MS - 1).notice_cleared);
    assert!(bar.tick(MESSAGE_MS).notice_cleared);
    let expired = bar.line(&catalog, "");
    assert!(expired.primary.is_empty() && expired.undo.is_empty());
    // A move, a copy and a tag change carry the same tail, each from its own builder.
    let moved =
        crate::ui_logic_file_ops::lines::with_undo(&catalog, "default", "Moved notes.txt".into());
    let tagged = format!(
        "Tagged notes.txt{}",
        crate::ui_logic_rail_places::tags::undo_hint(&catalog, "default")
    );
    for (receipt, sentence) in [(moved, "Moved notes.txt"), (tagged, "Tagged notes.txt")] {
        let split =
            crate::ui_logic_status_feedback::status::split_undo(&receipt, &catalog, "default");
        assert_eq!(
            split,
            (sentence.to_owned(), Hint::TransientUndo),
            "{receipt}"
        );
    }
    let embedded = "Created notes · z undoes.txt";
    assert_eq!(
        crate::ui_logic_status_feedback::status::split_undo(embedded, &catalog, "default"),
        (embedded.to_owned(), Hint::None),
        "advice inside a filename is not the receipt's tail"
    );
    let plain = crate::ui_logic_status_feedback::status::split_undo(
        "Copied notes.txt",
        &catalog,
        "default",
    );
    assert_eq!(
        plain,
        ("Copied notes.txt".to_owned(), Hint::None),
        "a line without the tail is untouched"
    );
}

/// An error receipt keeps its whole sentence, undo tail included, as Status.js hintOf leaves an error.
#[test]
fn an_error_receipt_offers_no_undo_target() {
    let catalog = english();
    let said =
        crate::ui_logic_file_ops::lines::with_undo(&catalog, "default", "Moved notes.txt".into());
    let mut done = Messages::new("default");
    done.say_receipt(&catalog, &said, false, "", 0);
    assert_eq!(
        done.line(&catalog, "").undo,
        "z undoes",
        "a receipt offers undo"
    );
    let mut failed = Messages::new("default");
    failed.say_receipt(&catalog, &said, true, "", 0);
    assert_eq!(
        failed.line(&catalog, "").undo,
        "",
        "an error offers no undo"
    );
    assert_eq!(
        failed.line(&catalog, "").primary,
        said,
        "an error keeps its undo tail"
    );
}
