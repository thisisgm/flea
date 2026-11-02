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
    assert_eq!(
        m.error, "Denied",
        "the retry's failure cleared on the first one's clock"
    );
}

// Expiration must advance the semantic label together with its rendered footer string.
#[test]
fn expiry_preserves_the_next_errors_semantic_projection() {
    use crate::tui::words::Text;
    let mut m = frame_tests::model();
    m.fail_text(Text::Key("menu.loading"));
    m.fail_text(Text::Key("pane.unavailable"));
    m.expire_message(m.error_at + super::ERROR_TIME + Duration::from_secs(1));
    m.error = "stale projection".into();
    m.words.refresh(
        &m.i18n,
        &mut m.message,
        &mut m.error,
        &mut m.errors,
        &mut m.transfer,
    );
    assert_eq!(m.error, m.i18n.tr("pane.unavailable"));
}

// Removing a queued preview failure must not shift the following operation's semantic label.
#[test]
fn preview_retraction_preserves_the_remaining_queue_projection() {
    use crate::tui::words::Text;
    let mut m = frame_tests::model();
    m.fail_text(Text::Key("menu.loading"));
    m.fail_preview(Text::Key("tui.preview.no_pixels"));
    m.fail_text(Text::Key("pane.unavailable"));
    m.retract_preview_errors(true);
    m.dismiss_error();
    m.error = "stale projection".into();
    m.words.refresh(
        &m.i18n,
        &mut m.message,
        &mut m.error,
        &mut m.errors,
        &mut m.transfer,
    );
    assert_eq!(m.error, m.i18n.tr("pane.unavailable"));
    assert!(m.errors.is_empty());
}
