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
    assert_eq!(transport.ended(true), Some(Event::Ended));
    // The helper observes eof-reached before pause, so libmpv reports the end first, in this order.
    assert_eq!(transport.paused(true), None, "the end's pause");
    assert!(transport.play(), "play at the end seeks to 0 first");
    assert_eq!(transport.paused(false), Some(Event::Playing));
    assert_eq!(transport.ended(false), None, "the rewind leaves the end");
    assert!(!transport.play());
}

// Stop at the end rewinds, so the play after it needs no second seek and is still heard.
#[test]
fn pv3_stop_at_the_end_then_play() {
    let mut transport = playing();
    assert_eq!(transport.ended(true), Some(Event::Ended));
    assert_eq!(transport.paused(true), None);
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
