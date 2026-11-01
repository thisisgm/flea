//! The Permissions card: one item's grid and octal field, or several items' mixed boxes, opening with Apply focused.
mod painted;

use super::{
    keys::{edit, Key},
    Effect,
};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_op3::permissions::{self as logic, Inspected, Skip, Store};

/// The board's card width at its 14 body.
pub const WIDTH: f32 = 480.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Box(usize),
    Octal,
    Cancel,
    Apply,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Held {
    path: String,
    directory: bool,
    mode: String,
    owner: String,
    group: String,
    uid: u64,
    gid: u64,
    reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Reading,
    Ready,
    Applying,
    /// The backend died; the card says so and only Cancel answers.
    Stopped(bool),
}

#[derive(Clone, Debug)]
pub struct Permissions {
    id: u64,
    paths: Vec<String>,
    kind: String,
    held: Held,
    store: Store,
    /// The octal text, or for several items the boxes the user set and cleared.
    pub text: String,
    set: u32,
    clear: u32,
    pub focus: Stop,
    phase: Phase,
    error: String,
    applied: Vec<Skip>,
    /// A failed batch's own error, held until the reread says whether a file it names is now skipped.
    refused: String,
}

fn text(reply: &Json, key: &str) -> String {
    reply
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
}

fn number(reply: &Json, key: &str) -> u64 {
    reply
        .get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.)
        .unwrap_or(0.) as u64
}

impl Permissions {
    /// `kind` is the listing's own kind word for the row (G1 row facts); several items inspect in one id block.
    pub fn open(id: u64, paths: Vec<String>, kind: &str) -> (Self, Vec<Effect>) {
        let base = id * logic::INSPECT_STRIDE;
        let asks = if paths.len() == 1 {
            vec![Effect::Send(logic::inspect_request(id, &paths[0]))]
        } else {
            paths
                .iter()
                .enumerate()
                .map(|(at, path)| Effect::Send(logic::inspect_request(base + at as u64, path)))
                .collect()
        };
        let card = Self {
            id,
            store: Store::new(paths.len()),
            paths,
            kind: kind.to_string(),
            held: Held::default(),
            text: String::new(),
            set: 0,
            clear: 0,
            focus: Stop::Cancel,
            phase: Phase::Reading,
            error: String::new(),
            applied: Vec::new(),
            refused: String::new(),
        };
        (card, asks)
    }

    fn several(&self) -> bool {
        self.paths.len() > 1
    }

    fn summary(&self) -> logic::Summary {
        logic::summarize(&self.store.modes, &self.store.reasons)
    }

    fn changeable(&self) -> bool {
        if self.several() {
            self.summary().changeable
        } else {
            self.held.reason.is_empty() && logic::parse(&self.held.mode).is_some()
        }
    }

    /// The boards keep Apply live with nothing changed: it is the focused default, and an unchanged mode applies as itself.
    fn can_apply(&self) -> bool {
        self.phase == Phase::Ready
            && self.changeable()
            && (self.several() || logic::parse(&self.text).is_some())
    }

    fn toggle(&mut self, at: usize) {
        let bit = 1 << (8 - at);
        if !self.changeable() || self.phase != Phase::Ready {
            return;
        }
        if self.several() {
            let summary = self.summary();
            logic::multi_toggle(&summary, &mut self.set, &mut self.clear, bit);
        } else {
            self.text = logic::toggle(&self.text, bit);
        }
    }

    fn stops(&self) -> Vec<Stop> {
        if self.phase != Phase::Ready || !self.changeable() {
            return vec![Stop::Cancel];
        }
        let mut stops: Vec<Stop> = (0..9).map(Stop::Box).collect();
        if !self.several() {
            stops.push(Stop::Octal);
        }
        stops.push(Stop::Cancel);
        if self.can_apply() {
            stops.push(Stop::Apply);
        }
        stops
    }

