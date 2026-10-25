//! The keyboard sheet: every key of the preset in its groups, and a query that narrows them to ranked rows.
use super::seams::Request;
use flea::i18n::I18n;
use flea::keys::{presentation, Frontend};
use flea::ui_logic_st1::{
    sheet::{self, Candidate, Decision, MenuEntry, RailEntry},
    sheetkeys::{sheet_key, step_cursor, SheetKey},
};
use std::sync::Arc;

/// The sources a query reads. Menus, rail places and recent files are seams for the menu, rail and recents contracts.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub menus: Vec<MenuEntry>,
    pub rail: Vec<RailEntry>,
    pub recent: Vec<String>,
    pub home: String,
}

pub struct Sheet {
    pub preset: String,
    pub query: String,
    pub cursor: usize,
    pub rows: Vec<presentation::SheetRow>,
    pub ranked: Vec<Candidate>,
    pub catalog: Arc<I18n>,
    pub sources: Sources,
    pub requests: Vec<Request>,
    /// What the last Enter decided, for the host to run once the sheet closes.
    pub decided: Option<Decision>,
    /// The results' scroll, moved only as far as the highlighted row needs.
    pub scroll: f32,
    /// Up or Down moved the highlight, so the next frame brings it into view.
    pub moved: bool,
}

// GPUI key names to the names the sheet's key decision reads.
fn named(key: &str) -> &str {
    match key {
        "escape" => "Escape",
        "up" => "Up",
        "down" => "Down",
        "enter" => "Return",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "shift" => "Shift",
        "control" => "Control",
        "alt" => "Alt",
        "platform" => "Meta",
        other => other,
    }
}

impl Sheet {
    pub fn new(preset: &str, catalog: Arc<I18n>, sources: Sources) -> Self {
        let rows = presentation::sheet(preset, Frontend::Gui, &catalog);
        Self {
            preset: preset.into(),
            query: String::new(),
            cursor: 0,
            rows,
            ranked: Vec::new(),
            catalog,
            sources,
            requests: Vec::new(),
            decided: None,
            scroll: 0.,
            moved: false,
        }
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>) {
        self.rows = presentation::sheet(&self.preset, Frontend::Gui, &catalog);
        self.catalog = catalog;
        self.requery();
    }

    pub fn set_preset(&mut self, preset: &str) {
        if self.preset != preset {
            self.preset = preset.into();
            self.rows = presentation::sheet(preset, Frontend::Gui, &self.catalog);
            self.requery();
        }
    }

    fn candidates(&self) -> Vec<Candidate> {
        let mut all = sheet::action_candidates(&self.rows);
        all.extend(sheet::menu_candidates(&self.sources.menus, |action| {
            presentation::hint_for(&self.preset, action).to_owned()
        }));
        all.extend(sheet::place_candidates(&self.sources.rail));
        all.extend(sheet::recent_candidates(
            &self.sources.recent,
            &self.sources.home,
        ));
        all
    }

    fn requery(&mut self) {
        self.ranked = if self.query.is_empty() {
            Vec::new()
        } else {
            sheet::rank(&self.candidates(), &self.query, &self.catalog)
        };
        self.cursor = 0;
        self.scroll = 0.;
    }

    /// Keeps the highlighted result inside a list this tall, rows of this height.
    pub fn reveal(&mut self, viewport: f32, row: f32) {
        let top = self.cursor as f32 * row;
        let most = (self.ranked.len() as f32 * row - viewport).max(0.);
        self.scroll = if top < self.scroll {
            top
        } else if top + row > self.scroll + viewport {
            top + row - viewport
        } else {
            self.scroll
        }
        .clamp(0., most);
    }

    /// The pointer over a result highlights it; a click runs it as Enter does.
    pub fn point(&mut self, index: usize, run: bool) {
        if index < self.ranked.len() {
            self.cursor = index;
            if run {
                self.key("enter", "");
            }
        }
    }

    pub fn key(&mut self, key: &str, text: &str) {
        let count = self.ranked.len() as i64;
        match sheet_key(&self.query, named(key), text) {
            SheetKey::Close => self.requests.push(Request::Close),
            SheetKey::Up => {
                self.cursor = step_cursor(self.cursor as i64, -1, count) as usize;
                self.moved = true;
            }
            SheetKey::Down => {
                self.cursor = step_cursor(self.cursor as i64, 1, count) as usize;
                self.moved = true;
            }
            SheetKey::Activate => {
                if let Some(row) = self.ranked.get(self.cursor) {
                    let decided = sheet::dispatch(row);
                    if decided != Decision::Disabled {
                        self.decided = Some(decided);
                        self.requests.push(Request::Close);
                    }
                }
            }
            SheetKey::Backspace => {
                self.query.pop();
                self.requery();
            }
            SheetKey::Type => {
                self.query.push_str(text);
                self.requery();
            }
            SheetKey::Ignore => {}
        }
    }
}
