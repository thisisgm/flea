//! pl4's places producer in the live rail: saved places become the drawn rows, and activation, menus and renaming leave as requests.
#[path = "producer_eject.rs"]
mod producer_eject;
#[path = "producer_ime.rs"]
mod producer_ime;
#[path = "producer_input.rs"]
mod producer_input;
#[path = "producer_reorder.rs"]
mod producer_reorder;
#[path = "producer_sources.rs"]
mod producer_sources;
#[path = "producer_view.rs"]
mod producer_view;

use super::content::{theme_tags, Provider};
use super::rows::{Laid, RailElement, RowsModel};
use super::seams::ThemeRoles;
use super::service::Places;
use crate::list::Typeface;
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_rail_places::order::{flat, rail_sections, saved_places, RailSection};
use flea::ui_logic_rail_places::places::{home_entries, stored_entries};
use flea::ui_logic_rail_places::rail::{Group, Kind, RailEntry};
use gpui::{
    div, prelude::*, AppContext, Context, EventEmitter, IntoElement, MouseButton, MouseDownEvent,
    Render, SharedString, Window,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// What a row asks the window to do; the shell routes opens and keeps the rest as seams for their units.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Open(String),
    Recent(Vec<flea::ui_logic_rail_places::recent::Visit>),
    Trash,
    Focus,
    Activate(RailEntry),
    Menu(RailEntry),
    Tab(RailEntry),
    Rename {
        entry: RailEntry,
        label: String,
    },
    /// The eject mark's click or its keyboard frame: the row's own release, as Sidebar.ejectRow.
    Release(RailEntry),
}

/// A captured rail request; deferred delivery never consults the later pane focus.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub side: usize,
    pub id: u64,
    pub event: Event,
}

/// The rail's input and service owner: its data, its drawn sections, its cursor and its one editor.
pub struct Producer {
    pub catalog: Arc<I18n>,
    pub places: gpui::Entity<Places>,
    _places_sub: gpui::Subscription,
    state: Json,
    home: String,
    dirs: String,
    loaded: bool,
    loading: bool,
    recent_pending: Option<Request>,
    inspected_visible: Vec<(usize, Json)>,
    history_seen: u64,
    pub(crate) side: usize,
    request_id: u64,
    editor_dragging: bool,
    ime_session: Option<u64>,
    ime_focus: Option<gpui::FocusHandle>,
    pub directory_reads: u64,
    tail: Vec<RailSection>,
    pub sections: Vec<RailSection>,
    pub provider: Provider,
    pub theme: ThemeRoles,
    pub polling: bool,
    pub model: Rc<RefCell<RowsModel>>,
    pub laid: Rc<RefCell<Laid>>,
    pub typeface: Typeface,
    events: RefCell<Vec<Request>>,
    /// The window's row-drag receiver: a drag from the listing released over the rows lands through it.
    pub(crate) drop_sink: Option<DropSink>,
}

/// Called with no producer lease held, so the window may read the rows it lands on.
pub(crate) type DropSink = Rc<dyn Fn(&mut gpui::Window, &mut gpui::App)>;

pub(crate) fn dirs_text(home: &str) -> String {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".config"));
    std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default()
}

/// Icons.sidebarGlyphFor, so a home row carries the same mark the rail's own map names.
pub fn home_glyph(label: &str) -> &'static str {
    match label {
        "Home" => "house",
        "Downloads" => "download",
        "Documents" => "file-text",
        "Pictures" => "image",
        "Videos" => "film",
        "Music" => "music",
        "Projects" => "folder-git-2",
        _ => "folder",
    }
}

fn built(
    catalog: &I18n,
    sections: Vec<RailSection>,
    theme: &ThemeRoles,
) -> (Provider, Rc<Vec<super::content::DrawnSection>>) {
    let mut provider = Provider::default();
    provider.update(catalog, sections, theme_tags(theme));
    let drawn = Rc::new(provider.drawn.clone());
    (provider, drawn)
}

impl EventEmitter<Event> for Producer {}

impl Producer {
    pub fn new(catalog: Arc<I18n>, state: &Json, exe: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut producer = Self::from_sections(catalog, Vec::new(), exe, cx);
        producer.state = state.clone();
        producer.home = std::env::var("HOME").unwrap_or_default();
        producer.loaded = false;
        producer.model.borrow_mut().adopt_places(state);
        producer.places.update(cx, |places, _| {
            places
                .favourites
                .records_changed(saved_places(state).favourites);
        });
        producer
    }

    pub fn from_sections(
        catalog: Arc<I18n>,
        sections: Vec<RailSection>,
        exe: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = ThemeRoles::default();
        let (provider, drawn) = built(&catalog, sections.clone(), &theme);
        let places = cx.new(|_| Places::new(Arc::clone(&catalog), exe));
        let subscription = cx.observe(&places, |producer, _, cx| producer.service_changed(cx));
        Self {
            catalog,
            places,
            _places_sub: subscription,
            state: Json::Null,
            home: String::new(),
            dirs: String::new(),
            loaded: true,
            loading: false,
            recent_pending: None,
            inspected_visible: Vec::new(),
            history_seen: 0,
            side: 0,
            request_id: 0,
            editor_dragging: false,
            ime_session: None,
            ime_focus: None,
            directory_reads: 0,
            tail: Vec::new(),
            sections,
            provider,
            theme,
            polling: false,
            model: Rc::new(RefCell::new(RowsModel::new(drawn))),
            laid: Rc::default(),
            typeface: Typeface::omarchy_default(),
            events: RefCell::default(),
            drop_sink: None,
        }
    }

