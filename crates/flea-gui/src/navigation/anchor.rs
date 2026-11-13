//! Re-reading the open listing without moving the user off it: cursor, marks and view by filename.
use super::history::{join, select_match};
use crate::pane::Pane;
use std::collections::BTreeSet;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Viewport {
    pub columns: usize,
    pub row_height: f32,
    pub origin: f32,
    pub scroll: f32,
    pub shown: Option<Rc<[usize]>>,
}

impl Viewport {
    pub fn offset(&self, index: usize) -> f32 {
        let view = self.shown.as_ref().map_or(index, |shown| {
            shown.iter().position(|&row| row == index).unwrap_or(0)
        });
        self.origin + (view / self.columns.max(1)) as f32 * self.row_height - self.scroll
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub index: usize,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub path: String,
    pub name: String,
    pub index: usize,
    pub start: usize,
    pub offset: f32,
    pub(crate) grid_offset: Option<f32>,
    pub select: bool,
    pub want_changed: bool,
    pub marks: Vec<Mark>,
    pub kept: BTreeSet<usize>,
    pub had_marks: bool,
    pub lone: bool,
    pub need_paths: Option<Vec<usize>>,
    pub locate: Option<u64>,
    pub locate_done: bool,
    pub landed: bool,
}

// The listing row holding a name in the window the pane holds now.
pub fn index_of(pane: &Pane, name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    pane.held
        .rows
        .iter()
        .position(|row| row.name == name)
        .map(|at| pane.held.index_at(at))
}

pub fn cursor_name(pane: &Pane) -> String {
    pane.held
        .row(pane.cursor)
        .map(|row| row.name.clone())
        .unwrap_or_default()
}

impl Anchor {
    // Marks carry by file identity: each marked row's name is recorded and looked up again on landing.
    pub fn capture(pane: &Pane, selected: &[usize], select: bool, want_changed: bool) -> Self {
        let marks: Vec<Mark> = selected
            .iter()
            .map(|&index| Mark {
                index,
                name: pane.held.row(index).map(|row| row.name.clone()),
            })
            .collect();
        Self {
            path: pane.path().into(),
            name: cursor_name(pane),
            index: pane.cursor,
            start: pane.held.start,
            offset: pane.cursor as f32 * pane.row_height() - pane.scroll_y,
            grid_offset: None,
            select,
            want_changed,
            had_marks: !marks.is_empty(),
            lone: selected.len() == 1,
            marks,
            kept: BTreeSet::new(),
            need_paths: None,
            locate: None,
            locate_done: false,
            landed: false,
        }
    }

    pub fn unnamed(&self) -> Vec<usize> {
        self.marks
            .iter()
            .filter(|mark| mark.name.is_none())
            .map(|mark| mark.index)
            .collect()
    }

    // The batched paths answer fills the names outside the held window before the re-list.
    pub fn fill_paths(&mut self, paths: &[String]) {
        let need = self.need_paths.take().unwrap_or_default();
        for (index, path) in need.iter().zip(paths) {
            let name = path.rsplit('/').next().unwrap_or(path).to_owned();
            for mark in self.marks.iter_mut().filter(|mark| mark.index == *index) {
                mark.name = Some(name.clone());
            }
        }
    }

    pub fn target(&self) -> Option<String> {
        (!self.name.is_empty()).then(|| join(&self.path, &self.name))
    }

    fn sweep(&mut self, pane: &Pane) {
        let mut pending = Vec::new();
        for mark in std::mem::take(&mut self.marks) {
            match mark.name.as_deref().map(|name| index_of(pane, name)) {
                Some(Some(at)) => {
                    self.kept.insert(at);
                }
                Some(None) => pending.push(mark),
                None => {}
            }
        }
        self.marks = pending;
    }

    pub fn land(&mut self, pane: &mut Pane, index: usize) {
        pane.cursor = index.min(pane.held.total.saturating_sub(1));
        let y = pane.cursor as f32 * pane.row_height() - self.offset;
        pane.scroll_to(y);
        pane.requests.initial_first = Some((pane.scroll_y / pane.row_height()).floor() as usize);
        self.landed = true;
    }

