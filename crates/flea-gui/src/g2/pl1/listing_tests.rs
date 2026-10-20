//! tests/mount-listing.qml: a populated listing then an empty one, and nothing of the first carried into the second.
use super::listing::{Effect, Listing, TIMEOUT_MS};
use super::process::Outcome;

fn listed(body: &str) -> Outcome {
    Outcome { code: 0, stdout: body.into(), stderr: String::new() }
}

#[test]
fn empty_listing_arrives_empty_and_carries_nothing() {
    let mut listing = Listing::new();
    assert!(matches!(&listing.poll(0)[..], [Effect::Start(job)] if job.argv == ["gio", "mount", "-li"]));
    let effects = listing.exited(&listed("Mount(0): share on fixture -> smb://fixture/share/\n"), 10);
    assert!(matches!(&effects[..], [Effect::Listed]));
    assert!(listing.text.contains("smb://fixture/share/"), "populated=share");
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
    assert!(listing.tick(20 + TIMEOUT_MS).iter().any(|e| matches!(e, Effect::Kill)));
    let effects = listing.exited(&listed("partial"), 20 + TIMEOUT_MS + 1);
    assert!(!effects.iter().any(|e| matches!(e, Effect::Listed)), "a killed listing is never reported");
    assert_eq!(listing.text, "good");
    assert!(listing.answered);
}