    fn inspect_ids(&self) -> Vec<u64> {
        if self.several() {
            (0..self.paths.len() as u64)
                .map(|at| self.id * logic::INSPECT_STRIDE + at)
                .collect()
        } else {
            vec![self.id]
        }
    }

    /// An issued fchmod cannot be cancelled, so nothing dismisses the card until its reply lands; several items close every inspect id.
    fn close(&self) -> Vec<Effect> {
        if self.phase == Phase::Applying {
            return Vec::new();
        }
        let mut effects: Vec<Effect> = self
            .inspect_ids()
            .into_iter()
            .map(|id| Effect::Send(logic::close_request(id)))
            .collect();
        effects.push(Effect::Close);
        effects
    }

    /// A failed batch may have changed some items, so every mode is read again and the pane rereads.
    fn reread(&mut self, i18n: &I18n, error: &str) -> Vec<Effect> {
        self.error = logic::refusal(i18n, error);
        if !self.applied.is_empty() {
            self.error = format!("{}\n{}", self.error, logic::skip_note(i18n, &self.applied));
        }
        self.refused = error.to_string();
        self.store = Store::new(self.paths.len());
        self.phase = Phase::Reading;
        self.focus = Stop::Cancel;
        let mut effects: Vec<Effect> = self
            .paths
            .iter()
            .enumerate()
            .map(|(at, path)| {
                Effect::Send(logic::inspect_request(
                    self.id * logic::INSPECT_STRIDE + at as u64,
                    path,
                ))
            })
            .collect();
        effects.push(Effect::Refresh);
        effects
    }

    fn apply(&mut self) -> Vec<Effect> {
        if !self.can_apply() {
            return Vec::new();
        }
        self.phase = Phase::Applying;
        self.error.clear();
        if !self.several() {
            return vec![Effect::Send(logic::apply_request(self.id, &self.text))];
        }
        let batch = logic::batch(&self.store, &self.paths, self.set, self.clear);
        self.applied = batch.skipped.clone();
        vec![Effect::Send(logic::batch_request(self.id, &batch))]
    }

    pub fn key(&mut self, key: Key) -> Vec<Effect> {
        if self.phase == Phase::Applying {
            return Vec::new();
        }
        if matches!(self.phase, Phase::Stopped(_)) {
            return if matches!(key, Key::Escape | Key::Enter | Key::Space) {
                vec![Effect::Close]
            } else {
                Vec::new()
            };
        }
        let stops = self.stops();
        let at = stops
            .iter()
            .position(|s| *s == self.focus)
            .unwrap_or(stops.len() - 1);
        match (key, self.focus) {
            (Key::Escape, _) => return self.close(),
            (Key::Tab, _) => self.focus = stops[(at + 1) % stops.len()],
            (Key::Backtab, _) => self.focus = stops[(at + stops.len() - 1) % stops.len()],
            (Key::Enter | Key::Space, Stop::Cancel) => return self.close(),
            (Key::Enter | Key::Space, Stop::Apply) => return self.apply(),
            (Key::Enter, Stop::Octal) if self.can_apply() => self.focus = Stop::Apply,
            (Key::Space, Stop::Box(b)) => self.toggle(b),
            (Key::Left, Stop::Box(b)) if b % 3 > 0 => self.focus = Stop::Box(b - 1),
            (Key::Right, Stop::Box(b)) if b % 3 < 2 => self.focus = Stop::Box(b + 1),
            (Key::Up, Stop::Box(b)) if b >= 3 => self.focus = Stop::Box(b - 3),
            (Key::Down, Stop::Box(b)) if b < 6 => self.focus = Stop::Box(b + 3),
            (Key::Left | Key::Right, Stop::Cancel) if self.can_apply() => self.focus = Stop::Apply,
            (Key::Left | Key::Right, Stop::Apply) if self.phase != Phase::Applying => {
                self.focus = Stop::Cancel
            }
            (typed, Stop::Octal) if self.changeable() && self.phase == Phase::Ready => {
                edit(&mut self.text, &typed);
            }
            _ => {}
        }
        Vec::new()
    }

