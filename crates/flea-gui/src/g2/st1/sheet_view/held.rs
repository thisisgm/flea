//! While keys are held or a chord is looked up, the shortcuts that use them in two columns, under group headings.
use super::super::scroll::Measured;
use super::super::{SheetView, COL_GAP, HEAD_SLOT, KEY_ROW, PAD};
use crate::g2::st1::{geometry::GAP, seams::Ink, sheet::Hover};
use crate::locale_geometry::unmirrored;
use flea::ui_logic_st1::sheet_keyboard::{group_of, Bound, SheetKeyboard};
use gpui::{
    div, prelude::*, px, Context, Div, ElementId, FontWeight, SharedString, Stateful, Window,
};

/// The rows one column holds before the list splits into two.
const SPLIT_AFTER: usize = 8;

impl SheetView {
    /// A list row that lights its keys while the pointer is over it; without a drawn keyboard the row is as it was.
    pub(in crate::g2::st1::sheet_view) fn hovering(
        &self,
        line: Div,
        id: impl Into<ElementId>,
        hover: Hover,
        ink: &Ink,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let line = line.id(id);
        if !self.sheet.balanced {
            return line;
        }
        let lit = self.sheet.hover.as_ref() == Some(&hover);
        line.mx(px(-6.))
            .px(px(6.))
            .rounded(px(4.))
            .when(lit, |line| line.bg(ink.hover))
            .on_hover(cx.listener(move |view, over: &bool, _, cx| {
                if *over {
                    view.sheet.hover = Some(hover.clone());
                } else if view.sheet.hover.as_ref() == Some(&hover) {
                    view.sheet.hover = None;
                }
                cx.notify();
            }))
    }
}

/// A chord's keys joined by a muted plus, as the sheet's own list joins alternatives with a muted slash.
/// A chord reads modifiers first in every language, as a right-to-left catalog isolates a keycap left to right.
pub(in crate::g2::st1::sheet_view) fn chord_line(chord: &str, ink: &Ink) -> Div {
    let mut line = div().flex().whitespace_nowrap().text_color(ink.foreground);
    for (index, key) in chord.split(" + ").enumerate() {
        if index > 0 {
            line = line.child(div().text_color(ink.muted).child(" + "));
        }
        line = line.child(SharedString::from(key.to_owned()));
    }
    div().flex().child(unmirrored(line))
}

/// A binding as its row reads it: a pair such as vim's `yy` is pressed twice, so it shows its key twice.
fn spelled(keyboard: &SheetKeyboard, bound: &Bound) -> String {
    let chord = keyboard.chord(bound.mask, bound.cap);
    if bound.pair() {
        format!("{chord} {chord}")
    } else {
        chord
    }
}

/// One entry of the held list: a group's heading, a gap where a group runs on into the next column, or a row.
#[derive(Clone, Copy)]
enum Item {
    Head(Option<usize>),
    Gap,
    Row(usize),
}

/// How tall an entry stands in its column.
fn height(item: &Item) -> f32 {
    match item {
        Item::Head(_) | Item::Gap => HEAD_SLOT,
        Item::Row(_) => KEY_ROW,
    }
}

/// Where the second column starts. A short list stays in one column; else the break falls between groups where the
/// columns differ least, as the rest list splits, and a single group splits in half, its heading kept with its rows.
fn split(items: &[Item], rows: usize, room: f32) -> usize {
    let total: f32 = items.iter().map(height).sum();
    if rows <= SPLIT_AFTER && total <= room {
        return items.len();
    }
    let tallest = |at: usize| {
        let left: f32 = items[..at].iter().map(height).sum();
        left.max(total - left)
    };
    let between = (1..items.len())
        .filter(|at| matches!(items[*at], Item::Head(_)))
        .min_by(|a, b| tallest(*a).total_cmp(&tallest(*b)));
    if let Some(at) = between {
        return at;
    }
    // The group runs on under a blank heading slot in the second column, so each column takes half of both.
    let half = (total + HEAD_SLOT) / 2.;
    let mut left = 0.;
    let mut at = 0;
    while at < items.len() && (at < 2 || left + height(&items[at]) <= half) {
        left += height(&items[at]);
        at += 1;
    }
    // A heading never ends a column away from its rows.
    if matches!(items[at - 1], Item::Head(_)) {
        at -= 1;
    }
    at
}

