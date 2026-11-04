//! tests/unlock-stdin.qml's phases against the card machine: stdin carries the passphrase, and every Cancel settles the drive.
use super::process::{Job, Outcome};
use super::unlock::{Card, Effect, Part, Phase, Proc, Stage, UNLOCK_TIMEOUT_MS};
use flea::ui_logic_pl1::unlock::{
    lock_note, mount_note, unlock_failed_note, unlocked_note, unlocking_note, wrong_note,
};

const CANARY: &str = "S3cr3t-canary-1006";

fn out(code: i32, stdout: &str, stderr: &str) -> Outcome {
    Outcome {
        code,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

fn unlocked() -> Outcome {
    out(0, "Unlocked /dev/sdb1 as /dev/dm-0.\n", "")
}

fn mounted() -> Outcome {
    out(0, "Mounted /dev/dm-0 at /run/media/gm/Backup\n", "")
}

fn refused() -> Outcome {
    out(1, "", "Error unlocking /dev/sdb1: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: Error unlocking /dev/sdb1\n")
}

fn other_refusal() -> Outcome {
    out(
        1,
        "",
        "Error: GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain: Not authorized\n",
    )
}

fn opened() -> Card {
    let mut card = Card::new();
    card.open_for("/dev/sdb1", "Backup", 500_000_000_000, "crypto_LUKS", "2");
    card
}

fn typed(card: &mut Card, text: &str) {
    card.type_text(text);
}

fn submit(card: &mut Card, now: u64) -> Vec<Effect> {
    typed(card, CANARY);
    card.submit(now)
}

fn starts(effects: &[Effect]) -> Vec<(Proc, &Job)> {
    effects
        .iter()
        .filter_map(|e| {
            if let Effect::Start(p, j) = e {
                Some((*p, j))
            } else {
                None
            }
        })
        .collect()
}

fn lock_of(effects: &[Effect]) -> Option<String> {
    starts(effects)
        .into_iter()
        .find(|(p, _)| *p == Proc::Lock)
        .map(|(_, j)| j.argv.join(" "))
}

/// No byte of the passphrase is left anywhere on the card, the way the QML sweep read every property.
fn holds_no_secret(card: &Card) -> bool {
    let shown = format!("{card:?}");
    !shown.contains(CANARY) && shown.contains("secret: Secret(0 bytes)") && card.field.is_empty()
}

fn note(card: &Card) -> Option<flea::ui_logic_pl1::Note> {
    card.note.as_ref().map(|(n, _)| n.clone())
}

#[test]
fn dialog_opens_typing_and_submit_moves_the_secret_to_stdin() {
    let mut card = opened();
    assert!(card.opened);
    assert_eq!(card.phase, Phase::Typing);
    assert!(card.note.is_none());
    assert_eq!(card.focus, Some(Part::Field));
    let effects = submit(&mut card, 0);
    let started = starts(&effects);
    assert_eq!(started.len(), 1);
    let (proc, job) = started[0];
    assert_eq!(proc, Proc::Unlock);
    assert_eq!(
        job.argv,
        [
            "udisksctl",
            "unlock",
            "--block-device",
            "/dev/sdb1",
            "--key-file",
            "/dev/stdin"
        ]
    );
    assert_eq!(
        job.stdin.as_ref().map(|s| s.expose()),
        Some(CANARY.as_bytes())
    );
    assert!(!job
        .argv
        .iter()
        .chain(job.env.iter().map(|(_, v)| v))
        .any(|a| a.contains(CANARY)));
    assert!(card.field.is_empty(), "submit clears the field");
    assert_eq!(
        card.focus,
        Some(Part::Cancel),
        "submit moves the keyboard to cancel"
    );
    assert_eq!(
        card.field.held_mask,
        CANARY.len(),
        "submit holds the submitted count"
    );
    assert_eq!(
        card.field.dots(),
        CANARY.len(),
        "submit draws that many masks"
    );
    assert_eq!(note(&card), Some(unlocking_note("Backup")));
    assert!(holds_no_secret(&card));
}

#[test]
fn refused_passphrase_is_wrong_and_retry_drops_the_held_mask() {
    let mut card = opened();
    submit(&mut card, 0);
    card.started(Proc::Unlock);
    assert!(card.exited(Proc::Unlock, &refused(), 10).is_empty());
    assert_eq!(card.phase, Phase::Wrong);
    assert_eq!(
        card.focus,
        Some(Part::Field),
        "refusal returns the keyboard to the field"
    );
    assert_eq!(
        card.note,
        Some((wrong_note(), true)),
        "localized failed keeps the wrong note"
    );
    assert!(card.field.error);
    assert!(holds_no_secret(&card), "wrong holds no secret");
    assert!(
        card.field.is_empty() && card.field.held_mask > 0,
        "wrong holds the submitted count"
    );
    assert_eq!(
        card.field.dots(),
        CANARY.len(),
        "wrong draws that many masks"
    );
    typed(&mut card, "x");
    assert_eq!(card.field.held_mask, 0, "retry edit drops the held mask");
    assert_eq!(card.field.dots(), 1, "retry draws the new text mask");
    assert!(
        card.note.is_none() && !card.field.error,
        "an edit clears the refusal"
    );
}

#[test]
fn held_dots_keep_unlock_available_and_resubmitting_them_starts_nothing() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &refused(), 10);
    assert!(
        card.unlock_available(),
        "the wrong board draws Unlock as the default beside the held dots"
    );
    assert_eq!(card.tab_stops(), [Part::Field, Part::Cancel, Part::Unlock]);
    assert!(
        card.submit(20).is_empty(),
        "held dots are only a mask, so nothing is sent again"
    );
    assert_eq!(card.note, Some((wrong_note(), true)));
    assert_eq!(card.field.dots(), CANARY.len());
    assert_eq!(card.stage(), Stage::Idle);
}

