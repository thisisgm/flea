//! Draws the keyboard sheet to the keymap boards: two columns of groups, or the query field over ranked results.
use super::geometry::{CLAMP, GAP};
use super::input::{Editor, Field, Host};
use super::marks::{self, spaced, washed};
use super::probe::Probe;
use super::seams::{Ink, Request};
use super::sheet::Sheet;
use super::view::SettingsView;
use crate::g2::mn1::keycaps;
use crate::list::Typeface;
use flea::ui_logic_st1::{caps, sheet::match_of};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, Context, Div,
    EventEmitter, FocusHandle, FontWeight, Render, SharedString, Stateful, Window,
};

mod scroll;
use scroll::{Measured, COLUMNS, LAST_ROWS};
#[path = "sheet_geometry.rs"]
mod fit;
pub use fit::card_rect;

#[cfg(test)]
#[path = "i18n_fit_tests.rs"]
mod i18n_fit_tests;

// Board keymap: a 628 by 684 card padded 16, the key column 117 plus a 5px margin, columns 14 apart, 19px headings over 25px rows.
pub const SHEET_W: f32 = 628.;
pub const SHEET_H: f32 = 684.;
pub const PAD: f32 = 16.;
pub const KEY_COL: f32 = 117.;
pub const KEY_ROW: f32 = 25.;
pub const HEAD_SLOT: f32 = 19.;
pub const COL_GAP: f32 = 14.;
const COLUMN_FADE: f32 = 18.;
// The title's 16px line, the field and the hairline with the card's 12px gaps between them.
const TITLE_H: f32 = 20.;
const FIELD_H: f32 = 30.;
const STACK_GAP: f32 = 12.;

pub struct SheetView {
    pub(super) editor: Editor,
    pub sheet: Sheet,
    pub focus: FocusHandle,
    pub typeface: Typeface,
    /// The compositor's rounding, a seam for the theme service as Facts.corner is for Settings.
    pub corner: f32,
    /// The scrolled body and its content as the latest frame laid them out.
    pub probe: Probe,
    // A keyboard group switch while the sheet is open relabels its keys at once.
    _layout: gpui::Subscription,
}

impl EventEmitter<Request> for SheetView {}

impl SheetView {
    pub fn new(mut sheet: Sheet, cx: &mut Context<Self>) -> Self {
        sheet.set_keycaps(keycaps::watch(cx));
        Self {
            _layout: keycaps::follow(cx, |view: &mut Self, caps| view.sheet.set_keycaps(caps)),
            sheet,
            editor: Editor::default(),
            focus: cx.focus_handle(),
            typeface: Typeface::omarchy_default(),
            corner: super::seams::COMPOSITOR_CORNER_DEFAULT,
            probe: Probe::default(),
        }
    }

    /// The key column: alternatives split by a muted slash, the typed run washed when only the keys match.
    fn keys(&self, keys: &str, run: Option<(usize, usize)>, lifted: bool, ink: &Ink) -> Div {
        let mut column = div()
            .w(px(KEY_COL))
            .flex_none()
            .mr(px(5.))
            .flex()
            .justify_end()
            .whitespace_nowrap()
            .text_color(ink.foreground);
        if run.is_some() {
            return column.child(washed(keys, run, lifted, ink.wash14, 0.));
        }
        for (index, key) in keys.split(" / ").enumerate() {
            if index > 0 {
                column = column.child(div().text_color(ink.muted).child(" / "));
            }
            column = column.child(SharedString::from(key.to_owned()));
        }
        column
    }

