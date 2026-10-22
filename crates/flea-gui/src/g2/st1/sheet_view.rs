//! Draws the keyboard sheet to the keymap boards: two columns of groups, or the query field over ranked results.
use super::geometry::{CLAMP, GAP};
use super::marks::{self, spaced, washed};
use super::seams::{Ink, Request};
use super::sheet::Sheet;
use super::view::SettingsView;
use crate::list::Typeface;
use flea::keys::SHEET_GROUPS;
use flea::ui_logic_st1::{caps, sheet::match_of};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, Context, Div,
    EventEmitter, FocusHandle, FontWeight, KeyDownEvent, Render, ScrollWheelEvent, SharedString,
    Stateful, Window,
};

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
    pub sheet: Sheet,
    pub focus: FocusHandle,
    pub typeface: Typeface,
    /// The compositor's rounding, a seam for the theme service as Facts.corner is for Settings.
    pub corner: f32,
}

impl EventEmitter<Request> for SheetView {}

impl SheetView {
    pub fn new(sheet: Sheet, cx: &mut Context<Self>) -> Self {
        Self {
            sheet,
            focus: cx.focus_handle(),
            typeface: Typeface::omarchy_default(),
            corner: 8.,
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        self.sheet
            .key(&stroke.key, stroke.key_char.as_deref().unwrap_or(""));
        self.flush(cx);
        cx.stop_propagation();
    }

    fn flush(&mut self, cx: &mut Context<Self>) {
        for request in std::mem::take(&mut self.sheet.requests) {
            cx.emit(request);
        }
        cx.notify();
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

    /// The groups split into two columns at the first group that would pass half the rows, each faded at its foot.
    fn columns(&self, ink: &Ink) -> Div {
        let catalog = &self.sheet.catalog;
        let mut columns: [Vec<AnyElement>; 2] = [Vec::new(), Vec::new()];
        let half = self.sheet.rows.len().div_ceil(2);
        let mut used = 0;
        for group in SHEET_GROUPS {
            let rows: Vec<_> = self
                .sheet
                .rows
                .iter()
                .filter(|row| row.group == group.id)
                .collect();
            if rows.is_empty() {
                continue;
            }
            let side = usize::from(used > 0 && used + rows.len() > half);
            used += rows.len();
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
            for row in rows {
                columns[side].push(
                    div()
                        .h(px(KEY_ROW))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(GAP))
                        .child(self.keys(&row.keys, None, false, ink))
                        .child(
                            div()
                                .whitespace_nowrap()
                                .child(SharedString::from(row.label.clone())),
                        )
                        .into_any_element(),
                );
            }
        }
        let fade = || {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(COLUMN_FADE))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(ink.surface.opacity(0.), 0.),
                    linear_color_stop(ink.surface, 1.),
                ))
        };
        let [left, right] = columns;
        let column = |rows: Vec<AnyElement>| {
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_hidden()
                .child(div().flex().flex_col().children(rows))
                .child(fade())
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .items_start()
            .gap(px(COL_GAP))
            .child(column(left))
            .child(column(right))
    }

    /// The ranked rows at the card's full width: the hit row hovered with its run bold, a dead row dimmed with its reason.
    fn results(&self, ink: &Ink, cx: &mut Context<Self>) -> Stateful<Div> {
        let catalog = &self.sheet.catalog;
        let query = &self.sheet.query;
        let rows = self.sheet.ranked.iter().enumerate().map(|(index, row)| {
            let lifted = index == self.sheet.cursor;
            let label = row.label.resolve(catalog);
            let label_run = match_of(&label, query).map(|run| (run.start, run.length));
            let key_run = if label_run.is_none() {
                match_of(&row.keys, query).map(|run| (run.start, run.length))
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
                .child(dim(self.keys(&row.keys, key_run, lifted, ink)))
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
        let scroll = self.sheet.scroll;
        div()
            .id("sheet-results")
            .flex_1()
            .min_h_0()
            .mx(px(-PAD))
            .relative()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, window, cx| {
                let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
                let most =
                    (view.sheet.ranked.len() as f32 * KEY_ROW - view.viewport(window)).max(0.);
                view.sheet.scroll = (view.sheet.scroll - delta).clamp(0., most);
                cx.notify();
            }))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(-scroll))
                    .flex()
                    .flex_col()
                    .children(rows),
            )
    }

    /// The results' height inside a card fitted to this window.
    fn viewport(&self, window: &Window) -> f32 {
        let (_, height) = card_size(window);
        height - 2. - 2. * PAD - TITLE_H - 1. - FIELD_H - 3. * STACK_GAP
    }
}

/// The board's card centred in a window this size and kept 8px clear of every edge; no sheet state moves it.
pub fn card_rect(width: f32, height: f32) -> (f32, f32, f32, f32) {
    let fit = |want: f32, room: f32| want.min((room - 2. * CLAMP).floor()).max(0.);
    let (card_w, card_h) = (fit(SHEET_W, width), fit(SHEET_H, height));
    (
        ((width - card_w) / 2.).round(),
        ((height - card_h) / 2.).round(),
        card_w,
        card_h,
    )
}

fn card_size(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let (_, _, card_w, card_h) = card_rect(f32::from(viewport.width), f32::from(viewport.height));
    (card_w, card_h)
}

impl Render for SheetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = Ink::inherited(&window.text_style(), theme);
        let searching = !self.sheet.query.is_empty();
        if searching {
            let viewport = self.viewport(window);
            self.sheet.reveal(viewport, KEY_ROW);
        }
        let viewport = window.viewport_size();
        let (left, top, card_w, card_h) =
            card_rect(f32::from(viewport.width), f32::from(viewport.height));
        let title = self.sheet.catalog.tr("keys.action.keymapSheet");
        let mut card = div()
            .id("sheet-card")
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
                SettingsView::field(&self.sheet.query, &ink, self.corner, &self.typeface)
                    .flex_none(),
            )
            .child(self.results(&ink, cx))
        } else {
            card.child(self.columns(&ink))
        };
        let card = marks::card(card);
        marks::ground(
            div().id("sheet-ground"),
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
