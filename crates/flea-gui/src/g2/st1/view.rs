//! Draws the Settings card over its ground: the head, the rail with its search field, and the pane's rows or results.
use super::draw::{Line, Rows};
use super::geometry::{self as g, BORDER, FADE, PAD_X, RAIL_W};
use super::marks::{self, mark, washed};
use super::panel::{Panel, Press, Side};
use super::seams::{Ink, Request};
use crate::list::Typeface;
use flea::ui_logic_st1::{
    caps,
    pages::SECTIONS,
    rows::{Kind, Row},
    visible,
};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, Context, Div, EventEmitter,
    FocusHandle, KeyDownEvent, MouseButton, MouseUpEvent, Render, ScrollWheelEvent, SharedString,
    Stateful, TextRun, Window,
};

pub struct SettingsView {
    pub panel: Panel,
    pub focus: FocusHandle,
    pub typeface: Typeface,
    pub probe: super::probe::Probe,
    pub plugins: Option<gpui::Entity<crate::g2::st2::view::PluginsPane>>,
}

impl EventEmitter<Request> for SettingsView {}

impl SettingsView {
    pub fn new(panel: Panel, cx: &mut Context<Self>) -> Self {
        Self {
            panel,
            plugins: None,
            probe: super::probe::Probe::default(),
            focus: cx.focus_handle(),
            typeface: Typeface::omarchy_default(),
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if super::plugins::key(self, event, cx) {
            cx.stop_propagation();
            return;
        }
        let stroke = &event.keystroke;
        let text = stroke.key_char.as_deref().unwrap_or("");
        self.panel.key(&stroke.key, text, stroke.modifiers.shift);
        self.flush(cx);
        cx.stop_propagation();
    }

    /// Hands every request the panel queued to the host and redraws.
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        for request in self.panel.take_requests() {
            cx.emit(request);
        }
        cx.notify();
    }

    fn searching(&self) -> bool {
        self.panel
            .query
            .as_deref()
            .is_some_and(|query| !query.is_empty())
    }

