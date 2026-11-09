use super::fake::picture;
use crate::g2::media_preview::engine::{feed, Incoming};
use crate::g2::media_preview::sandbox::read;
use flea::ui_logic_media_preview::frame::{header, Event, Record};

// A slow paint drops old pictures: two frames before a take wake the player once and hand it the newest.
#[test]
fn pv3_frames_coalesce_into_one_wake_and_the_newest_picture() {
    let (sender, feed) = feed();
    let (first, second) = (picture(1), picture(2));
    assert!(sender.frame(first));
    assert!(sender.frame(second.clone()));
    assert_eq!(feed.incoming.try_recv(), Ok(Incoming::Frame));
    assert!(
        feed.incoming.try_recv().is_err(),
        "the second picture woke nothing"
    );
    assert_eq!(feed.frames.take().map(|image| image.id), Some(second.id));
    assert!(feed.frames.take().is_none(), "a take empties the slot");
    assert!(sender.frame(picture(3)));
    assert_eq!(
        feed.incoming.try_recv(),
        Ok(Incoming::Frame),
        "after a take the next picture wakes again"
    );
}

#[test]
fn pv3_events_queue_in_order_and_a_dropped_player_stops_the_reader() {
    let (sender, feed) = feed();
    assert!(sender.event(Event::Playing));
    assert!(sender.event(Event::Position(100)));
    assert_eq!(
        feed.incoming.try_recv(),
        Ok(Incoming::Event(Event::Playing))
    );
    assert_eq!(
        feed.incoming.try_recv(),
        Ok(Incoming::Event(Event::Position(100)))
    );
    drop(feed);
    assert!(
        !sender.event(Event::Paused),
        "the reader learns the player is gone"
    );
    assert!(!sender.frame(picture(1)));
}

// A helper that floods small records blocks on its own pipe; the UI thread's queue stays bounded.
#[test]
fn pv3_a_flooding_helper_is_bounded() {
    let line = Event::Position(1).render();
    let mut one = header(Record::Event {
        length: line.len() as u32,
    })
    .to_vec();
    one.extend(line.bytes());
    let bytes: Vec<u8> = one
        .iter()
        .copied()
        .cycle()
        .take(one.len() * 100_000)
        .collect();
    let (sender, feed) = feed();
    let reader = std::thread::spawn(move || read(bytes.as_slice(), &sender));
    std::thread::sleep(std::time::Duration::from_millis(300));
    let queued = feed.incoming.len();
    assert!(queued <= 512, "{queued} records queued for the UI thread");
    assert!(!reader.is_finished(), "the reader waits on the full queue");
    drop(feed);
    reader
        .join()
        .expect("the reader ends once the player lets go");
}
