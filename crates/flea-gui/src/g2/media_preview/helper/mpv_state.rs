use super::{rotation::Rotation, DURATION, MAX_SIDE, VIDEO};
use flea::ui_logic_media_preview::{
    frame::Event, picture::decode_size, tonemap::Tonemap, tonemap_pace::Pace, transport::Transport,
};
use std::time::Instant;

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
    pub(super) rotation: Rotation,
    pub(super) tonemap: Tonemap,
    /// Frames sent since the helper started, which the tone map's pace counts against the clip's own.
    pub(super) presented: u64,
    pub(super) pace: Pace,
    /// The pace's wall clock starts here.
    pub(super) origin: Instant,
    /// The CPUs the helper may use, as available_parallelism reports them inside the jail.
    pub(super) cpus: u32,
    /// A short window not caused by the map was logged once.
    pub(super) pace_noted: bool,
    /// The level whose floor was logged, so each floor is logged once.
    pub(super) floor_logged: Option<u32>,
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
            rotation: Rotation::None,
            box_size: (0, 0),
            tonemap: Tonemap::default(),
            presented: 0,
            pace: Pace::default(),
            origin: Instant::now(),
            cpus: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
            pace_noted: false,
            floor_logged: None,
        }
    }

    pub(super) fn facts(&self) -> Event {
        Event::Loaded {
            video: self.video,
            duration_ms: self.duration_ms,
        }
    }

    /// Fit upright dimensions so the wire bounds and the host's box apply after rotation.
    pub(super) fn update_size(&mut self) {
        // Under the tone map mpv's dwidth and dheight are the mapped size, and they can arrive one event apart.
        if let Some(size) = self.tonemap.size() {
            self.size = size;
            return;
        }
        let source = (
            self.clip.0.clamp(0, MAX_SIDE) as u32,
            self.clip.1.clamp(0, MAX_SIDE) as u32,
        );
        let (width, height) = self.rotation.dimensions(source);
        self.size = decode_size(width, height, self.box_size.0, self.box_size.1);
    }

    /// A metadata angle that arrives after the clip size refits the upright picture at once.
    pub(super) fn rotate(&mut self, degrees: i64) {
        self.rotation = Rotation::from_degrees(degrees);
        self.update_size();
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
