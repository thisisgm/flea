use super::{DURATION, VIDEO};
use flea::ui_logic_media_preview::{frame::Event, tonemap::Tonemap, transport::Transport};

/// What the loop knows about the open file.
pub(super) struct State {
    pub(super) transport: Transport,
    pub(super) loaded: bool,
    pub(super) video: bool,
    pub(super) duration_ms: i64,
    pub(super) position_ms: i64,
    pub(super) sent_ms: i64,
    pub(super) seeking: Option<i64>,
    pub(super) clip: (i64, i64),
    pub(super) box_size: (u32, u32),
    pub(super) size: (u32, u32),
    pub(super) tonemap: Tonemap,
}

impl State {
    pub(super) fn new(autoplay: bool) -> Self {
        Self {
            transport: Transport::new(autoplay),
            loaded: false,
            video: false,
            duration_ms: 0,
            position_ms: 0,
            sent_ms: 0,
            seeking: None,
            clip: (0, 0),
            size: (0, 0),
            box_size: (0, 0),
            tonemap: Tonemap::default(),
        }
    }

    pub(super) fn facts(&self) -> Event {
        Event::Loaded {
            video: self.video,
            duration_ms: self.duration_ms,
        }
    }

    pub(super) fn metadata(&mut self, property: u64, value: i64) -> Option<Event> {
        let before = self.facts();
        match property {
            DURATION => self.duration_ms = value,
            VIDEO => self.video = value > 0,
            _ => return None,
        }
        (self.loaded && self.facts() != before).then(|| self.facts())
    }
}
