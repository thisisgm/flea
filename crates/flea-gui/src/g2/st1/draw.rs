//! One Settings pane row per kind, as ui/SettingsRow.qml draws it at the settings boards' measures.
use super::geometry::{self as g, CHECK, CHIP_GAP, CHIP_H, GAP, LABEL_X, MARK_SLOT, PAD_X};
use super::marks::{mark, washed};
use super::panel::Press;
use super::seams::Ink;
use super::view::SettingsView;
use flea::ui_logic_st1::{
    rows::{Kind, Master, Role, Row},
    search::Run,
    visible::marks,
};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent,
    SharedString,
};

mod hint;

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
        }
    }
}

/// The checkbox every surface draws: empty against filled, the tick and the mixed bar cut in the surface.
pub fn check(state: Master, ink: &Ink, corner: f32) -> Div {
    let filled = matches!(state, Master::All | Master::Some);
    div()
        .size(px(CHECK))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_2()
        .rounded(px(corner / 2.))
        .border_color(if filled {
            ink.foreground
        } else {
            ink.muted_frame
        })
        .when(filled, |box_| box_.bg(ink.foreground))
        .when(state == Master::All, |box_| {
            box_.child(mark("checkmark-bold", 12., ink.surface))
        })
        .when(state == Master::Some, |box_| {
            box_.child(div().w(px(8.)).h(px(2.)).bg(ink.surface))
        })
}

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

    fn label(&self, line: &Line<'_>, ink: Hsla) -> Div {
        let run = line.run.map(|run| (run.start, run.length));
        washed(&self.text(line.row), run, line.lifted, self.ink.wash14, 0.).text_color(ink)
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
        let height = self.heights.get(at).copied().unwrap_or(g::ROW_H);
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
            Kind::KeyPreview => self.key_preview(base, row),
            Kind::Hero => self.hero(base, row),
            Kind::Favourite => self.favourite(base, line, cx),
            _ => self.control(base, line, cx),
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
        let mut end = div().h(px(14.)).flex().items_center().gap(px(GAP));
        if let Some(section) = &line.section {
            end = end.child(SharedString::from(section.clone()));
        } else if row.master {
            let spacing = f32::from(self.view.typeface.caption) * 0.14;
            end = end
                .child(washed(
                    &row.value.resolve(catalog),
                    None,
                    false,
                    ink.wash14,
                    spacing,
                ))
                .child(check(row.state, ink, self.corner()));
        } else if !row.action.is_empty() {
            end = end.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(ink.foreground)
                    .child(mark("plus", 12., ink.foreground))
                    .child(SharedString::from(row.value.resolve(catalog))),
            );
        }
        let _ = cx;
        base.flex()
            .flex_col()
            .when(!first, |group| {
                group.child(div().mt(px(7.)).h(px(1.)).bg(ink.rule))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(px(PAD_X))
                    .pt(px(if first { 16. } else { 19. }))
                    .pb(px(7.8))
                    .line_height(px(g::GROUP_LINE))
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
            .gap(px(2.))
            .pt(px(7.))
            .pl(px(LABEL_X))
            .pr(px(PAD_X))
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
                            .h(px(18.))
                            .line_height(px(18.))
                            .text_color(if marked { ink.foreground } else { ink.muted })
                            .child(SharedString::from(size.to_string())),
                    )
                    .child(
                        div()
                            .w(px(if marked { 3. } else { 2. }))
                            .h(px(if marked { 5. } else { 3. }))
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

    fn key_preview(&self, base: gpui::Stateful<Div>, row: &Row) -> gpui::Stateful<Div> {
        let ink = self.ink;
        let cell = (self.view.panel.pane.0 - 2. * PAD_X - GAP) / 2.;
        base.flex()
            .flex_wrap()
            .gap(px(GAP))
            .py(px(7.))
            .px(px(PAD_X))
            .text_size(self.view.typeface.caption)
            .children(row.items.iter().map(|item| {
                div()
                    .w(px(cell))
                    .h(px(CHIP_H))
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .child(
                        div()
                            .w(px(48.))
                            .mr(px(5.))
                            .flex()
                            .justify_end()
                            .text_color(ink.foreground)
                            .child(SharedString::from(item.keys.clone())),
                    )
                    .when(!item.glyph.is_empty(), |cell| {
                        cell.child(mark(item.glyph, 16., ink.muted_frame))
                    })
                    .child(div().whitespace_nowrap().text_color(ink.foreground).child(
                        SharedString::from(item.label.resolve(&self.view.panel.catalog)),
                    ))
            }))
    }

    fn hero(&self, base: gpui::Stateful<Div>, row: &Row) -> gpui::Stateful<Div> {
        let catalog = &self.view.panel.catalog;
        base.pt(px(14.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(GAP))
            .text_color(self.ink.foreground)
            .child(mark("flea", 38., self.ink.accent))
            .child(
                div()
                    .line_height(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child(SharedString::from(row.label.resolve(catalog))),
            )
            .child(
                div()
                    .line_height(px(16.))
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
        let drawn = match (row.mark, row.glyph) {
            ("", "") => None,
            ("", glyph) => Some(mark(glyph, g::ROW_MARK, ink)),
            ("flea", _) => Some(mark("flea", g::ROW_MARK, ink)),
            (brand, _) => Some(mark(brand, g::BRAND_MARK, ink)),
        };
        div()
            .w(px(MARK_SLOT))
            .h(px(g::ROW_MARK))
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
                .size(px(24.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(mark(glyph, 19., ink.muted_frame))
        };
        let width = self.view.panel.pane.0;
        base.flex()
            .items_center()
            .gap(px(GAP))
            .px(px(PAD_X))
            .child(div().w(px(MARK_SLOT)).flex().justify_center().child(mark(
                if row.glyph.is_empty() {
                    "folder"
                } else {
                    row.glyph
                },
                g::ROW_MARK,
                if error { ink.error } else { ink.muted_frame },
            )))
            .child(
                self.label(line, if error { ink.error } else { ink.foreground })
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
        let value = div()
            .max_w(px(width * 0.56))
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
            .gap(px(GAP))
            .px(px(PAD_X))
            .child(self.slot(row))
            .child(
                self.label(line, label_ink)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden(),
            )
            .when(shows_value, |line| line.child(value))
            .when(!row.caption.is_empty(), |line| {
                line.child(
                    div()
                        .max_w(px((width * 0.5).round()))
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
                        .child(mark("chevron-right", MARK_SLOT, ink.muted_frame))
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
                line.child(mark("lock", MARK_SLOT, ink.muted_frame))
            })
            .when(row.kind == Kind::Check, |line| {
                line.child(check(row_state(row), ink, self.corner()))
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
                    16.,
                    tone,
                ))
            } else {
                cell.h(px(CHIP_H))
                    .min_w(px(CHIP_H))
                    .px(px(7.))
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
            .gap(px(if glyphs { 17. } else { CHIP_GAP }))
            .when(glyphs, |strip| strip.mr(px(GAP)).mt(px(-2.)));
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
