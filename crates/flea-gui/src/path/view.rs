//! The path surface's state: the crumb strips, the open field and its jump, and what they ask of the window.
use super::{
    crumbs::{alias_at, Alias},
    entry::{Commit, Entry, Tab},
    field::Line,
    jump_line,
    paint::Paint,
    peek_line, should_navigate,
    strip::Strip,
    suggestions::{Ask, Enter, Jump, Pointer, Popup, Ranker, ANSWER_LIMIT_MS},
    Answer, Said,
};
use flea::i18n::I18n;
use gpui::{Context, EventEmitter, FocusHandle, SharedString, Subscription, Task, Window};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

/// What the window tells the path surface: each pane's drawn path and the facts a commit is judged by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inputs {
    pub git_label: String,
    /// Shared strings, so the window hands them over every frame without allocating.
    pub paths: [SharedString; 2],
    pub home: SharedString,
    pub aliases: Vec<Alias>,
    pub side: usize,
    pub dual: bool,
    pub failed: bool,
    pub show_hidden: bool,
    pub blocked: bool,
    pub address_path: bool,
    /// The picker's strip elides its parents from the left and keeps the leaf whole.
    pub elide_start: bool,
    /// The focused pane's window size, which the peek behind Tab asks for.
    pub first: usize,
}

/// What the path surface asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Open { side: usize, path: String },
    Network(String),
    Said(String),
    Send { side: usize, line: String },
    Focus(usize),
    ReadHistory,
    Closed,
}

pub struct PathView {
    pub(crate) drop_target: Option<(usize, usize)>,
    pub inputs: Inputs,
    pub entry: Entry,
    pub jump: Jump,
    pub strips: [Strip; 3],
    pub scroll: f32,
    pub(super) wheel: f32,
    pub focus: FocusHandle,
    pub paint: Rc<RefCell<Paint>>,
    pub line: RefCell<Line>,
    /// The field was opened at least once, so its frame has a place to report.
    pub built: bool,
    pub corner: f32,
    pub(crate) probe: Option<crate::visual::Probe>,
    pub(crate) pointer: Pointer,
    pub(super) pressed_row: Option<(usize, String)>,
    ranker: Box<dyn Ranker>,
    favourites: Vec<String>,
    recent: Vec<String>,
    pub(super) catalog: Arc<I18n>,
    asks: Vec<Ask>,
    hold: Option<Task<()>>,
    _blur: Option<Subscription>,
    /// The field's Undo, Cut, Copy, Paste and Select All card while a right press holds it open.
    pub(crate) text_menu: super::text_menu::TextMenu,
}

impl EventEmitter<Out> for PathView {}

impl PathView {
    pub fn new(catalog: Arc<I18n>, ranker: Box<dyn Ranker>, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        Self {
            inputs: Inputs::default(),
            entry: Entry::default(),
            jump: Jump::default(),
            strips: Default::default(),
            scroll: 0.,
            wheel: 0.,
            focus,
            paint: Rc::new(RefCell::new(Paint::new())),
            line: RefCell::new(Line::default()),
            built: false,
            drop_target: None,
            corner: 0.,
            probe: None,
            pointer: Pointer::default(),
            pressed_row: None,
            ranker,
            favourites: Vec::new(),
            recent: Vec::new(),
            catalog,
            asks: Vec::new(),
            hold: None,
            _blur: None,
            text_menu: Default::default(),
        }
    }

    pub fn sync(&mut self, inputs: Inputs, cx: &mut Context<Self>) {
        if self.sync_frame(inputs) {
            cx.notify();
        }
    }

    /// The parent is already rebuilding this frame, so no second frame is requested.
    pub(crate) fn sync_frame(&mut self, inputs: Inputs) -> bool {
        if self.inputs == inputs {
            return false;
        }
        if !inputs.dual {
            self.strips[1].clear();
            self.strips[2].clear();
        }
        self.inputs = inputs;
        true
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>) {
        self.catalog = catalog;
    }

