//! The Settings panel's state: the section, the side that holds the keyboard, the cursor row and the search query.
use super::geometry::{self, CARD_W};
use super::seams::{Facts, Request, Store};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_st1::{
    about,
    edit::{self, Edit},
    pages::{self, State, SECTIONS},
    rows::{first_row, focusable, step_row, Kind, Row},
    search::{self, Group},
    Text,
};
use std::sync::Arc;

mod search_keys;

/// What a pointer pressed on a pane row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Row,
    Step,
    Pick(usize),
    Remove,
    /// A favourite's grip released this many rows from where it was pressed.
    Drag(isize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Rail,
    Pane,
}

pub struct Panel {
    pub section: &'static str,
    pub side: Side,
    pub cursor: usize,
    /// The pane's scroll, moved only as far as the cursor row needs.
    pub scroll: f32,
    /// None while the search field is closed; Some with the typed text while it is open.
    pub query: Option<String>,
    /// The highlighted result while a query stands, counted across every group's hits.
    pub hit: usize,
    pub rows: Vec<Row>,
    pub results: Vec<Group>,
    pub save_error: Option<Text>,
    pub requests: Vec<Request>,
    pub catalog: Arc<I18n>,
    pub facts: Facts,
    /// The pane's inside as last laid out, and each row's line count as the window shaped it.
    pub pane: (f32, f32),
    pub lines: Vec<usize>,
    /// View's height, which sets the card's, so the card keeps one height whichever section is open.
    pub view_height: f32,
    /// A favourite's grip held down: its row and where the press began.
    pub grip: Option<(usize, f32)>,
    store: Box<dyn Store>,
    everything: Vec<(usize, Vec<Row>)>,
}

impl Panel {
    pub fn new(store: Box<dyn Store>, facts: Facts, catalog: Arc<I18n>) -> Self {
        let mut panel = Self {
            section: SECTIONS[0].id,
            side: Side::Pane,
            cursor: 0,
            scroll: 0.,
            query: None,
            hit: 0,
            rows: Vec::new(),
            results: Vec::new(),
            save_error: None,
            requests: Vec::new(),
            catalog,
            facts,
            pane: geometry::pane_size(CARD_W, geometry::BOARD_CARD_H),
            lines: Vec::new(),
            view_height: 0.,
            grip: None,
            store,
            everything: Vec::new(),
        };
        panel.rebuild();
        panel.cursor = first_row(&panel.rows);
        panel
    }

    pub fn document(&self) -> &Json {
        self.store.document()
    }

    fn preset(&self) -> &str {
        pages::at(self.store.document(), &["keys"])
            .and_then(Json::as_str)
            .unwrap_or("default")
    }

    fn section_rows(&self, section: &str) -> Vec<Row> {
        let data = self.store.document();
        let state = State {
            home: &self.facts.home,
            preset: self.preset(),
            base_size: self.facts.base_size,
            monitor_scale: self.facts.monitor_scale,
            save_error: self.save_error.clone(),
            ..State::new(data)
        };
        let checks = pages::at(data, &["updates", "autoCheck"]).and_then(Json::as_bool);
        // Plugins rows are the plugin unit's, handed in through its own seam; none are registered yet.
        pages::rows(section, &state, Vec::new(), || {
            about::rows(
                &self.facts.about,
                about::auto_check(checks),
                &self.facts.language,
                Vec::new(),
            )
        })
    }

    /// Rows come from the document, so every write or catalog change rebuilds the pane and any standing search.
    pub fn rebuild(&mut self) {
        self.rows = self.section_rows(self.section);
        self.view_height = geometry::heights(&self.section_rows("view"), &[])
            .iter()
            .sum();
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        if self.query.is_some() {
            self.everything = SECTIONS
                .iter()
                .enumerate()
                .map(|(index, section)| (index, self.section_rows(section.id)))
                .collect();
            self.results = search::search(
                &self.everything,
                self.query.as_deref().unwrap_or(""),
                &self.catalog,
            );
            self.hit = self.hit.min(self.hit_count().saturating_sub(1));
        }
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>) {
        self.catalog = catalog;
        self.rebuild();
    }

    /// The title names the pane; while a query stands no section is current, so it reads Settings.
    pub fn title(&self) -> String {
        if self.query.as_deref().is_some_and(|query| !query.is_empty()) {
            return self.catalog.tr("settings.title");
        }
        let id = if self.section == "columns" {
            "view"
        } else {
            self.section
        };
        self.catalog.tr(SECTIONS[pages::section_index(id)].label)
    }

    pub fn show_section(&mut self, id: &'static str) {
        self.section = id;
        self.rebuild();
        self.cursor = first_row(&self.rows);
        self.scroll = 0.;
        self.reveal();
    }

    /// Each row's height: shaped hint lines once drawn, a monospace estimate before.
    pub fn heights(&self) -> Vec<f32> {
        if self.lines.len() == self.rows.len() {
            return geometry::heights(&self.rows, &self.lines);
        }
        let lines: Vec<usize> = self
            .rows
            .iter()
            .map(|row| match row.kind {
                Kind::Hint => {
                    let (size, inset) = if row.footer {
                        (12., 2. * geometry::PAD_X)
                    } else {
                        (12., geometry::LABEL_X + geometry::PAD_X)
                    };
                    geometry::estimated_lines(
                        &row.label.resolve(&self.catalog),
                        size,
                        self.pane.0 - inset,
                    )
                }
                _ => 1,
            })
            .collect();
        geometry::heights(&self.rows, &lines)
    }

