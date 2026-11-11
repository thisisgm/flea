//! The strip: its geometry and reorder math, its drag, hover and focus state, and the Element that paints it.
use super::{
    state::{Live, TabState},
    transfer::{Lift, PendingDrop},
};
use crate::list::{Palette, Typeface};
use gpui::{
    fill, point, px, relative, size, App, Bounds, CursorStyle, Element, ElementId, GlobalElementId,
    GlyphRun, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels,
    SharedString, Style, TextRun, TransformationMatrix, Window,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

// Board tabs-two.html: a 27 strip with a 1 hair, tabs from 14, 26 tall, label at 14, a 24 close slot, a 16 mark.
pub const HEIGHT: f32 = 27.;
pub const PAD_X: f32 = 14.;
pub const HIT_MIN: f32 = 24.;
const TAB_HEIGHT: f32 = 26.;
const LABEL_X: f32 = 14.;
const MARK: f32 = 16.;
const ACCENT_EDGE: f32 = 3.;
const SEPARATOR_TOP: f32 = 7.;
const SEPARATOR_HEIGHT: f32 = 12.;
const HAIR: f32 = 0.12;
// The keyboard frame lies inside a tab's box, clear of the rule, the current tab's accent edge and the separator.
const FRAME_INSET: f32 = 3.;
// The tab a drag holds draws ghosted, at the shipped disabled opacity.
const GHOST: f32 = 0.55;

// How long a drag rests on a tab before the tab is selected: long enough to cross it on the way elsewhere.
pub const HOVER_SWITCH_MS: u64 = 400;
// Acknowledged lifts settle when their captured pane finishes loading.
pub const ACK_POLL_MS: u64 = 100;
// Board tabs.html: the tab menu opens 3 under the strip, clear of the current tab's accent bar.
pub const MENU_GAP: f32 = 3.;

const X_MARK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M6 6l12 12 M18 6 6 18"/></svg>"#;
const PLUS_MARK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M5 12h14 M12 5v14"/></svg>"#;

// A drag names the insertion point, not the tab it takes.
pub fn insertion_at(x: f64, tab_width: f64, count: usize) -> usize {
    if tab_width.is_nan() || tab_width <= 0. || count == 0 {
        return 0;
    }
    let slot = (x / tab_width + 0.5).floor();
    slot.clamp(0., count as f64) as usize
}

// The { and } keys move one place, clamping at either end rather than wrapping.
pub fn step(from: usize, delta: isize, count: usize) -> usize {
    let last = count.saturating_sub(1) as isize;
    (from as isize + delta).clamp(0, last) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailDrop {
    pub to: usize,
    pub line: usize,
}

// A favourite drag answers where the row lands and the line draws: (40, 1, 30, 5) drops at 2, line at 3.
pub fn rail_reorder(dy: f64, from: usize, row_height: f64, count: usize) -> RailDrop {
    let n = count.max(1) as i64;
    let moved = (dy / row_height + 0.5).floor() as i64;
    let from = from as i64;
    RailDrop {
        to: (from + moved).clamp(0, n - 1) as usize,
        line: (from + moved + i64::from(dy >= 0.)).clamp(0, n) as usize,
    }
}

// A move keeps the current tab current: the dragged tab follows its destination, and a crossed tab shifts back.
pub fn reorder<T>(items: &mut Vec<T>, from: usize, to: usize, index: usize) -> usize {
    if from >= items.len() || to >= items.len() || from == to {
        return index;
    }
    let moving = items.remove(from);
    items.insert(to, moving);
    if index == from {
        to
    } else if from < index && to >= index {
        index - 1
    } else if from > index && to <= index {
        index + 1
    } else {
        index
    }
}

// The strip's width share: a caption's twelve cells plus the close slot, never under three close slots.
pub fn tab_width(width: f32, count: usize, caption: f32) -> f32 {
    let avail = (width - HIT_MIN).max(0.);
    let most = (caption * 12.).round() + HIT_MIN + 2. * PAD_X;
    let least = HIT_MIN * 3.;
    least.max(most.min(avail / count.max(1) as f32)).round()
}

// The window's panes as the strip reaches them; the window's shell and the tests' world both answer it.
pub trait Panes {
    // Runs f on side's strip and pane; false when side has no pane.
    fn visit(&mut self, side: usize, f: &mut dyn FnMut(&mut TabState, &mut dyn Live)) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(usize),
    Close(usize),
    New,
    Nothing,
}

#[derive(Debug, Default)]
pub struct Bar {
    // The pane the strip stands for: the focused pane, which Dual composition names.
    pub pane: Option<usize>,
    pub width: f32,
    pub caption: f32,
    pub self_pid: String,
    // dragFrom is the tab held, drop_at the insertion point (0..=count) its pointer names.
    pub drag_from: Option<usize>,
    pub drop_at: Option<usize>,
    // Gesture state, separate from the outstanding lifts awaiting their own ack.
    pub out_payload: String,
    pub out_active: bool,
    pub out_outside: bool,
    pub out_refused: bool,
    pub out_index: Option<usize>,
    pub out_path: String,
    pub out_pid: String,
    pub out_token: String,
    pub own_accepted: bool,
    pub pending: Option<PendingDrop>,
    pub deadline: Option<u64>,
    // The lift outlives the drag's own end, until the taken ack or the wait ends.
    pub ack_lifted_at: u64,
    pub taken_queue: VecDeque<(String, String)>,
    pub ack_running: bool,
    pub lifts: Vec<Lift>,
    // The strip's focus stop: the tab at focused_index, or New tab one past the last; the focus ring moves it.
    pub stop_focused: bool,
    pub focused_index: usize,
    pub hover: Option<(usize, u64)>,
    pub close_hover: Option<usize>,
    // Settings > View's Show tabs as icons: every tab narrows to its mark and has no close slot.
    pub icons: bool,
    // The tab count close_hover was read against.
    hover_count: usize,
    serial: u64,
}

impl Bar {
    pub fn new(self_pid: impl Into<String>) -> Self {
        Self {
            self_pid: self_pid.into(),
            caption: 12.,
            ..Self::default()
        }
    }

    pub(crate) fn next_serial(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }

    pub fn count(&self, panes: &mut dyn Panes) -> usize {
        let mut count = 1;
        if let Some(side) = self.pane {
            panes.visit(side, &mut |state, _| count = state.count());
        }
        count
    }

    pub fn tab_width(&self, count: usize) -> f32 {
        if self.icons {
            super::icons::WIDTH
        } else {
            tab_width(self.width, count, self.caption)
        }
    }

    pub fn drag_to(&self) -> Option<usize> {
        let (from, at) = (self.drag_from?, self.drop_at?);
        Some(if at > from { at - 1 } else { at })
    }

    // The slot each tab draws in while a drag holds one; slot i holds layout_order()[i].
    pub fn layout_order(&self, count: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..count).collect();
        if let (Some(from), Some(to)) = (self.drag_from, self.drag_to()) {
            reorder(&mut order, from, to, 0);
        }
        order
    }

    pub fn drag_started(&mut self, index: Option<usize>) {
        self.drag_from = index;
        self.drop_at = index.map(|index| index + 1);
    }

    pub fn drag_moved(&mut self, x: f64, count: usize) {
        if self.drag_from.is_some() {
            self.drop_at = Some(insertion_at(x, f64::from(self.tab_width(count)), count));
        }
    }

    pub fn drag_finished(&mut self, panes: &mut dyn Panes, now: u64) {
        if let (Some(from), Some(to), Some(side)) = (self.drag_from, self.drag_to(), self.pane) {
            panes.visit(side, &mut |state, pane| state.move_tab(pane, from, to));
        }
        self.drag_from = None;
        self.drop_at = None;
        self.drain_lifts(panes, now);
    }

    // A drag released by the platform without a move clears the gesture and a lift that never left.
    pub fn drag_cancelled(&mut self) {
        self.drag_from = None;
        self.drop_at = None;
        if !self.out_active && self.ack_lifted_at == 0 {
            self.clear_ack();
        }
    }

    // Only an accepted enter arms the switch, so a refused drag never leaves a timer running.
    pub fn hover_entered(&mut self, index: usize, accepted: bool, now: u64) {
        if accepted {
            self.hover = Some((index, now));
        }
    }

    pub fn hover_exited(&mut self) {
        self.hover = None;
    }

    // The resting drag selects its tab once the switch delay has passed; true when it did.
    pub fn hover_tick(&mut self, panes: &mut dyn Panes, now: u64) -> bool {
        let (Some((index, since)), Some(side)) = (self.hover, self.pane) else {
            return false;
        };
        if now.saturating_sub(since) < HOVER_SWITCH_MS {
            return false;
        }
        self.hover = None;
        let mut switched = false;
        panes.visit(side, &mut |state, pane| {
            if state.current() != index {
                state.select_at(pane, index);
                switched = true;
            }
        });
        switched
    }

    // What a strip point names, x from the strip's left edge; the close slot never shrinks.
    pub fn hit(&self, x: f32, count: usize) -> Hit {
        let width = self.tab_width(count);
        let local = x - PAD_X;
        if local < 0. {
            return Hit::Nothing;
        }
        let slot = (local / width).floor() as usize;
        if slot < count {
            let order = self.layout_order(count);
            let index = order[slot];
            return if !self.icons && local >= (slot as f32 + 1.) * width - HIT_MIN {
                Hit::Close(index)
            } else {
                Hit::Tab(index)
            };
        }
        if local < count as f32 * width + HIT_MIN {
            Hit::New
        } else {
            Hit::Nothing
        }
    }

    // The close mark under the pointer brightens; a running drag or a pointer off the strip names none.
    pub fn hovered(&mut self, x: Option<f32>, count: usize) -> bool {
        let hit = x
            .filter(|_| self.drop_at.is_none())
            .map(|x| self.hit(x, count));
        let hover = match hit {
            Some(Hit::Close(index)) => Some(index),
            _ => None,
        };
        let changed = hover != self.close_hover;
        self.close_hover = hover;
        self.hover_count = count;
        changed
    }

    // A strip that hides or changes its count keeps no bright mark; the next pointer move names one again.
    pub fn settle(&mut self, count: Option<usize>) {
        if count != Some(self.hover_count) {
            self.close_hover = None;
        }
    }

    // A secondary press on a tab or its mark asks the tab menu for that tab, its top MENU_GAP under the strip.
    pub fn menu_at(&self, x: f32, count: usize) -> Option<MenuAsk> {
        match self.hit(x, count) {
            Hit::Tab(index) | Hit::Close(index) => Some(MenuAsk {
                index,
                x,
                y: HEIGHT + MENU_GAP,
            }),
            Hit::New | Hit::Nothing => None,
        }
    }

    // A press on the strip: a middle press closes the tab, a left press closes on the mark and selects elsewhere.
    pub fn tap(&mut self, panes: &mut dyn Panes, x: f32, middle: bool) {
        let Some(side) = self.pane else { return };
        let count = self.count(panes);
        let hit = self.hit(x, count);
        panes.visit(side, &mut |state, pane| match (hit, middle) {
            (Hit::Tab(index) | Hit::Close(index), true) => state.close_at(pane, index),
            (Hit::Close(index), false) => state.close_at(pane, index),
            (Hit::Tab(index), false) => state.select_at(pane, index),
            (Hit::New, false) => state.open_new(pane, None),
            _ => {}
        });
    }

    // The frame the strip paints this turn, read once off the model.
    pub fn frame(
        &self,
        state: &TabState,
        current_path: &str,
        home: &str,
        home_label: &str,
    ) -> Frame {
        let count = state.count();
        let order = self.layout_order(count);
        let mut slots = vec![0; count];
        for (slot, index) in order.iter().enumerate() {
            slots[*index] = slot;
        }
        Frame {
            labels: state
                .labels(current_path, home, home_label)
                .into_iter()
                .map(SharedString::from)
                .collect(),
            current: state.current(),
            slots,
            ghost: self.drag_from.filter(|_| self.drop_at.is_some()),
            line: self.drag_to(),
            focus: self.stop_focused.then_some(self.focused_index),
            close_hover: self.close_hover,
            tab_width: self.tab_width(count),
            icons: None,
        }
    }
}

// The tab a menu acts on and where its top-left sits, strip-relative; the menu host owns the entries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuAsk {
    pub index: usize,
    pub x: f32,
    pub y: f32,
}