    /// The favourites the jump ranks first, from the places producer.
    pub fn set_favourites(&mut self, favourites: Vec<String>) {
        self.favourites = favourites;
    }

    /// The recent-history producer's answer to ReadHistory.
    pub fn history_read(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        self.recent = paths;
        self.jump.history_read(&mut self.asks);
        self.flush(cx);
    }

    /// The recent-history file changed, so the next open reads it again.
    pub fn history_changed(&mut self) {
        self.jump.history_changes += 1;
    }

    pub fn editing(&self) -> bool {
        self.entry.editing
    }

    /// The seam the focus ring reads: the list cursor greys while this holds the keyboard.
    pub fn holds_keyboard(&self, window: &Window) -> bool {
        self.entry.editing && self.focus.is_focused(window)
    }

    fn path(&self) -> &str {
        &self.inputs.paths[self.inputs.side.min(1)]
    }

    /// Fits strip slot to its room and reports whether its pane has the keyboard; a hidden strip is cleared instead.
    pub fn fit(&mut self, slot: usize, budget: usize) -> bool {
        let side = slot.saturating_sub(1).min(1);
        let path = &self.inputs.paths[side];
        let alias = alias_at(path, &self.inputs.aliases);
        if slot == 0 && self.inputs.address_path {
            self.strips[slot].fit_path(path, &self.inputs.home, budget);
        } else if slot == 0 && self.inputs.elide_start {
            self.strips[slot].fit_start(path, &self.inputs.home, alias.as_ref(), budget);
        } else {
            self.strips[slot].fit(path, &self.inputs.home, alias.as_ref(), budget);
        }
        self.inputs.side == side
    }

