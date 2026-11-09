//! One Settings pane row per kind, as ui/SettingsRow.qml draws it at the settings boards' measures.
use super::geometry::{self as g, Scale, CHIP_GAP, CHIP_H, GAP, MARK_SLOT, PAD_X};
use super::marks::{mark, washed};
use super::panel::Press;
use super::seams::Ink;
use super::view::SettingsView;
use flea::ui_logic_settings_pages::{
    rows::{Kind, Master, Role, Row},
    search::Run,
    visible::marks,
};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Div, FontWeight, HighlightStyle, Hsla, MouseButton,
    MouseDownEvent, SharedString, StyledText,
};

mod hint;
pub(super) mod user_keys;

/// The key column of the Settings > Keys board's preview.
pub const KEYS_W: f32 = 48.;

/// One pane line: a section row at its index, or a search result with its typed run and its hit number.
pub struct Line<'a> {
    pub row: &'a Row,
    pub index: Option<usize>,
    pub hit: Option<usize>,
    pub run: Option<Run>,
    pub lifted: bool,
    /// The section a result group came from, named at the heading's end.
    pub section: Option<String>,
    /// Whether every option of a choice fits beside its label, decided from shaped widths.
    pub segment: bool,
    /// A key preview's key column, widened from shaped widths only where a user chord outgrows the board's 48.
    pub keys_w: f32,
    /// A control row's value and caption caps from shaped widths, so the drawn label keeps the room it was measured in.
    pub room: Option<g::Room>,
    /// A wrapping hint's units as the pane places them, packed at the measured width.
    pub units: Vec<String>,
}

impl<'a> Line<'a> {
    pub fn row(row: &'a Row, index: usize, lifted: bool) -> Self {
        Self {
            row,
            index: Some(index),
            hit: None,
            run: None,
            lifted,
            section: None,
            segment: false,
            keys_w: KEYS_W,
            room: None,
            units: Vec::new(),
        }
    }

    pub fn result(row: &'a Row, run: Option<Run>, hit: Option<usize>, lifted: bool) -> Self {
        Self {
            row,
            index: None,
            hit,
            run,
            lifted,
            section: None,
            segment: false,
            keys_w: KEYS_W,
            room: None,
            units: Vec::new(),
        }
    }

    /// The run its label draws bold, in characters: a lifted hit's only.
    pub fn bold_run(&self) -> Option<(usize, usize)> {
        let run = self.run.filter(|_| self.lifted)?;
        Some((run.start, run.length))
    }
}

pub use super::marks::check;

fn row_state(row: &Row) -> Master {
    if row.on {
        Master::All
    } else {
        Master::None
    }
}

pub struct Rows<'v> {
    pub view: &'v SettingsView,
    pub ink: &'v Ink,
    pub heights: &'v [f32],
}

