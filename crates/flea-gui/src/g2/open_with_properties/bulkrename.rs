//! The bulk rename card: find and replace over the old names, editable new lines, and Edit in Terminal through the opener lane.
use super::{
    keys::{edit, Key},
    shell::{Block, Button, Card, Field, Ink, NameLine},
    Effect,
};
use flea::i18n::I18n;
use flea::ui_logic_open_with_properties::bulkrename::{self as logic, Reloaded, Stop};

#[cfg(test)]
#[path = "m9_g2rows_04_tests.rs"]
mod m9_g2rows_04_tests;

pub const WIDTH: f32 = 640.;
/// The card's buttons left to right; card draws them in this order and press_button reads it.
const BUTTONS: [Stop; 3] = [Stop::Edit, Stop::Cancel, Stop::Rename];

#[derive(Clone, Debug)]
pub struct BulkRename {
    paths: Vec<String>,
    old: Vec<String>,
    /// The names find and replace work over: the old names, or the lines an editor or a hand edit left.
    base: Vec<String>,
    pub find: String,
    pub replace: String,
    pub names: Vec<String>,
    pub focus: Stop,
    editing: bool,
    error: String,
}

impl BulkRename {
    /// Opens with Find focused and every new name its old name.
    pub fn open(paths: Vec<String>) -> Self {
        let old: Vec<String> = paths
            .iter()
            .map(|p| flea::ui_logic_open_with_properties::permissions::leaf_of(p).to_string())
            .collect();
        Self {
            paths,
            base: old.clone(),
            names: old.clone(),
            old,
            find: String::new(),
            replace: String::new(),
            focus: Stop::Find,
            editing: false,
            error: String::new(),
        }
    }

    fn recompute(&mut self) {
        self.names = logic::apply_find_replace(&self.base, &self.find, &self.replace);
    }

    fn counts(&self) -> logic::Counts {
        logic::counts(&self.old, &self.names, &logic::reasons(&self.names))
    }

    fn rename(&self) -> Vec<Effect> {
        if !logic::can_rename(self.counts()) || self.editing {
            return Vec::new();
        }
        logic::request(&self.paths, &self.names)
            .map_or_else(Vec::new, |line| vec![Effect::Send(line), Effect::Close])
    }

    /// Enter and a click on Edit in Terminal both land here, with the names the card shows now (#305).
    fn edit_in_terminal(&mut self) -> Vec<Effect> {
        if self.editing {
            return Vec::new();
        }
        self.editing = true;
        self.error.clear();
        vec![Effect::EditNames {
            folder: logic::folder_of(self.paths.first().map_or("", String::as_str)).to_string(),
            names: self.names.clone(),
        }]
    }

    /// The editor exited: its lines become the new names, or the card says why they were refused.
    pub fn edited(&mut self, i18n: &I18n, outcome: Result<String, &'static str>) {
        self.editing = false;
        match outcome.map(|text| logic::reload(&text, self.old.len())) {
            Ok(Reloaded::Names(names)) => {
                self.base = names;
                self.find.clear();
                self.replace.clear();
                self.recompute();
            }
            Ok(Reloaded::TooLarge) => self.error = i18n.tr("dialogs.file.bulk.err_limits"),
            Ok(Reloaded::CountChanged) => self.error = i18n.tr("dialogs.file.bulk.err_count"),
            Err(key) => self.error = i18n.tr(key),
        }
    }

    /// A press on a stop, from Enter or a click, so both reach the same action.
    pub fn press(&mut self, stop: Stop) -> Vec<Effect> {
        self.focus = stop;
        match stop {
            Stop::Cancel => vec![Effect::Close],
            Stop::Edit => self.edit_in_terminal(),
            _ => self.rename(),
        }
    }

    /// A click on the card's button at an index: the stop the card drew there, pressed as Enter presses it.
    pub fn press_button(&mut self, at: usize) -> Vec<Effect> {
        BUTTONS
            .get(at)
            .map_or_else(Vec::new, |&stop| self.press(stop))
    }

