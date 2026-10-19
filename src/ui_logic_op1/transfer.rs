// The transfer the card and the status line track, folded from the wire one sample at a time.
use super::{count, duration, n, parts, size, Part};
use crate::i18n::I18n;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transfer {
    pub id: u64,
    pub moving: bool,
    pub n: u64,
    pub index: u64,
    pub name: String,
    pub running: bool,
    pub extract: bool,
    pub redo: String,
    pub done: u64,
    pub bytes: u64,
    pub total: u64,
    pub moved: u64,
    pub scanned: u64,
    pub writing: bool,
    pub drive: String,
    pub no_cancel: bool,
    pub failure_reported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub index: u64,
    pub name: String,
}

impl Transfer {
    // An extract is the one verb not derived from moving: it drives this same card and its Cancel.
    pub fn started(id: u64, moving: bool, n: u64, extract: bool) -> Self {
        Self {
            id,
            moving,
            n,
            running: true,
            extract,
            ..Self::default()
        }
    }

    // The byte sample for the item in flight; done stays where it was, a sample fills the item in.
    pub fn sampled(&self, index: u64, name: &str, bytes: u64, total: u64, scanned: u64) -> Self {
        let settled = if scanned > 0 { scanned } else { self.scanned };
        if index < self.done {
            return Self {
                moved: self.moved + bytes,
                scanned: settled,
                ..self.clone()
            };
        }
        Self {
            index,
            name: name.into(),
            done: index,
            bytes,
            total,
            scanned: settled,
            ..self.clone()
        }
    }

    // That item's own terminal line: it counts whole from here and its byte sample is spent.
    pub fn item_done(&self, index: u64, name: &str) -> Self {
        let sampled = if self.index == index {
            if self.total > 0 {
                self.total
            } else {
                self.bytes
            }
        } else {
            0
        };
        Self {
            index,
            name: name.into(),
            done: index + 1,
            bytes: 0,
            total: 0,
            moved: self.moved + sampled,
            ..self.clone()
        }
    }

    pub fn items_done(&self, held: &[Held]) -> Self {
        held.iter()
            .fold(self.clone(), |t, h| t.item_done(h.index, &h.name))
    }

    pub fn moved_bytes(&self) -> u64 {
        self.moved + self.bytes
    }

    // The last directory flush: every file is already complete, so the counted bytes stay.
    pub fn mark_writing(&self, drive: &str) -> Self {
        Self {
            writing: true,
            drive: drive.into(),
            name: String::new(),
            bytes: 0,
            total: 0,
            ..self.clone()
        }
    }

    // The flush dims Cancel because every file is already complete, so nothing is left to stop.
    pub fn cancel_enabled(&self) -> bool {
        !self.no_cancel && self.running && !self.writing
    }

    // The bar is the whole transfer: one item's total only when the transfer is that one item.
    pub fn transfer_total(&self) -> u64 {
        if self.n == 1 && self.total > 0 {
            self.total
        } else {
            self.scanned
        }
    }

    pub fn fraction(&self) -> f64 {
        if self.n == 0 {
            return 0.0;
        }
        let total = self.transfer_total();
        let at = if total > 0 {
            self.moved_bytes() as f64 / total as f64
        } else {
            let share = if self.total > 0 {
                self.bytes as f64 / self.total as f64
            } else {
                0.0
            };
            (self.done as f64 + share) / self.n as f64
        };
        unit(at)
    }

    // The card's headline, the count with no name in it; the status line adds the name.
    pub fn head(&self, catalog: &I18n) -> String {
        if self.writing {
            let drive = if self.drive.is_empty() {
                catalog.tr("jobs.transfer.the_drive")
            } else {
                self.drive.clone()
            };
            return catalog.tr_args("jobs.transfer.writing_to", &[&drive]);
        }
        if self.extract {
            return catalog.tr("jobs.transfer.extracting");
        }
        let (at, of) = (count(catalog, self.index + 1), count(catalog, self.n));
        if !self.redo.is_empty() {
            return catalog.tr_args("jobs.transfer.redoing", &[&at, &of, &self.redo]);
        }
        catalog.tr_args(
            if self.moving {
                "jobs.transfer.moving"
            } else {
                "jobs.transfer.copying"
            },
            &[&at, &of],
        )
    }

    // The item in flight and its size; a directory has no total, so it names itself alone.
    pub fn file_line(&self, catalog: &I18n) -> String {
        if self.writing || self.name.is_empty() {
            return String::new();
        }
        if self.total > 0 {
            catalog.tr_args(
                "jobs.transfer.file",
                &[&self.name, &size(catalog, self.total)],
            )
        } else {
            self.name.clone()
        }
    }

