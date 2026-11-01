use crate::tui::render::frame_tests;
use std::time::Duration;

// GM 2026-10-20: an error clears on its own after the error time, and the next queued error then takes its own full time.
#[test]
fn a_footer_error_clears_on_its_own_after_the_error_time() {
    let mut m = frame_tests::model();
    let at = std::time::Instant::now();
    m.fail("Preview could not be shown".into());
    m.fail("Second".into());
    m.expire_message(at + Duration::from_secs(7));
    assert_eq!(m.error, "Preview could not be shown");
    m.expire_message(at + Duration::from_secs(9));
    assert_eq!(m.error, "Second", "the error stayed past the error time");
    m.expire_message(at + Duration::from_secs(16));
    assert_eq!(m.error, "Second", "the next error gets its own full time");
    m.expire_message(at + Duration::from_secs(17));
    assert!(m.error.is_empty());
}
// A retry that fails with the same words is read for the full error time again.
#[test]
fn a_retry_that_fails_again_is_read_for_the_full_error_time() {
    let mut m = frame_tests::model();
    m.fail("Denied".into());
    m.error_at = m.error_at.checked_sub(Duration::from_secs(7)).unwrap();
    m.fail("Denied".into());
    m.expire_message(std::time::Instant::now() + Duration::from_secs(2));
    assert_eq!(m.error, "Denied", "the retry's failure cleared on the first one's clock");
}
