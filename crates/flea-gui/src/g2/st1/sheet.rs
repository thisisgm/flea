//! The keyboard sheet: every key of the preset in its groups, and a query that narrows them to ranked rows.
use super::seams::Request;
use crate::g2::mn1::keycaps;
use flea::i18n::I18n;
use flea::keys::{display::KeycapSnapshot, presentation, Frontend, SheetGroup, SHEET_GROUPS};
use flea::ui_logic_st1::{
    sheet::{self, Candidate, Decision, MenuEntry, RailEntry},
    sheetkeys::{sheet_key, step_cursor, SheetKey},
};
use std::collections::HashMap;
use std::sync::Arc;

/// The sources a query reads. Menus, rail places and recent files are seams for the menu, rail and recents contracts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// The active keyboard group, or none where its keycaps are unknown and the canonical legends draw.
    pub keycaps: Option<KeycapSnapshot>,
    // Each canonical chord the rows hold, as the group's keycaps draw it; resolved here, never in paint.
    caps: HashMap<String, String>,
    // Each ranked result's keys as drawn, isolated in a right-to-left catalog.
    drawn: Vec<String>,
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
        let mut sheet = Self {
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
            keycaps: None,
            caps: HashMap::new(),
            drawn: Vec::new(),
        };
        sheet.relabel();
        sheet
    }

    /// A keyboard group switch relabels the keys and ranks the query again, keeping the rows and the highlighted result.
    pub fn set_keycaps(&mut self, keycaps: Option<KeycapSnapshot>) {
        if self.keycaps != keycaps {
            self.keycaps = keycaps;
            self.rerank();
        }
    }

    /// The keys a row's canonical chord draws on the active group.
    pub fn cap<'a>(&'a self, keys: &'a str) -> &'a str {
        self.caps.get(keys).map_or(keys, String::as_str)
    }

    /// The keys ranked result `index` draws; its candidate already carries the group's keycaps.
    pub fn result_keys(&self, index: usize) -> &str {
        self.drawn.get(index).map_or("", String::as_str)
    }

    fn relabel(&mut self) {
        let rtl = self.catalog.locale().right_to_left;
        let chord = |keys: &str, keycaps| keycaps::chord(keys, keycaps, rtl);
        let rows = self
            .rows
            .iter()
            .map(|row| (row.keys.clone(), chord(&row.keys, self.keycaps.as_ref())));
        self.caps = rows.collect();
        self.drawn = self
            .ranked
            .iter()
            .map(|row| chord(&row.keys, None))
            .collect();
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>) {
        self.rows = presentation::sheet(&self.preset, Frontend::Gui, &catalog);
        self.catalog = catalog;
        self.rerank();
    }

    // Ranks the query again and keeps the highlighted result where it still ranks.
    fn rerank(&mut self) {
        let (cursor, scroll) = (self.cursor, self.scroll);
        let selected = self.ranked.get(cursor).cloned();
        self.requery();
        self.cursor = selected
            .and_then(|selected| {
                self.ranked.iter().position(|row| {
                    row.section == selected.section
                        && row.action == selected.action
                        && row.menu_action == selected.menu_action
                        && row.parent_action == selected.parent_action
                        && row.path == selected.path
                        && row.place == selected.place
                        && row.rail_index == selected.rail_index
                })
            })
            .unwrap_or_else(|| cursor.min(self.ranked.len().saturating_sub(1)));
        self.scroll = scroll;
    }

    pub fn set_preset(&mut self, preset: &str) {
        if self.preset != preset {
            self.preset = preset.into();
            self.rows = presentation::sheet(preset, Frontend::Gui, &self.catalog);
            self.requery();
        }
    }

    pub fn set_sources(&mut self, sources: Sources) {
        if self.sources != sources {
            self.sources = sources;
            self.requery();
        }
    }

    /// The resting sheet as drawn: each group's rows in the group's own order, and the column it sits in.
    pub fn groups(&self) -> Vec<(usize, &'static SheetGroup, Vec<&presentation::SheetRow>)> {
        let half = self.rows.len().div_ceil(2);
        let mut used = 0;
        let mut out = Vec::new();
        for group in SHEET_GROUPS {
            let rows: Vec<_> = group
                .actions
                .iter()
                .filter_map(|action| self.rows.iter().find(|row| row.action == *action))
                .collect();
            if rows.is_empty() {
                continue;
            }
            // A group opens the second column whole once it would pass half the rows.
            let side = usize::from(used > 0 && used + rows.len() > half);
            used += rows.len();
            out.push((side, group, rows));
        }
        out
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
        // The query matches the keys as the active group draws them, so a typed keycap finds its row.
        for candidate in &mut all {
            candidate.keys = keycaps::chord(&candidate.keys, self.keycaps.as_ref(), false);
        }
        all
    }

    pub(super) fn query_changed(&mut self, query: String) {
        self.query = query;
        self.requery();
    }

    fn requery(&mut self) {
        self.ranked = if self.query.is_empty() {
            Vec::new()
        } else {
            sheet::rank(&self.candidates(), &self.query, &self.catalog)
        };
        self.relabel();
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
