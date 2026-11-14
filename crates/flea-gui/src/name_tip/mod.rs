//! One resting-pointer expansion for the visible List or Columns name, shared by a window's panes.
mod columns;
mod element;
mod events;
pub(crate) use events::key_events;
mod list;
mod shell;
pub(crate) use shell::{picker, picker_keys};
#[cfg(test)]
mod list_tests;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use element::Overlay;
use gpui::{Bounds, Font, GlyphRun, Hsla, Pixels, Point, Task};
pub(crate) use list::list_candidate;
use std::{cell::RefCell, rc::Rc};

// The tab path tip's existing pointer-rest delay, without a new setting.
const REST_MS: u64 = crate::tabs::icons::REST_MS;

#[derive(Clone, PartialEq)]
pub(crate) struct Candidate {
    // The painter, listing, track and index distinguish equal names in different panes and generations.
    identity: (usize, usize, usize, usize),
    epoch: usize,
    name: String,
    font: Font,
    size: Pixels,
    row: Bounds<Pixels>,
    hit: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    text_x: Pixels,
    height: Pixels,
    pad: Pixels,
    ground: Hsla,
    ink: Hsla,
    framed: bool,
    hair: Hsla,
}

impl Candidate {
    fn same_name(&self, next: &Self) -> bool {
        self.identity == next.identity
            && self.epoch == next.epoch
            && self.name == next.name
            && self.font == next.font
            && self.size == next.size
            && self.row == next.row
            && self.hit == next.hit
            && self.clip == next.clip
    }
}

// Resolve the text line box before Dual narrows its independent mark slot.
fn line_height(face: &crate::list::Typeface, row_height: f32) -> Pixels {
    gpui::px(
        crate::list::Metrics::for_typeface(face)
            .slot
            .min(row_height),
    )
}

fn plate_radius(rounding: f32) -> Pixels {
    gpui::px(rounding / 2.)
}

struct Shaped {
    run: GlyphRun,
}

#[derive(Default)]
pub(crate) struct State {
    candidate: Option<Candidate>,
    shaped: Option<Shaped>,
    timer: Option<Task<()>>,
    ready: bool,
    suppressed: bool,
    pressed: bool,
    pointer: Option<Point<Pixels>>,
    generation: u64,
    // Monotonic work counters, also read by the deterministic regression gate.
    names_shaped: usize,
    tips_built: usize,
}

impl State {
    fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.candidate = None;
        self.shaped = None;
        self.timer = None;
        self.ready = false;
    }

    fn dismiss(&mut self) -> bool {
        let shown = self.shaped.is_some();
        self.clear();
        self.suppressed = true;
        shown
    }

    fn moved(&mut self, at: Point<Pixels>, pressed: bool) -> bool {
        if self.pointer == Some(at) && self.pressed == pressed {
            return false;
        }
        self.clear();
        self.pointer = Some(at);
        self.pressed = pressed;
        self.suppressed = false;
        true
    }

    fn pointer_moved(&mut self, at: Point<Pixels>, pressed: bool, next: Option<Candidate>) -> bool {
        if !pressed
            && !self.suppressed
            && self.shaped.is_some()
            && self
                .candidate
                .as_ref()
                .zip(next.as_ref())
                .is_some_and(|(previous, next)| previous.same_name(next) && next.hit.contains(&at))
        {
            self.pointer = Some(at);
            self.pressed = false;
            self.candidate = next;
            return false;
        }
        self.moved(at, pressed)
    }

    fn reconcile(&mut self, next: Option<Candidate>) -> bool {
        let next = next.filter(|_| !self.suppressed && !self.pressed);
        if self.candidate == next {
            return false;
        }
        // A still pointer must not transfer a pending rest to replaced rows, a menu or another view.
        if let Some(previous) = &self.candidate {
            if next.as_ref().is_some_and(|next| previous.same_name(next)) {
                self.candidate = next;
                return false;
            }
            self.dismiss();
            return false;
        }
        self.clear();
        self.candidate = next;
        self.candidate.is_some()
    }
}

pub(crate) type Owner = Rc<RefCell<State>>;

// Compare the retained label before any shaping. A clipped last glyph also warrants expansion.
fn cut(name: &str, label: &str, width: Pixels, room: Pixels) -> bool {
    !name.is_empty() && (name != label || width > room + gpui::px(crate::row::FIT_SLACK))
}