    /// The backend went away: before Apply the card cannot read, during Apply the change may not have finished.
    pub fn stopped(&mut self) {
        self.phase = Phase::Stopped(self.phase == Phase::Applying);
        self.focus = Stop::Cancel;
    }

    /// Sample input: an inspect reply fills the grid; an apply reply closes with a status line and a pane reread.
    pub fn reply(&mut self, i18n: &I18n, reply: &Json) -> Vec<Effect> {
        if matches!(self.phase, Phase::Stopped(_)) {
            return Vec::new();
        }
        let opening =
            self.phase == Phase::Reading && self.error.is_empty() && self.refused.is_empty();
        let ok = reply.get("ok").and_then(Json::as_bool) == Some(true);
        let id = number(reply, "id");
        match reply.get("op").and_then(Json::as_str) {
            Some("inspect") if !self.several() && id == self.id => {
                if ok {
                    self.held = Held {
                        path: text(reply, "path"),
                        directory: reply.get("directory").and_then(Json::as_bool) == Some(true),
                        mode: text(reply, "mode"),
                        owner: text(reply, "owner"),
                        group: text(reply, "group"),
                        uid: number(reply, "uid"),
                        gid: number(reply, "gid"),
                        reason: text(reply, "reason"),
                    };
                    self.text = self.held.mode.clone();
                } else {
                    self.error = logic::closed(&text(reply, "error"));
                }
                self.phase = Phase::Ready;
            }
            Some("inspect") if self.several() => {
                let base = self.id * logic::INSPECT_STRIDE;
                let Some(at) = id
                    .checked_sub(base)
                    .map(|a| a as usize)
                    .filter(|a| *a < self.paths.len())
                else {
                    return Vec::new();
                };
                let answer = if ok {
                    Inspected::Ok {
                        mode: text(reply, "mode"),
                        reason: text(reply, "reason"),
                    }
                } else {
                    Inspected::Refused {
                        error: text(reply, "error"),
                    }
                };
                let path = self.paths[at].clone();
                if self.store.note(at, &path, answer) {
                    self.phase = Phase::Ready;
                    // A batch refused for a file the reread cannot inspect is named by the held note, which quotes that refusal.
                    if !self.refused.is_empty()
                        && self
                            .store
                            .skipped
                            .iter()
                            .any(|skip| skip.why == self.refused)
                    {
                        self.error.clear();
                    }
                    self.refused.clear();
                }
            }
            Some("apply") if id == self.id && self.phase == Phase::Applying => {
                if ok {
                    self.phase = Phase::Ready;
                    return [
                        vec![
                            Effect::Message(i18n.tr("dialogs.file.permissions.changed")),
                            Effect::Refresh,
                        ],
                        self.close(),
                    ]
                    .concat();
                }
                self.phase = Phase::Ready;
                self.error = logic::refusal(i18n, &text(reply, "error"));
                self.focus = Stop::Cancel;
            }
            Some("applyMany") if id == self.id && self.phase == Phase::Applying => {
                // One batch is one reply; a refusal keeps the card open with the reason, and a rollback that failed leaves some items changed.
                self.phase = Phase::Ready;
                if ok {
                    return [
                        vec![
                            Effect::Message(logic::multi_result(i18n, &self.applied)),
                            Effect::Refresh,
                        ],
                        self.close(),
                    ]
                    .concat();
                }
                return self.reread(i18n, &text(reply, "error"));
            }
            _ => {}
        }
        if self.phase == Phase::Ready && (opening || !self.changeable()) {
            self.focus = if self.can_apply() {
                Stop::Apply
            } else {
                Stop::Cancel
            };
        }
        Vec::new()
    }
}

mod draw;
pub mod host;
mod read;
