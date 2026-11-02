//! The stand-in's body blocks: Open With's list, bulk rename's names, the permission grid, the facts and every other part.
use super::*;
use gpui::{div, prelude::*, px, relative, AnyElement, Div};

impl Draw<'_> {
    /// Records the part's painted bounds under `name` when the card is measured, as Open With's readers ask.
    pub(super) fn track<E: ParentElement>(&self, element: E, name: &str) -> E {
        match self.measure {
            Some(frame) => frame.track(element, name.to_owned(), String::new()),
            None => element,
        }
    }

    fn names(
        &self,
        old_label: &str,
        old: &[String],
        new_label: &str,
        new: &[NameLine],
        focused: bool,
        error: &str,
    ) -> Div {
        let t = self.t;
        let line = || {
            self.text(BOARD_BODY)
                .h(self.px(NAME_LINE))
                .flex()
                .items_center()
        };
        let mut old_column = div()
            .flex()
            .flex_col()
            .py(self.px(3.))
            .border_t_1()
            .border_r_1()
            .border_b_1()
            .border_color(gpui::transparent_black());
        for name in old {
            old_column = old_column.child(line().text_color(t.muted).child(name.clone()));
        }
        let mut new_column = div()
            .flex()
            .flex_col()
            .py(self.px(3.))
            .px(self.px(BUTTON_PAD))
            .border_1()
            .border_color(if focused {
                t.accent_frame
            } else {
                t.muted_frame
            })
            .rounded(px(self.corner));
        for name in new {
            if let Some(input) = &name.input {
                let row = line().child(crate::g2::op1::field::mount(
                    input.clone(),
                    div().flex_1().min_w_0().h_full(),
                    self.typeface,
                ));
                new_column = new_column.child(if name.reason.is_empty() {
                    row
                } else {
                    row.child(self.text(CAPTION).child(name.reason.clone()))
                });
                continue;
            }
            let row = line().text_color(t.ink(name.ink)).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(name.text.clone()),
            );
            new_column = new_column.child(if name.reason.is_empty() {
                row
            } else {
                row.child(self.text(CAPTION).child(name.reason.clone()))
            });
        }
        let half = |label: &str, column: Div| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(self.label(label))
                .child(column)
        };
        let error = self
            .text(CAPTION)
            .h(self.px(ERROR_LINE))
            .line_height(self.px(ERROR_LINE))
            .mt(self.px(6.))
            .text_color(t.error)
            .child(error.to_string());
        div()
            .flex()
            .gap(self.px(GAP))
            .child(half(old_label, old_column))
            .child(half(new_label, new_column).child(error))
    }

    fn grid(&self, columns: &[String], rows: &[(String, [Check; 3])]) -> Div {
        let cell = || div().flex_1().flex().justify_center();
        let mut head = self
            .text(CAPTION)
            .h(self.px(GRID_HEAD))
            .flex()
            .items_center()
            .text_color(self.t.fg)
            .child(div().w(self.px(LABEL_COLUMN)).flex_none());
        for name in columns {
            head = head.child(cell().child(self.caption(name, GRID_HEAD_LINE, self.t.fg)));
        }
        let mut grid = div().flex().flex_col().child(head);
        for (row, (name, checks)) in rows.iter().enumerate() {
            let mut line = self
                .text(BOARD_BODY)
                .h(self.px(LINE_40))
                .flex()
                .items_center()
                .text_color(self.t.fg)
                .child(
                    div()
                        .w(self.px(LABEL_COLUMN))
                        .flex_none()
                        .child(name.clone()),
                );
            for (column, check) in checks.iter().enumerate() {
                let at = row * 3 + column;
                line = line.child(
                    cell().child(
                        self.wired(
                            Control::Check(at),
                            div()
                                .id(("op3-check", at))
                                .debug_selector(move || format!("op3-check-{at}"))
                                .child(self.check(*check)),
                        ),
                    ),
                );
            }
            grid = grid.child(line);
        }
        grid
    }

    pub(super) fn block(&self, block: &Block) -> AnyElement {
        let t = self.t;
        match block {
            Block::Field(field) => self
                .wired(
                    Control::Field,
                    self.track(self.field(field), "search")
                        .id("op3-field")
                        .debug_selector(|| "op3-field".into()),
                )
                .into_any_element(),
            Block::Fields(left, right) => div()
                .flex()
                .gap(self.px(GAP))
                .child(div().flex_1().min_w_0().child(self.field(left)))
                .child(div().flex_1().min_w_0().child(self.field(right)))
                .into_any_element(),
            Block::Names {
                old_label,
                old,
                new_label,
                new,
                focused,
                error,
            } => self
                .names(old_label, old, new_label, new, *focused, error)
                .into_any_element(),
            Block::Apps(rows, filler) => self.apps(rows, *filler).into_any_element(),
            Block::CheckLine { check, label } => self
                .wired(
                    Control::Check(0),
                    self.text(BOARD_BODY)
                        .mt(self.px(-4.))
                        .flex()
                        .items_center()
                        .gap(self.px(ROW_GAP))
                        .text_color(t.fg)
                        .child(self.check(*check))
                        .child(label.clone())
                        .id("op3-always")
                        .debug_selector(|| "op3-always".into()),
                )
                .into_any_element(),
            Block::Note {
                text,
                inset,
                ink,
                lift,
            } => self
                .track(div(), &format!("note:{text}"))
                .debug_selector({
                    let text = text.clone();
                    move || format!("op3-note:{text}")
                })
                .text_size(self.px(CAPTION))
                .line_height(relative(1.5))
                .mt(self.px(-*lift))
                .pl(self.px(*inset))
                .text_color(t.ink(*ink))
                .child(text.clone())
                .into_any_element(),
            Block::Subject {
                glyph: mark,
                name,
                kind,
            } => self
                .text(BOARD_BODY)
                .h(self.px(LINE_40))
                .flex()
                .items_center()
                .gap(self.px(ROW_GAP))
                .text_color(t.fg)
                .child(glyph(*mark, 19. * self.s, t.muted_frame))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(name.clone()),
                )
                .child(self.text(CAPTION).child(kind.clone()))
                .into_any_element(),
            Block::Scope(text) => self.scope(text).into_any_element(),
            Block::Program { glyph: mark, text } => self
                .text(BOARD_BODY)
                .flex()
                .items_center()
                .gap(self.px(ROW_GAP))
                .text_color(t.fg)
                .child(glyph(*mark, 19. * self.s, t.fg))
                .child(text.clone())
                .into_any_element(),
            Block::Rule { above, below } => self
                .hairline()
                .mt(self.px(*above))
                .mb(self.px(*below))
                .into_any_element(),
            Block::Grid { columns, rows } => self.grid(columns, rows).into_any_element(),
            Block::Info { label, value, tail } => self
                .text(BOARD_BODY)
                .h(self.px(LINE_40))
                .flex()
                .items_center()
                .gap(self.px(ROW_GAP))
                .text_color(t.fg)
                .child(
                    div()
                        .w(self.px(LABEL_COLUMN))
                        .flex_none()
                        .child(label.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(value.clone()),
                )
                .child(self.text(CAPTION).child(tail.clone()))
                .into_any_element(),
            Block::Section { head, lines } => {
                let mut section = div()
                    .flex()
                    .flex_col()
                    .text_size(self.px(CAPTION))
                    .text_color(t.fg)
                    .child(
                        div()
                            .line_height(self.px(18.))
                            .pb(self.px(3.))
                            .child(self.caption(head, 18., t.fg)),
                    );
                for (at, line) in lines.iter().enumerate() {
                    let height = if at + 1 == lines.len() { 20. } else { 16. };
                    section = section.child(div().line_height(self.px(height)).child(line.clone()));
                }
                section.into_any_element()
            }
            Block::Facts(facts) => self.facts(facts).into_any_element(),
            Block::Stack(blocks) => {
                let mut stack = div().flex().flex_col();
                for block in blocks {
                    stack = stack.child(self.block(block));
                }
                stack.into_any_element()
            }
        }
    }
}
