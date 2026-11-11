//! Trash thumbnail state: no row asks for an image, since v0.3.12 draws the generic icon on every Trash row; replies still land by ticket.
use super::rows::{Line, TrashRow};
use std::collections::HashMap;

/// The settle before the first batch, and before every later one.
pub const SETTLE_FIRST_MS: u64 = 70;
pub const SETTLE_LATER_MS: u64 = 120;
/// Row results kept; the cap bounds a policy bug, since only viewport rows are ever recorded.
pub const CAP: usize = 240;

/// preview.thumbnails in ui.json.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Images,
    Media,
}

/// The thumbnail mode and the three storage-class switches of ExtThumbs.js.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub mode: Mode,
    pub usb: bool,
    pub network: bool,
    pub phone: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self { mode: Mode::Media, usb: true, network: false, phone: false }
    }
}

impl Policy {
    fn class_off(&self, class: &str) -> bool {
        match class {
            "usb" => !self.usb,
            "network" => !self.network,
            "phone" => !self.phone,
            _ => false,
        }
    }
    /// A class switched off asks the shared cache only, so an existing image may draw without a decoder.
    pub fn cache_only(&self, class: &str) -> bool {
        self.class_off(class) && self.mode != Mode::Off
    }
}

/// What a row index holds: asked and waiting, or answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Asked,
    CacheAsked,
    /// An answer; empty is a refusal, which keeps the glyph.
    File(String),
    CacheMiss,
}

impl Slot {
    fn pending(&self) -> bool {
        matches!(self, Self::Asked | Self::CacheAsked)
    }
}

/// The identity a request was made for; a reply counts only while the row still matches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub id: u64,
    uri: String,
    path: String,
    identity: String,
    storage_class: Option<String>,
}

impl Ticket {
    fn same(&self, row: Option<&TrashRow>) -> bool {
        let Some(row) = row else { return false };
        let Some(thumb) = &row.thumbnail else { return false };
        self.uri == row.uri && self.path == thumb.path && self.identity == thumb.identity && self.storage_class == row.storage_class
    }
}

/// The held metadata the scheduler reads: rows from `first`, out of `total`.
#[derive(Clone, Copy)]
pub struct Held<'a> {
    pub first: usize,
    pub rows: &'a [TrashRow],
    pub total: usize,
}

impl Held<'_> {
    pub fn row(&self, index: usize) -> Option<&TrashRow> {
        index.checked_sub(self.first).and_then(|at| self.rows.get(at))
    }
}

/// Thumbs.viewport: the rows a request may name, the partly shown bottom row included.
pub fn viewport(scroll_y: f32, row_height: f32, height: f32, total: usize) -> Option<(usize, usize)> {
    if total == 0 || row_height <= 0. {
        return None;
    }
    let first = (scroll_y.max(0.) / row_height).floor() as usize;
    let visible_rows = (height / row_height).ceil();
    let bottom = scroll_y.max(0.) + visible_rows * row_height;
    let last = ((bottom / row_height).ceil() as usize).saturating_sub(1).min(total - 1);
    (first <= last).then_some((first, last))
}

#[derive(Debug, Default)]
pub struct Thumbs {
    slots: HashMap<usize, Slot>,
    order: Vec<usize>,
    tickets: HashMap<usize, Ticket>,
    next_id: u64,
    later: bool,
    settle: u64,
    out: Vec<String>,
}

