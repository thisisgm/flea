// The thumbnail map a listing keeps: only viewport rows are asked, each once, and a bounded order evicts the oldest.
use super::ask::{cache_only, ClassSwitches};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Asked,
    CacheAsked,
    CacheMissed,
    // An empty file is a refusal: the thumbnailer answered with nothing.
    File(String),
}

impl Entry {
    // Asked and not answered yet; only these have a job to cancel.
    pub fn pending(&self) -> bool { matches!(self, Entry::Asked | Entry::CacheAsked) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    Off,
    Images,
    #[default]
    Media,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub name: String,
    pub thumbnailable: bool,
    pub icon: String,
    pub appimage: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    pub file: HashMap<usize, Entry>,
    pub order: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub ask: Vec<usize>,
    pub drop: Vec<usize>,
    pub cache_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub first: usize,
    pub last: i64,
}

// src/backend/thumbs.rs THUMB_SIZE: the longest side of a cache file made from a larger original.
pub const CACHE_SIZE: f64 = 256.0;

// The rows a request may name: the viewport in row indices, clamped to the listing's last row.
pub fn viewport(content_y: f64, row_height: f64, visible_rows: f64, total: usize) -> Viewport {
    let first = (content_y / row_height).floor().max(0.0) as usize;
    let bottom = content_y + visible_rows * row_height;
    let last = ((bottom / row_height).ceil() as i64 - 1).min(total as i64 - 1);
    Viewport { first, last }
}

// The backend's icon tells image from video thumbnails; an AppImage's own icon is its identity.
pub fn allowed(row: Option<&Row>, mode: Mode) -> bool {
    let Some(row) = row else { return false };
    if !row.thumbnailable || mode == Mode::Off {
        return false;
    }
    mode != Mode::Images || row.icon == "image-x-generic" || row.appimage
}

fn row_at(rows: &[Row], held: usize, at: usize) -> Option<&Row> { at.checked_sub(held).and_then(|i| rows.get(i)) }

impl Map {
    // Only visible rows, only rows the backend called thumbnailable, only rows never asked.
    pub fn plan(&self, rows: &[Row], held: usize, first: usize, last: i64, mode: Mode) -> Plan {
        let mut plan = Plan::default();
        let mut at = first as i64;
        while at <= last {
            let index = at as usize;
            if !self.file.contains_key(&index) && allowed(row_at(rows, held, index), mode) {
                plan.ask.push(index);
            }
            at += 1;
        }
        let mut pending: Vec<usize> = self.file.iter().filter(|(_, e)| e.pending()).map(|(k, _)| *k).collect();
        pending.sort_unstable();
        for at in pending {
            if (at as i64) < first as i64 || at as i64 > last || !allowed(row_at(rows, held, at), mode) {
                plan.drop.push(at);
            }
        }
        plan
    }

    // A cancelled row is forgotten here too, because the backend forgets it when it drops the job.
    pub fn forget(&mut self, row: usize) {
        self.file.remove(&row);
        self.order.retain(|r| *r != row);
    }

    // A miss on a generating class returns to unasked; on a cache-only class it stays a miss.
    pub fn miss(&mut self, row: usize, generating: bool, cap: usize) {
        if generating {
            self.forget(row);
        } else {
            self.remember(row, "", cap);
        }
    }

    pub fn applied(&mut self, work: &Plan) {
        for row in &work.drop {
            self.forget(*row);
        }
        let mark = if work.cache_only { Entry::CacheAsked } else { Entry::Asked };
        for row in &work.ask {
            self.file.insert(*row, mark.clone());
            self.order.push(*row);
        }
    }

    // The cap bounds a policy bug, not normal use: only the rows a viewport held are ever recorded.
    pub fn remember(&mut self, row: usize, file: &str, cap: usize) {
        let previous = self.file.get(&row).cloned();
        if previous.is_none() {
            self.order.push(row);
        }
        let entry = if file.is_empty() && previous == Some(Entry::CacheAsked) {
            Entry::CacheMissed
        } else {
            Entry::File(file.into())
        };
        self.file.insert(row, entry);
        while self.order.len() > cap {
            let oldest = self.order.remove(0);
            self.file.remove(&oldest);
        }
    }

    pub fn file_for(&self, row: usize) -> &str {
        match self.file.get(&row) {
            Some(Entry::File(file)) => file,
            _ => "",
        }
    }

    // Answered with nothing, which is not the same as not answered yet.
    pub fn refused(&self, row: usize) -> bool { matches!(self.file.get(&row), Some(Entry::File(f)) if f.is_empty()) }

    // A live answer re-lists rows: each finished thumbnail follows its row, by name, to its new index.
    pub fn carried(&self, old_rows: &[Row], old_held: usize, new_rows: &[Row], new_held: usize) -> Map {
        let at: HashMap<&str, usize> =
            new_rows.iter().enumerate().filter(|(_, r)| r.thumbnailable).map(|(j, r)| (r.name.as_str(), new_held + j)).collect();
        let mut next = Map::default();
        for index in &self.order {
            let (Some(Entry::File(file)), Some(row)) = (self.file.get(index), row_at(old_rows, old_held, *index)) else {
                continue;
            };
            if file.is_empty() {
                continue;
            }
            if let Some(&target) = at.get(row.name.as_str()) {
                next.file.insert(target, Entry::File(file.clone()));
                next.order.push(target);
            }
        }
        next
    }

    // A class switched on decodes again: only its cache-only marks leave the map.
    pub fn forget_misses(&mut self) {
        let file = &mut self.file;
        self.order.retain(|at| match file.get(at) {
            Some(Entry::CacheMissed) | Some(Entry::CacheAsked) => {
                file.remove(at);
                false
            }
            _ => true,
        });
    }

    // The thumbed line's record: a cache-only miss on a generating class returns to unasked.
    pub fn thumb_reply(&mut self, row: usize, file: &str, cap: usize, known: bool, storage_class: &str, switches: &ClassSwitches) -> bool {
        if file.is_empty() && self.file.get(&row) == Some(&Entry::CacheAsked) && (!known || !cache_only(storage_class, switches)) {
            self.miss(row, true, cap);
            return true;
        }
        self.remember(row, file, cap);
        false
    }
}

// How much a picture may be enlarged to fit its box, and never past limit.
pub fn fit_scale(box_w: f64, box_h: f64, width: f64, height: f64, limit: f64) -> f64 {
    limit.min(box_w / width.max(1.0)).min(box_h / height.max(1.0))
}

// A cache file stands in for its original, so it is enlarged at most to the original's own size.
pub fn thumb_limit(thumb_w: f64, thumb_h: f64, original_w: f64, original_h: f64) -> f64 {
    let thumb_longest = thumb_w.max(thumb_h).max(1.0);
    let original_longest = original_w.max(original_h).max(0.0);
    if original_longest > 0.0 {
        return original_longest / thumb_longest;
    }
    if thumb_longest < CACHE_SIZE {
        1.0
    } else {
        f64::INFINITY
    }
}

// A video poster and a vector fill the frame; a raster image keeps the never-enlarge cap (v0.3.12).
pub fn preview_limit(is_video: bool, vector: bool, thumb_w: f64, thumb_h: f64, original_w: f64, original_h: f64) -> f64 {
    if is_video || vector {
        return f64::INFINITY;
    }
    thumb_limit(thumb_w, thumb_h, original_w, original_h)
}
