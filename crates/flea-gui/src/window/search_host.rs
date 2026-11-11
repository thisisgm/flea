//! Effect staging keeps the real session published before navigation consumes it.
use crate::{
    g2::file_search::{self, Host, HostPane, Search},
    pane::swap::Session as PaneSession,
};
use flea::ui_logic_file_search::Wire;
use std::{cell::RefCell, rc::Rc};
pub(crate) struct Runtime {
    pub(crate) kept: Option<super::Kept>,
    pub owner: Search,
    pub input: Option<gpui::Entity<file_search::input::QueryInput>>,
    pub rows_generation: usize,
    /// The generation an Enter committed before its rows landed; the row acts when they do.
    pub(crate) enter_after: Option<u64>,
    pub strip: Rc<RefCell<file_search::strip::StripState>>,
    pub(super) origin: Option<crate::window::startup::View>,
    pub(super) scroll: f32,
    /// The contents blocks' shaped state, made again when the list's words or face change.
    pub(super) contents: Option<(
        Rc<crate::row::format::Words>,
        Rc<RefCell<file_search::contents::ContentsState>>,
    )>,
}

impl Runtime {
    pub(crate) fn set_typeface(&mut self, face: crate::list::Typeface) {
        let mut strip = self.strip.borrow_mut();
        if strip.typeface != face {
            strip.typeface = face;
            self.contents = None;
        }
    }

    pub fn new(face: crate::list::Typeface) -> Self {
        Self {
            kept: None,
            // The GUI wires the contents producer, so Ctrl+G switches the strip to contents instead of doing nothing.
            owner: Search::new(Wire {
                contents: true,
                ..Wire::default()
            }),
            input: None,
            rows_generation: 0,
            enter_after: None,
            strip: Rc::new(RefCell::new(file_search::strip::StripState::new(face))),
            origin: None,
            scroll: 0.,
            contents: None,
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

impl crate::state::Shell {
    pub(crate) fn search_input(
        &mut self,
        side: usize,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Entity<file_search::input::QueryInput>> {
        use gpui::AppContext;
        let host = self.panes[side].as_mut()?;
        let generation = host.search.owner.entry.borrow().session()?;
        if host
            .search
            .input
            .as_ref()
            .is_none_or(|input| input.read(cx).generation != generation)
        {
            let strip = host.search.strip.borrow();
            let input = file_search::input::QueryInput::new(
                cx.weak_entity(),
                side,
                generation,
                host.search.owner.entry.clone(),
                strip.edit.line.clone(),
                strip.edit.input_bounds.clone(),
            );
            host.search.input = Some(cx.new(|_| input));
        }
        host.search.input.clone()
    }
}
