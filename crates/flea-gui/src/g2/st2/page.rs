//! The Plugins section's state: the installed list, one plugin's page, the Add and Remove cards, and replies matched by request id.
use super::seam::{Effect, PluginsWire};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_st2::manifest::Invalid;
use flea::ui_logic_st2::pages::{self, AddCard, Row, Update};
use flea::ui_logic_st2::sentence_key;
use flea::ui_logic_st2::wire::{self, Plugin, Reply};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Card {
    #[default]
    None,
    Add(AddCard),
    Remove {
        plugin: String,
        name: String,
        busy: bool,
        error: Option<String>,
    },
}

/// The keys the section acts on, named after the G1 key table's actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Space,
    Escape,
    Backspace,
    Text(String),
}

// One request a card sends once its borrow ends: op, plugin and extra fields.
type Request = (&'static str, String, Vec<(&'static str, Json)>);

pub struct PluginsPage {
    pub(super) i18n: Arc<I18n>,
    pub(super) plugins: Vec<Plugin>,
    pub(super) open: Option<String>,
    pub(super) rows: Vec<Row>,
    pub(super) cursor: Option<usize>,
    // The row id the cursor follows across a rebuild.
    pub(super) focus: String,
    // A plugin just added: the cursor lands on its row once a listing carries it.
    pub(super) want: Option<String>,
    pub(super) next: u64,
    pub(super) listed: u64,
    pub(super) pending: HashMap<u64, String>,
    pub(super) updates: HashMap<String, Update>,
    pub(super) card: Card,
    pub(super) airdrop: flea::ui_logic_st2::airdrop::Page,
    // What a chip pick or a key asked of the host, sent once the pane has the event.
    pub(super) effect: Effect,
}

impl PluginsPage {
    pub fn new(i18n: Arc<I18n>) -> Self {
        Self {
            i18n,
            plugins: Vec::new(),
            open: None,
            rows: Vec::new(),
            cursor: None,
            focus: String::new(),
            want: None,
            next: 0,
            listed: 0,
            pending: HashMap::new(),
            updates: HashMap::new(),
            card: Card::None,
            airdrop: Default::default(),
            effect: Effect::None,
        }
    }

    /// Asks for the installed list; the section shows nothing until it arrives.
    pub fn start(&mut self, wire: &dyn PluginsWire) {
        self.request(wire, "list", "", Vec::new());
    }