impl Rows<'_> {
    fn text(&self, row: &Row) -> String {
        row.label.resolve(&self.view.panel.catalog)
    }

    fn corner(&self) -> f32 {
        self.view.panel.facts.corner
    }

    /// The text size the pane measures at.
    fn s(&self) -> Scale {
        self.view.panel.scale
    }

    /// A board measure at the text size.
    fn px(&self, board: f32) -> gpui::Pixels {
        px(self.s().of(board))
    }

    /// The row's label; a wrapped one keeps its typed run in one styled text, so the run never stops the wrap.
    fn label(&self, line: &Line<'_>, ink: Hsla, wrapped: bool) -> Div {
        let run = line.run.map(|run| (run.start, run.length));
        let text = self.text(line.row);
        if !wrapped {
            return washed(&text, run, line.lifted, self.ink.wash14, 0.).text_color(ink);
        }
        let at = |chars: usize| {
            text.char_indices()
                .nth(chars)
                .map_or(text.len(), |(byte, _)| byte)
        };
        let style = if line.bold_run().is_some() {
            HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..HighlightStyle::default()
            }
        } else {
            HighlightStyle {
                background_color: Some(self.ink.wash14),
                ..HighlightStyle::default()
            }
        };
        let runs: Vec<_> = run
            .map(|(start, length)| (at(start)..at(start + length), style))
            .into_iter()
            .collect();
        let id = line.row.id.clone();
        div()
            .debug_selector(move || format!("settings-label-{id}"))
            .line_height(px(self.s().line(g::WRAP_LINE)))
            .text_color(ink)
            .child(StyledText::new(text).with_highlights(runs))
    }

    pub fn draw(&self, lines: &[Line<'_>], cx: &mut Context<SettingsView>) -> Vec<AnyElement> {
        lines
            .iter()
            .enumerate()
            .map(|(at, line)| self.line(line, at, cx))
            .collect()
    }

    fn line(&self, line: &Line<'_>, at: usize, cx: &mut Context<SettingsView>) -> AnyElement {
        let row = line.row;
        let height = self.heights.get(at).copied().unwrap_or(self.s().row);
        let greyed = !row.available || (row.kind == Kind::Check && row.inert);
        let base = super::probe::row_probe(self.view, &row.id)
            .id(("settings-line", at))
            .relative()
            .flex_none()
            .h(px(height))
            .when(greyed, |line| line.opacity(0.55))
            .when(line.lifted, |line| line.bg(self.ink.hover));
        let base = self.pointer(base, line, cx);
        match row.kind {
            Kind::Group => self.group(base, line, at == 0, cx),
            Kind::Hint => self.hint(base, line),
            Kind::Ruler => self.ruler(base, line, cx),
            Kind::KeyPreview => self.key_preview(base, row, line.keys_w),
            Kind::Hero => self.hero(base, row),
            Kind::Favourite => self.favourite(base, line, cx),
            _ => self.control(base, line, height > self.s().row, cx),
        }
        .into_any_element()
    }

    fn group(
        &self,
        base: gpui::Stateful<Div>,
        line: &Line<'_>,
        first: bool,
        cx: &mut Context<SettingsView>,
    ) -> gpui::Stateful<Div> {
        let row = line.row;
        let ink = self.ink;
        let catalog = &self.view.panel.catalog;
        let run = line.run.map(|run| (run.start, run.length));
        let mut end = div()
            .h(self.px(14.))
            .flex()
            .items_center()
            .gap(self.px(GAP));
        if let Some(section) = &line.section {
            end = end.child(SharedString::from(section.clone()));
        } else if row.master {
            let spacing = self.view.heading_spacing();
            end = end
                .child(washed(
                    &row.value.resolve(catalog),
                    None,
                    false,
                    ink.wash14,
                    spacing,
                ))
                .child(check(row.state, ink, self.corner(), self.s()));
        } else if !row.action.is_empty() {
            end = end.child(
                div()
                    .flex()
                    .items_center()
                    .gap(self.px(8.))
                    .text_color(ink.foreground)
                    .child(mark("plus", self.s().of(12.), ink.foreground))
                    .child(SharedString::from(row.value.resolve(catalog))),
            );
        }
        let _ = cx;
        base.flex()
            .flex_col()
            .when(!first, |group| {
                group.child(div().mt(self.px(7.)).h(px(1.)).bg(ink.rule))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(self.px(PAD_X))
                    .pt(self.px(if first { 16. } else { 19. }))
                    .pb(px(self.s().line(7.8)))
                    .line_height(px(self.s().line(g::GROUP_LINE)))
                    .text_size(self.view.typeface.caption)
                    .text_color(ink.muted)
                    .child(
                        self.view
                            .heading(&self.text(row), run, line.lifted, ink)
                            .font_weight(FontWeight::BOLD),
                    )
                    .child(div().flex_1())
                    .child(end),
            )
    }

    fn role_ink(&self, role: Role) -> Hsla {
        match role {
            Role::Error => self.ink.error,
            Role::Accent => self.ink.accent,
            Role::Foreground => self.ink.foreground,
            _ => self.ink.muted,
        }
    }

    fn ruler(
        &self,
        base: gpui::Stateful<Div>,
        line: &Line<'_>,
        cx: &mut Context<SettingsView>,
    ) -> gpui::Stateful<Div> {
        let row = line.row;
        let ink = self.ink;
        let index = line.index.or(line.hit);
        let hit = line.hit;
        base.flex()
            .gap(self.px(2.))
            .pt(self.px(7.))
            .pl(px(self.s().label_x()))
            .pr(self.px(PAD_X))
            .text_size(self.view.typeface.caption)
            .children(row.stops.iter().enumerate().map(|(stop, size)| {
                let marked = stop == row.index;
                div()
                    .id(("settings-stop", stop))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .opacity(if marked { 1. } else { 0.4 })
                    .child(
                        div()
                            .h(self.px(18.))
                            .line_height(self.px(18.))
                            .text_color(if marked { ink.foreground } else { ink.muted })
                            .child(SharedString::from(size.to_string())),
                    )
                    .child(
                        div()
                            .w(self.px(if marked { 3. } else { 2. }))
                            .h(self.px(if marked { 5. } else { 3. }))
                            .bg(if row.on && marked {
                                ink.accent
                            } else {
                                ink.muted_frame
                            }),
                    )
                    .when_some(index.filter(|_| row.on), |tick, index| {
                        tick.cursor_pointer()
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.panel.press_shown(index, hit, Press::Pick(stop));
                                view.flush(cx);
                                cx.stop_propagation();
                            }))
                    })
            }))
    }

    fn hero(&self, base: gpui::Stateful<Div>, row: &Row) -> gpui::Stateful<Div> {
        let catalog = &self.view.panel.catalog;
        base.pt(self.px(14.))
            .flex()
            .flex_col()
            .items_center()
            .gap(self.px(GAP))
            .text_color(self.ink.foreground)
            .child(mark("flea", self.s().of(38.), self.ink.accent))
            .child(
                div()
                    .line_height(self.px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child(SharedString::from(row.label.resolve(catalog))),
            )
            .child(
                div()
                    .line_height(self.px(16.))
                    .text_size(self.view.typeface.caption)
                    .child(SharedString::from(row.value.resolve(catalog))),
            )
    }

    /// The mark slot every control row keeps, open where the board gives the row no mark.
    fn slot(&self, row: &Row) -> Div {
        let urgent = row.role == Role::Error;
        let ink = if urgent {
            self.ink.error
        } else {
            self.ink.muted_frame
        };
        let s = self.s();
        let drawn = match (row.mark, row.glyph) {
            ("", "") => None,
            ("", glyph) => Some(mark(glyph, s.of(g::ROW_MARK), ink)),
            ("flea", _) => Some(mark("flea", s.of(g::ROW_MARK), ink)),
            (brand, _) => Some(mark(brand, s.of(g::BRAND_MARK), ink)),
        };
        div()
            .w(self.px(MARK_SLOT))
            .h(self.px(g::ROW_MARK))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .children(drawn)
    }

    fn favourite(
        &self,
        base: gpui::Stateful<Div>,
        line: &Line<'_>,
        cx: &mut Context<SettingsView>,
    ) -> gpui::Stateful<Div> {
        let row = line.row;
        let ink = self.ink;
        let error = !row.error.is_empty();
        let shown = if row.display.is_empty() {
            row.value.resolve(&self.view.panel.catalog)
        } else {
            row.display.clone()
        };
        let index = line.index;
        let button = |id: &'static str, glyph: &'static str| {
            div()
                .id((id, index.unwrap_or(0)))
                .size(self.px(24.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(mark(glyph, self.s().of(19.), ink.muted_frame))
        };
        let width = self.view.panel.pane.0;
        base.flex()
            .items_center()
            .gap(self.px(GAP))
            .px(self.px(PAD_X))
            .child(
                div()
                    .w(self.px(MARK_SLOT))
                    .flex()
                    .justify_center()
                    .child(mark(
                        if row.glyph.is_empty() {
                            "folder"
                        } else {
                            row.glyph
                        },
                        self.s().of(g::ROW_MARK),
                        if error { ink.error } else { ink.muted_frame },
                    )),
            )
            .child(
                self.label(line, if error { ink.error } else { ink.foreground }, false)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden(),
            )
            .child(
                div()
                    .max_w(px(width * 0.46))
                    .text_size(self.view.typeface.caption)
                    .text_color(if error { ink.error } else { ink.muted })
                    .whitespace_nowrap()
                    .text_ellipsis_middle()
                    .child(SharedString::from(shown)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .child(self.favourite_control(
                        index,
                        &row.id,
                        "drag",
                        button("settings-grip", "list").when_some(index, |grip, index| {
                            // The press is held until the button comes up anywhere in the window, which counts the rows passed.
                            grip.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                                    view.panel.grip = Some((index, f32::from(event.position.y)));
                                    cx.stop_propagation();
                                }),
                            )
                        }),
                    ))
                    .child(self.favourite_control(
                        index,
                        &row.id,
                        "remove",
                        button("settings-remove", "x").when_some(index, |remove, index| {
                            remove.on_click(cx.listener(move |view, _, _, cx| {
                                view.panel.press(index, Press::Remove);
                                view.flush(cx);
                            }))
                        }),
                    )),
            )
    }

    /// A grip or remove control reports its centre to the settings reader only while its row shows it.
    fn favourite_control(
        &self,
        index: Option<usize>,
        row: &str,
        part: &str,
        control: gpui::Stateful<Div>,
    ) -> Div {
        let wrap = div().relative().child(control);
        match index {
            Some(_) => self.view.probe.attach(wrap, format!("{row}:{part}")),
            None => wrap,
        }
    }

    /// A label and its control: value, caption, segment or chevron, lock, box, in SettingsRow.qml's order.
    fn control(
        &self,
        base: gpui::Stateful<Div>,
        line: &Line<'_>,
        wrapped: bool,
        cx: &mut Context<SettingsView>,
    ) -> gpui::Stateful<Div> {
        let row = line.row;
        let ink = self.ink;
        let catalog = &self.view.panel.catalog;
        let urgent = row.role == Role::Error;
        let label_ink = if urgent {
            ink.error
        } else if matches!(row.kind, Kind::Lock | Kind::Fact) {
            ink.muted
        } else {
            ink.foreground
        };
        let segment = row.kind == Kind::Choice && line.segment && row.labels.len() > 1;
        let steps = row.kind == Kind::Choice && !segment;
        let width = self.view.panel.pane.0;
        let shows_value = matches!(row.kind, Kind::Fact | Kind::Action)
            || steps
            || (row.kind == Kind::Check && !row.value.is_empty());
        let value_ink = if row.role == Role::Accent {
            ink.accent
        } else if steps || row.role == Role::Live {
            ink.foreground
        } else {
            ink.muted
        };
        let index = line.index.or(line.hit);
        let hit = line.hit;
        let room = line.room.unwrap_or(g::Room {
            label: 0.,
            value: width * g::VALUE_SHARE,
            caption: (width * g::CAPTION_SHARE).round(),
        });
        let value = div()
            .max_w(px(room.value))
            .whitespace_nowrap()
            .text_color(value_ink)
            .map(|value| {
                if row.elide_head {
                    value.text_ellipsis_start()
                } else {
                    value.text_ellipsis()
                }
            })
            .child(SharedString::from(row.value.resolve(catalog)));
        base.flex()
            .items_center()
            .gap(self.px(GAP))
            .px(self.px(PAD_X))
            .child(self.slot(row))
            .child(
                self.label(line, label_ink, wrapped)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden(),
            )
            .when(shows_value, |line| line.child(value))
            .when(!row.caption.is_empty(), |line| {
                line.child(
                    div()
                        .max_w(px(room.caption))
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(self.view.typeface.caption)
                        .text_color(ink.muted)
                        .child(SharedString::from(row.caption.resolve(catalog))),
                )
            })
            .when(segment, |line| {
                line.child(self.segment(row, index, hit, cx))
            })
            .when(steps || (row.kind == Kind::Action && !row.inert), |line| {
                line.child(
                    div()
                        .id(("settings-chevron", index.unwrap_or(0)))
                        .child(mark(
                            crate::locale_geometry::trailing_chevron(
                                crate::locale_geometry::Direction::from_locale(catalog.locale())
                                    .is_rtl(),
                            ),
                            self.s().of(MARK_SLOT),
                            ink.muted_frame,
                        ))
                        .when_some(index.filter(|_| steps), |chevron, index| {
                            chevron.on_click(cx.listener(move |view, _, _, cx| {
                                view.panel.press_shown(index, hit, Press::Step);
                                view.flush(cx);
                                cx.stop_propagation();
                            }))
                        }),
                )
            })
            .when(row.kind == Kind::Lock, |line| {
                line.child(mark("lock", self.s().of(MARK_SLOT), ink.muted_frame))
            })
            .when(row.kind == Kind::Check, |line| {
                line.child(check(row_state(row), ink, self.corner(), self.s()))
            })
    }

    /// Words in a set wear the chip, the chosen one in foreground; View's four marks stand bare, as on the board.
    fn segment(
        &self,
        row: &Row,
        index: Option<usize>,
        hit: Option<usize>,
        cx: &mut Context<SettingsView>,
    ) -> Div {
        let ink = self.ink;
        let catalog = &self.view.panel.catalog;
        let glyphs = marks(row);
        let cells = row.labels.iter().enumerate().map(|(stop, label)| {
            let current = *label == row.value;
            let tone = if current {
                ink.foreground
            } else {
                ink.muted_frame
            };
            let cell = self
                .view
                .probe
                .attach(div(), format!("choice:{}:{stop}", row.id))
                .id(("settings-cell", stop))
                .flex_none();
            let cell = if glyphs {
                cell.child(mark(
                    row.values
                        .get(stop)
                        .map_or("file", |value| view_mark(value)),
                    self.s().of(g::VIEW_MARK),
                    tone,
                ))
            } else {
                cell.h(self.px(CHIP_H))
                    .min_w(self.px(CHIP_H))
                    .px(self.px(g::CHIP_PAD))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .rounded(px(self.corner()))
                    .border_color(tone)
                    .text_size(self.view.typeface.caption)
                    .text_color(if current { ink.foreground } else { ink.muted })
                    .child(SharedString::from(label.resolve(catalog)))
            };
            (cell, stop, current)
        });
        let mut out = div()
            .flex()
            .items_center()
            .gap(self.px(if glyphs { g::VIEW_MARK_GAP } else { CHIP_GAP }))
            .when(glyphs, |strip| strip.mr(self.px(GAP)).mt(self.px(-2.)));
        for (cell, stop, current) in cells {
            out = out.child(cell.when_some(index, |cell, index| {
                cell.when(!current, |cell| cell.cursor_pointer())
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !current {
                            view.panel.press_shown(index, hit, Press::Pick(stop));
                            view.flush(cx);
                        }
                        cx.stop_propagation();
                    }))
            }));
        }
        out
    }
}

/// The view a Last-used view value names, as its mark.
fn view_mark(value: &str) -> &'static str {
    match value {
        "columns" => "columns",
        "grid" => "grid",
        "dual" => "dual",
        _ => "list",
    }
}
