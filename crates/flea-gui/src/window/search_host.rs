//! Effect staging keeps the real session published before navigation consumes it.
use crate::{
    g2::sr1::{self, Host, HostPane, Search},
    pane::swap::Session as PaneSession,
};
use flea::ui_logic_sr1::Wire;
use std::{cell::RefCell, rc::Rc};
pub(crate) struct Runtime {
    pub owner: Search,
    pub rows_generation: usize,
    pub strip: Rc<RefCell<sr1::strip::StripState>>,
    pub(super) origin: Option<crate::window::startup::View>,
    pub(super) scroll: f32,
}

impl Runtime {
    pub fn new(face: crate::list::Typeface) -> Self {
        Self {
            // The GUI wires the contents producer, so Ctrl+G switches the strip to contents instead of doing nothing.
            owner: Search::new(Wire {
                contents: true,
                ..Wire::default()
            }),
            rows_generation: 0,
            strip: Rc::new(RefCell::new(sr1::strip::StripState::new(face))),
            origin: None,
            scroll: 0.,
        }
    }
}

pub(super) enum Effect {
    Send(String),
    Relist(String, String, bool),
    Reset,
    Settle,
}

// Effects are applied only after publish, so navigation never observes the previous session copy.
pub(super) struct Pending<'a> {
    pub(super) pane: HostPane<'a>,
    pub(super) clipboard: Option<String>,
    pub(super) effects: Vec<Effect>,
    pub(super) session: Option<PaneSession>,
}

impl Host for Pending<'_> {
    fn pane(&self) -> HostPane<'_> {
        self.pane
    }
    fn send(&mut self, line: String) {
        self.effects.push(Effect::Send(line));
    }
    fn leave_recent(&mut self, path: &str) {
        self.effects
            .push(Effect::Relist(path.into(), String::new(), false));
    }
    fn reset_landed(&mut self) {
        self.effects.push(Effect::Reset);
    }
    fn relist(&mut self, path: &str, select: &str) {
        self.effects
            .push(Effect::Relist(path.into(), select.into(), true));
    }
    fn reveal(&mut self, folder: &str, select: &str) {
        self.effects
            .push(Effect::Relist(folder.into(), select.into(), false));
    }
    fn restart_settle(&mut self) {
        self.effects.push(Effect::Settle);
    }
    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.clone()
    }
    fn publish(&mut self, session: PaneSession) {
        self.session = Some(session);
    }
}