/// Where a column running past the list's foot is cut: through the middle of its last row at least half in view, as
/// the rest list cuts its columns, so the fade lies over that row and a heading never shows without its rows.
fn cut(column: &[Item], foot: f32) -> Option<f32> {
    let full: f32 = column.iter().map(height).sum();
    if full <= foot + 0.5 {
        return None;
    }
    let mut top = 0.;
    let mut cut = None;
    for item in column {
        if matches!(item, Item::Row(_)) && top + KEY_ROW / 2. <= foot {
            cut = Some(top + KEY_ROW / 2.);
        }
        top += height(item);
    }
    cut
}

impl SheetView {
    /// The held list while modifiers are held or a chord is looked up, in two columns that scroll; none at rest or while searching.
    pub(in crate::g2::st1::sheet_view) fn held_rows(
        &self,
        window: &Window,
        measured: &Measured,
        ink: &Ink,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let keyboard = self.sheet.keyboard.as_ref()?;
        let room = self.list_room(window)?;
        // A search keeps its field and results: there Ctrl edits the text, as it does without a keyboard.
        if (self.sheet.held == 0 && self.sheet.chord.is_none()) || self.searching() {
            return None;
        }
        let rows = keyboard.rows(self.sheet.held, self.sheet.chord);
        if rows.is_empty() {
            return Some(super::heads::empty(
                self.sheet.catalog.tr("keys.held.none"),
                ink,
            ));
        }
        let mut items = Vec::new();
        for (index, bound) in rows.iter().enumerate() {
            let group = group_of(&bound.action);
            if index == 0 || group != group_of(&rows[index - 1].action) {
                items.push(Item::Head(group));
            }
            items.push(Item::Row(index));
        }
        let at = split(&items, rows.len(), room);
        let mut second = items.split_off(at);
        // A group running on into the second column keeps its rows on the first column's grid.
        if matches!(second.first(), Some(Item::Row(_))) {
            second.insert(0, Item::Gap);
        }
        // One chord column for the whole board, in the caption's monospace advance, so labels never move between keys.
        let advance = f32::from(self.typeface.caption) * 0.6;
        let longest = keyboard
            .bound
            .iter()
            .map(|bound| spelled(keyboard, bound).chars().count())
            .max()
            .unwrap_or(0);
        // Up to a little over half a column, so a long chord in a narrow window clips rather than squeezing the labels.
        let column = (self.card(window).2 - 2. * PAD - COL_GAP) / 2.;
        let chord_w = (longest as f32 * advance).min(column * 0.55);
        let mut grid = div()
            .flex()
            .items_start()
            .gap(px(COL_GAP))
            .debug_selector(|| "sheet-held".into());
        // The list's foot in its scrolled content, where a column running past it is cut and faded.
        let foot = self.sheet.scroll + measured.body;
        for (side, column) in [items, second].into_iter().enumerate() {
            let full: f32 = column.iter().map(height).sum();
            let cut = cut(&column, foot);
            let mut drawn = div().flex().flex_col();
            for item in column {
                drawn = match item {
                    Item::Head(group) => {
                        drawn.child(self.heading(super::heads::group_key(group), ink))
                    }
                    Item::Gap => drawn.child(div().h(px(HEAD_SLOT)).flex_none()),
                    Item::Row(index) => {
                        let bound = rows[index];
                        let line = div()
                            .h(px(KEY_ROW))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(GAP))
                            // A looked-up chord's own binding reads bold among those that add more to it.
                            .when(
                                self.sheet.chord.is_some() && bound.mask == self.sheet.held,
                                |line| line.font_weight(FontWeight::BOLD),
                            )
                            .child(
                                chord_line(&spelled(keyboard, bound), ink)
                                    .w(px(chord_w))
                                    .flex_none()
                                    .overflow_hidden()
                                    .mr(px(5.)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(SharedString::from(bound.label.clone())),
                            );
                        let hover = Hover::Binding {
                            mask: bound.mask,
                            cap: bound.cap,
                        };
                        drawn.child(self.hovering(line, ("sheet-held-row", index), hover, ink, cx))
                    }
                };
            }
            let column = div().flex_1().min_w_0().relative();
            // Clipped at the cut rather than painted over, at the column's full height so the list still scrolls on.
            grid = grid.child(match cut {
                Some(cut) => column
                    .h(px(full))
                    .child(drawn.h(px(cut)).overflow_hidden())
                    .child(Self::fade(side, cut, ink)),
                None => column.child(drawn),
            });
        }
        // A list taller than its room scrolls under the wheel as the rest list does.
        Some(self.body(grid, cx))
    }
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
