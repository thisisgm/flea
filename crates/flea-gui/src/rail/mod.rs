//! The rail host: when the rail is drawn, where it stands and who holds the keyboard; the rows are the places producer's.
pub mod geometry;
pub mod host;

use flea::jsondoc::Json;
use geometry::{Placement, Point};
use std::time::{Duration, Instant};

// ui/js/Motion.js: OutCubic, 180 ms to reveal, 140 ms to hide, a 120 ms rest at the edge.
pub const OPEN_MS: u64 = 180;
pub const CLOSE_MS: u64 = 140;
pub const DWELL_MS: u64 = 120;
// ui/PaneRail.qml settleMs: crossing the edge on the way elsewhere never flashes the rail.
pub const SETTLE_MS: u64 = 220;

/// How the saved places settings place the rail; auto-hide wins over Show sidebar, as in Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Docked,
    Hidden,
    Overlay,
}

impl Mode {
    pub fn from_places(places: &Json) -> Self {
        if places.get("autoHide").and_then(Json::as_bool) == Some(true) {
            Self::Overlay
        } else if places.get("rail").and_then(Json::as_str) == Some("hidden") {
            Self::Hidden
        } else {
            Self::Docked
        }
    }
}

/// Where a press or a hide sends the keyboard; the focus owner applies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handoff {
    Rail,
    List,
}

/// The fill the active place draws: Omarchy's grey selected fill unless the rail holds the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plate {
    Grey,
    Accent,
}

/// What asks for the overlay rail; any one of them keeps it up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wants {
    pub edge: bool,
    pub over: bool,
    pub menu: bool,
    pub releasing: bool,
    pub drag: bool,
    pub focus: bool,
}

/// Work the host started, so a test can prove a withdrawn or hidden rail starts none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub builds: u32,
    pub timers: u32,
    pub reveals: u32,
}

#[derive(Clone, Copy, Debug)]
struct Slide {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Slide {
    fn at(&self, now: Instant) -> f32 {
        let total = self.duration.as_secs_f32();
        let t = if total == 0. {
            1.
        } else {
            (now.saturating_duration_since(self.start).as_secs_f32() / total).min(1.)
        };
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.to - self.from) * eased
    }
}

/// The ui/PaneRail.qml state machine on an explicit clock; the GPUI host feeds it events and arms its one timer.
pub struct Rail {
    mode: Mode,
    wants: Wants,
    edge_shown: bool,
    revealed: bool,
    built: bool,
    hold: bool,
    dwell: Option<Instant>,
    settle: Option<Instant>,
    slide: Slide,
    reduced: bool,
    pub counters: Counters,
}

