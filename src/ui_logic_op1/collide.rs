// The question a paste or a drop asks before it lands on names that exist, and the lines it sends.
use super::{count, n, wire::Collision};
use crate::i18n::I18n;
use crate::json::escape;

pub use super::fat::{fat_note, fat_title, filesystem_label, grouped, Fat, FAT_BUTTONS, FAT_START};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Cancel,
    Skip,
    Merge,
    Replace,
    Keep,
    Refuse,
}

impl Choice {
    pub fn wire(self) -> &'static str {
        match self {
            Choice::Cancel => "cancel",
            Choice::Skip => "skip",
            Choice::Merge => "merge",
            Choice::Replace => "replace",
            Choice::Keep => "keep",
            Choice::Refuse => "refuse",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Choice::Cancel | Choice::Refuse => "jobs.button.cancel",
            Choice::Skip => "jobs.button.skip",
            Choice::Merge => "jobs.button.merge",
            Choice::Replace => "jobs.button.replace",
            Choice::Keep => "jobs.button.keep_both",
        }
    }
}

// Left to right as the board draws them: Keep Both, the default, at the trailing end.
pub const BUTTONS: [Choice; 5] = [
    Choice::Cancel,
    Choice::Skip,
    Choice::Merge,
    Choice::Replace,
    Choice::Keep,
];
// Where the card opens, so a reflexive Enter takes the one choice that changes nothing already there.
pub const START: Choice = Choice::Keep;
pub const PRIMARY: Choice = Choice::Keep;
pub const DESTRUCTIVE: Choice = Choice::Replace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Tab,
    Backtab,
    Activate,
    Escape,
    Other,
}

// h and l are Left and Right; Shift+Tab reaches a handler as Backtab or as Tab with Shift held.
pub fn key(name: &str, shift: bool) -> Key {
    match name {
        "h" | "left" => Key::Left,
        "l" | "right" => Key::Right,
        "tab" if shift => Key::Backtab,
        "tab" => Key::Tab,
        "backtab" => Key::Backtab,
        "enter" | "return" | "space" | "kp_enter" => Key::Activate,
        "escape" => Key::Escape,
        _ => Key::Other,
    }
}

pub fn shown(merge: bool) -> Vec<Choice> {
    BUTTONS
        .iter()
        .copied()
        .filter(|b| merge || *b != Choice::Merge)
        .collect()
}

// h and l stop at the ends; Tab and Backtab come round. A focus not in order starts from start.
pub fn walk<T: Copy + PartialEq>(order: &[T], focus: T, key: Key, start: T) -> T {
    let last = order.len().saturating_sub(1);
    let at = order
        .iter()
        .position(|b| *b == focus)
        .unwrap_or_else(|| order.iter().position(|b| *b == start).unwrap_or(0));
    match key {
        Key::Left => order[at.saturating_sub(1)],
        Key::Right => order[(at + 1).min(last)],
        Key::Backtab => order[(at + last) % order.len()],
        Key::Tab => order[(at + 1) % order.len()],
        _ => order[at],
    }
}

pub fn moved(focus: Choice, key: Key, merge: bool) -> Choice {
    walk(&shown(merge), focus, key, START)
}

// The folder's own name, which is what the title says; the root has no leaf, so it says itself.
pub fn folder_name(dest: &str) -> &str {
    let leaf = super::leaf(dest);
    if leaf.is_empty() {
        "/"
    } else {
        leaf
    }
}

// "screenshot.png already exists in Pictures." for one, "3 items already exist in Downloads." for several.
pub fn title(catalog: &I18n, names: &[Collision], total: u64, dest: &str) -> String {
    match names.first() {
        Some(first) if total == 1 => {
            catalog.tr_args("jobs.collide.exists_one", &[&first.name, folder_name(dest)])
        }
        _ => catalog.trn_args("jobs.collide.exists_many", n(total), &[folder_name(dest)]),
    }
}

