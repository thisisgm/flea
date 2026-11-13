//! Selection indices retain the QML lone-mark and additive range contracts without reading file metadata.
use std::rc::Rc;

#[derive(Default)]
pub struct Selection {
    pub snapshot: Option<Rc<[usize]>>,
    lone: bool,
    /// Set by a click on blank ground; the next plain move selects its row (FINDER-BEHAVIOR, v0.3.12).
    ground: bool,
    anchor: usize,
    shift: Option<(usize, usize, Vec<usize>)>,
}

impl Selection {
    pub fn sync(&mut self, rows: &[usize]) {
        if self.snapshot.as_deref().unwrap_or(&[]) != rows {
            self.reset();
            self.changed(rows);
        }
    }
    pub fn changed(&mut self, rows: &[usize]) {
        self.ground &= rows.is_empty();
        if self.snapshot.as_deref().unwrap_or(&[]) == rows {
            return;
        }
        self.snapshot = (!rows.is_empty()).then(|| Rc::from(rows));
    }
    pub fn is_lone(&self) -> bool {
        self.lone
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn only(&mut self, rows: &mut Vec<usize>, row: usize) {
        rows.clear();
        rows.push(row);
        self.lone = true;
        self.anchor = row;
        self.shift = None;
        self.changed(rows);
    }
    pub fn toggle(&mut self, rows: &mut Vec<usize>, row: usize) {
        match rows.binary_search(&row) {
            Ok(at) => {
                rows.remove(at);
            }
            Err(at) => rows.insert(at, row),
        }
        self.lone = false;
        self.anchor = row;
        self.shift = None;
        self.changed(rows);
    }
    pub fn promote(&mut self, rows: &[usize], row: usize) -> bool {
        let promote = self.lone && rows == [row];
        if promote {
            self.lone = false;
            self.shift = None;
        }
        promote
    }
    /// A click on blank ground clears the marks, and the next plain move selects the row it lights.
    pub fn ground(&mut self, rows: &mut Vec<usize>) {
        rows.clear();
        self.reset();
        self.ground = true;
    }
    pub fn follow(&mut self, rows: &mut Vec<usize>, row: usize) {
        if (self.lone && rows.len() == 1) || (self.ground && rows.is_empty()) {
            self.only(rows, row);
        } else {
            self.shift = None;
        }
    }
    pub fn extend(
        &mut self,
        rows: &mut Vec<usize>,
        from: usize,
        to: usize,
        pulled: Option<usize>,
        shown: &super::positions::Rows,
    ) {
        if self.shift.as_ref().is_none_or(|(_, last, _)| *last != from) {
            self.anchor = pulled.unwrap_or(from);
            self.shift = Some((self.anchor, from, rows.clone()));
        }
        let (anchor, last, base) = self.shift.as_mut().expect("range began");
        rows.clone_from(base);
        let range = (*anchor).min(to)..=(*anchor).max(to);
        if let Some(list) = shown.as_deref() {
            if shown.ascending() {
                let first = list.partition_point(|row| *row < *range.start());
                let end = list.partition_point(|row| *row <= *range.end());
                rows.extend_from_slice(&list[first..end]);
            } else if let Some(end) = shown.view_of(to) {
                // A ranked range runs in drawn order; a hidden anchor stands before view 0, as the arrows treat it.
                let first = shown.view_of(*anchor).unwrap_or(0);
                rows.extend_from_slice(&list[first.min(end)..=first.max(end)]);
            }
        } else {
            rows.extend(range);
        }
        rows.sort_unstable();
        rows.dedup();
        *last = to;
        self.lone = false;
        self.changed(rows);
    }
    pub fn clear(&mut self, rows: &mut Vec<usize>) {
        rows.clear();
        self.reset();
    }
    pub fn all(
        &mut self,
        rows: &mut Vec<usize>,
        total: usize,
        shown: Option<&[usize]>,
        invert: bool,
    ) {
        let before = std::mem::take(rows);
        if let Some(shown) = shown {
            rows.extend(
                shown
                    .iter()
                    .copied()
                    .filter(|row| !invert || before.binary_search(row).is_err()),
            );
        } else {
            rows.extend((0..total).filter(|row| !invert || before.binary_search(row).is_err()));
        }
        rows.sort_unstable();
        rows.dedup();
        self.lone = false;
        self.shift = None;
        self.changed(rows);
    }
}