impl Rail {
    pub fn new(mode: Mode, reduced: bool, now: Instant) -> Self {
        let mut rail = Self {
            mode,
            wants: Wants::default(),
            edge_shown: false,
            revealed: false,
            built: false,
            hold: false,
            dwell: None,
            settle: None,
            slide: Slide {
                from: 0.,
                to: 0.,
                start: now,
                duration: Duration::ZERO,
            },
            reduced,
            counters: Counters::default(),
        };
        rail.count_build(false);
        rail
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_reduced(&mut self, reduced: bool) {
        self.reduced = reduced;
    }

    /// A switch of the setting is a fresh start: nothing revealed until the pointer asks, and a held keyboard returns to the list.
    pub fn set_mode(&mut self, mode: Mode, now: Instant) -> Option<Handoff> {
        if mode == self.mode {
            return None;
        }
        let was_live = self.live();
        let overlay = (mode == Mode::Overlay) != (self.mode == Mode::Overlay);
        self.mode = mode;
        if overlay {
            self.dwell = None;
            self.settle = None;
            self.edge_shown = false;
            self.built = false;
            self.revealed = false;
            self.slide = Slide {
                from: 0.,
                to: 0.,
                start: now,
                duration: Duration::ZERO,
            };
        }
        self.count_build(was_live);
        let stranded = self.wants.focus && !self.shown();
        if stranded {
            self.wants.focus = false;
        }
        stranded.then_some(Handoff::List)
    }

    pub fn wants(&self) -> Wants {
        self.wants
    }

    /// The pointer entered or left the reveal strip; entering builds the rows so the dwell covers the build.
    pub fn edge(&mut self, hovered: bool, now: Instant) {
        if self.mode != Mode::Overlay {
            return;
        }
        self.wants.edge = hovered;
        if !hovered {
            self.dwell = None;
            self.edge_shown = false;
        } else {
            let was_live = self.live();
            self.built = true;
            self.count_build(was_live);
            if self.revealed {
                self.edge_shown = true;
            } else if self.dwell.is_none() {
                self.dwell = Some(now + Duration::from_millis(DWELL_MS));
                self.counters.timers += 1;
            }
        }
        self.wanted(now);
    }

    pub fn over(&mut self, hovered: bool, now: Instant) {
        self.wants.over = hovered;
        self.wanted(now);
    }

    pub fn menu(&mut self, open: bool, now: Instant) {
        self.wants.menu = open;
        self.wanted(now);
    }

    pub fn drag(&mut self, over: bool, now: Instant) {
        self.wants.drag = over;
        self.wanted(now);
    }

    /// A release in flight keeps the rows alive, undrawn, through a hide, and holds an overlay rail up.
    pub fn releasing(&mut self, active: bool, now: Instant) {
        self.wants.releasing = active;
        if active {
            self.hold = true;
        } else {
            let was_live = self.live();
            self.hold = false;
            self.count_build(was_live);
        }
        self.wanted(now);
    }

    /// The rail took or lost the keyboard; Tab in reveals at once and Tab out withdraws with no settle.
    pub fn focus(&mut self, held: bool, now: Instant) {
        if held == self.wants.focus {
            return;
        }
        self.wants.focus = held;
        self.wanted(now);
        if !held && !self.pointer_wanted() && self.revealed {
            self.settle = None;
            self.reveal(false, now);
        }
    }

    /// Fires a due dwell or settle; returns true when anything changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self.dwell.is_some_and(|at| at <= now) {
            self.dwell = None;
            self.edge_shown = true;
            self.wanted(now);
            changed = true;
        }
        if self.settle.is_some_and(|at| at <= now) {
            self.settle = None;
            self.reveal(false, now);
            changed = true;
        }
        changed
    }

    /// How long until the next dwell or settle is due, so the host arms exactly one timer.
    pub fn deadline(&self, now: Instant) -> Option<Duration> {
        [self.dwell, self.settle]
            .into_iter()
            .flatten()
            .min()
            .map(|at| at.saturating_duration_since(now))
    }

    fn pointer_wanted(&self) -> bool {
        let w = self.wants;
        self.edge_shown || w.over || w.menu || w.releasing || w.drag
    }

    fn wanted(&mut self, now: Instant) {
        if self.mode != Mode::Overlay {
            return;
        }
        if self.pointer_wanted() || self.wants.focus {
            self.settle = None;
            self.reveal(true, now);
        } else if self.revealed && self.settle.is_none() {
            self.settle = Some(now + Duration::from_millis(SETTLE_MS));
            self.counters.timers += 1;
        }
    }

    fn reveal(&mut self, on: bool, now: Instant) {
        if on == self.revealed {
            return;
        }
        let was_live = self.live();
        self.revealed = on;
        if on {
            self.built = true;
            self.counters.reveals += 1;
        }
        self.count_build(was_live);
        // A reversal starts from where the rail stands, never from an end.
        let from = self.slide.at(now);
        let to = if on { 1. } else { 0. };
        let ms = if on { OPEN_MS } else { CLOSE_MS };
        self.slide = Slide {
            from: if self.reduced { to } else { from },
            to,
            start: now,
            duration: Duration::from_millis(if self.reduced { 0 } else { ms }),
        };
    }

    fn count_build(&mut self, was_live: bool) {
        if !was_live && self.live() {
            self.counters.builds += 1;
        }
    }

    /// The rail is not withdrawn: docked and shown, or an overlay the pointer or keyboard revealed.
    pub fn shown(&self) -> bool {
        match self.mode {
            Mode::Docked => true,
            Mode::Hidden => false,
            Mode::Overlay => self.revealed,
        }
    }

    /// PaneRail.qml revealed: an overlay keeps its settled reveal, a docked or hidden rail reads the pointer and keyboard wants.
    pub fn revealed(&self) -> bool {
        match self.mode {
            Mode::Overlay => self.revealed,
            Mode::Docked | Mode::Hidden => self.pointer_wanted() || self.wants.focus,
        }
    }

    /// The places rows exist: built once for an overlay and then kept, so later reveals keep cursor and scroll.
    pub fn live(&self) -> bool {
        self.hold
            || match self.mode {
                Mode::Docked => true,
                Mode::Hidden => false,
                Mode::Overlay => self.built,
            }
    }

    fn held(&self) -> bool {
        self.hold && !self.shown()
    }

    /// 0 withdrawn to 1 shown, OutCubic from where the last change found it.
    pub fn slide(&self, now: Instant) -> f32 {
        match self.mode {
            Mode::Overlay => self.slide.at(now),
            Mode::Docked => 1.,
            Mode::Hidden => 0.,
        }
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.mode == Mode::Overlay && self.slide.at(now) != self.slide.to
    }

    /// The rows paint: never while a release hold keeps them past a hide, and an overlay only once it has moved in.
    pub fn drawn(&self, now: Instant) -> bool {
        !self.held() && self.live() && self.slide(now) > 0.
    }

    /// The places producer may poll devices and mounts only while the rows are alive and up.
    pub fn polling(&self) -> bool {
        self.live() && !self.held() && self.shown()
    }

    pub fn plate(&self) -> Plate {
        if self.wants.focus {
            Plate::Accent
        } else {
            Plate::Grey
        }
    }

    /// A press in the window: on a drawn overlay rail it claims the keyboard, elsewhere the list takes it.
    pub fn press(&self, placement: &Placement, point: Point, now: Instant) -> Option<Handoff> {
        if !placement.column.contains(point) {
            return None;
        }
        if self.shown() && self.drawn(now) && placement.rail.contains(point) {
            return (self.mode == Mode::Overlay).then_some(Handoff::Rail);
        }
        Some(Handoff::List)
    }

    /// A row drag over the reveal area no rail row took is refused; before the first build and while a release hold keeps the rows undrawn the area is the strip.
    pub fn refuses_drop(&self, placement: &Placement, point: Point) -> bool {
        // ui/PaneRail.qml dragReveal is max(strip, rail) and the rail is 0 wide until built and while held.
        let lane = if self.live() && !self.held() {
            placement.drop
        } else {
            placement.strip
        };
        self.mode == Mode::Overlay && lane.contains(point)
    }

    pub fn placement(&self, width: f32, column: geometry::Rect, now: Instant) -> Placement {
        geometry::place(self.mode, width, column, self.slide(now))
    }
}