#[test]
fn other_refusal_names_no_passphrase() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &other_refusal(), 10);
    assert_eq!(card.phase, Phase::Wrong);
    assert_eq!(note(&card), Some(unlock_failed_note("Backup")));
    assert_eq!(card.focus, Some(Part::Field));
    assert!(holds_no_secret(&card));
}

#[test]
fn retry_unlocks_mounts_and_closes_clean() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &refused(), 5);
    let effects = submit(&mut card, 10);
    assert_eq!(
        starts(&effects)[0].1.stdin.as_ref().map(|s| s.expose()),
        Some(CANARY.as_bytes()),
        "a retry hands stdin again"
    );
    let effects = card.exited(Proc::Unlock, &unlocked(), 20);
    assert_eq!(
        starts(&effects)[0].1.argv,
        ["udisksctl", "mount", "--block-device", "/dev/dm-0"]
    );
    assert_eq!(card.stage(), Stage::Mount);
    let effects = card.exited(Proc::Mount, &mounted(), 30);
    assert!(
        matches!(&effects[..], [Effect::Closed, Effect::Unlocked { path, label, note }]
        if path == "/run/media/gm/Backup" && label == "Backup" && *note == unlocked_note("Backup"))
    );
    assert!(!card.opened, "success clears and closes");
    assert!(card.field.is_empty());
    assert!(holds_no_secret(&card), "success holds no secret");
}

#[test]
fn never_started_says_what_happened_and_stays_actionable() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(
        Proc::Unlock,
        &out(-1, "", "No such file or directory (os error 2)"),
        5,
    );
    assert_eq!(card.phase, Phase::Wrong);
    assert_eq!(note(&card), Some(unlock_failed_note("Backup")));
    assert!(holds_no_secret(&card) && card.field.is_empty());
    typed(&mut card, "a");
    assert!(
        card.unlock_available(),
        "never started leaves the card actionable"
    );
}