    // The line under the bar as figures and words: moved, of total, rate, and time left when both exist.
    pub fn byte_parts(&self, catalog: &I18n, rate: u64) -> Vec<Part> {
        let moved = self.moved_bytes();
        if moved == 0 {
            return Vec::new();
        }
        let moved_text = size(catalog, moved);
        if self.writing {
            let key = if self.moving {
                "jobs.bytes.moved_waiting"
            } else {
                "jobs.bytes.copied_waiting"
            };
            return parts(&catalog.tr(key), &[&moved_text]);
        }
        let total = self.transfer_total();
        let rate_text = catalog.tr_args("jobs.bytes.rate", &[&size(catalog, rate)]);
        if total > 0 && rate > 0 {
            let left = duration(total.saturating_sub(moved) / rate);
            return parts(
                &catalog.tr("jobs.bytes.of_left"),
                &[&moved_text, &size(catalog, total), &rate_text, &left],
            );
        }
        if total > 0 {
            return parts(
                &catalog.tr("jobs.bytes.of"),
                &[&moved_text, &size(catalog, total), &rate_text],
            );
        }
        let key = if self.moving {
            "jobs.bytes.moved"
        } else {
            "jobs.bytes.copied"
        };
        parts(&catalog.tr(key), &[&moved_text, &rate_text])
    }
}

// A bar figure held to 0 through 1, where a missing or non-finite one reads as empty.
pub fn unit(value: f64) -> f64 {
    if value.is_nan() || value <= 0.0 {
        0.0
    } else {
        value.min(1.0)
    }
}

pub fn grouped(catalog: &I18n, value: u64) -> String {
    catalog.trn("jobs.items", n(value))
}

// The held-items beat: ok item lines wait up to BEAT_MS so a burst of small files repaints once.
pub const BEAT_MS: u64 = 100;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wire {
    pub transfer: Transfer,
    pub held: Vec<Held>,
    pub held_since: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fold {
    Nothing,
    Drawn,
    // The first named failure owns the status slot until acknowledged; later ones stay in the summary.
    Failed { name: String, error: String },
    // The finished transfer, and whether its line goes out as an error.
    Done { transfer: Transfer, error: bool },
}

impl Wire {
    pub fn started(&mut self, id: u64, moving: bool, count: u64, extract: bool) {
        self.held.clear();
        self.held_since = None;
        self.transfer = Transfer::started(id, moving, count, extract);
    }

    // Held lines land before the line after them, so the card never steps backwards.
    pub fn flush(&mut self) -> bool {
        self.held_since = None;
        if self.held.is_empty() {
            return false;
        }
        self.transfer = self.transfer.items_done(&self.held);
        self.held.clear();
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn progress(
        &mut self,
        id: u64,
        index: u64,
        name: &str,
        bytes: u64,
        total: u64,
        scanned: u64,
        phase: &str,
        drive: &str,
    ) -> Fold {
        if id != self.transfer.id {
            return Fold::Nothing;
        }
        self.flush();
        self.transfer = if phase == "writing" {
            self.transfer.mark_writing(drive)
        } else {
            self.transfer.sampled(index, name, bytes, total, scanned)
        };
        Fold::Drawn
    }

    // An ok line is held for the beat; a failure flushes at once, and only the first one names itself.
    pub fn item(
        &mut self,
        id: u64,
        index: u64,
        name: &str,
        ok: bool,
        error: &str,
        now: u64,
    ) -> Fold {
        if id != self.transfer.id {
            return Fold::Nothing;
        }
        if ok {
            self.held.push(Held {
                index,
                name: name.into(),
            });
            self.held_since.get_or_insert(now);
            return Fold::Nothing;
        }
        self.flush();
        let first = error != "cancelled" && !self.transfer.failure_reported;
        self.transfer = Transfer {
            failure_reported: self.transfer.failure_reported || first,
            ..self.transfer.item_done(index, name)
        };
        if first {
            Fold::Failed {
                name: name.into(),
                error: error.into(),
            }
        } else {
            Fold::Drawn
        }
    }

    // The beat's own tick: the held lines land once BEAT_MS has passed since the first of them.
    pub fn tick(&mut self, now: u64) -> bool {
        match self.held_since {
            Some(since) if now.saturating_sub(since) >= BEAT_MS => self.flush(),
            _ => false,
        }
    }

    // A failure nobody has named yet goes out as an error; the card is spent either way.
    pub fn done(&mut self, id: u64, failed: u64) -> Fold {
        if id != self.transfer.id {
            return Fold::Nothing;
        }
        self.flush();
        let error = failed > 1 || failed > 0 && !self.transfer.failure_reported;
        let transfer = std::mem::take(&mut self.transfer);
        Fold::Done { transfer, error }
    }
}
