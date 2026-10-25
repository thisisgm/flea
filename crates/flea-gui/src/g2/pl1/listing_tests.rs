//! tests/mount-listing.qml: a populated listing then an empty one, and nothing of the first carried into the second.
use super::listing::{Effect, Listing, POLL_MS, TIMEOUT_MS};
use super::process::Outcome;

fn listed(body: &str) -> Outcome {
    Outcome {
        code: 0,
        stdout: body.into(),
        stderr: String::new(),
    }
}

#[test]
fn empty_listing_arrives_empty_and_carries_nothing() {
    let mut listing = Listing::new();
    assert!(
        matches!(&listing.poll(0)[..], [Effect::Start(job)] if job.argv == ["gio", "mount", "-li"])
    );
    let effects = listing.exited(
        &listed("Mount(0): share on fixture -> smb://fixture/share/\n"),
        10,
    );
    assert!(matches!(&effects[..], [Effect::Listed]));
    assert!(
        listing.text.contains("smb://fixture/share/"),
        "populated=share"
    );
    listing.poll(60);
    let effects = listing.exited(&listed(""), 70);
    assert!(matches!(&effects[..], [Effect::Listed]));
    assert_eq!(listing.text, "", "empty=none carried=none");
}

#[test]
fn a_poll_mid_listing_runs_when_it_ends() {
    let mut listing = Listing::new();
    listing.poll(0);
    assert!(listing.poll(5).is_empty());
    let effects = listing.exited(&listed("a"), 10);
    assert!(matches!(&effects[..], [Effect::Listed, Effect::Start(_)]));
}

#[test]
fn a_timed_out_listing_keeps_the_last_good_one() {
    let mut listing = Listing::new();
    listing.poll(0);
    listing.exited(&listed("good"), 10);
    listing.poll(20);
    assert!(listing
        .tick(20 + TIMEOUT_MS)
        .iter()
        .any(|e| matches!(e, Effect::Kill)));
    let effects = listing.exited(&listed("partial"), 20 + TIMEOUT_MS + 1);
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::Listed)),
        "a killed listing is never reported"
    );
    assert_eq!(listing.text, "good");
    assert!(listing.answered);
}

#[test]
fn running_listing_waits_only_on_its_timeout_and_never_starts_a_second_poll() {
    let mut listing = Listing::new();
    assert!(matches!(&listing.tick(0)[..], [Effect::Start(_)]));
    let late = POLL_MS + 1;
    assert_eq!(
        listing.due(late),
        Some(TIMEOUT_MS),
        "a running listing schedules nothing before its own timeout"
    );
    assert!(listing.poll(1).is_empty(), "no second gio while one runs");
    // The exit lands well before POLL_MS, so only the poll asked for mid-listing can start the next one.
    let effects = listing.exited(&listed(""), 2);
    assert!(
        matches!(&effects[..], [Effect::Listed, Effect::Start(_)]),
        "the poll asked for mid-listing runs as the listing ends"
    );
    let mut quiet = Listing::new();
    quiet.tick(0);
    assert!(
        matches!(&quiet.exited(&listed(""), 2)[..], [Effect::Listed]),
        "with no ask, an exit before POLL_MS starts nothing"
    );
}