    /// keys.toml's pathBar action, and the pointer's double click on inert ground.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.path().to_owned();
        if !self.entry.start(&path) {
            return;
        }
        self.built = true;
        self.scroll = 0.;
        // Losing the keyboard closes the bar, so it never commits against a window that has moved on.
        if self._blur.is_none() {
            self._blur = Some(cx.on_blur(&self.focus, window, |view, _, cx| view.close(cx)));
        }
        window.focus(&self.focus, cx);
        if self.ranker.ranks() {
            self.jump.open(&mut self.asks);
        }
        self.flush(cx);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.entry.close() {
            return;
        }
        self.jump.close();
        self.text_menu.close();
        self.pressed_row = None;
        self.hold = None;
        cx.emit(Out::Closed);
        cx.notify();
    }

    // Every ask the jump queued goes out now, each to the focused pane's backend.
    fn flush(&mut self, cx: &mut Context<Self>) {
        let side = self.inputs.side;
        for ask in std::mem::take(&mut self.asks) {
            cx.emit(match ask {
                Ask::Jump { id, ranking } => Out::Send {
                    side,
                    line: jump_line(id, ranking, &self.favourites, &self.recent),
                },
                Ask::ReadHistory => Out::ReadHistory,
            });
        }
    }

    pub(super) fn say(&self, said: Option<Said>, cx: &mut Context<Self>) {
        if let Some(said) = said {
            cx.emit(Out::Said(said.text(&self.catalog)));
        }
    }

    /// Enter on the line, or a jump that declined it.
    pub fn commit(&mut self, cx: &mut Context<Self>) {
        if !self.entry.editing || self.entry.marked.is_some() {
            return;
        }
        let (path, home, failed, side) = (
            self.path().to_owned(),
            self.inputs.home.clone(),
            self.inputs.failed,
            self.inputs.side,
        );
        let commit = self.entry.commit(&path, &home, failed);
        self.jump.close();
        self.hold = None;
        cx.emit(Out::Closed);
        match commit {
            Commit::Open(path) => cx.emit(Out::Open { side, path }),
            Commit::Network(path) => cx.emit(Out::Network(path)),
            Commit::Refused => self.say(Some(Said::RefusedUri), cx),
            Commit::Nothing => {}
        }
        cx.notify();
    }

    pub(super) fn choose(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.entry.editing || self.entry.marked.is_some() {
            return;
        }
        let (current, failed, side) =
            (self.path().to_owned(), self.inputs.failed, self.inputs.side);
        self.close(cx);
        if should_navigate(&path, &current, failed) {
            cx.emit(Out::Open { side, path });
        }
    }

    pub(super) fn enter(&mut self, enter: Enter, window: &mut Window, cx: &mut Context<Self>) {
        match enter {
            Enter::Chosen(path) => self.choose(path, cx),
            Enter::Declined => self.commit(cx),
            Enter::Held => {
                self.hold = Some(cx.spawn_in(window, async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(ANSWER_LIMIT_MS))
                        .await;
                    let _ = view.update(cx, |view, cx| {
                        if view.jump.expire() {
                            view.commit(cx);
                        }
                    });
                }));
            }
        }
    }

    /// Tab: the bar's own completion, from the kept rows or a fresh peek.
    pub(super) fn tab(&mut self, cx: &mut Context<Self>) {
        if !self.entry.editing || self.entry.marked.is_some() {
            return;
        }
        self.jump.path_typed();
        let (path, home) = (self.path().to_owned(), self.inputs.home.clone());
        match self.entry.complete(&path, &home, self.inputs.show_hidden) {
            Tab::Answered(said) => {
                self.say(said, cx);
                self.edited(cx);
            }
            Tab::Asked(ask) => {
                let line = peek_line(&ask.dir, self.inputs.first, ask.hidden);
                cx.emit(Out::Send {
                    side: self.inputs.side,
                    line,
                });
            }
        }
        cx.notify();
    }

    /// Whether this view owns the answer: a jump always, a peek only when it is the reply to the view's own ask.
    pub fn claims(&self, answer: &Answer) -> bool {
        match answer {
            Answer::Peeked(peeked) => self.entry.asked(peeked),
            Answer::Jumped(_) => true,
        }
    }

    /// A peeked or jumped line from the focused pane's backend, parsed on its reader thread.
    pub fn receive(&mut self, answer: Answer, window: &mut Window, cx: &mut Context<Self>) {
        let (path, home) = (self.path().to_owned(), self.inputs.home.clone());
        match answer {
            Answer::Peeked(peeked) => {
                if let Some(said) = self.entry.complete_with(&peeked, &path, &home) {
                    self.say(said, cx);
                    self.edited(cx);
                }
            }
            Answer::Jumped(jumped) => {
                let shown = self.jump.shown();
                let enter = self.jump.take(
                    &jumped,
                    self.ranker.as_mut(),
                    &home,
                    &self.entry.text,
                    &mut self.asks,
                );
                self.flush(cx);
                self.reshown(shown);
                if let Some(enter) = enter {
                    self.enter(enter, window, cx);
                }
            }
        }
        cx.notify();
    }

    /// The line changed: the rows rank again and the cursor returns to the first.
    pub(super) fn edited(&mut self, cx: &mut Context<Self>) {
        let shown = self.jump.shown();
        self.jump.rank(self.ranker.as_ref(), &self.entry.text);
        self.reshown(shown);
        cx.notify();
    }

    fn reshown(&mut self, was: bool) {
        if self.jump.shown() && !was {
            self.pointer.appeared();
        }
        self.scroll = 0.;
        self.wheel = 0.;
        self.reveal();
    }

    pub(super) fn reveal(&mut self) {
        let Some(cursor) = self.jump.cursor else {
            return;
        };
        let rows = self.jump.entries.len();
        let scale = self.paint.borrow().scale();
        let popup = self.paint.borrow().popup.map_or_else(
            || Popup::measure(scale, f32::MAX, 0., 0., rows),
            |kept| Popup::framed(scale, kept.x, kept.y, kept.width, kept.height, rows),
        );
        self.scroll = popup.reveal(self.scroll, cursor, rows);
    }

    /// Enter while the jump may still answer: its chosen row, a hold, or None for the line itself.
    pub(super) fn press_enter(&mut self) -> Option<Enter> {
        self.jump
            .press_enter(self.ranker.as_ref(), &self.entry.text)
    }
}