    pub fn entries(&self) -> Vec<RailEntry> {
        flat(&self.sections)
    }

    pub fn count(&self) -> usize {
        self.sections.iter().map(|section| section.rows.len()).sum()
    }

    pub fn shown_entries(&self) -> Vec<SharedString> {
        self.provider
            .drawn
            .iter()
            .flat_map(|section| section.rows.iter().map(|row| row.label.clone()))
            .collect()
    }

    pub fn cursor(&self) -> Option<usize> {
        self.model.borrow().cursor
    }

    pub fn rename_index(&self) -> Option<usize> {
        self.model.borrow().renaming
    }

    pub fn rename_text(&self) -> String {
        self.model.borrow().editor.text.clone()
    }

    pub fn field_shown(&self) -> bool {
        let renaming = self.model.borrow().renaming;
        renaming.is_some_and(|index| {
            self.laid
                .borrow()
                .row(index)
                .is_some_and(|row| row.editor.is_some())
        })
    }

    pub fn set_polling(&mut self, polling: bool, cx: &mut Context<Self>) {
        if self.polling == polling {
            return;
        }
        self.polling = polling;
        if !polling {
            self.cancel_rename(cx);
            self.model.borrow_mut().focused = false;
            self.model.borrow_mut().mark_focused = false;
        }
        self.load(cx);
        cx.notify();
    }

    /// A language or settings change rebuilds the rows in place and drops any editor on a vanished row.
    pub fn adopt_state(&mut self, state: &Json, cx: &mut Context<Self>) {
        self.state = state.clone();
        self.model.borrow_mut().adopt_places(state);
        self.places
            .update(cx, |places, cx| places.adopt_state(state, cx));
        self.rebuild(cx);
        cx.notify();
    }

    pub fn trash_count(&mut self, count: u64, cx: &mut Context<Self>) {
        if self.model.borrow().trash_count != count {
            self.model.borrow_mut().trash_count = count;
            cx.notify();
        }
    }

    pub fn write_favourite(
        &mut self,
        operation: &flea::ui_logic_rail_places::favourites::Operation,
        cx: &mut Context<Self>,
    ) {
        self.places
            .update(cx, |places, cx| places.write(operation, cx));
    }

    /// The lit row follows the pane's place, as Sidebar.settleLit resolves it.
    pub fn settle_lit(&mut self, path: &str, cx: &mut Context<Self>) {
        self.settle_view(
            path,
            &flea::ui_logic_rail_places::placeview::View::Dir {
                share_key: String::new(),
            },
            cx,
        );
    }

    pub(crate) fn settle_view(
        &mut self,
        path: &str,
        view: &flea::ui_logic_rail_places::placeview::View,
        cx: &mut Context<Self>,
    ) {
        let lit = flea::ui_logic_rail_places::placeview::held_by(&self.entries(), path, view);
        if self.model.borrow().lit != lit {
            self.model.borrow_mut().lit = lit;
            cx.notify();
        }
    }

    pub fn drain_events(&self) -> Vec<Request> {
        std::mem::take(&mut *self.events.borrow_mut())
    }

    fn request(&mut self, event: Event) -> Request {
        self.request_id += 1;
        Request {
            side: self.side,
            id: self.request_id,
            event,
        }
    }

    fn queue(&mut self, event: Event) {
        let request = self.request(event);
        self.events.borrow_mut().push(request);
    }

    pub(crate) fn entry(&self, index: usize) -> Option<RailEntry> {
        self.entries().into_iter().nth(index)
    }

    pub(crate) fn row_at(&self, position: gpui::Point<gpui::Pixels>) -> Option<usize> {
        let (x, y) = (f32::from(position.x), f32::from(position.y));
        self.laid
            .borrow()
            .rows
            .iter()
            .find(|row| {
                x >= row.rect.x
                    && x < row.rect.x + row.rect.width
                    && y >= row.rect.y
                    && y < row.rect.y + row.rect.height
            })
            .map(|row| row.index)
    }

    fn press(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(index) = self.row_at(event.position) else {
            self.cancel_rename(cx);
            return;
        };
        // Settings can rebuild the entries between the last prepaint and this press.
        let Some(entry) = self.entry(index) else {
            self.cancel_rename(cx);
            return;
        };
        // A press on the eject mark is the mark's own; it releases on a release inside its hit box.
        if event.button == MouseButton::Left && self.mark_press(event.position, cx) {
            return;
        }
        // PaneRail.qml focusPress: a rail press leaves the keyboard where it was; only the open editor claims it.
        if self.model.borrow().renaming == Some(index) {
            self.queue(Event::Focus);
            self.editor_press(event, cx);
            cx.stop_propagation();
            return;
        }
        self.model.borrow_mut().pointer_focus = true;
        self.cancel_rename(cx);
        match event.button {
            MouseButton::Left if self.reorder_press(index, &entry, f32::from(event.position.y)) => {
                cx.notify()
            }
            MouseButton::Left => self.activate(index, cx),
            MouseButton::Right => {
                self.model.borrow_mut().cursor = Some(index);
                self.queue(Event::Menu(entry));
                cx.notify();
            }
            MouseButton::Middle => {
                self.retry_availability(&entry, cx);
                self.model.borrow_mut().cursor = Some(index);
                self.queue(Event::Tab(entry));
                cx.notify();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "producer_stale_hit_tests.rs"]
mod stale_hit_tests;

#[cfg(test)]
#[path = "producer_inspection_tests.rs"]
mod inspection_tests;