    pub fn set_catalog(&mut self, i18n: Arc<I18n>) {
        self.i18n = i18n;
        self.rebuild();
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn card(&self) -> &Card {
        &self.card
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    pub fn open_plugin(&self) -> Option<&Plugin> {
        let id = self.open.as_deref()?;
        self.plugins.iter().find(|p| p.manifest.id == id)
    }

    /// The pane's title: Plugins on the list, the plugin's name on its page.
    pub fn title(&self) -> String {
        match self.open_plugin() {
            Some(plugin) => pages::name(plugin, &self.i18n),
            None => self.i18n.tr("plugins.title"),
        }
    }

    pub fn catalog(&self) -> &I18n {
        &self.i18n
    }

    pub(super) fn request(
        &mut self,
        wire: &dyn PluginsWire,
        op: &str,
        plugin: &str,
        extra: Vec<(&str, Json)>,
    ) -> bool {
        self.next += 1;
        if wire
            .send(wire::request_line(self.next, op, plugin, extra))
            .is_err()
        {
            self.failed();
            return false;
        }
        self.pending.insert(self.next, op.to_string());
        true
    }

    fn sentence(&self, error: &Invalid) -> String {
        self.i18n
            .tr_args(&sentence_key(error.code), &[&error.detail])
    }

    /// One reply line; false when it changed nothing, as for a malformed or stale line.
    pub fn receive(&mut self, line: &str) -> bool {
        match wire::parse(line) {
            Ok(Reply::Listing(listing)) => {
                // Every op answers with a listing under its own id, so an older id is a superseded state.
                if listing.id < self.listed {
                    return false;
                }
                self.listed = listing.id;
                self.plugins = listing.plugins;
                if self.open_plugin().is_none() {
                    self.open = None;
                }
                self.rebuild();
                true
            }
            Ok(Reply::Outcome(outcome)) => {
                let Some(op) = self.pending.remove(&outcome.id) else {
                    return false;
                };
                let error = outcome.error.as_ref().map(|e| self.sentence(e));
                match (op.as_str(), &mut self.card) {
                    ("add", Card::Add(card)) if card.busy => match error {
                        Some(sentence) => {
                            card.busy = false;
                            card.error = Some(sentence);
                        }
                        None => {
                            self.card = Card::None;
                            self.want = Some(outcome.plugin.clone());
                        }
                    },
                    (
                        "remove",
                        Card::Remove {
                            busy, error: shown, ..
                        },
                    ) if *busy => match error {
                        Some(sentence) => {
                            *busy = false;
                            *shown = Some(sentence);
                        }
                        None => {
                            self.card = Card::None;
                            self.focus = outcome.plugin.clone();
                            self.open = None;
                        }
                    },
                    ("update", _) => {
                        let state = match error {
                            Some(_) => Update::Failed,
                            None => Update::Current(outcome.version.clone()),
                        };
                        self.updates.insert(outcome.plugin.clone(), state);
                    }
                    _ => return false,
                }
                self.rebuild();
                true
            }
            Err(_) => false,
        }
    }

    /// The backend went away: nothing pending can answer, so every busy state settles.
    pub fn failed(&mut self) {
        self.pending.clear();
        for state in self.updates.values_mut() {
            if *state == Update::Checking {
                *state = Update::Failed;
            }
        }
        match &mut self.card {
            Card::Add(card) => card.busy = false,
            Card::Remove { busy, .. } => *busy = false,
            Card::None => {}
        }
        self.rebuild();
    }

    pub(super) fn rebuild(&mut self) {
        self.rows = match self.open_plugin() {
            Some(plugin) => {
                let update = self
                    .updates
                    .get(&plugin.manifest.id)
                    .cloned()
                    .unwrap_or_default();
                pages::page_rows_with(plugin, &update, Some(&self.airdrop_page()), &self.i18n)
            }
            None => pages::list_rows(&self.plugins, &self.i18n),
        };
        let wanted = self
            .want
            .as_deref()
            .and_then(|w| self.rows.iter().position(|r| r.focusable() && r.id() == w));
        if wanted.is_some() {
            self.want = None;
        }
        let found = wanted.or_else(|| {
            self.rows
                .iter()
                .position(|r| r.focusable() && !self.focus.is_empty() && r.id() == self.focus)
        });
        self.cursor = found.or_else(|| self.near(self.cursor.unwrap_or(0)));
        self.focus = self
            .cursor
            .map(|i| self.rows[i].id().to_string())
            .unwrap_or_default();
    }

    // The focusable row at or after index, else the last one before it.
    fn near(&self, index: usize) -> Option<usize> {
        let after = (index..self.rows.len()).find(|&i| self.rows[i].focusable());
        after.or_else(|| {
            (0..index.min(self.rows.len()))
                .rev()
                .find(|&i| self.rows[i].focusable())
        })
    }

    pub(super) fn put(&mut self, index: usize) {
        self.cursor = Some(index);
        self.focus = self.rows[index].id().to_string();
    }

    fn step(&mut self, down: bool) {
        let Some(at) = self.cursor else { return };
        let next = if down {
            (at + 1..self.rows.len()).find(|&i| self.rows[i].focusable())
        } else {
            (0..at).rev().find(|&i| self.rows[i].focusable())
        };
        if let Some(next) = next {
            self.put(next);
        }
    }

    pub fn key(&mut self, key: Key, wire: &dyn PluginsWire) -> Effect {
        // A card holds the keyboard; its request goes out once the card's borrow ends.
        let mut send: Option<Request> = None;
        match &mut self.card {
            Card::Add(card) => match key {
                Key::Text(text) if !card.busy => {
                    card.text.push_str(&text);
                    card.error = None;
                }
                Key::Backspace if !card.busy => {
                    card.text.pop();
                    card.error = None;
                }
                Key::Enter if card.can_add() => {
                    card.busy = true;
                    send = Some((
                        "add",
                        String::new(),
                        vec![("source", Json::Str(card.text.trim().to_string()))],
                    ));
                }
                Key::Escape => self.card = Card::None,
                _ => {}
            },
            Card::Remove { plugin, busy, .. } => match key {
                Key::Enter if !*busy => {
                    *busy = true;
                    send = Some(("remove", plugin.clone(), Vec::new()));
                }
                Key::Escape => self.card = Card::None,
                _ => {}
            },
            Card::None => return self.list_key(key, wire),
        }
        if let Some((op, plugin, extra)) = send {
            self.request(wire, op, &plugin, extra);
        }
        Effect::None
    }

    fn list_key(&mut self, key: Key, wire: &dyn PluginsWire) -> Effect {
        match key {
            Key::Up => self.step(false),
            Key::Down => self.step(true),
            Key::Escape if self.open.is_some() => self.back(),
            Key::Escape => return Effect::Close,
            Key::Enter => return self.activate(true, wire),
            Key::Space => return self.activate(false, wire),
            Key::Left | Key::Right => self.adjust(key == Key::Right, wire),
            Key::Backspace | Key::Text(_) => {}
        }
        Effect::None
    }
}
