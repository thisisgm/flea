//! The pinned Rust player and one frame callback shared by every Omi in a window.
use gpui::{App, EntityId, Global, Window, WindowId};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
    sync::OnceLock,
    time::Instant,
};

fn kit() -> &'static omi::Kit {
    static KIT: OnceLock<omi::Kit> = OnceLock::new();
    KIT.get_or_init(omi::Kit::bundled)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Thinking,
    Working,
    Upload,
    Download,
}
impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Working => "working",
            Self::Upload => "upload",
            Self::Download => "download",
        }
    }
    pub fn label(self) -> &'static str {
        kit()
            .mode(self.id())
            .and_then(|mode| mode.label.as_deref())
            .unwrap_or(self.id())
    }
    pub fn for_job(kind: &str) -> Self {
        match kind {
            "send" => Self::Upload,
            "fetch" | "download" => Self::Download,
            "nearby" => Self::Thinking,
            _ => Self::Working,
        }
    }
}

/// One Omi art grid in device pixels: a drawn side is a whole number of grids, never fewer than one.
pub const OMI_GRID_PX: f32 = 22.;

pub fn device_side(slot: f32) -> f32 {
    (slot.round() / OMI_GRID_PX).floor().max(1.) * OMI_GRID_PX
}

pub struct Motion {
    player: omi::Player,
    rects: Vec<omi::Rect>,
    since: Option<Instant>,
    seconds: f32,
}
impl Motion {
    pub fn new(mode: Mode) -> Self {
        Self {
            player: omi::Player::with_options(
                kit(),
                omi::Options {
                    mode: Some(mode.id().into()),
                    ..Default::default()
                },
            ),
            rects: Vec::new(),
            since: None,
            seconds: 0.,
        }
    }
    pub fn set(&mut self, mode: Mode) {
        if self.player.animate() {
            self.player.set(mode.id()).expect("bundled Omi mode");
        } else {
            self.player
                .set_instant(mode.id())
                .expect("bundled Omi mode");
        }
    }
    pub fn sample(&mut self, moving: bool, pinned: Option<f32>, side: f32, now: Instant) {
        self.player.set_body_motion(side >= 44.);
        if !moving || pinned.is_some() {
            self.rest();
        }
        if let Some(seconds) = pinned {
            self.seconds = seconds.max(0.);
            self.rects = self
                .player
                .mode_rects(self.player.mode(), f64::from(self.seconds))
                .expect("bundled Omi mode");
        } else if !moving {
            self.seconds = 0.;
            self.rects = self
                .player
                .mode_rects(self.player.mode(), 0.)
                .expect("bundled Omi mode");
        } else {
            let since = *self.since.get_or_insert(now);
            self.seconds = now.saturating_duration_since(since).as_secs_f32();
            self.player.frame(f64::from(self.seconds) * 1000.);
            if !self.player.animate() {
                self.player.set_animate(true);
            }
            self.rects = self.player.rects();
        }
    }
    // A still target owns both the displayed rects and the player's next loop.
    fn rest(&mut self) {
        if self.player.animate() {
            let mode = self.player.mode().to_owned();
            self.player.set_animate(false);
            self.player.set_instant(&mode).expect("bundled Omi mode");
        }
    }
    pub fn seconds(&self) -> f32 {
        self.seconds
    }
    pub fn rects(&self) -> &[omi::Rect] {
        &self.rects
    }
    pub fn view(&self) -> [f64; 4] {
        self.player.kit().view
    }
    #[cfg(test)]
    pub fn settled(&self) -> bool {
        self.player.settled()
    }
}

#[derive(Default)]
struct Frames(HashMap<WindowId, Weak<RefCell<Vec<EntityId>>>>);
impl Global for Frames {}

pub fn request_frame(window: &Window, cx: &mut App) {
    let frames = cx.default_global::<Frames>();
    frames.0.retain(|_, pending| pending.strong_count() > 0);
    let id = window.window_handle().window_id();
    let entity = window.current_view();
    if let Some(pending) = frames.0.get(&id).and_then(Weak::upgrade) {
        let mut pending = pending.borrow_mut();
        if !pending.contains(&entity) {
            pending.push(entity);
        }
        return;
    }
    let pending = Rc::new(RefCell::new(vec![entity]));
    frames.0.insert(id, Rc::downgrade(&pending));
    window.on_next_frame(move |_, cx| {
        for entity in pending.borrow().iter() {
            cx.notify(*entity);
        }
    });
}
