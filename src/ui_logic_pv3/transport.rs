//! The helper's play state over libmpv: which pause changes the GUI hears, and when Play must rewind first.
use super::frame::Event;

/// libmpv reports a property only when its value changes, so stop and end are tracked here rather than as one-shot flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transport {
    loaded: bool,
    paused: bool,
    /// Stopped until the next unpause: the pause changes a stop causes are not news.
    stopped: bool,
    /// At the end of the clip, where keep-open parks the decoder paused.
    ended: bool,
    /// mpv refuses a seek until the file loads, so the newest target before then waits here.
    pending: Option<i64>,
}

impl Transport {
    pub fn new(autoplay: bool) -> Self {
        Self {
            loaded: false,
            paused: !autoplay,
            stopped: false,
            ended: false,
            pending: None,
        }
    }

    /// The file opened: Loaded is followed by where play stands.
    pub fn loaded(&mut self) -> Event {
        self.loaded = true;
        if self.paused {
            Event::Paused
        } else {
            Event::Playing
        }
    }

    /// Where to seek now; before the file loads the target waits for `pending`, the newest one winning.
    pub fn seek(&mut self, target: i64) -> Option<i64> {
        if self.loaded {
            return Some(target);
        }
        self.pending = Some(target);
        None
    }

    /// The seek that waited for the load, taken once.
    pub fn pending(&mut self) -> Option<i64> {
        self.pending.take()
    }

    /// Play at the end starts over, as QMediaPlayer::play does at EndOfMedia; true when the caller must seek to 0 first.
    pub fn play(&mut self) -> bool {
        self.ended
    }

    /// Stop pauses and rewinds; the caller sends the returned event at once.
    pub fn stop(&mut self) -> Event {
        self.stopped = true;
        Event::Stopped
    }

    /// What a change of the pause flag tells the GUI: a stop's own pause is not news, and the end's pause is the end.
    pub fn paused(&mut self, paused: bool) -> Option<Event> {
        self.paused = paused;
        if !self.loaded {
            return None;
        }
        if paused {
            return match (self.stopped, self.ended) {
                (true, _) => None,
                (false, true) => Some(Event::Ended),
                (false, false) => Some(Event::Paused),
            };
        }
        self.stopped = false;
        Some(Event::Playing)
    }

    /// What a change of the end flag tells the GUI: mpv sets it once the decoder drains, while buffered audio still plays,
    /// so the end is heard when keep-open parks the clip paused, or at once when it was already paused.
    pub fn ended(&mut self, ended: bool) -> Option<Event> {
        self.ended = ended;
        (ended && self.paused && self.loaded && !self.stopped).then_some(Event::Ended)
    }
}
