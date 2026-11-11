//! What Enter, Space, the arrows and a click do to the row under the cursor.
use super::page::{Card, PluginsPage};
use super::seam::{Effect, PluginsWire};
use flea::jsondoc::Json;
use flea::ui_logic_plugin_settings::airdrop;
use flea::ui_logic_plugin_settings::builtin::SHELF;
use flea::ui_logic_plugin_settings::pages::{
    self, AddCard, Row, Update, ADD_ID, BACK_ID, REMOVE_ID, UPDATE_ID,
};
use flea::ui_logic_plugin_settings::schema::Value;

impl PluginsPage {
    /// A click on a row puts the cursor there and acts as Enter does.
    pub fn click(&mut self, index: usize, wire: &dyn PluginsWire) -> Effect {
        if self.card != Card::None || !self.rows.get(index).is_some_and(Row::focusable) {
            return Effect::None;
        }
        self.put(index);
        self.activate(true, wire)
    }

    /// A click on a row's checkbox toggles it without opening anything.
    pub fn toggle(&mut self, index: usize, wire: &dyn PluginsWire) {
        if self.card != Card::None || !self.rows.get(index).is_some_and(Row::focusable) {
            return;
        }
        self.put(index);
        self.activate(false, wire);
    }

    /// A click on one chip of a segment row picks that value.
    pub fn pick(&mut self, index: usize, value: &str, wire: &dyn PluginsWire) {
        if self.card != Card::None {
            return;
        }
        if let Some(Row::Choice {
            id,
            available: true,
            ..
        }) = self.rows.get(index).cloned()
        {
            self.put(index);
            self.set(&id, Value::Choice(value.to_string()), wire);
        }
    }

    pub(super) fn back(&mut self) {
        self.focus = self.open.take().unwrap_or_default();
        self.cursor = None;
        self.rebuild();
    }

    fn switch(&mut self, plugin: &str, on: bool, wire: &dyn PluginsWire) {
        self.request(
            wire,
            if on { "enable" } else { "disable" },
            plugin,
            Vec::new(),
        );
    }

    // A row id names its plugin and key: shelf.<key> for the Shelf, <plugin>:<key> for a schema field.
    fn set(&mut self, row: &str, value: Value, wire: &dyn PluginsWire) {
        if let (airdrop::RECEIVING_ID, Value::Choice(value)) = (row, &value) {
            return self.airdrop_receiving(value);
        }
        let (plugin, key) = match row.strip_prefix("shelf.") {
            Some(key) => (SHELF.to_string(), key.to_string()),
            None => match row.rsplit_once(':') {
                Some((plugin, key)) => (plugin.to_string(), key.to_string()),
                None => return,
            },
        };
        self.request(
            wire,
            "set",
            &plugin,
            vec![("key", Json::Str(key)), ("value", value.to_json())],
        );
    }

    pub(super) fn activate(&mut self, enter: bool, wire: &dyn PluginsWire) -> Effect {
        let Some(row) = self.cursor.map(|i| self.rows[i].clone()) else {
            return Effect::None;
        };
        match row {
            Row::Group { id, .. } if id == ADD_ID => {
                self.card = Card::Add(AddCard::default());
            }
            Row::Group { id, .. } if id == "pinFolder" => return Effect::PinFolder,
            Row::Group {
                switch: Some(on), ..
            } => {
                if let Some(plugin) = self.open.clone() {
                    self.switch(&plugin, !on, wire);
                }
            }
            Row::Plugin { id, .. } if enter => {
                self.open = Some(id);
                self.cursor = None;
                self.focus.clear();
                self.rebuild();
            }
            Row::Plugin { id, on, .. } => self.switch(&id, !on, wire),
            Row::Check {
                id,
                on,
                available: true,
                ..
            } => self.set(&id, Value::Bool(!on), wire),
            Row::Choice { .. } | Row::Number { .. } => self.adjust(true, wire),
            Row::Action { id, .. } if id == UPDATE_ID => {
                if let Some(plugin) = self.open.clone() {
                    if self.request(wire, "update", &plugin, Vec::new()) {
                        self.updates.insert(plugin, Update::Checking);
                        self.rebuild();
                    }
                }
            }
            Row::Action { id, .. } if id == REMOVE_ID => {
                if let Some(plugin) = self.open_plugin() {
                    let name = pages::name(plugin, &self.i18n);
                    self.card = Card::Remove {
                        plugin: plugin.manifest.id.clone(),
                        name,
                        busy: false,
                        error: None,
                    };
                }
            }
            Row::Action { id, .. } if id == BACK_ID => self.back(),
            Row::Action { id, .. } if id == airdrop::FOLDER_ID => {
                return Effect::AirdropFolder(self.airdrop_page().folder)
            }
            Row::Favourite { path, .. } => return Effect::Open(path),
            _ => {}
        }
        Effect::None
    }

    // Right or Enter moves a segment to its next value and a number up a step; Left goes the other way, stopping at the ends.
    pub(super) fn adjust(&mut self, up: bool, wire: &dyn PluginsWire) {
        let Some(row) = self.cursor.map(|i| self.rows[i].clone()) else {
            return;
        };
        match row {
            Row::Choice {
                id,
                values,
                selected,
                available: true,
                ..
            } => {
                let at = values.iter().position(|v| *v == selected).unwrap_or(0);
                let next = if up {
                    (at + 1).min(values.len().saturating_sub(1))
                } else {
                    at.saturating_sub(1)
                };
                if next != at {
                    self.set(&id, Value::Choice(values[next].clone()), wire);
                }
            }
            Row::Number {
                id,
                value,
                min,
                max,
                step,
                available: true,
                ..
            } => {
                let next = if up {
                    value.saturating_add(step)
                } else {
                    value.saturating_sub(step)
                }
                .clamp(min, max);
                if next != value {
                    self.set(&id, Value::Int(next), wire);
                }
            }
            _ => {}
        }
    }
}