    /// The groups split into two columns at the first group that would pass half the rows, each faded at its own foot.
    fn columns(&self, measured: &Measured, ink: &Ink, cx: &mut Context<Self>) -> Stateful<Div> {
        let catalog = &self.sheet.catalog;
        let groups = self.sheet.groups();
        let last = [0, 1].map(|side| groups.iter().rposition(|(on, ..)| *on == side));
        let mut columns: [Vec<AnyElement>; 2] = [Vec::new(), Vec::new()];
        for (index, (side, group, rows)) in groups.iter().enumerate() {
            let side = *side;
            let label = caps::heading(&catalog.tr(group.label_key), catalog.language());
            columns[side].push(
                div()
                    .h(px(HEAD_SLOT))
                    .flex_none()
                    .flex()
                    .items_end()
                    .pb(px(2.))
                    .line_height(px(16.))
                    .text_color(ink.muted)
                    .child(spaced(&label, 1.))
                    .into_any_element(),
            );
            for (at, row) in rows.iter().enumerate() {
                let line = div()
                    .h(px(KEY_ROW))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .child(self.keys(self.sheet.cap(&row.keys), None, false, ink))
                    .child(
                        div()
                            .debug_selector(move || format!("sheet-label-{index}-{at}"))
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(row.label.clone())),
                    );
                let line = if last[side] == Some(index) && at + 1 == rows.len() {
                    self.probe.masked(line, LAST_ROWS[side])
                } else {
                    line
                };
                columns[side].push(line.into_any_element());
            }
        }
        let below = measured.most - self.sheet.scroll > 1.;
        let foot = self.sheet.scroll + measured.body;
        let mut grid = div().flex().items_start().gap(px(COL_GAP));
        for (side, rows) in columns.into_iter().enumerate() {
            let rows = self.probe.attach(
                div().flex_none().flex().flex_col().children(rows),
                COLUMNS[side],
            );
            let column = div()
                .flex_1()
                .min_w_0()
                .relative()
                .debug_selector(move || format!("sheet-column-{side}"));
            // Board keymap: a column running past the body's foot is cut there, one ending within a row of it on its last row's half.
            let cut = measured.feet[side]
                .filter(|_| below)
                .filter(|end| end.end > foot - 2. * (end.end - end.half))
                .map(|end| (end.end, if end.end > foot { foot } else { end.half }));
            // The rows are clipped at the cut rather than painted over, so a translucent ground never shows them under the fade.
            grid = grid.child(match cut {
                Some((end, cut)) => column
                    .h(px(end))
                    .child(
                        div()
                            .h(px(cut))
                            .overflow_hidden()
                            .debug_selector(move || format!("sheet-clip-{side}"))
                            .child(rows),
                    )
                    .child(Self::fade(side, cut, ink)),
                None => column.child(rows),
            });
        }
        self.body(grid, cx).id("sheet-rest")
    }

    /// The 18px fade over a column's cut, as each clipped menu ends to say it scrolls.
    fn fade(side: usize, cut: f32, ink: &Ink) -> Div {
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(cut - COLUMN_FADE))
            .h(px(COLUMN_FADE))
            .debug_selector(move || format!("sheet-fade-{side}"))
            .bg(linear_gradient(
                180.,
                linear_color_stop(ink.surface.opacity(0.), 0.),
                linear_color_stop(ink.surface, 1.),
            ))
    }

    /// The ranked rows at the card's full width: the hit row hovered with its run bold, a dead row dimmed with its reason.
    fn results(&self, ink: &Ink, cx: &mut Context<Self>) -> Stateful<Div> {
        let catalog = &self.sheet.catalog;
        let query = &self.sheet.query;
        let rows = self.sheet.ranked.iter().enumerate().map(|(index, row)| {
            let lifted = index == self.sheet.cursor;
            let label = row.label.resolve(catalog);
            let label_run = match_of(&label, query).map(|run| (run.start, run.length));
            let keys = self.sheet.result_keys(index);
            let key_run = if label_run.is_none() {
                match_of(keys, query).map(|run| (run.start, run.length))
            } else {
                None
            };
            let place = row.where_.resolve(catalog);
            let reason = row.reason.resolve(catalog);
            let dim = |piece: Div| {
                if row.disabled {
                    piece.opacity(0.55)
                } else {
                    piece
                }
            };
            let mut wording = div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .flex()
                .whitespace_nowrap()
                .child(washed(&label, label_run, lifted, ink.wash14, 0.));
            if !place.is_empty() {
                wording = wording.child(div().text_color(ink.muted).child(SharedString::from(
                    format!(" {}", catalog.tr_args("settings.sheet.in", &[&place])),
                )));
            }
            div()
                .id(("sheet-result", index))
                .h(px(KEY_ROW))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(GAP))
                .px(px(PAD))
                .whitespace_nowrap()
                .when(lifted, |line| line.bg(ink.hover))
                .on_mouse_move(cx.listener(move |view, _, _, cx| {
                    if view.sheet.cursor != index {
                        view.sheet.point(index, false);
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.sheet.point(index, true);
                    view.flush(cx);
                }))
                .child(dim(self.keys(keys, key_run, lifted, ink)))
                .child(dim(wording))
                .when(row.disabled && !reason.is_empty(), |line| {
                    line.child(
                        div()
                            .flex_none()
                            .text_color(ink.muted)
                            .child(SharedString::from(reason)),
                    )
                })
        });
        self.body(div().flex().flex_col().children(rows), cx)
            .mx(px(-PAD))
            .id("sheet-results")
    }

    /// The results' height inside a card fitted to this window.
    fn viewport(&self, window: &Window) -> f32 {
        let (_, height) = card_size(window);
        height - 2. - 2. * PAD - TITLE_H - 1. - FIELD_H - 3. * STACK_GAP
    }
}