impl Thumbs {
    /// The lines to send, oldest first.
    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.out)
    }
    pub fn delay_ms(&self) -> u64 {
        if self.later { SETTLE_LATER_MS } else { SETTLE_FIRST_MS }
    }
    /// The settle generation a timer must carry; a newer restart or a reset outdates it.
    pub fn settle(&self) -> u64 {
        self.settle
    }
    pub fn slot(&self, index: usize) -> Option<&Slot> {
        self.slots.get(&index)
    }
    pub fn ticket(&self, index: usize) -> Option<&Ticket> {
        self.tickets.get(&index)
    }
    pub fn tickets(&self) -> usize {
        self.tickets.len()
    }
    pub fn recorded(&self) -> usize {
        self.slots.len()
    }

    /// The image a row draws, empty while waiting, refused or switched off.
    pub fn file_for(&self, index: usize, policy: Policy) -> &str {
        match self.slots.get(&index) {
            Some(Slot::File(file)) if policy.mode != Mode::Off => file,
            _ => "",
        }
    }

    fn forget(&mut self, index: usize) {
        self.slots.remove(&index);
        self.order.retain(|at| *at != index);
    }

    fn cancel(&mut self, indices: &[usize]) {
        let mut ids = Vec::new();
        for index in indices {
            if let Some(ticket) = self.tickets.remove(index) {
                ids.push(ticket.id);
            }
            self.forget(*index);
        }
        if !ids.is_empty() {
            self.out.push(Line::new("trashthumbcancel").numbers("ids", &ids).finish());
        }
    }

    /// Close, hide, list, restore and delete: every ticket cancelled and every state cleared at once.
    pub fn reset(&mut self) {
        self.settle += 1;
        let all: Vec<usize> = self.tickets.keys().copied().collect();
        self.cancel(&all);
        self.slots.clear();
        self.order.clear();
        self.later = false;
    }

    /// Starts the settle again; the caller times delay_ms() and calls request with this generation.
    pub fn restart(&mut self, ready: bool) -> Option<(u64, u64)> {
        if !ready {
            return None;
        }
        self.settle += 1;
        Some((self.settle, self.delay_ms()))
    }

    /// New rows: a ticket whose row changed under it is cancelled.
    pub fn sync(&mut self, held: Held, ready: bool) -> Option<(u64, u64)> {
        let drop: Vec<usize> = self.tickets.iter().filter(|(index, ticket)| !ticket.same(held.row(**index))).map(|(index, _)| *index).collect();
        self.cancel(&drop);
        self.restart(ready)
    }

    /// A scroll cancels pending rows that left the viewport, held or not.
    pub fn scroll(&mut self, span: Option<(usize, usize)>, ready: bool) -> Option<(u64, u64)> {
        let (first, last) = span.unwrap_or((1, 0));
        let gone: Vec<usize> = self.order.iter().copied().filter(|index| self.slots[index].pending() && (*index < first || *index > last)).collect();
        self.cancel(&gone);
        self.restart(ready)
    }

    /// The settle fired: ask for every visible image row never asked, drop pending rows out of view.
    pub fn request(&mut self, generation: u64, held: Held, span: Option<(usize, usize)>, policy: Policy, ready: bool) {
        if !ready || generation != self.settle {
            return;
        }
        // v0.3.12 draws the generic document icon on every Trash row (TrashView.qml), so no Trash row asks for a thumbnail.
        let allowed = |_: Option<&TrashRow>| false;
        let (first, last) = span.unwrap_or((1, 0));
        let ask: Vec<usize> = (first..=last).filter(|i| !self.slots.contains_key(i) && allowed(held.row(*i))).collect();
        let drop: Vec<usize> = self.order.iter().copied()
            .filter(|i| self.slots[i].pending() && (*i < first || *i > last || !allowed(held.row(*i)))).collect();
        self.cancel(&drop);
        for index in &ask {
            let row = held.row(*index).expect("an allowed row is held");
            let thumb = row.thumbnail.as_ref().expect("an allowed row has an image");
            let class = row.storage_class.as_deref().unwrap_or("");
            let cache_only = policy.cache_only(class);
            self.next_id += 1;
            self.slots.insert(*index, if cache_only { Slot::CacheAsked } else { Slot::Asked });
            self.order.push(*index);
            self.tickets.insert(*index, Ticket {
                id: self.next_id, uri: row.uri.clone(), path: thumb.path.clone(),
                identity: thumb.identity.clone(), storage_class: row.storage_class.clone(),
            });
            self.out.push(Line::new("trashthumb").number("id", self.next_id).text("original", &row.original)
                .flag("cacheOnly", cache_only).text("path", &thumb.path).text("provider", &thumb.provider)
                .text("identity", &thumb.identity).number("mtime", thumb.mtime).finish());
        }
        if !ask.is_empty() {
            self.later = true;
        }
    }

    /// A trashthumbed reply: taken only for a pending row whose identity still matches its ticket.
    pub fn receive(&mut self, id: u64, file: &str, held: Held, ready: bool) -> bool {
        if !ready {
            return false;
        }
        let Some(index) = self.tickets.iter().find(|(_, t)| t.id == id).map(|(i, _)| *i) else { return false };
        if !self.tickets[&index].same(held.row(index)) || !self.slots.get(&index).is_some_and(Slot::pending) {
            return false;
        }
        let slot = match (file.is_empty(), &self.slots[&index]) {
            (true, Slot::CacheAsked) => Slot::CacheMiss,
            _ => Slot::File(file.into()),
        };
        self.slots.insert(index, slot);
        while self.order.len() > CAP {
            let oldest = self.order.remove(0);
            self.slots.remove(&oldest);
        }
        let evicted: Vec<usize> = self.tickets.keys().copied().filter(|i| !self.slots.contains_key(i)).collect();
        self.cancel(&evicted);
        true
    }
}
