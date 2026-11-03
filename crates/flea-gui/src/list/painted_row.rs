//! Retained row text and clipboard geometry shared by the list painter and readers.
use super::{row_text::run_of, Metrics, RowState};
use crate::{
    grid::element::Thumbnail,
    row::{cache::Prepared, marks::ClipMark},
};
use gpui::{px, Bounds, Font, GlyphRun, Pixels, SharedString, Window};
use std::{rc::Rc, sync::Arc};

impl RowState {
    /// A cursor bolds its match, and a marked filter row carries no match wash.
    pub fn washes_run(self, searching: bool) -> bool {
        !self.cursor && (searching || !self.selected)
    }
}

/// The drag owner's caption for a drop target row, shaped once per label, face and size.
pub struct DropLine {
    label: SharedString,
    font: Font,
    size: Pixels,
    pub(crate) run: GlyphRun,
}

impl DropLine {
    pub(crate) fn shaped<'a>(
        slot: &'a mut Option<DropLine>,
        label: &SharedString,
        font: &Font,
        size: Pixels,
        window: &Window,
    ) -> &'a GlyphRun {
        if !slot
            .as_ref()
            .is_some_and(|line| line.label == *label && line.font == *font && line.size == size)
        {
            let run = GlyphRun::new(window.text_system().layout_line(
                label,
                size,
                &[run_of(label.len(), font)],
                None,
            ));
            *slot = Some(DropLine {
                label: label.clone(),
                font: font.clone(),
                size,
                run,
            });
        }
        &slot.as_ref().expect("shaped above").run
    }
}

/// One visible row this frame: its index, its shaped lines once held, its clipboard mark and its states.
#[derive(Clone)]
pub struct Shown {
    pub index: usize,
    /// The position the row is drawn at: its index, or its place in a filter's drawn rows.
    pub view: usize,
    pub prepared: Option<Rc<Prepared>>,
    pub clip: Option<ClipMark>,
    pub state: RowState,
    /// A cursor match is bold unless this is a marked row under a filter.
    pub bold_match: bool,
    /// The thumbnail drawn in the mark slot, lent while the row's facts match the answer.
    pub picture: Option<Arc<Thumbnail>>,
}

impl Shown {
    pub(super) fn name_line(&self) -> Option<&GlyphRun> {
        let prepared = self.prepared.as_ref()?;
        Some(
            prepared
                .bold
                .get()
                .filter(|_| self.bold_match)
                .unwrap_or(&prepared.name),
        )
    }

    pub(super) fn name_slot(&self, mut name: Bounds<Pixels>, metrics: &Metrics) -> Bounds<Pixels> {
        if self.clip.is_some() {
            name.size.width = (name.size.width - px(metrics.gap + metrics.clip)).max(px(0.));
        }
        name
    }

    /// The clipboard bounds used by paint: one gap after the drawn name, as v0.3.12's Row.qml places it (EY-349).
    pub fn clip_bounds(&self, name: Bounds<Pixels>, metrics: &Metrics) -> Option<Bounds<Pixels>> {
        if self.state.renaming {
            return None;
        }
        let drawn = self.name_line()?.line().width;
        let mut slot = self.name_slot(name, metrics);
        slot.size.width = slot.size.width.min(drawn);
        Some(self.clip?.bounds(slot, px(metrics.clip), px(metrics.gap)))
    }
}

#[cfg(test)]
#[path = "painted_row_tests.rs"]
mod tests;