fn card_size(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let (_, _, card_w, card_h) = card_rect(f32::from(viewport.width), f32::from(viewport.height));
    (card_w, card_h)
}

impl Render for SheetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_editor();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = Ink::inherited(&window.text_style(), theme);
        let searching = !self.sheet.query.is_empty() || !self.editor.entry.text.is_empty();
        let measured = self.laid_out(window, cx);
        let (left, top, card_w, card_h) = fit::for_sheet(&self.sheet, &self.typeface, window);
        let title = self.sheet.catalog.tr("keys.action.keymapSheet");
        let mut card = div()
            .id("sheet-card")
            .debug_selector(|| "sheet-card".into())
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(card_w))
            .h(px(card_h))
            .p(px(PAD))
            .flex()
            .flex_col()
            .gap(px(STACK_GAP))
            .overflow_hidden()
            .bg(ink.surface)
            .border_1()
            .border_color(ink.muted_frame)
            .rounded(px(self.corner))
            .child(
                div()
                    .h(px(TITLE_H))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .text_size(px(16.))
                    .font_weight(FontWeight::BOLD)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .font_weight(FontWeight::NORMAL)
                            .text_size(self.typeface.caption)
                            .text_color(ink.muted)
                            .child("esc"),
                    ),
            )
            .child(div().h(px(1.)).flex_none().bg(ink.rule));
        card = if searching {
            card.child(
                SettingsView::field(
                    cx.entity(),
                    &ink,
                    self.corner,
                    &self.typeface,
                    super::geometry::Scale::default(),
                )
                .flex_none()
                .relative()
                .child(
                    div()
                        .id("sheet-query-clear")
                        .debug_selector(|| "sheet-query-clear".into())
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .w(px(30.))
                        .cursor_pointer()
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.editor.clear();
                            view.sheet.query_changed(String::new());
                            cx.notify();
                            cx.stop_propagation();
                        })),
                ),
            )
            .child(self.results(&ink, cx))
        } else {
            card.child(self.columns(&measured, &ink, cx)).child(
                div()
                    .absolute()
                    .left(px(PAD + 32.))
                    .right(px(PAD + 30.))
                    .top(px(PAD + TITLE_H + 1. + 2. * STACK_GAP))
                    .h(px(FIELD_H))
                    .child(Field::mount(cx.entity(), &self.typeface, false)),
            )
        };
        let card = marks::card(card);
        marks::ground(
            div()
                .on_children_prepainted(self.probe.first_child("card"))
                .id("sheet-ground"),
            cx.listener(|view, _, _, cx| {
                view.sheet.requests.push(Request::Close);
                view.flush(cx);
            }),
        )
        .track_focus(&self.focus)
        .on_key_down(cx.listener(Self::key_down))
        .size_full()
        .relative()
        .bg(ink.scrim)
        .font_family(self.typeface.font.family.clone())
        .text_size(self.typeface.caption)
        .text_color(ink.foreground)
        .child(card)
    }
}
