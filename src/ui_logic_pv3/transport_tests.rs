use super::frame::Event;
use super::transport::Transport;

fn playing() -> Transport {
    let mut transport = Transport::new(true);
    assert_eq!(transport.loaded(), Event::Playing);
    transport
}

// A Stop while already paused changes no property, so the next unpause must still reach the GUI as Playing.
#[test]
fn pv3_a_stop_while_paused_never_swallows_the_next_play() {
    let mut transport = playing();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert_eq!(transport.stop(), Event::Stopped);
    assert_eq!(
        transport.paused(false),
        Some(Event::Playing),
        "the play after the stop is heard"
    );
    assert_eq!(
        transport.paused(true),
        Some(Event::Paused),
        "and a later pause is news again"
    );
}

// A Stop while playing pauses mpv; that pause is part of the stop, and the next play is heard.
#[test]
fn pv3_a_stop_while_playing_hides_its_own_pause() {
    let mut transport = playing();
    assert_eq!(transport.stop(), Event::Stopped);
    assert_eq!(transport.paused(true), None, "the stop's own pause");
    assert_eq!(transport.paused(true), None, "stopped stays stopped");
    assert_eq!(transport.paused(false), Some(Event::Playing));
}

// keep-open parks mpv paused at the end; Play there rewinds first, and the end's pause is not a Paused.
#[test]
fn pv3_play_at_the_end_rewinds_and_is_heard() {
    let mut transport = playing();
    assert!(!transport.play(), "a play mid-clip never rewinds");
    // The helper observes eof-reached before pause, so libmpv reports the end first, in this order.
    assert_eq!(transport.ended(true), None, "buffered audio still plays");
    assert_eq!(
        transport.paused(true),
        Some(Event::Ended),
        "the end's pause"
    );
    assert!(transport.play(), "play at the end seeks to 0 first");
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.ended(false), None, "the rewind leaves the end");
    assert!(!transport.play());
}

// Stop at the end rewinds, so the play after it needs no second seek and is still heard.
#[test]
fn pv3_stop_at_the_end_then_play() {
    let mut transport = playing();
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
    assert_eq!(transport.stop(), Event::Stopped);
    assert_eq!(transport.ended(false), None);
    assert!(!transport.play());
    assert_eq!(transport.paused(false), Some(Event::Playing));
}

// Nothing is reported before the file loads, and a paused open loads as Paused.
#[test]
fn pv3_a_paused_open_reports_nothing_before_it_loads() {
    let mut transport = Transport::new(false);
    assert_eq!(transport.paused(true), None);
    assert_eq!(transport.loaded(), Event::Paused);
}

// mpv refuses a seek until the file loads; the newest target waits for the load and is applied once.
#[test]
fn pv3_a_seek_before_the_load_waits_for_it() {
    let mut transport = Transport::new(true);
    assert_eq!(transport.seek(0), None, "nothing is sent before the load");
    assert_eq!(transport.seek(5000), None);
    assert_eq!(transport.loaded(), Event::Playing);
    assert_eq!(transport.pending(), Some(5000), "the newest target wins");
    assert_eq!(transport.pending(), None, "and is taken once");
    assert_eq!(
        transport.seek(1000),
        Some(1000),
        "after the load a seek goes at once"
    );
    assert_eq!(transport.pending(), None);
}

// libmpv raises eof-reached once the decoder drains, 0.4 s before a 2 s clip stops playing, so Ended waits for the clock.
#[test]
fn pv3_the_end_is_heard_when_playback_reaches_it() {
    let mut transport = playing();
    assert_eq!(
        transport.ended(true),
        None,
        "the decoder drained, the sink still plays"
    );
    assert!(
        !transport.play(),
        "Play cannot rewind before audible completion"
    );
    assert_eq!(
        transport.paused(true),
        Some(Event::Ended),
        "keep-open parks the clip at its end"
    );
}

#[test]
fn pv3_a_requested_pause_during_the_buffered_tail_resumes_without_rewinding() {
    let mut transport = playing();
    assert_eq!(transport.ended(true), None);
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert!(!transport.play(), "resume preserves the unheard tail");
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.paused(true), Some(Event::Ended));
    assert!(transport.play(), "natural completion still rewinds on Play");
}

#[test]
fn pv3_a_requested_pause_before_eof_does_not_suppress_a_later_end() {
    let mut transport = playing();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert!(!transport.play());
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.ended(true), None);
    assert_eq!(transport.paused(true), Some(Event::Ended));
}

#[test]
fn pv3_decoder_eof_after_a_requested_pause_keeps_the_unheard_tail() {
    let mut transport = playing();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert_eq!(
        transport.ended(true),
        None,
        "late decoder EOF is not audible completion"
    );
    transport.pause();
    assert_eq!(
        transport.ended(true),
        None,
        "a repeated Pause preserves its provenance"
    );
    assert!(!transport.play(), "resume must not rewind the unheard tail");
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.paused(true), Some(Event::Ended));
}

#[test]
fn pv3_play_request_before_unpause_ack_keeps_late_eof_from_ending_the_tail() {
    let mut transport = playing();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert!(!transport.play());
    assert_eq!(
        transport.ended(true),
        None,
        "Play has not been observed yet"
    );
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.paused(true), Some(Event::Ended));
}

#[test]
fn pv3_a_seek_that_mpv_does_not_accept_preserves_manual_pause() {
    let mut transport = playing();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert_eq!(transport.seek(1000), Some(1000));
    assert_eq!(
        transport.ended(true),
        None,
        "a rejected seek cannot consume pause intent"
    );
    assert!(!transport.play());
}

// An eof that lands on a pause nobody asked for is the end, and the next Play rewinds.
#[test]
fn pv3_an_eof_after_an_unrequested_pause_ends_and_rewinds() {
    let mut transport = playing();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    assert_eq!(transport.ended(true), Some(Event::Ended));
    assert!(transport.play(), "Play after the end starts over");
}

// A paused endpoint seek ends when native seek completion confirms its position, without requiring another pause change.
#[test]
fn pv3_reaching_the_end_while_paused_ends_at_once() {
    let mut transport = playing();
    transport.pause();
    assert_eq!(transport.paused(true), Some(Event::Paused));
    transport.pause();
    assert_eq!(transport.seek(2000), Some(2000));
    transport.seek_accepted();
    assert_eq!(transport.ended(true), None);
    assert_eq!(
        transport.seek_completed(true, true, true),
        Some(Event::Ended)
    );
}
