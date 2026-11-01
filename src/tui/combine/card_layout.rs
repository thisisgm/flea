//! Measured translated segments share their paint and pointer intervals.
use super::*;

pub(super) fn column_needs(m: &Model) -> (usize, usize, usize) {
    let label = |key: &str| text_width(&m.i18n.tr(&format!("dialogs.output.{key}")).to_uppercase());
    let choices = |names: &[&str]| {
        names
            .iter()
            .map(|name| text_width(&m.i18n.tr(&format!("dialogs.output.choice.{name}"))))
            .sum::<usize>()
            + 2 * names.len().saturating_sub(1)
    };
    (
        21.max(label("layout"))
            .max(label("spacing"))
            .max(choices(&["Row", "Column", "Grid"]) + 2)
            .max(choices(&["None", "Small", "Large"]) + 2),
        10.max(label("columns") + 2),
        35.max(label("format"))
            .max(choices(&["png", "jpg", "webp"]))
            .max(label("background").saturating_sub(10))
            .max(choices(&["Transparent", "White", "Black"]).saturating_sub(10)),
    )
}
pub(super) fn splits(m: &Model, content: usize) -> (usize, usize) {
    let (left, middle, right) = column_needs(m);
    let total = left + middle + right;
    (content * left / total, content * (left + middle) / total)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn add(
    m: &Model,
    g: &mut Geometry,
    focus: usize,
    row: usize,
    start: usize,
    end: usize,
    choices: &[&str],
    chosen: &str,
    enabled: bool,
) {
    if row < g.card.top || row >= g.card.top + g.card.shown {
        return;
    }
    let mut x = start;
    for (choice, text) in choices.iter().enumerate() {
        let text = if matches!(focus, 2 | 6) {
            (*text).to_owned()
        } else {
            m.i18n.tr(&format!("dialogs.output.choice.{text}"))
        };
        let remaining = choices.len() - choice;
        let available = end.min(g.card.content()).saturating_sub(x);
        let share = available.saturating_sub(2 * remaining.saturating_sub(1)) / remaining;
        let width = (text_width(&text) + usize::from(matches!(focus, 2 | 6))).min(share);
        if width > 0 {
            g.fields.push(Field {
                focus,
                choice,
                rect: Box {
                    x: g.card.left() + x,
                    y: g.card.y + 3 + row - g.card.top,
                    width,
                    height: 1,
                },
                text,
                chosen: choices[choice] == chosen,
                enabled,
            });
        }
        x += width + 2;
    }
}
