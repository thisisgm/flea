//! The rest list's group headings over the keyboard's lists: the held rows and the search's results.
use super::super::{SheetView, HEAD_SLOT, KEY_COL, KEY_ROW, PAD};
use crate::g2::st1::{marks::spaced, seams::Ink};
use flea::keys::SHEET_GROUPS;
use flea::ui_logic_st1::caps;
use gpui::{div, prelude::*, px, Div, SharedString, Stateful};

/// The heading key for a rest-list group, or for the keys outside every group.
pub(in crate::g2::st1::sheet_view) fn group_key(group: Option<usize>) -> &'static str {
    group.map_or("keys.group.other", |group| SHEET_GROUPS[group].label_key)
}

/// A muted line where a list has nothing to show, under where its first heading would sit.
pub(in crate::g2::st1::sheet_view) fn empty(text: String, ink: &Ink) -> Div {
    let line = div()
        .pt(px(HEAD_SLOT))
        .h(px(HEAD_SLOT + KEY_ROW))
        .flex()
        .items_center();
    line.text_color(ink.muted).child(SharedString::from(text))
}

impl SheetView {
    /// A group's heading over its rows, the same in the rest list, the held list and the search.
    pub(in crate::g2::st1::sheet_view) fn heading(&self, label_key: &str, ink: &Ink) -> Div {
        let catalog = &self.sheet.catalog;
        let label = caps::heading(&catalog.tr(label_key), catalog.language());
        div()
            .h(px(HEAD_SLOT))
            .flex_none()
            .flex()
            .items_end()
            .pb(px(2.))
            .line_height(px(16.))
            .text_color(ink.muted)
            .child(spaced(&label, 1.))
    }

    /// The search's results; over a keyboard each group's first result sits under its heading, as the rest list reads.
    pub(in crate::g2::st1::sheet_view) fn with_heads(
        &self,
        rows: impl Iterator<Item = Stateful<Div>>,
        ink: &Ink,
    ) -> Div {
        let mut list = div().flex().flex_col();
        let mut count = 0;
        for (index, row) in rows.enumerate() {
            if let Some(group) = self.sheet.result_head(index) {
                list = list.child(self.heading(group_key(group), ink).px(px(PAD)));
            }
            list = list.child(row);
            count += 1;
        }
        // Over a keyboard a search that finds nothing says so, under where a heading would sit.
        let query = &self.sheet.query;
        if count == 0 && self.sheet.keyboard.is_some() && !query.is_empty() {
            let none = self
                .sheet
                .catalog
                .tr_args("search.nothing_matches", &[query]);
            list = list.child(empty(none, ink).px(px(PAD)));
        }
        list
    }

    /// The key column's width: the board's own, or over a keyboard as wide as its longest spelled keys, up to half
    /// again the board's, so a plugin action with many keys never pushes the labels out; longer keys are clipped.
    pub(in crate::g2::st1::sheet_view) fn key_col(&self) -> f32 {
        if self.sheet.keyboard.is_none() {
            return KEY_COL;
        }
        let advance = f32::from(self.typeface.caption) * 0.6;
        (self.sheet.longest as f32 * advance).clamp(KEY_COL, KEY_COL * 1.5)
    }

    /// One alternative of a row's keys: over a keyboard its chord with a muted plus, as the held list draws it.
    pub(in crate::g2::st1::sheet_view) fn key_part(&self, key: &str, ink: &Ink) -> Div {
        if self.sheet.keyboard.is_some() {
            return super::held::chord_line(key, ink);
        }
        div().child(SharedString::from(key.to_owned()))
    }
}
