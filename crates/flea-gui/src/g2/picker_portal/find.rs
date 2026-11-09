//! "/" in the chooser: the main window's Filter040 line over the chooser's listing, its strip under the column titles (picker-filter board).
use super::picker::Picker;
use crate::g2::listing_filter::{
    filter::{Asked, Filter, LineKey, PaneSeam},
    keys::{self, Classified},
    present,
    strip::StripState,
    wire::{self, Line},
};
use crate::g2::picker_listing::view::ListingView;
use crate::list::Typeface;
use gpui::{prelude::*, AnyView, AppContext, Context, Entity, KeyDownEvent, WeakEntity, Window};
use std::{cell::RefCell, rc::Rc};

// The cursor keys a standing filter steps through its drawn rows.
const MOVES: [&str; 4] = ["up", "down", "home", "end"];

pub struct Find {
    pub owner: Filter,
    strip: Rc<RefCell<StripState>>,
    view: Option<Entity<QueryView>>,
    pub(super) entry: crate::path::entry::Entry,
    pub(super) drawn: RefCell<super::text::Drawn>,
    kept: Option<(usize, Rc<[usize]>, String)>,
    // The backend's listing stamp on the rows in force, the numbering every answer must name.
    stamp: Option<usize>,
}

struct QueryView {
    picker: WeakEntity<Picker>,
    strip: Rc<RefCell<StripState>>,
}

impl gpui::Render for QueryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if cx.try_global::<crate::presentation::Fonts>().is_some() {
            self.strip.borrow_mut().typeface = Typeface::for_window(window, cx);
        }
        gpui::div()
            .size_full()
            .children(self.picker.upgrade().map(|picker| {
                super::text::Input::new(picker, self.strip.borrow().typeface.clone())
                    .strip(self.strip.clone())
            }))
    }
}

impl Find {
    pub fn new(typeface: Typeface) -> Self {
        Self {
            owner: Filter::default(),
            strip: Rc::new(RefCell::new(StripState::new(typeface))),
            view: None,
            entry: Default::default(),
            drawn: RefCell::default(),
            kept: None,
            stamp: None,
        }
    }

    // FilterStrip's way out shows in the footer too while a query stands.
    pub fn holds_query(&self) -> bool {
        !self.owner.query().is_empty()
    }
}

// The chooser's listing behind the filter: its cursor and scroll; a chooser holds no marks for the filter to prune.
struct Seam<'a> {
    view: &'a mut ListingView,
    stamp: usize,
    moved: &'a mut bool,
    selected: &'a [usize],
    marks: &'a mut Vec<usize>,
    unmarked: &'a mut bool,
}

impl PaneSeam for Seam<'_> {
    fn listing(&self) -> usize {
        self.stamp
    }
    fn total(&self) -> usize {
        self.view.listing().total()
    }
    fn cursor(&self) -> Option<usize> {
        (self.total() > 0).then(|| self.view.listing().cursor())
    }
    fn set_cursor(&mut self, row: usize) {
        self.view.listing_mut().move_cursor(row);
        *self.moved = true;
    }
    fn show_view(&mut self, _: usize) {
        *self.moved = true;
    }
    fn selected(&self) -> Vec<usize> {
        self.selected.to_vec()
    }
    fn unmark(&mut self, rows: &[usize]) {
        *self.unmarked |= !rows.is_empty();
    }
    fn mark(&mut self, rows: &[usize]) {
        self.marks.extend_from_slice(rows);
    }
    fn focus_list(&mut self) {}
    fn refill(&mut self) {
        *self.moved = true;
    }
}