#[test]
fn unstarted_child_is_ended_at_the_bound() {
    let mut card = opened();
    submit(&mut card, 0);
    assert_eq!(card.deadline(), Some(UNLOCK_TIMEOUT_MS));
    assert!(card.tick(UNLOCK_TIMEOUT_MS - 1).is_empty());
    let effects = card.tick(UNLOCK_TIMEOUT_MS);
    assert!(matches!(&effects[..], [Effect::Kill(Proc::Unlock)]));
    assert_eq!(note(&card), Some(unlock_failed_note("Backup")));
    assert_eq!(card.stage(), Stage::Idle);
}

#[test]
fn timer_leaves_a_started_child_running_and_cancel_never_kills_it() {
    let mut card = opened();
    submit(&mut card, 0);
    card.started(Proc::Unlock);
    assert!(
        card.tick(UNLOCK_TIMEOUT_MS * 4).is_empty(),
        "no timer ends a started child"
    );
    assert_eq!(card.phase, Phase::Unlocking);
    let effects = card.close();
    assert!(
        matches!(&effects[..], [Effect::Closed]),
        "cancel leaves the client running"
    );
    assert!(
        !card.opened && holds_no_secret(&card),
        "cancel closes and holds no secret"
    );
    assert_eq!(card.stage(), Stage::Abandon);
}

#[test]
fn close_during_mount_kills_it_and_its_late_exit_locks_quietly() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &unlocked(), 5);
    let effects = card.close();
    assert!(matches!(
        &effects[..],
        [Effect::Kill(Proc::Mount), Effect::Closed]
    ));
    let effects = card.exited(Proc::Mount, &out(-1, "", ""), 10);
    assert_eq!(
        lock_of(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdb1")
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Closed | Effect::Unlocked { .. })),
        "the late exit takes no focus"
    );
    assert!(card.exited(Proc::Lock, &out(0, "", ""), 20).is_empty());
    assert!(
        !card.opened && card.phase != Phase::Wrong && note(&card) != Some(mount_note("Backup")),
        "close during mount stays closed"
    );
    assert!(holds_no_secret(&card));
}

#[test]
fn close_during_mount_posts_a_refused_lock() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &unlocked(), 5);
    card.close();
    card.exited(Proc::Mount, &out(-1, "", ""), 10);
    let effects = card.exited(Proc::Lock, &out(1, "", "busy"), 20);
    assert!(matches!(&effects[..], [Effect::LockFailed(n)] if *n == lock_note("Backup")));
}

#[test]
fn failed_mount_relocks_and_says_so_then_a_retry_unlocks() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &unlocked(), 5);
    let effects = card.exited(Proc::Mount, &out(1, "", "Error mounting"), 10);
    assert_eq!(
        lock_of(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdb1")
    );
    assert_eq!(card.stage(), Stage::Relock);
    assert!(card.exited(Proc::Lock, &out(0, "", ""), 15).is_empty());
    assert_eq!(card.phase, Phase::Wrong);
    assert_eq!(note(&card), Some(mount_note("Backup")));
    assert!(
        holds_no_secret(&card),
        "failed mount says so and holds no secret"
    );
    submit(&mut card, 20);
    card.exited(Proc::Unlock, &unlocked(), 25);
    let effects = card.exited(Proc::Mount, &mounted(), 30);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Unlocked { path, .. } if path == "/run/media/gm/Backup")));
    assert!(
        !card.opened && holds_no_secret(&card),
        "retry after a failed mount closes clean"
    );
}

#[test]
fn failed_mount_with_a_refused_lock_tells_both() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &unlocked(), 5);
    card.exited(Proc::Mount, &out(1, "", ""), 10);
    let effects = card.exited(Proc::Lock, &out(1, "", ""), 15);
    assert!(matches!(&effects[..], [Effect::LockFailed(n)] if *n == lock_note("Backup")));
    assert_eq!(note(&card), Some(mount_note("Backup")));
}

#[test]
fn cancelled_unlock_locks_back_after_a_late_success() {
    let mut card = opened();
    submit(&mut card, 0);
    card.started(Proc::Unlock);
    card.close();
    let effects = card.exited(Proc::Unlock, &unlocked(), 50);
    assert_eq!(
        lock_of(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdb1")
    );
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::Closed)),
        "cancelled unlock takes no focus"
    );
    assert!(
        card.exited(Proc::Lock, &out(0, "", ""), 60).is_empty(),
        "successful late lock posts nothing"
    );
    assert!(!card.opened);
    assert_eq!(card.stage(), Stage::Idle);
}