    pub fn key(&mut self, key: Key) -> Vec<Effect> {
        let lines = self.names.len();
        match (&key, self.focus) {
            (Key::Escape, _) => return vec![Effect::Close],
            (Key::Tab, _) => self.focus = logic::next_stop(Some(self.focus), false, lines),
            (Key::Backtab, _) => self.focus = logic::next_stop(Some(self.focus), true, lines),
            (Key::Enter | Key::Down, Stop::Line(at)) if at + 1 < lines => {
                self.focus = Stop::Line(at + 1)
            }
            (Key::Up, Stop::Line(at)) if at > 0 => self.focus = Stop::Line(at - 1),
            (Key::Enter, Stop::Line(_)) => {}
            (Key::Enter, stop) => return self.press(stop),
            (Key::Left | Key::Right, Stop::Edit | Stop::Cancel | Stop::Rename) => {
                let row = [Stop::Edit, Stop::Cancel, Stop::Rename];
                let at = row.iter().position(|s| *s == self.focus).unwrap_or(2);
                self.focus = row[if key == Key::Left {
                    at.saturating_sub(1)
                } else {
                    (at + 1).min(2)
                }];
            }
            (_, Stop::Find) if edit(&mut self.find, &key) => self.recompute(),
            (_, Stop::Replace) if edit(&mut self.replace, &key) => self.recompute(),
            (_, Stop::Line(at)) => {
                let mut line = logic::name_at(&self.names, at).to_string();
                if edit(&mut line, &key) {
                    self.base = self.names.clone();
                    self.base[at] = line;
                    self.find.clear();
                    self.replace.clear();
                    self.recompute();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    pub fn card(&self, i18n: &I18n) -> Card {
        let tr = |key: &str| i18n.tr(key);
        let reasons = logic::reasons(&self.names);
        let counts = logic::counts(&self.old, &self.names, &reasons);
        let new = self
            .old
            .iter()
            .enumerate()
            .map(|(at, old)| {
                let text = logic::name_at(&self.names, at).to_string();
                let (ink, reason) = match reasons[at] {
                    Some(why) => (Ink::Error, logic::reason_text(i18n, why)),
                    None if text != *old => (Ink::Foreground, String::new()),
                    None => (Ink::Muted, String::new()),
                };
                NameLine {
                    input: None,
                    text,
                    ink,
                    reason,
                }
            })
            .collect();
        let field = |label: &str, value: &str, stop: Stop| Field {
            input: None,
            label: Some(tr(label)),
            value: value.to_string(),
            placeholder: String::new(),
            glyph: None,
            focused: self.focus == stop,
        };
        // An editor's refusal takes the count line's place in the error ink, as v0.3.12 did.
        let (summary, ink) = if self.error.is_empty() {
            (logic::footer(i18n, counts), Ink::Foreground)
        } else {
            (self.error.clone(), Ink::Error)
        };
        let body = vec![
            Block::Fields(
                field("dialogs.file.bulk.find", &self.find, Stop::Find),
                field("dialogs.file.bulk.replace", &self.replace, Stop::Replace),
            ),
            Block::Names {
                old_label: tr("dialogs.file.bulk.old_names"),
                old: self.old.clone(),
                new_label: tr("dialogs.file.bulk.new_names"),
                new,
                focused: matches!(self.focus, Stop::Line(_)),
                error: logic::clash_line(i18n, &reasons),
            },
            Block::Note {
                text: summary,
                inset: 0.,
                ink,
                lift: 0.,
            },
        ];
        let button = |label: String, stop: Stop, primary: bool, enabled: bool| Button {
            label,
            primary,
            focused: self.focus == stop,
            enabled,
        };
        Card {
            language: i18n.language().to_owned(),
            title: logic::title(i18n, self.old.len()),
            esc: tr("dialogs.file.esc"),
            width: WIDTH,
            bleed: false,
            body,
            buttons: vec![
                button(
                    tr("dialogs.file.bulk.edit"),
                    BUTTONS[0],
                    false,
                    !self.editing,
                ),
                button(tr("dialogs.file.cancel"), BUTTONS[1], false, true),
                button(
                    logic::rename_label(i18n, counts.renamed),
                    BUTTONS[2],
                    true,
                    logic::can_rename(counts) && !self.editing,
                ),
            ],
        }
    }
}

impl BulkRename {
    pub(crate) fn ime_value(&self) -> Option<(crate::g2::file_ops::dialog_editor::Slot, &str)> {
        use crate::g2::file_ops::dialog_editor::Slot;
        if self.editing {
            return None;
        }
        match self.focus {
            Stop::Find => Some((Slot::BulkFind, &self.find)),
            Stop::Replace => Some((Slot::BulkReplace, &self.replace)),
            Stop::Line(at) => self
                .names
                .get(at)
                .map(|name| (Slot::BulkLine(at), name.as_str())),
            _ => None,
        }
    }
    pub(crate) fn ime_set(&mut self, slot: crate::g2::file_ops::dialog_editor::Slot, text: &str) {
        use crate::g2::file_ops::dialog_editor::Slot;
        match slot {
            Slot::BulkFind => self.find = text.into(),
            Slot::BulkReplace => self.replace = text.into(),
            Slot::BulkLine(at) if at < self.names.len() => {
                self.base = self.names.clone();
                self.base[at] = text.into();
                self.find.clear();
                self.replace.clear();
            }
            _ => return,
        }
        self.recompute();
    }
}