// What one frame of the strip draws.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub labels: Vec<SharedString>,
    pub current: usize,
    // The slot each tab draws in.
    pub slots: Vec<usize>,
    pub ghost: Option<usize>,
    pub line: Option<usize>,
    pub focus: Option<usize>,
    pub close_hover: Option<usize>,
    pub tab_width: f32,
    // Each tab's mark and whole path while Show tabs as icons is on.
    pub icons: Option<super::icons::Icons>,
}

// Shaped labels kept across frames, each already elided to its tab; nine tabs bound it.
pub struct Labels {
    pub typeface: Typeface,
    entries: Vec<(SharedString, u32, GlyphRun)>,
    drawn: Vec<usize>,
    pub misses: u64,
}

impl Labels {
    pub fn new(typeface: Typeface) -> Self {
        Self {
            typeface,
            entries: Vec::new(),
            drawn: Vec::new(),
            misses: 0,
        }
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
    }

    fn shape(&self, text: &str, window: &Window) -> GlyphRun {
        let run = TextRun {
            len: text.len(),
            font: self.typeface.font.clone(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        GlyphRun::new(
            window
                .text_system()
                .layout_line(text, self.typeface.caption, &[run], None),
        )
    }

    // Right elision, the shipped label's own: the longest prefix that fits beside its ellipsis.
    fn elided(&self, label: &str, room: f32, window: &Window) -> GlyphRun {
        let whole = self.shape(label, window);
        if f32::from(whole.line().width) <= room {
            return whole;
        }
        // ends[k - 1] is where the first k characters end.
        let ends: Vec<usize> = label
            .char_indices()
            .map(|(at, _)| at)
            .skip(1)
            .chain([label.len()])
            .collect();
        let (mut low, mut high) = (0, ends.len().saturating_sub(1));
        let mut best = self.shape("\u{2026}", window);
        while low < high {
            let mid = (low + high).div_ceil(2);
            let run = self.shape(&format!("{}\u{2026}", &label[..ends[mid - 1]]), window);
            if f32::from(run.line().width) <= room {
                best = run;
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        best
    }

    fn prepare(&mut self, frame: &Frame, window: &Window) {
        let room = (frame.tab_width - LABEL_X - HIT_MIN).max(0.);
        let key = room.round() as u32;
        // Over its bound the cache keeps only this frame's own labels, so nothing drawn is ever evicted.
        if self.entries.len() + frame.labels.len() > 2 * super::state::MAX {
            self.entries
                .retain(|(text, width, _)| *width == key && frame.labels.contains(text));
        }
        self.drawn.clear();
        for label in &frame.labels {
            let found = self
                .entries
                .iter()
                .position(|(text, width, _)| *width == key && text == label);
            let at = match found {
                Some(at) => at,
                None => {
                    self.misses += 1;
                    let run = self.elided(label, room, window);
                    self.entries.push((label.clone(), key, run));
                    self.entries.len() - 1
                }
            };
            self.drawn.push(at);
        }
    }
}

pub struct StripElement {
    frame: Frame,
    labels: Rc<RefCell<Labels>>,
    feedback: Option<Rc<RefCell<super::held::StripFrames>>>,
    controls: Option<super::accessibility::Controls>,
}

#[path = "bar_feedback.rs"]
mod feedback;
use feedback::frame_box;

impl IntoElement for StripElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[path = "bar_element.rs"]
mod element;
