//! Draws the Settings card over its ground: the head, the rail with its search field, and the pane's rows or results.
use super::draw::{Line, Rows};
use super::geometry::{self as g, Scale, BORDER, FADE, PAD_X, RAIL_W};
use super::input::{Editor, Host};
use super::marks::{self, mark};
use super::panel::{Panel, Press, Side};
use super::seams::{Ink, Request};
use crate::list::Typeface;
use crate::window::input::reading;
use flea::ui_logic_settings_pages::{
    pages::SECTIONS,
    rows::{Kind, Row},
    visible,
};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, Context, Div, EventEmitter,
    FocusHandle, KeyDownEvent, MouseButton, MouseUpEvent, Render, ScrollWheelEvent, SharedString,
    Stateful, TextRun, Window,
};

#[cfg(test)]
#[path = "m9_g2rows_02_view_tests.rs"]
mod m9_g2rows_02;

pub struct SettingsView {
    pub(super) editor: Editor,
    pub panel: Panel,
    pub focus: FocusHandle,
    pub typeface: Typeface,
    pub probe: super::probe::Probe,
    pub plugins: Option<gpui::Entity<crate::g2::plugin_settings::view::PluginsPane>>,
}

impl EventEmitter<Request> for SettingsView {}

impl SettingsView {
    pub fn new(panel: Panel, cx: &mut Context<Self>) -> Self {
        Self {
            panel,
            editor: Editor::default(),
            plugins: None,
            probe: super::probe::Probe::default(),
            focus: cx.focus_handle(),
            typeface: Typeface::omarchy_default(),
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.query_key(event, window, cx) {
            return;
        }
        if super::plugins::key(self, event, window, cx) {
            return;
        }
        let stroke = &event.keystroke;
        // A query types its text; otherwise the panel navigates, mirrored with the window.
        let mirrored = window.layout_mirrored() && self.panel.query.is_none();
        let text = reading(stroke.key_char.as_deref().unwrap_or(""), mirrored);
        self.panel
            .key(reading(&stroke.key, mirrored), text, stroke.modifiers.shift);
        self.flush(cx);
        cx.stop_propagation();
    }

    /// Hands every request the panel queued to the host and redraws.
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        let requests = self.panel.take_requests();
        if requests.contains(&Request::Close) {
            self.editor.retire();
        }
        self.sync_editor();
        self.sync_plugin_input(cx);
        for request in requests {
            cx.emit(request);
        }
        cx.notify();
    }

