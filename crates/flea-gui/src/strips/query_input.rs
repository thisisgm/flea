//! The platform adapter, line rules and caret geometry that Find and the folder filter share over one Entry.
use crate::{
    path::{
        entry::Entry,
        field::{ime_bounds, ime_index, Line},
    },
    state::Shell,
};
use gpui::{
    point, px, size, App, Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection,
    WeakEntity, Window,
};
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    ops::Range,
    rc::Rc,
};

/// The accent wash under selected query text.
pub const SELECTION: f32 = 0.24;
/// The keys a composing input method keeps, so its candidate arrows never reach the line or the list.
pub const CANDIDATE_KEYS: [&str; 8] = [
    "left", "right", "up", "down", "home", "end", "pageup", "pagedown",
];

/// The strip an adapter serves: whether its pane still owns the line, how a commit lands, how a change redraws.
pub trait Route: 'static {
    fn owns(shell: &Shell, side: usize, entry: &Rc<RefCell<Entry>>) -> bool;
    fn commit(
        shell: &mut Shell,
        side: usize,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Shell>,
    );
    fn redraw(shell: &mut Shell, side: usize, cx: &mut Context<Shell>);
}

pub struct QueryInput<R> {
    pub owner: WeakEntity<Shell>,
    pub side: usize,
    pub generation: u64,
    pub entry: Rc<RefCell<Entry>>,
    pub line: Rc<RefCell<Line>>,
    pub bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    route: PhantomData<R>,
}

impl<R: Route> QueryInput<R> {
    pub fn new(
        owner: WeakEntity<Shell>,
        side: usize,
        generation: u64,
        entry: Rc<RefCell<Entry>>,
        line: Rc<RefCell<Line>>,
        bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    ) -> Self {
        Self {
            owner,
            side,
            generation,
            entry,
            line,
            bounds,
            route: PhantomData,
        }
    }

    pub(crate) fn focused(&self, window: &Window, cx: &App) -> bool {
        let Some(owner) = self.owner.upgrade() else {
            return false;
        };
        let shell = owner.read(cx);
        shell.focus_side == self.side
            && shell.focus.is_focused(window)
            && !shell.input_blocked(cx)
            && R::owns(shell, self.side, &self.entry)
            && self.entry.borrow().session() == Some(self.generation)
    }

    fn active(&self, window: &Window, cx: &App) -> bool {
        self.focused(window, cx) && self.line.borrow().accepts(&self.entry.borrow())
    }

    fn committed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.entry.borrow().generation;
        let side = self.side;
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |shell, cx| {
                R::commit(shell, side, generation, window, cx);
                R::redraw(shell, side, cx);
            });
        }
    }

    fn redraw(&self, cx: &mut Context<Self>) {
        let side = self.side;
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |shell, cx| R::redraw(shell, side, cx));
        }
    }
}

impl<R: Route> EntityInputHandler for QueryInput<R> {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.active(window, cx)
            .then(|| self.entry.borrow().ime_text(&range, adjusted))
            .flatten()
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.active(window, cx)
            .then(|| self.entry.borrow().ime_selection())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.active(window, cx)
            .then(|| self.entry.borrow().ime_marked())
            .flatten()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active(window, cx) && { self.entry.borrow_mut().ime_unmark() } {
            self.committed(window, cx);
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active(window, cx) && { self.entry.borrow_mut().ime_replace(range, text) } {
            self.committed(window, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active(window, cx) && { self.entry.borrow_mut().ime_mark(range, text, selected) } {
            self.redraw(cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.active(window, cx)
            .then(|| {
                self.bounds.get().map(|bounds| {
                    ime_bounds(&self.line.borrow(), &self.entry.borrow(), &range, bounds)
                })
            })
            .flatten()
    }
    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.active(window, cx)
            .then(|| {
                self.bounds.get().map(|bounds| {
                    ime_index(
                        &self.line.borrow(),
                        &self.entry.borrow(),
                        at.x - bounds.left(),
                    )
                })
            })
            .flatten()
    }
}

/// Opens the line on `text` with the caret after it.
pub fn reopen(entry: &mut Entry, text: &str) {
    entry.begin(text);
    let end = entry.text.len();
    entry.move_to(end);
}

/// Keeps the line on the committed query while it types, except while the input method composes.
pub fn follow(entry: &mut Entry, typing: bool, committed: &str) {
    if !typing {
        entry.close();
    } else if !entry.editing {
        reopen(entry, committed);
    } else if entry.marked.is_none() && entry.text != committed {
        let end = entry.text.len();
        entry.replace(Some(0..end), committed);
    }
}

/// The line's text when a retained callback still names this open line and it holds a settled change.
pub fn settled(entry: &Entry, typing: bool, generation: u64, committed: &str) -> Option<String> {
    (typing
        && entry.session() == Some(generation)
        && entry.marked.is_none()
        && entry.text != committed)
        .then(|| entry.text.clone())
}

/// Escape inside a composition drops it and puts the committed query back.
pub fn cancel_composition(entry: &mut Entry, committed: &str) {
    entry.ime_key("escape");
    let end = entry.text.len();
    entry.replace(Some(0..end), committed);
}

/// Left, Right, Home and End move or extend the caret; any other key is not the caret's.
pub fn move_caret(entry: &mut Entry, key: &str, shift: bool) -> bool {
    let at = entry.cursor();
    let to = match key {
        "left" if !shift && !entry.selected.is_empty() => entry.selected.start,
        "right" if !shift && !entry.selected.is_empty() => entry.selected.end,
        "left" => entry.previous(at),
        "right" => entry.next(at),
        "home" => 0,
        "end" => entry.text.len(),
        _ => return false,
    };
    if shift {
        entry.select_to(to);
    } else {
        entry.move_to(to);
    }
    true
}

/// Where the caret, preedit underline and selection band stand in one frame.
pub struct Placed {
    pub caret: Option<Bounds<Pixels>>,
    pub marked: Option<Bounds<Pixels>>,
    pub selection: Option<Bounds<Pixels>>,
}

/// Scrolls the line so the caret stays inside `band`, the input-method rectangle, and lays out what the caret marks.
pub fn place(
    line: &mut Line,
    entry: &Entry,
    band: Bounds<Pixels>,
    caret_w: Pixels,
    gap: Pixels,
) -> Placed {
    line.revision = entry.revision;
    let advance = line.entry_caret(entry, entry.cursor());
    let inside = (band.size.width - caret_w - gap).max(px(0.));
    if advance - line.scroll > inside {
        line.scroll = advance - inside;
    } else if advance < line.scroll {
        line.scroll = advance;
    }
    let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
    line.scroll = line.scroll.min((width - inside).max(px(0.))).max(px(0.));
    let origin = band.left() - line.scroll;
    let span = |range: &Range<usize>| {
        let (from, to) = line.entry_span(entry, range);
        Bounds::new(
            point(origin + from, band.top()),
            size(to - from, band.size.height),
        )
    };
    Placed {
        caret: (entry.editing && entry.selected.is_empty()).then(|| {
            Bounds::new(
                point(origin + advance + gap, band.top()),
                size(caret_w, band.size.height),
            )
        }),
        marked: entry.marked.as_ref().map(|range| {
            let under = span(range);
            Bounds::new(
                point(under.left(), under.bottom() - px(1.)),
                size(under.size.width, px(1.)),
            )
        }),
        selection: (!entry.selected.is_empty()).then(|| span(&entry.selected)),
    }
}