impl Picker {
    // One change to the filter: its request goes out, the cursor's window is asked, then the rows and strip follow.
    pub(super) fn find_update(
        &mut self,
        change: impl FnOnce(&mut Filter, &mut dyn PaneSeam) -> Option<Asked>,
        cx: &mut Context<Self>,
    ) -> bool {
        let stamp = self.find.stamp.unwrap_or(usize::MAX);
        let mut moved = false;
        let selected = self.browse.listing.read(cx).selected_indices();
        let mut marks = Vec::new();
        let mut unmarked = false;
        let owner = &mut self.find.owner;
        let asked = self.browse.listing.update(cx, |view, _| {
            change(
                owner,
                &mut Seam {
                    view,
                    stamp,
                    moved: &mut moved,
                    selected: &selected,
                    marks: &mut marks,
                    unmarked: &mut unmarked,
                },
            )
        });
        if let Some(asked) = asked {
            self.send_listing(wire::request(asked.id, &asked.query).trim_end());
        }
        if !self.find.owner.typing() {
            self.find.entry.close();
        }
        self.find_refresh(cx);
        if moved {
            let cursor = self.browse.listing.read(cx).listing().cursor();
            self.move_cursor(cursor, cx);
        }
        if !marks.is_empty() {
            self.select_indices(marks, cx);
        }
        if unmarked {
            self.prune_filtered_marks(cx);
        }
        cx.notify();
        unmarked
    }

    // The drawn rows, the query's run and the strip, all from the answer in force.
    pub(super) fn find_refresh(&mut self, cx: &mut Context<Self>) {
        let stamp = self.find.stamp.unwrap_or(usize::MAX);
        let owner = &self.find.owner;
        if let Some(drawn) = owner.drawn(stamp) {
            self.find.kept = Some((stamp, Rc::from(drawn.rows()), drawn.query.clone()));
        } else if owner.asked().is_none()
            || !self.find.kept.as_ref().is_some_and(|held| held.0 == stamp)
        {
            self.find.kept = None;
        }
        let shown = self.find.kept.as_ref().map(|held| held.1.clone());
        let strip = if owner.up() {
            let folder = std::path::Path::new(&self.listing.location)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("/");
            let mut text = present::strip(owner, stamp, folder, &self.locale.catalog);
            if owner.asked().is_some() && owner.drawn(stamp).is_none() && self.find.kept.is_some() {
                text.summary = self.find.strip.borrow().text.summary.clone();
            }
            if owner.typing() {
                text.query = self.find.entry.text.clone().into();
            }
            self.find.strip.borrow_mut().text = text;
            let state = Rc::clone(&self.find.strip);
            let picker = cx.entity().downgrade();
            let view = self
                .find
                .view
                .get_or_insert_with(|| {
                    cx.new(|_| QueryView {
                        picker,
                        strip: state,
                    })
                })
                .clone();
            view.update(cx, |_, cx| cx.notify());
            Some(AnyView::from(view))
        } else {
            None
        };
        let query = self
            .find
            .kept
            .as_ref()
            .map_or_else(String::new, |held| held.2.clone());
        self.browse.listing.update(cx, |view, cx| {
            view.set_filtered(shown, &query, strip);
            cx.notify();
        });
        self.sync_facts(cx);
    }

