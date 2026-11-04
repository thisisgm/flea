//! Multi-selection facts and the three kind marks drawn by the preview column.
use super::{fact_row, Subject};
use crate::g2::pv1::glyph::Glyph;
use flea::ui_logic_pv1::facts;
use gpui::{canvas, div, point, prelude::*, px, size, Bounds, Hsla, IntoElement};

pub(crate) fn rows(subject: &Subject) -> Vec<Option<facts::Row>> {
    subject
        .selected
        .iter()
        .map(|row| Some(fact_row(row)))
        .collect()
}

pub(super) fn stack(subject: &Subject, mark: f32, ink: Hsla) -> impl IntoElement {
    let marks = facts::multi_marks(&rows(subject));
    let across = (mark / 4.).round();
    let down = (mark / 11.).round();
    let steps = marks.len().saturating_sub(1) as f32;
    let width = mark + steps * across;
    let height = mark + steps * down;
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .debug_selector(|| "column-kind-stack".into())
                .w(px(width))
                .h(px(height))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, cx| {
                            for (index, icon) in marks.iter().enumerate().rev() {
                                let index = index as f32;
                                let at = bounds.origin
                                    + point(px((steps - index) * across), px(index * down));
                                Glyph::kind(icon, 0).paint(
                                    Bounds::new(at, size(px(mark), px(mark))),
                                    ink.opacity(1. - index * 0.3),
                                    window,
                                    cx,
                                );
                            }
                        },
                    )
                    .size_full(),
                ),
        )
}