// The row under the list, empty when every collision is already on it.
pub fn more(catalog: &I18n, total: u64, shown: usize) -> String {
    let rest = total.saturating_sub(shown as u64);
    if rest == 0 {
        String::new()
    } else {
        catalog.trn("jobs.collide.more", n(rest))
    }
}

// One question at a time, open or in flight: a second is refused out loud rather than replacing the first.
pub fn refused(opened: bool, pending: bool) -> bool {
    opened || pending
}

pub fn activates(key: Key) -> bool {
    key == Key::Activate
}

// A refused question ends the wait the way an answered one does: the card never opens.
pub fn dropped_pending(place: &str, input: &str) -> bool {
    place == "stale" && (input == "collisions" || input == "mergecollisions")
}

// The waiting request as the ordered JSON members it is sent with; an answer is added on the end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    fields: Vec<(String, String)>,
}

pub fn text(value: &str) -> String {
    format!("\"{}\"", escape(value))
}

pub fn texts(values: &[&str]) -> String {
    format!(
        "[{}]",
        values.iter().map(|v| text(v)).collect::<Vec<_>>().join(",")
    )
}

pub fn numbers(values: &[u64]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

impl Request {
    // Sample input: Request::new().with("c", &text("transfer")).with("op", &text("copy")).with("dest", &text("/b"))
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: &str, raw: &str) -> Self {
        self.set(key, raw);
        self
    }

    // A member already there keeps its place and takes the new value, the way Object.assign writes.
    pub fn set(&mut self, key: &str, raw: &str) {
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = raw.into(),
            None => self.fields.push((key.into(), raw.into())),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn named(&self, key: &str) -> bool {
        self.get(key)
            .is_some_and(|v| v != "[]" && v != "0" && v != "\"\"" && v != "null")
    }

    pub fn is_link(&self) -> bool {
        self.get("c") == Some("\"link\"")
    }

    pub fn json(&self) -> String {
        let members: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("\"{}\":{v}", escape(k)))
            .collect();
        format!("{{{}}}", members.join(","))
    }

    // Rows are stamped with the numbering in force; a menu's capture, a stamped request and no rows stay as they are.
    pub fn waiting(&self, held: u64) -> Self {
        let mut out = self.clone();
        if !self.named("menuId") && self.named("rows") && self.get("listing").is_none() && held > 0
        {
            out.set("listing", &held.to_string());
        }
        out
    }
}

impl Request {
    // The send's own stamp: any rows request without a listing takes the numbering in force, a menu's included.
    pub fn stamp_at_send(&self, held: u64) -> Self {
        let mut out = self.clone();
        if self.get("rows").is_some() && self.get("listing").is_none() && held > 0 {
            out.set("listing", &held.to_string());
        }
        out
    }
}

// The question names the sources the transfer will, in the backend's order: a drag's paths, a menu, paths, rows.
fn asked(c: &str, request: &Request, probe: &[&str], id: u64, fat: bool) -> String {
    let mut out = Request::new()
        .with("c", &text(c))
        .with("id", &id.to_string())
        .with("dest", request.get("dest").unwrap_or("\"\""));
    if fat {
        if let Some(f) = request.get("fat").filter(|_| request.named("fat")) {
            out.set("fat", f);
        }
    }
    if !probe.is_empty() {
        out.set("paths", &texts(probe));
    } else if request.named("menuId") {
        out.set("menuId", request.get("menuId").unwrap_or("0"));
    } else if request.named("paths") {
        out.set("paths", request.get("paths").unwrap_or("[]"));
    } else {
        out.set("rows", request.get("rows").unwrap_or("[]"));
        if let Some(listing) = request.get("listing") {
            out.set("listing", listing);
        }
    }
    out.json()
}

pub fn question(request: &Request, probe: &[&str], id: u64) -> String {
    asked("collisions", request, probe, id, true)
}

// After Merge, one walk of the sources against the destination, before anything is written.
pub fn merge_question(request: &Request, probe: &[&str], id: u64) -> String {
    asked("mergecollisions", request, probe, id, false)
}