    /// The search field the sheet uses: magnifier, the typed text and caret, and the clear mark at its end.
    pub(super) fn field(text: &str, ink: &Ink, corner: f32, typeface: &Typeface) -> Div {
        div()
            .h(px(g::FIELD_H))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(g::GAP))
            .border_1()
            .border_color(ink.accent_frame)
            .rounded(px(corner))
            .bg(ink.background)
            .text_size(typeface.body)
            .text_color(ink.foreground)
            .child(mark("search", 14., ink.muted))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(SharedString::from(text.to_owned()))
                    .child(div().ml(px(1.)).w(px(1.)).h(px(16.)).bg(ink.foreground)),
            )
            .child(div().flex_1())
            .child(mark("x", 12., ink.muted))
    }

    fn rail(&self, ink: &Ink, cx: &mut Context<Self>) -> Div {
        let searching = self.searching();
        let mut rail = self
            .probe
            .attach(div(), "rail")
            .w(px(RAIL_W))
            .h_full()
            .flex_none()
            .pt(px(g::RAIL_PAD_TOP))
            .border_r_1()
            .border_color(ink.rule)
            .flex()
            .flex_col();
        if let Some(query) = &self.panel.query {
            rail = rail.child(div().px(px(PAD_X)).pb(px(g::GAP)).child(Self::field(
                query,
                ink,
                self.panel.facts.corner,
                &self.typeface,
            )));
        }
        rail.children(SECTIONS.iter().map(|section| {
            // While a query stands no section is current; Columns is View's own page.
            let current = !searching
                && (section.id == self.panel.section
                    || (self.panel.section == "columns" && section.id == "view"));
            let keys = current && self.panel.side == Side::Rail;
            let id = section.id;
            self.probe
                .attach(div(), format!("rail:{id}"))
                .id(SharedString::from(format!("settings-rail-{id}")))
                .relative()
                .h(px(g::RAIL_ROW_H))
                .flex()
                .items_center()
                .gap(px(g::GAP))
                .px(px(PAD_X))
                .cursor_pointer()
                .when(keys, |row| {
                    row.bg(ink.wash18)
                        .child(div().absolute().inset_0().bg(ink.hover))
                })
                .when(current && !keys, |row| row.bg(ink.selected))
                .when(keys, |row| {
                    row.child(
                        div()
                            .absolute()
                            .left(px(-BORDER))
                            .top_0()
                            .bottom_0()
                            .w(px(2.))
                            .bg(ink.accent_frame),
                    )
                })
                .child(mark(
                    section.glyph,
                    g::RAIL_MARK,
                    if current {
                        ink.foreground
                    } else {
                        ink.muted_frame
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(self.panel.catalog.tr(section.label))),
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.panel.choose(id);
                    view.flush(cx);
                }))
        }))
    }

    /// The pane's lines: the section's rows, or while a query stands each result group's heading and hits.
    fn lines(&self) -> Vec<Line<'_>> {
        if !self.searching() {
            let lifted = |index: usize| index == self.panel.cursor && self.panel.side == Side::Pane;
            return self
                .panel
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| Line::row(row, index, lifted(index)))
                .collect();
        }
        let mut out = Vec::new();
        let mut hit = 0;
        for group in &self.panel.results {
            let rows = self.panel.everything_rows(group.section);
            if let Some(heading) = group.heading {
                // A group with no rows of its own is one result, its heading.
                let own = group.hits.is_empty().then_some(hit);
                let mut line = Line::result(
                    &rows[heading],
                    group.heading_run,
                    own,
                    own == Some(self.panel.hit),
                );
                line.section = Some(self.panel.catalog.tr(SECTIONS[group.section].label));
                out.push(line);
            }
            if group.hits.is_empty() {
                hit += 1;
            }
            for found in &group.hits {
                out.push(Line::result(
                    &rows[found.row],
                    found.run,
                    Some(hit),
                    hit == self.panel.hit,
                ));
                hit += 1;
            }
        }
        out
    }

    /// The segment rule from the shaped widths: every option beside the label, or the chevron walk.
    fn fits(&self, row: &Row, width: f32, window: &Window) -> bool {
        let shaped = |text: String, size| {
            let text = SharedString::from(text);
            let run = TextRun {
                len: text.len(),
                font: self.typeface.font.clone(),
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(text, size, &[run], None)
                    .width,
            )
        };
        let catalog = &self.panel.catalog;
        let options: Vec<f32> = row
            .labels
            .iter()
            .map(|label| shaped(label.resolve(catalog), self.typeface.caption))
            .collect();
        if visible::marks(row) {
            return true;
        }
        visible::segment_fits(
            shaped(row.label.resolve(catalog), self.typeface.body),
            &options,
            width,
        )
    }

    /// Shapes every hint at the pane's width, so heights match the drawn wraps in any language and size.
    fn measure(&self, rows: &[Row], width: f32, window: &Window) -> Vec<usize> {
        let font = self.typeface.font.clone();
        rows.iter()
            .map(|row| {
                if row.kind != Kind::Hint || row.elide_right {
                    return 1;
                }
                let text = SharedString::from(row.label.resolve(&self.panel.catalog));
                let inset = if row.footer {
                    2. * PAD_X
                } else {
                    g::LABEL_X + PAD_X
                };
                let run = TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color: Default::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_text(
                        text,
                        self.typeface.caption,
                        &[run],
                        Some(px(width - inset)),
                        None,
                    )
                    .map_or(1, |lines| {
                        lines
                            .iter()
                            .map(|line| line.wrap_boundaries().len() + 1)
                            .sum()
                    })
            })
            .collect()
    }

    fn pane(&mut self, ink: &Ink, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.panel.section == "plugins" {
            if let Some(view) = &self.plugins {
                return div()
                    .id("settings-pane")
                    .relative()
                    .flex_1()
                    .h_full()
                    .child(view.clone());
            }
        }
        let mut lines = self.lines();
        let searching = self.searching();
        let width = self.panel.pane.0;
        for line in &mut lines {
            line.segment = line.row.kind == Kind::Choice && self.fits(line.row, width, window);
        }
        let heights = if searching {
            let rows: Vec<Row> = lines.iter().map(|line| line.row.clone()).collect();
            g::heights(&rows, &self.measure(&rows, width, window))
        } else {
            self.panel.heights()
        };
        let (_, height) = self.panel.pane;
        // The highlighted result's lines, from its group's heading when it is the group's first hit.
        let hit = lines
            .iter()
            .position(|line| searching && line.hit == Some(self.panel.hit))
            .map(|at| {
                let heading = at > 0 && lines[at - 1].hit.is_none();
                (if heading { at - 1 } else { at }, at)
            });
        let body = Rows {
            view: self,
            ink,
            heights: &heights,
        }
        .draw(&lines, cx);
        let scroll = if searching {
            self.panel.results_laid_out(&heights, hit)
        } else {
            self.panel.scroll
        };
        let fade = g::fade_shown(&heights, scroll, height);
        self.probe
            .attach(div(), "pane")
            .id("settings-pane")
            .relative()
            .flex_1()
            .h_full()
            .pb(px(g::PANE_FOOT))
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.panel.wheel(super::wheel::travel(event.delta));
                cx.notify();
            }))
            .child(
                div()
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(-scroll))
                            .flex()
                            .flex_col()
                            .children(body),
                    )
                    .when(fade, |pane| {
                        pane.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(FADE))
                                .bg(linear_gradient(
                                    180.,
                                    linear_color_stop(ink.surface.opacity(0.), 0.),
                                    linear_color_stop(ink.surface, 1.),
                                )),
                        )
                    }),
            )
    }

    /// A heading in the capitals its language writes, spaced as the boards space it; a typed run keeps its wash.
    pub(super) fn heading(
        &self,
        label: &str,
        run: Option<(usize, usize)>,
        lifted: bool,
        ink: &Ink,
    ) -> Div {
        let language = self.panel.catalog.language();
        let upper = |text: &str| caps::heading(text, language);
        let spacing = f32::from(self.typeface.caption) * 0.14;
        let Some((start, length)) = run else {
            return washed(&upper(label), None, lifted, ink.wash14, spacing);
        };
        // Capitals can change a run's length, as ß does, so each part is set in capitals on its own.
        let at = |chars: usize| {
            label
                .char_indices()
                .nth(chars)
                .map_or(label.len(), |(byte, _)| byte)
        };
        let (from, to) = (at(start), at(start + length));
        let (head, piece, tail) = (
            upper(&label[..from]),
            upper(&label[from..to]),
            upper(&label[to..]),
        );
        let run = (head.chars().count(), piece.chars().count());
        washed(
            &format!("{head}{piece}{tail}"),
            Some(run),
            lifted,
            ink.wash14,
            spacing,
        )
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.probe.clear();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = Ink::inherited(&window.text_style(), theme);
        let viewport = window.viewport_size();
        let (card_w, card_h) = g::card_size(
            f32::from(viewport.width),
            f32::from(viewport.height),
            self.panel.view_height,
        );
        let pane = g::pane_size(card_w, card_h);
        let lines = self.measure(&self.panel.rows, pane.0, window);
        self.panel.laid_out(pane, lines);
        let corner = self.panel.facts.corner;
        let left = ((f32::from(viewport.width) - card_w) / 2.).round();
        let top = ((f32::from(viewport.height) - card_h) / 2.).round();
        marks::ground(
            div()
                .on_children_prepainted(self.probe.first_child("card"))
                .id("settings-ground"),
            cx.listener(|view, _, _, cx| {
                view.panel.requests.push(Request::Close);
                view.flush(cx);
            }),
        )
        .track_focus(&self.focus)
        .on_key_down(cx.listener(Self::key_down))
        .size_full()
        .relative()
        .bg(ink.scrim)
        .font_family(self.typeface.font.family.clone())
        .text_size(self.typeface.body)
        .text_color(ink.foreground)
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|view, event: &MouseUpEvent, _, cx| {
                if let Some((index, from)) = view.panel.grip.take() {
                    let rows =
                        ((f32::from(event.position.y) - from) / g::FAVOURITE_H).round() as isize;
                    view.panel.press(index, Press::Drag(rows));
                    view.flush(cx);
                }
            }),
        )
        .child(marks::card(
            div()
                .id("settings-card")
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(card_w))
                .h(px(card_h))
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(ink.surface)
                .border_1()
                .border_color(ink.muted_frame)
                .rounded(px(corner))
                .child(self.head(&ink, cx))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(self.rail(&ink, cx))
                        .child(self.pane(&ink, window, cx)),
                ),
        ))
    }
}
