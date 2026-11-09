//! The rest list's group headings over the keyboard's lists: the held rows and the search's results.
use super::super::{fit, SheetView, HEAD_SLOT, KEY_COL, KEY_ROW, PAD};
use super::keyboard_width;
use crate::g2::settings_pages::{marks::spaced, seams::Ink};
use crate::list::Typeface;
use flea::keys::SHEET_GROUPS;
use flea::ui_logic_settings_pages::caps;
use gpui::{div, prelude::*, px, Div, SharedString, Stateful, Window};

/// The heading key for a rest-list group, or for the keys outside every group.
pub(in crate::g2::settings_pages::sheet_view) fn group_key(group: Option<usize>) -> &'static str {
    group.map_or("keys.group.other", |group| SHEET_GROUPS[group].label_key)
}

/// A muted line where a list has nothing to show, under where its first heading would sit.
pub(in crate::g2::settings_pages::sheet_view) fn empty(text: String, ink: &Ink) -> Div {
    let line = div()
        .pt(px(HEAD_SLOT))
        .h(px(HEAD_SLOT + KEY_ROW))
        .flex()
        .items_center();
    line.text_color(ink.muted).child(SharedString::from(text))
}

impl SheetView {
    /// A group's heading over its rows, the same in the rest list, the held list and the search.
    pub(in crate::g2::settings_pages::sheet_view) fn heading(
        &self,
        label_key: &str,
        ink: &Ink,
    ) -> Div {
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
    pub(in crate::g2::settings_pages::sheet_view) fn with_heads(
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

    /// The key column this frame draws: the board's, measured as the rest list spells its keys. Over a keyboard the
    /// rest list's two columns keep it to half again the board's; search results run the keyboard's whole width, so a
    /// result with several chords gets up to half of it. Longer keys are clipped.
    pub(in crate::g2::settings_pages::sheet_view) fn key_column(&self, window: &Window) -> f32 {
        let face = &self.typeface;
        let Some(unit) = self.unit(window) else {
            return fit::key_column(&self.sheet, face, window);
        };
        let widest = |keys: &mut dyn Iterator<Item = &str>| {
            keys.map(|keys| drawn_chords(keys, face, window))
                .fold(KEY_COL, f32::max)
        };
        let sheet = &self.sheet;
        if !self.searching() {
            let groups = sheet.groups();
            let mut rest = groups
                .iter()
                .flat_map(|(_, _, rows)| rows)
                .map(|row| sheet.cap(&row.keys));
            return widest(&mut rest).min(KEY_COL * 1.5 * fit::text_scale(face));
        }
        let mut results = (0..sheet.ranked.len()).map(|at| sheet.result_keys(at));
        widest(&mut results).min(keyboard_width(unit) / 2.)
    }

    /// One alternative of a row's keys: over a keyboard its chord with a muted plus, as the held list draws it.
    pub(in crate::g2::settings_pages::sheet_view) fn key_part(&self, key: &str, ink: &Ink) -> Div {
        if self.sheet.keyboard.is_some() {
            return super::held::chord_line(key, ink);
        }
        div().child(SharedString::from(key.to_owned()))
    }
}

// Sample keys: "Y / CTRL + C"
// Over a keyboard SheetView::keys lays out each key, each " + " and each " / " as its own element (held::chord_line),
// so their rounded widths add up.
fn drawn_chords(keys: &str, face: &Typeface, window: &Window) -> f32 {
    let width = |text: &str| fit::shaped(text, face, window).ceil();
    let (plus, slash) = (width(" + "), width(" / "));
    keys.split(" / ")
        .enumerate()
        .map(|(index, chord)| {
            let joined = if index > 0 { slash } else { 0. };
            chord
                .split(" + ")
                .enumerate()
                .map(|(at, key)| width(key) + if at > 0 { plus } else { 0. })
                .sum::<f32>()
                + joined
        })
        .sum()
}
