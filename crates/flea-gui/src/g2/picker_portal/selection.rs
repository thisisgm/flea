//! Path identities are validated by the check child; ranges resolve names on the listing child.
use super::Picker;
use flea::jsondoc::Json;
use flea::ui_logic_picker_portal::{self as logic, Check};
use gpui::{Context, Task};
use std::{
    collections::{HashSet, VecDeque},
    rc::Rc,
    time::Duration,
};

const PATHS_PENDING: u64 = u64::MAX;

struct Paths {
    base: Vec<String>,
    prune: bool,
}

#[derive(Default)]
pub struct Selection {
    anchor: Option<usize>,
    last: Option<usize>,
    base: Vec<String>,
    requests: VecDeque<Option<Paths>>,
    queued: VecDeque<(Vec<usize>, Paths)>,
    pub(super) dirty: bool,
    bytes: u64,
    deadline: Option<Task<()>>,
}

impl Picker {
    pub fn marks_allowed(&self) -> bool {
        self.req.multiple && !logic::saving(&self.req)
    }

    pub(super) fn selection_permitted(&self) -> bool {
        self.marks_allowed()
            && !self.backend_unavailable()
            && !self.answered()
            && !self.listing.failed
            && self.listing.state != "loading"
            && !self.accept_state().submitting()
    }

    fn selection_available(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.selection_permitted() {
            return false;
        }
        if self.mark_request > 0 {
            self.say_key("picker.portal.selection_busy", false, cx);
            return false;
        }
        true
    }

    #[cfg(test)]
    pub(super) fn range_anchor(&self) -> Option<usize> {
        self.selection.anchor
    }

    pub fn end_range(&mut self) {
        self.selection.anchor = None;
        self.selection.last = None;
        self.selection.base.clear();
    }

    pub fn toggle_mark(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.selection_available(cx) {
            return;
        }
        self.end_range();
        let held = self.browse.listing.read(cx).listing().held();
        let Some(row) = index
            .checked_sub(held.start)
            .and_then(|at| held.rows.get(at))
        else {
            return;
        };
        if flea::ui_logic_picker_listing::directory(row.dir, row.mode, &row.icon)
            != logic::folder_mode(&self.req)
        {
            return;
        }
        let path = logic::row_path(&self.listing.location, &row.name);
        self.accept_marks = false;
        self.mark_request = self.check(&Check::Mark {
            path: &path,
            directory: logic::folder_mode(&self.req),
            multiple: true,
        });
        cx.notify();
    }

    pub fn extend_to(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.selection_permitted() {
            return;
        }
        let listing = self.browse.listing.read(cx);
        let from = self.listing.cursor;
        let shown = listing.shown();
        let view_of = |row| {
            shown.map_or(row, |rows| {
                rows.iter().position(|at| *at == row).unwrap_or(0)
            })
        };
        let (from, to) = (view_of(from), view_of(index));
        if self.selection.anchor.is_none() || self.selection.last != Some(from) {
            self.selection.anchor = Some(from);
            self.selection.base = self.marks.iter().map(|mark| mark.path.clone()).collect();
        }
        self.selection.last = Some(to);
        let anchor = self.selection.anchor.unwrap_or(from);
        let rows: Vec<usize> = (anchor.min(to)..=anchor.max(to))
            .filter_map(|at| shown.map_or(Some(at), |rows| rows.get(at).copied()))
            .collect();
        let base = self.selection.base.clone();
        self.resolve_selection(rows, base, cx);
        self.move_cursor(index, cx);
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        if !self.selection_permitted() {
            return;
        }
        self.end_range();
        let listing = self.browse.listing.read(cx);
        let rows = listing.shown().map_or_else(
            || (0..listing.listing().total()).collect(),
            <[usize]>::to_vec,
        );
        let base = self.marks.iter().map(|mark| mark.path.clone()).collect();
        self.resolve_selection(rows, base, cx);
    }

    pub(super) fn select_indices(&mut self, rows: Vec<usize>, cx: &mut Context<Self>) {
        if !self.selection_permitted() {
            return;
        }
        let base = self.marks.iter().map(|mark| mark.path.clone()).collect();
        self.resolve_selection(rows, base, cx);
    }

    fn resolve_selection(&mut self, rows: Vec<usize>, base: Vec<String>, cx: &mut Context<Self>) {
        self.resolve_paths(rows, Paths { base, prune: false }, cx);
    }

