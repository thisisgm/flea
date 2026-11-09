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
    /// Decoder EOF can precede the audible end while the sink drains.
    eof: bool,
    /// Manual pause intent survives notifications until observed unpause, Stop or a completed endpoint seek.
    pause_requested: bool,
    /// Accepted seeks suppress old EOF until their native completion is read.
    seeking: bool,
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
            eof: false,
            pause_requested: !autoplay,
            seeking: false,
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

    /// Acceptance leaves pause intent intact and invalidates the former endpoint until native completion.
    pub fn seek_accepted(&mut self) {
        self.pause_requested |= self.paused;
        self.seeking = true;
        self.ended = false;
    }

    /// Fresh native state after seeking finishes distinguishes the endpoint from decoder EOF over a buffered tail.
    pub fn seek_completed(&mut self, paused: bool, eof: bool, at_end: bool) -> Option<Event> {
        self.seeking = false;
        self.eof = eof;
        self.pause_requested &= !at_end;
        self.ended = at_end && paused && !self.stopped;
        self.paused(paused)
    }

    /// The seek that waited for the load, taken once.
    pub fn pending(&mut self) -> Option<i64> {
        self.pending.take()
    }

    /// Play at the end starts over, as QMediaPlayer::play does at EndOfMedia; true when the caller must seek to 0 first.
    pub fn play(&mut self) -> bool {
        self.ended
    }

    /// Records the user's pause before its property change arrives.
    pub fn pause(&mut self) {
        self.pause_requested |= !self.paused;
    }

    /// Stop pauses and rewinds; the caller sends the returned event at once.
    pub fn stop(&mut self) -> Event {
        self.stopped = true;
        self.seeking = false;
        self.ended = false;
        self.pause_requested = false;
        Event::Stopped
    }

    /// What a change of the pause flag tells the GUI: a stop's own pause is not news, and the end's pause is the end.
    pub fn paused(&mut self, paused: bool) -> Option<Event> {
        self.paused = paused;
        if !self.loaded {
            return None;
        }
        if paused {
            if self.stopped {
                return None;
            }
            self.ended |= self.eof && !self.pause_requested && !self.seeking;
            return Some(if self.ended {
                Event::Ended
            } else {
                Event::Paused
            });
        }
        self.pause_requested = false;
        self.stopped = false;
        self.ended = false;
        Some(Event::Playing)
    }

    /// Decoder EOF becomes audible completion only when keep-open pauses, or a paused seek reaches the end.
    pub fn ended(&mut self, ended: bool) -> Option<Event> {
        self.eof = ended;
        if !ended {
            return None;
        }
        if self.paused && self.loaded && !self.stopped && !self.pause_requested && !self.seeking {
            self.ended = true;
            Some(Event::Ended)
        } else {
            None
        }
    }
}