#[test]
fn refused_late_lock_names_the_drive() {
    let mut card = opened();
    submit(&mut card, 0);
    card.close();
    card.exited(Proc::Unlock, &unlocked(), 50);
    let effects = card.exited(Proc::Lock, &out(1, "", ""), 60);
    assert!(matches!(&effects[..], [Effect::LockFailed(n)] if *n == lock_note("Backup")));
    assert!(!card.opened);
}

#[test]
fn failed_late_unlock_issues_no_lock_and_posts_nothing() {
    let mut card = opened();
    submit(&mut card, 0);
    card.close();
    assert!(card.exited(Proc::Unlock, &refused(), 50).is_empty());
    assert!(!card.opened && card.stage() == Stage::Idle);
}

#[test]
fn resubmit_behind_a_running_client_queues_and_runs_after_the_late_lock() {
    let mut card = opened();
    submit(&mut card, 0);
    card.started(Proc::Unlock);
    card.close();
    card.open_for("/dev/sdc1", "Stick", 0, "crypto_LUKS", "");
    assert!(submit(&mut card, 10).is_empty());
    assert!(card.queued() && card.phase == Phase::Unlocking && card.field.is_empty());
    assert_eq!(note(&card), Some(unlocking_note("Stick")));
    assert!(
        !format!("{card:?}").contains(CANARY),
        "a queued secret is never printed"
    );
    let effects = card.exited(Proc::Unlock, &unlocked(), 20);
    assert_eq!(
        lock_of(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdb1"),
        "late lock locks the cancelled drive"
    );
    let effects = card.exited(Proc::Lock, &out(0, "", ""), 30);
    let started = starts(&effects);
    assert_eq!(
        started[0].1.argv[3], "/dev/sdc1",
        "queued unlock runs after the late lock"
    );
    assert_eq!(
        started[0].1.stdin.as_ref().map(|s| s.expose()),
        Some(CANARY.as_bytes())
    );
    card.exited(
        Proc::Unlock,
        &out(0, "Unlocked /dev/sdc1 as /dev/dm-1.\n", ""),
        40,
    );
    let effects = card.exited(
        Proc::Mount,
        &out(0, "Mounted /dev/dm-1 at /run/media/gm/Stick.\n", ""),
        50,
    );
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Unlocked { label, .. } if label == "Stick")));
}

#[test]
fn close_drops_the_queued_submit_and_a_waiting_one_is_bounded() {
    let mut card = opened();
    submit(&mut card, 0);
    card.started(Proc::Unlock);
    card.close();
    card.open_for("/dev/sdc1", "Stick", 0, "", "");
    submit(&mut card, 10);
    card.close();
    assert!(
        !card.queued() && holds_no_secret(&card),
        "close drops the queued submit"
    );
    card.open_for("/dev/sdc1", "Stick", 0, "", "");
    submit(&mut card, 20);
    assert!(card.tick(20 + UNLOCK_TIMEOUT_MS).is_empty());
    assert_eq!(card.phase, Phase::Wrong);
    assert_eq!(
        note(&card),
        Some(unlock_failed_note("Stick")),
        "queued submit is bounded"
    );
    assert!(!card.queued() && holds_no_secret(&card));
    assert!(
        card.exited(Proc::Unlock, &refused(), 30).is_empty(),
        "bounded queue starts nothing later"
    );
    assert_eq!(card.stage(), Stage::Idle);
}