    fn resolve_paths(&mut self, rows: Vec<usize>, pending: Paths, cx: &mut Context<Self>) {
        if self.mark_request > 0 {
            self.selection.queued.push_back((rows, pending));
            return;
        }
        let rows = rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let line = format!(r#"{{"c":"paths","rows":[{rows}]}}"#);
        if !self.send_listing(&line) {
            return;
        }
        self.selection.requests.push_back(Some(pending));
        self.mark_request = PATHS_PENDING;
        self.selection.deadline = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |picker, cx| {
                if picker.mark_request == PATHS_PENDING {
                    if let Some(pending) = picker.selection.requests.back_mut() {
                        *pending = None;
                    }
                    picker.mark_request = 0;
                    picker.end_range();
                    picker.say_key("picker.browser.selection_timeout", true, cx);
                }
            });
        }));
    }

    pub fn selection_paths(&mut self, document: &Json, cx: &mut Context<Self>) {
        let Some(Some(pending)) = self.selection.requests.pop_front() else {
            return;
        };
        if self.mark_request != PATHS_PENDING {
            return;
        }
        self.selection.deadline = None;
        let Some(items) = document.get("paths").and_then(Json::as_array) else {
            self.mark_request = 0;
            self.end_range();
            return;
        };
        let Some(returned) = items
            .iter()
            .map(|item| {
                item.as_str()
                    .filter(|path| path.starts_with('/'))
                    .map(str::to_owned)
            })
            .collect::<Option<Vec<_>>>()
        else {
            self.mark_request = 0;
            self.end_range();
            self.selection.queued.clear();
            self.say_key("picker.browser.selection_timeout", true, cx);
            return;
        };
        let mut paths = pending.base;
        if pending.prune {
            let matched: HashSet<_> = returned.iter().collect();
            paths.retain(|path| {
                std::path::Path::new(path).parent()
                    != Some(std::path::Path::new(&self.listing.location))
                    || matched.contains(path)
            });
        } else {
            paths.extend(returned);
        }
        let mut seen = HashSet::new();
        paths.retain(|path| path.starts_with('/') && seen.insert(path.clone()));
        self.accept_marks = false;
        self.mark_request = self.check(&Check::Select {
            paths: &paths,
            directory: logic::folder_mode(&self.req),
        });
        cx.notify();
    }

    pub fn mark_changed(&mut self, cx: &mut Context<Self>) {
        self.selection.bytes = self
            .marks
            .iter()
            .fold(0_u64, |total, mark| total.saturating_add(mark.bytes));
        let paths = Rc::new(self.marks.iter().map(|mark| mark.path.clone()).collect());
        let allowed = self.marks_allowed();
        self.browse.listing.update(cx, |listing, cx| {
            listing.set_marks(paths, allowed);
            cx.notify();
        });
        cx.notify();
    }

    pub fn selection_status(&self) -> String {
        flea::ui_logic_picker_listing::status_line(
            &self.locale.catalog,
            self.marks.len(),
            self.selection.bytes,
        )
    }

    pub(super) fn selection_reset(&mut self) {
        self.end_range();
        if self.mark_request == PATHS_PENDING {
            self.mark_request = 0;
        }
        for request in &mut self.selection.requests {
            *request = None;
        }
        self.selection.deadline = None;
        self.selection.queued.clear();
    }

    pub(super) fn selection_failed(&mut self) {
        self.selection.queued.clear();
    }

    pub(super) fn selection_feedback(
        &mut self,
        result: &logic::CheckResult,
        cx: &mut Context<Self>,
    ) {
        if result.removed > 0 {
            let text = self
                .locale
                .catalog
                .trn("picker.browser.selection_changed", result.removed as i64);
            self.say(text, true, cx);
        } else if !result.skipped_items.is_empty() {
            let skipped: Vec<(&str, &str)> = result
                .skipped_items
                .iter()
                .map(|(path, why)| (path.as_str(), why.as_str()))
                .collect();
            let text = flea::ui_logic_picker_listing::select::skipped_note(
                &self.locale.catalog,
                self.marks.len(),
                self.marks.len() + skipped.len(),
                &skipped,
            );
            self.say(text, true, cx);
        }
    }

    pub(super) fn selection_settled(&mut self, cx: &mut Context<Self>) {
        if self.answered() || self.mark_request > 0 {
            return;
        }
        if let Some((rows, pending)) = self.selection.queued.pop_front() {
            self.resolve_paths(rows, pending, cx);
        } else if std::mem::take(&mut self.selection.dirty) {
            self.validate_marks(false);
        }
    }

    pub(super) fn prune_filtered_marks(&mut self, cx: &mut Context<Self>) {
        if !self.selection_permitted() || self.marks.is_empty() {
            return;
        }
        let Some(rows) = self.browse.listing.read(cx).shown().map(<[usize]>::to_vec) else {
            return;
        };
        let base = self.marks.iter().map(|mark| mark.path.clone()).collect();
        self.resolve_paths(rows, Paths { base, prune: true }, cx);
    }
}