    // The keys the filter owns ahead of the chooser's: the line while it has the caret, "/" to open it, and steps through a standing answer.
    pub(super) fn find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let stroke = &event.keystroke;
        let plain =
            !(stroke.modifiers.control || stroke.modifiers.alt || stroke.modifiers.platform);
        if !self.find.owner.typing() {
            if plain && stroke.key_char.as_deref() == Some("/") {
                if self.find.stamp.is_some() && self.listing.state == "ready" {
                    self.find.entry.begin(self.find.owner.query());
                    window.focus(&self.focus, cx);
                    self.find_update(
                        |filter, _| {
                            filter.start();
                            None
                        },
                        cx,
                    );
                }
                return true;
            }
            let stamp = self.find.stamp.unwrap_or(usize::MAX);
            if !(plain
                && self.find.owner.drawn(stamp).is_some()
                && MOVES.contains(&stroke.key.as_str()))
            {
                return false;
            }
            let key = stroke.key.clone();
            self.find_update(
                |filter, pane| {
                    match key.as_str() {
                        "down" => filter.step(1, pane),
                        "up" => filter.step(-1, pane),
                        "home" => filter.set_cursor_view(0, pane),
                        _ => filter.set_cursor_view(isize::MAX, pane),
                    }
                    None
                },
                cx,
            );
            return true;
        }
        let key_name = stroke.key.as_str();
        let was_marked = self.find.entry.marked.is_some();
        if self.find.entry.ime_key(key_name) {
            self.find_changed(cx);
            return true;
        }
        if self.find.entry.marked.is_some() {
            return false;
        }
        if was_marked {
            self.find_changed(cx);
        }
        let key = match keys::classify("default", event, window.modifiers()) {
            Classified::Paste => {
                let text = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .unwrap_or_default();
                if self.find.entry.ime_replace(None, &text) {
                    self.find_changed(cx);
                }
                return true;
            }
            Classified::Key(LineKey::Text(_)) => return false,
            Classified::Key(LineKey::Backspace) => {
                self.find.entry.erase(false);
                self.find_changed(cx);
                return true;
            }
            Classified::Key(key) => key,
        };
        // Enter on the line opens a single exact match, so one keystroke after the query reaches the file.
        let stamp = self.find.stamp.unwrap_or(usize::MAX);
        let single = matches!(key, LineKey::Enter)
            && self.find.owner.asked().is_none()
            && self
                .find
                .owner
                .drawn(stamp)
                .is_some_and(|drawn| drawn.narrowed.exact.len() == 1);
        let mut consumed = true;
        self.find_update(
            |filter, pane| {
                // The chooser's find line has no v0.3.12 strip behind it, so its Ctrl+A still marks the matches.
                if matches!(key, LineKey::SelectAll) {
                    filter.select_all(pane);
                    return None;
                }
                let (took, asked) = filter.line_key(key, pane);
                consumed = took;
                asked
            },
            cx,
        );
        if single {
            let cursor = self.browse.listing.read(cx).listing().cursor();
            self.activate_row(cursor, cx);
        }
        // A cursor key committed the line and now steps through the drawn rows.
        consumed || self.find_key(event, window, cx)
    }

    // Escape clears a standing filter first; false when no filter is up, so the chooser's own Escape runs.
    pub(super) fn find_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.find.owner.up() {
            return false;
        }
        self.find_update(
            |filter, pane| {
                filter.escape(pane);
                None
            },
            cx,
        );
        true
    }

    // A `filtered` line: the answer in force lands whole, a stale one is dropped, a broken one is said and never used.
    pub(super) fn find_line(&mut self, line: &str, cx: &mut Context<Self>) {
        match wire::read(line, self.find.owner.oldest()) {
            Ok(Line::Answer(answer)) => {
                let pruned = self.find_update(
                    |filter, pane| {
                        let _ = filter.receive(answer, pane);
                        None
                    },
                    cx,
                );
                if !pruned {
                    self.prune_filtered_marks(cx);
                }
            }
            Ok(Line::Other | Line::Stale) => {}
            Err(detail) => {
                let text = self
                    .locale
                    .catalog
                    .tr_args("window.backend_protocol", &[&detail]);
                self.say(text, true, cx);
            }
        }
    }

    // Rows under a new stamp: a re-read or sort of this folder asks the standing query again.
    pub(super) fn find_stamp(&mut self, stamp: usize, cx: &mut Context<Self>) {
        if self.find.stamp == Some(stamp) {
            return;
        }
        self.find.stamp = Some(stamp);
        if self.find.owner.up() {
            self.find_update(|filter, pane| filter.renumbered(pane), cx);
        }
    }

    // A new folder drops the filter, as the main window's does.
    pub(super) fn find_abandon(&mut self, cx: &mut Context<Self>) {
        self.find.entry.close();
        self.find.kept = None;
        let stamp = self.find.stamp.take().unwrap_or(usize::MAX);
        if !self.find.owner.up() {
            return;
        }
        // The old folder's cursor window is never asked: the new folder's list request follows.
        let (owner, mut moved) = (&mut self.find.owner, false);
        let mut marks = Vec::new();
        let mut unmarked = false;
        self.browse.listing.update(cx, |view, _| {
            owner.abandon(&mut Seam {
                view,
                stamp,
                moved: &mut moved,
                selected: &[],
                marks: &mut marks,
                unmarked: &mut unmarked,
            })
        });
        self.find_refresh(cx);
    }
}