// The waiting transfer with the answer on it; the id lets the backend cover only the names it listed.
pub fn transfer(request: &Request, choice: Choice, id: u64) -> String {
    request
        .clone()
        .with("collide", &text(choice.wire()))
        .with("collideId", &id.to_string())
        .json()
}

// Paste as links answers like a transfer; rows ride along only when named, so a paths-only paste skips listing checks.
pub fn link(request: &Request, choice: Choice, id: u64) -> String {
    let mut out = Request::new()
        .with("c", &text("link"))
        .with("op", request.get("op").unwrap_or("\"\""))
        .with("paths", request.get("paths").unwrap_or("[]"))
        .with("dest", request.get("dest").unwrap_or("\"\""))
        .with("collide", &text(choice.wire()))
        .with("collideId", &id.to_string());
    if let Some(f) = request.get("fat").filter(|_| request.named("fat")) {
        out.set("fat", f);
    }
    if request.named("rows") {
        out.set("rows", request.get("rows").unwrap_or("[]"));
    }
    if let Some(listing) = request.get("listing") {
        out.set("listing", listing);
    }
    out.json()
}

// The answer line for whichever request is waiting.
pub fn answer(request: &Request, choice: Choice, id: u64) -> String {
    if request.is_link() {
        link(request, choice, id)
    } else {
        transfer(request, choice, id)
    }
}

// The merge transfer with the second card's answer on it; the id names the walk it answers.
pub fn merge_transfer(request: &Request, inner: Choice, id: u64) -> String {
    request
        .clone()
        .with("collide", &text("merge"))
        .with("collideId", &id.to_string())
        .with("inner", &text(inner.wire()))
        .json()
}

// The second card names the folder it merges into: the one merging folder, else the destination.
pub fn merge_dest(dest: &str, outer: &[Collision]) -> String {
    let folders: Vec<&Collision> = outer.iter().filter(|c| c.dir).collect();
    match folders.as_slice() {
        [one] => format!("{}/{}", dest.trim_end_matches('/'), one.name),
        _ => dest.into(),
    }
}

// The board's merged line: whole sentences joined once, the undo hint its suffix when anything merged.
#[derive(Clone, Copy, Debug, Default)]
pub struct Merged<'a> {
    pub merged: u64,
    pub kept_both: u64,
    pub into: &'a str,
    pub failed: u64,
    pub skipped: u64,
    pub cancelled: bool,
    pub note: &'a str,
}

pub fn merged_line(catalog: &I18n, preset: &str, m: Merged<'_>) -> String {
    let line = merged_body(catalog, m);
    if m.merged > 0 {
        line + &super::undo_hint(catalog, preset)
    } else {
        line
    }
}

/// The merge receipt alone, for a footer that draws its undo advice as a separate target.
pub fn merged_body(catalog: &I18n, m: Merged<'_>) -> String {
    let mut segments = vec![catalog.trn_args("jobs.merged.into", n(m.merged), &[m.into])];
    if m.kept_both > 0 {
        segments.push(catalog.tr_args("jobs.merged.kept_both", &[&count(catalog, m.kept_both)]));
    }
    if m.failed > 0 {
        segments.push(catalog.tr_args("jobs.merged.failed", &[&count(catalog, m.failed)]));
    }
    if m.skipped > 0 {
        segments.push(catalog.tr_args("jobs.merged.skipped", &[&count(catalog, m.skipped)]));
    }
    if m.cancelled {
        segments.push(catalog.tr("jobs.merged.cancelled"));
    }
    if m.merged > 0 && !m.note.is_empty() {
        segments.push(m.note.into());
    }
    segments.join(&catalog.tr("jobs.merged.join"))
}

pub fn explain(catalog: &I18n) -> String {
    catalog.tr("jobs.collide.explain")
}

pub fn waiting_refusal(catalog: &I18n) -> String {
    catalog.tr("jobs.collide.waiting")
}