#[test]
fn resubmit_behind_a_killed_mount_waits_for_its_lock() {
    let mut card = opened();
    submit(&mut card, 0);
    card.exited(Proc::Unlock, &unlocked(), 5);
    card.close();
    card.open_for("/dev/sdc1", "Stick", 0, "", "");
    assert!(submit(&mut card, 10).is_empty());
    assert!(
        card.queued() && card.stage() == Stage::Abandon,
        "resubmit behind a killed mount queues"
    );
    let effects = card.exited(Proc::Mount, &out(-1, "", ""), 20);
    assert_eq!(
        lock_of(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdb1"),
        "killed mount locks its own drive"
    );
    let effects = card.exited(Proc::Lock, &out(0, "", ""), 30);
    assert_eq!(starts(&effects)[0].1.argv[3], "/dev/sdc1");
    card.exited(
        Proc::Unlock,
        &out(0, "Unlocked /dev/sdc1 as /dev/dm-1.\n", ""),
        40,
    );
    assert_eq!(
        card.stage(),
        Stage::Mount,
        "queued unlock runs after the killed mount"
    );
    card.close();
    let effects = card.exited(Proc::Mount, &out(-1, "", ""), 50);
    assert_eq!(
        lock_of(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdc1"),
        "cancelled queued mount locks its drive"
    );
}

#[test]
fn cancel_before_start_withholds_stdin_and_still_settles() {
    let mut card = opened();
    submit(&mut card, 0);
    let effects = card.close();
    assert!(matches!(
        &effects[..],
        [Effect::WithholdInput(Proc::Unlock), Effect::Closed]
    ));
    assert!(!card.opened && card.stage() == Stage::Abandon);
    assert!(
        matches!(
            &card.started(Proc::Unlock)[..],
            [Effect::WithholdInput(Proc::Unlock)]
        ),
        "a late start still closes stdin empty"
    );
    assert!(card.exited(Proc::Unlock, &refused(), 10).is_empty());
    assert_eq!(card.stage(), Stage::Idle, "early cancel settles idle");
    card.open_for("/dev/sdb1", "Backup", 0, "", "");
    submit(&mut card, 20);
    card.exited(Proc::Unlock, &unlocked(), 30);
    let effects = card.exited(Proc::Mount, &mounted(), 40);
    assert!(
        effects.iter().any(|e| matches!(e, Effect::Unlocked { .. })),
        "submit after an early cancel unlocks"
    );
}

#[test]
fn empty_submit_after_cancel_does_nothing_and_keeps_the_abandon() {
    let mut card = opened();
    submit(&mut card, 0);
    card.started(Proc::Unlock);
    card.close();
    card.open_for("/dev/sdb1", "Backup", 0, "", "");
    assert!(
        !card.unlock_available(),
        "the board disables Unlock on an empty field"
    );
    assert!(card.submit(10).is_empty());
    assert_eq!(card.phase, Phase::Typing);
    assert_eq!(card.stage(), Stage::Abandon);
    let effects = card.exited(Proc::Unlock, &unlocked(), 20);
    assert!(
        lock_of(&effects).is_some(),
        "empty submit after cancel still locks the late unlock"
    );
    card.exited(Proc::Lock, &out(0, "", ""), 30);
    assert_eq!(card.stage(), Stage::Idle);
    assert!(
        card.opened && card.phase == Phase::Typing,
        "a quiet late lock leaves the reopened card untouched"
    );
}

#[test]
fn tab_walks_field_cancel_unlock_and_unlocking_holds_cancel_alone() {
    let mut card = opened();
    assert_eq!(card.tab_stops(), [Part::Field, Part::Cancel]);
    typed(&mut card, "a");
    assert_eq!(card.tab_stops(), [Part::Field, Part::Cancel, Part::Unlock]);
    card.step_tab(false);
    assert_eq!(card.focus, Some(Part::Cancel));
    card.step_tab(false);
    assert_eq!(card.focus, Some(Part::Unlock));
    card.step_tab(false);
    assert_eq!(card.focus, Some(Part::Field));
    card.step_tab(true);
    assert_eq!(card.focus, Some(Part::Unlock));
    card.submit(0);
    assert_eq!(card.tab_stops(), [Part::Cancel]);
    card.type_text("ignored");
    assert!(card.field.is_empty(), "typing while unlocking is refused");
}
