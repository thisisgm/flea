use super::frame::Event;
use super::transport::Transport;

#[test]
fn pv3_a_paused_near_end_seek_keeps_the_unheard_tail() {
    let mut transport = Transport::new(true);
    transport.loaded();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert_eq!(transport.seek(1800), Some(1800));
    transport.seek_accepted();
    assert_eq!(
        transport.ended(true),
        None,
        "decoder EOF is not the seek endpoint"
    );
    assert_eq!(
        transport.seek_completed(true, true, false),
        Some(Event::Paused)
    );
    assert!(!transport.play(), "resume must retain the seek target");
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.paused(true), Some(Event::Ended));
}

#[test]
fn pv3_a_queued_pre_seek_eof_cannot_end_an_interior_seek() {
    let mut transport = Transport::new(true);
    transport.loaded();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert_eq!(transport.seek(10000), Some(10000));
    transport.seek_accepted();
    assert_eq!(transport.ended(true), None, "old EOF can precede EOF-clear");
    assert!(!transport.play());
    assert_eq!(transport.ended(false), None);
}

#[test]
fn pv3_an_accepted_seek_back_from_the_end_drops_the_rewind_request() {
    let mut transport = Transport::new(true);
    transport.loaded();
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
    assert!(transport.play());
    transport.seek(1000);
    transport.seek_accepted();
    assert!(
        !transport.play(),
        "the accepted seek already leaves the endpoint"
    );
}

#[test]
fn pv3_initially_paused_near_end_seek_never_confuses_decoder_eof_with_completion() {
    let mut transport = Transport::new(false);
    assert_eq!(transport.loaded(), Event::Paused);
    transport.seek(1800);
    transport.seek_accepted();
    assert_eq!(transport.ended(true), None);
    assert_eq!(
        transport.seek_completed(true, true, false),
        Some(Event::Paused)
    );
    assert!(!transport.play());
}

#[test]
fn pv3_a_seek_blocks_old_end_flags_even_if_an_unpause_notification_arrives_first() {
    let mut transport = Transport::new(true);
    transport.loaded();
    transport.seek(1000);
    transport.seek_accepted();
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert!(!transport.play());
    assert_eq!(
        transport.seek_completed(false, false, false),
        Some(Event::Playing)
    );
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
}

#[test]
fn pv3_a_stop_rewind_does_not_leave_the_seek_guard_active() {
    let mut transport = Transport::new(true);
    transport.loaded();
    transport.seek(0);
    transport.seek_accepted();
    assert_eq!(transport.stop(), Event::Stopped);
    assert_eq!(transport.paused(true), None);
    assert_eq!(transport.ended(false), None);
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
}

#[test]
fn pv3_a_playing_seek_that_finishes_before_its_restart_is_handled_still_ends() {
    let mut transport = Transport::new(true);
    transport.loaded();
    transport.seek(1800);
    transport.seek_accepted();
    assert_eq!(
        transport.seek_completed(true, true, false),
        Some(Event::Ended)
    );
    assert!(
        transport.play(),
        "a naturally completed seek must rewind on Play"
    );
}

#[test]
fn pv3_a_paused_seek_back_from_ended_preserves_pause_after_completion() {
    let mut transport = Transport::new(true);
    transport.loaded();
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
    transport.seek(1800);
    transport.seek_accepted();
    assert_eq!(
        transport.seek_completed(true, true, false),
        Some(Event::Paused)
    );
    assert!(!transport.play(), "seek back resumes the selected tail");
}

#[test]
fn pv3_a_completed_endpoint_seek_confirms_video_end_without_decoder_eof() {
    let mut transport = Transport::new(false);
    assert_eq!(transport.loaded(), Event::Paused);
    transport.seek(2000);
    transport.seek_accepted();
    assert_eq!(
        transport.seek_completed(true, false, true),
        Some(Event::Ended)
    );
    assert_eq!(transport.ended(false), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
    assert!(
        transport.play(),
        "completed endpoint survives queued decoder state"
    );
    transport.seek(0);
    transport.seek_accepted();
    assert_eq!(
        transport.seek_completed(false, false, false),
        Some(Event::Playing)
    );
    assert!(!transport.play());
}