    fn rail(&self, ink: &Ink, cx: &mut Context<Self>) -> Div {
        let s = self.panel.scale;
        let searching = self.panel.searching();
        let mut rail = self
            .probe
            .attach(div(), "rail")
            .w(px(s.of(RAIL_W)))
            .h_full()
            .flex_none()
            .pt(px(s.of(g::RAIL_PAD_TOP)))
            .border_r_1()
            .border_color(ink.rule)
            .flex()
            .flex_col();
        if self.panel.query.is_some() {
            rail = rail.child(
                div().px(px(s.of(PAD_X))).pb(px(s.of(g::GAP))).child(
                    Self::field(cx.entity(), ink, self.panel.facts.corner, &self.typeface, s)
                        .relative()
                        .child(self.search_clear(cx)),
                ),
            );
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
                .h(px(s.rail_row))
                .flex()
                .items_center()
                .gap(px(s.of(g::GAP)))
                .px(px(s.of(PAD_X)))
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
                    s.of(g::RAIL_MARK),
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
        if !self.panel.searching() {
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
        if let Some(footer) = self.search_footer(&out) {
            out.push(footer);
        }
        out
    }

    /// The segment rule from the shaped widths: every option beside the label, or the chevron walk.
    pub(super) fn fits(&self, row: &Row, width: f32, window: &Window) -> bool {
        let catalog = &self.panel.catalog;
        let options: Vec<f32> = row
            .labels
            .iter()
            .map(|label| self.shaped(label.resolve(catalog), self.typeface.caption, window))
            .collect();
        if visible::marks(row) {
            return true;
        }
        visible::segment_fits(
            self.shaped(row.label.resolve(catalog), self.typeface.body, window),
            &options,
            width,
        )
    }

    /// Shapes every hint and colliding label at the pane's width, with each row's bold search run, so heights match the drawn wraps in any language and size.
    fn measure<'r>(
        &self,
        rows: impl IntoIterator<Item = (&'r Row, Option<(usize, usize)>)>,
        width: f32,
        window: &Window,
    ) -> Vec<usize> {
        let font = self.typeface.font.clone();
        rows.into_iter()
            .map(|(row, bold)| {
                if matches!(
                    row.kind,
                    Kind::Fact | Kind::Lock | Kind::Check | Kind::Choice | Kind::Action
                ) && !row.elide_right
                {
                    return self.label_lines(row, bold, width, window);
                }
                if row.kind != Kind::Hint || row.elide_right {
                    return 1;
                }
                let s = self.panel.scale;
                let inset = if row.footer {
                    2. * s.of(PAD_X)
                } else {
                    s.label_x() + s.of(PAD_X)
                };
                // Each unit is shaped whole, as the pane places it, and packed into lines.
                let text = row.label.resolve(&self.panel.catalog);
                g::hint_lines(&text, (width - inset).max(0.), |unit| {
                    let run = TextRun {
                        len: unit.len(),
                        font: font.clone(),
                        color: Default::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    f32::from(
                        window
                            .text_system()
                            .shape_line(
                                SharedString::from(unit.to_owned()),
                                self.typeface.caption,
                                &[run],
                                None,
                            )
                            .width,
                    )
                })
            })
            .collect()
    }

    fn pane(&mut self, ink: &Ink, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.panel.section == "plugins" && !self.panel.searching() {
            if let Some(view) = &self.plugins {
                return div()
                    .id("settings-pane")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            view.panel.side = Side::Pane;
                            view.sync_plugin_input(cx);
                        }),
                    )
                    .relative()
                    .flex_1()
                    .h_full()
                    .child(view.clone());
            }
        }
        let mut lines = self.lines();
        let searching = self.panel.searching();
        let width = self.panel.pane.0;
        for line in &mut lines {
            line.segment = line.row.kind == Kind::Choice && self.fits(line.row, width, window);
            line.room = Some(self.room(line.row, line.segment, width, window));
            super::draw::user_keys::measure(line, &self.typeface, window);
        }
        let heights = if searching {
            let rows: Vec<Row> = lines.iter().map(|line| line.row.clone()).collect();
            let measured = self.measure(
                lines.iter().map(|line| (line.row, line.bold_run())),
                width,
                window,
            );
            g::heights(&rows, &measured, self.panel.scale)
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
            .pb(px(self.panel.scale.of(g::PANE_FOOT)))
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
                                .h(px(self.panel.scale.of(FADE)))
                                .bg(linear_gradient(
                                    180.,
                                    linear_color_stop(ink.surface.opacity(0.), 0.),
                                    linear_color_stop(ink.surface, 1.),
                                )),
                        )
                    }),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_editor();
        self.sync_plugin_input(cx);
        self.probe.clear();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = Ink::inherited(&window.text_style(), theme);
        let viewport = window.viewport_size();
        let s = Scale::at(&self.typeface);
        self.panel.set_scale(s);
        let (card_w, card_h) = g::card_size(
            f32::from(viewport.width),
            f32::from(viewport.height),
            self.panel.view_height,
            s,
        );
        let pane = g::pane_size(card_w, card_h, s);
        let lines = self.measure(
            self.panel.rows.iter().map(|row| (row, None)),
            pane.0,
            window,
        );
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
        .capture_key_down(cx.listener(Self::search_capture))
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
                    let rows = ((f32::from(event.position.y) - from)
                        / view.panel.scale.of(g::FAVOURITE_H))
                    .round() as isize;
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
        .child(self.probe.attach(div().absolute().inset_0(), "ground"))
        .children(self.plugin_card(cx))
    }
}