    // The one take: marks land once, on the same files; a gone file stays gone.
    pub fn finish(&mut self, selected: &mut Vec<usize>) {
        self.marks.clear();
        if !self.had_marks {
            return;
        }
        selected.clear();
        selected.extend(std::mem::take(&mut self.kept));
        // Anchor.js finishAnchor: a lone mark comes back lone, so a plain move still carries it.
        self.lone = self.lone && selected.len() == 1;
    }

    // Runs on the landing; returns the locate batch still owed, or None when the anchor resolved.
    pub fn apply(&mut self, pane: &mut Pane, selected: &mut Vec<usize>) -> Option<Vec<String>> {
        self.sweep(pane);
        let at = index_of(pane, &self.name);
        let total = pane.held.total;
        if !self.marks.is_empty() && self.locate.is_none() {
            let paths: Vec<String> = self
                .marks
                .iter()
                .filter_map(|mark| mark.name.as_deref())
                .map(|name| join(&self.path, name))
                .collect();
            if total > 0 {
                self.land(pane, at.unwrap_or(self.index));
            }
            return Some(paths);
        }
        if total > 0 {
            self.land(pane, at.unwrap_or(self.index));
        }
        if self.select && total > 0 {
            selected.clear();
            selected.push(pane.cursor);
        }
        self.finish(selected);
        None
    }

    // The batched locate answer: rows outside the held window join kept, the gone stay gone.
    pub fn located(&mut self, matches: &[(String, usize)], selected: &mut Vec<usize>) {
        self.locate_done = true;
        for mark in &self.marks {
            let Some(name) = &mark.name else { continue };
            let path = join(&self.path, name);
            if let Some((_, index)) = matches.iter().find(|(own, _)| *own == path) {
                self.kept.insert(*index);
            }
        }
        self.finish(selected);
    }
}

// A preference re-list keeps cursor and marks by name; a mark outside the window clears them whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Preference {
    pub path: String,
    pub name: String,
    pub index: usize,
    pub start: usize,
    pub selected: Vec<String>,
}

impl Preference {
    pub fn capture(pane: &Pane, selected: &[usize]) -> Self {
        let names: Option<Vec<String>> = selected
            .iter()
            .map(|&index| pane.held.row(index).map(|row| row.name.clone()))
            .collect();
        Self {
            path: pane.path().into(),
            name: cursor_name(pane),
            index: pane.cursor,
            start: pane.held.start,
            selected: names.unwrap_or_default(),
        }
    }

    pub fn apply(&self, pane: &mut Pane, selected: &mut Vec<usize>) {
        let total = pane.held.total;
        let cursor =
            index_of(pane, &self.name).or_else(|| (total > 0).then(|| self.index.min(total - 1)));
        let marks: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|name| index_of(pane, name))
            .collect();
        if let Some(cursor) = cursor {
            pane.cursor = cursor;
            pane.reveal_cursor();
        }
        selected.clear();
        selected.extend(marks);
        selected.sort_unstable();
        selected.dedup();
    }
}

// The deep target a listing names: a match takes the cursor and a lone selection.
pub fn apply_select(pane: &mut Pane, target: &str, selected: &mut Vec<usize>) -> bool {
    let path = pane.path().to_owned();
    let names = pane.held.rows.iter().map(|row| row.name.as_str());
    let Some(at) = select_match(names, target, &path) else {
        return false;
    };
    pane.cursor = pane.held.index_at(at);
    // Nav.js applyPendingSelect lands through setCursor, which keeps ScrollOff's three rows of context (EY-363).
    pane.reveal_cursor_keeping(3);
    pane.requests.initial_first = Some((pane.scroll_y / pane.row_height()).floor() as usize);
    selected.clear();
    selected.push(pane.cursor);
    true
}