    /// The window's own layout: a pane of another size or a hint that wraps otherwise moves the scroll.
    pub fn laid_out(&mut self, pane: (f32, f32), lines: Vec<usize>) {
        if pane != self.pane || lines != self.lines {
            self.pane = pane;
            self.lines = lines;
            self.reveal();
        }
    }

    fn reveal(&mut self) {
        self.scroll = geometry::reveal(
            &self.rows,
            &self.heights(),
            self.cursor,
            self.scroll,
            self.pane.1,
        );
    }

    /// A wheel turn over the pane, kept inside the content.
    pub fn wheel(&mut self, delta: f32) {
        let most = (self.heights().iter().sum::<f32>() - self.pane.1).max(0.);
        self.scroll = (self.scroll - delta).clamp(0., most);
    }

    /// The pointer moved over a row: the pane takes the keys and the cursor goes where the pointer is.
    pub fn pointer(&mut self, index: usize) {
        if self.rows.get(index).is_some_and(focusable) {
            self.side = Side::Pane;
            self.cursor = index;
        }
    }

    /// A press on a row, a chevron, a segment cell or a ruler tick, each the writer its key reaches.
    pub fn press(&mut self, index: usize, target: Press) {
        if !self.rows.get(index).is_some_and(focusable) {
            return;
        }
        self.side = Side::Pane;
        self.cursor = index;
        match target {
            Press::Row => self.activate(index),
            Press::Step => self.step(index, 1),
            Press::Pick(stop) => {
                let row = &self.rows[index];
                let change = edit::pick(row, self.store.document(), self.facts.base_size, stop);
                self.apply(change);
            }
            Press::Remove => {
                if let Some(at) = self.rows[index].favourite {
                    self.requests.push(Request::RemoveFavourite(at));
                }
            }
            Press::Drag(rows) => {
                let Some(from) = self.rows[index].favourite else {
                    return;
                };
                let last = self
                    .rows
                    .iter()
                    .filter(|row| row.kind == Kind::Favourite)
                    .count()
                    .saturating_sub(1) as isize;
                let to = (from as isize + rows).clamp(0, last) as usize;
                if to != from {
                    self.requests.push(Request::MoveFavourite { from, to });
                }
            }
        }
    }

    /// A click on a rail section: the rail takes the keys and the section opens.
    pub fn choose(&mut self, section: &'static str) {
        self.side = Side::Rail;
        self.show_section(section);
    }

    fn write(&mut self, patch: &Json) {
        self.save_error = self.store.write(patch).err();
        self.rebuild();
        self.reveal();
    }

    fn apply(&mut self, change: Edit) {
        match change {
            Edit::None => {}
            Edit::Patch(patch) => self.write(&patch),
            Edit::Open(section) => self.show_section(section),
            Edit::Host(id) if id == "keyboardSheet" => {
                self.requests.push(Request::Close);
                self.requests.push(Request::OpenSheet);
            }
            Edit::Host(id) if id == "startFolder" => {
                self.choose_start_folder(&self.facts.pane_path.clone())
            }
            Edit::Host(id) => self.requests.push(Request::Host(id)),
        }
    }

    /// Chosen folder: the folder start mode and this path, in one write, and only with a pane behind the panel.
    pub fn choose_start_folder(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        self.write(&edit::start_folder(path));
    }

    pub fn activate(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let change = edit::activate(
            self.section,
            row,
            self.store.document(),
            self.facts.base_size,
        );
        self.apply(change);
    }

    pub fn step(&mut self, index: usize, direction: i32) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let change = edit::step(row, self.store.document(), self.facts.base_size, direction);
        self.apply(change);
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.side == Side::Rail {
            let at = pages::section_index(self.section) as isize;
            let to = (at + delta).clamp(0, SECTIONS.len() as isize - 1) as usize;
            self.show_section(SECTIONS[to].id);
            return;
        }
        self.cursor = step_row(&self.rows, self.cursor, delta);
        self.reveal();
    }

    /// One key press in GPUI's key names; every key stops here, so nothing reaches the listing behind the panel.
    pub fn key(&mut self, key: &str, text: &str, shift: bool) {
        if self.query.is_some() {
            return self.search_key(key, text);
        }
        if key == "escape" {
            return self.requests.push(Request::Close);
        }
        if key == "tab" {
            self.side = if self.side == Side::Rail {
                Side::Pane
            } else {
                Side::Rail
            };
            return;
        }
        if text == "/" {
            self.query = Some(String::new());
            self.hit = 0;
            return;
        }
        let row = self.rows.get(self.cursor).cloned();
        let favourite =
            self.side == Side::Pane && row.as_ref().is_some_and(|row| row.kind == Kind::Favourite);
        if favourite && shift && matches!(text, "J" | "K") {
            return self.press(self.cursor, Press::Drag(if text == "J" { 1 } else { -1 }));
        }
        if favourite && (text == "x" || key == "delete") {
            return self.press(self.cursor, Press::Remove);
        }
        match (key, text) {
            ("down", _) | (_, "j") => self.move_cursor(1),
            ("up", _) | (_, "k") => self.move_cursor(-1),
            ("right", _) | (_, "l") if self.side == Side::Pane => self.step(self.cursor, 1),
            ("left", _) | (_, "h") if self.side == Side::Pane => self.step(self.cursor, -1),
            ("enter" | "space", _) if self.side == Side::Rail => self.side = Side::Pane,
            ("enter", _) => self.activate(self.cursor),
            // Space toggles a check or a master, and never walks a choice.
            ("space", _)
                if row.is_some_and(|row| matches!(row.kind, Kind::Check | Kind::Group)) =>
            {
                self.activate(self.cursor)
            }
            _ => {}
        }
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }
}
