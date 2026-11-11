//! The HIG Print card uses the shared card frame and button grammar.
use super::*;
use crate::tui::{
    cells::{fit, text_width},
    render::cards::{self, Button, Card},
    theme::Theme,
};
// The body row holding the Duplex and Grayscale checkboxes, which mute while unsupported or queried.
const OPTION_ROW: usize = 6;

fn title(m: &Model) -> String {
    let form = m.printing.card.as_ref().unwrap();
    if m.menu_count > 1 {
        m.i18n
            .trn("dialogs.output.title.print_many", m.menu_count as i64)
    } else {
        let path = form
            .paths
            .first()
            .map(String::as_str)
            .unwrap_or_else(|| m.menu_path.to_str().filter(|p| !p.is_empty()).unwrap_or(""));
        let name = if path.is_empty() {
            m.rows.get(&m.cursor).map_or("", |r| r.name.as_str())
        } else {
            path.rsplit('/').next().unwrap_or(path)
        };
        m.i18n.tr_args("dialogs.output.title.print", &[name])
    }
}
pub(super) fn no_printer(form: &Form) -> bool {
    form.queried && form.destinations.is_empty()
}
fn labels(m: &Model) -> [Button; 2] {
    let f = m.printing.card.as_ref().unwrap();
    [
        Button {
            label: m.i18n.tr("dialogs.output.cancel"),
            default: false,
            focused: f.focus == 5,
            enabled: true,
        },
        Button {
            label: m.i18n.tr(if no_printer(f) {
                "dialogs.output.setup"
            } else {
                "dialogs.output.print"
            }),
            default: true,
            focused: f.focus == 6,
            enabled: f.queried
                && f.query.is_none()
                && f.error.is_empty()
                && m.printing.run.is_none(),
        },
    ]
}
pub(super) fn field_column(m: &Model, content: usize) -> usize {
    FIELD_COLUMN
        .max(text_width(&m.heading("dialogs.output.copies")) + 2)
        .max(text_width(&m.i18n.tr("dialogs.output.two_sided")) + 6)
        .min(content / 2)
}
type Part = (usize, String, bool, bool);
fn body(m: &Model) -> Vec<Vec<Part>> {
    let f = m.printing.card.as_ref().unwrap();
    let tr = |key: &str| m.i18n.tr(key);
    let column = field_column(m, cards::room(m.columns));
    if no_printer(f) {
        return vec![vec![(0, tr("dialogs.output.no_printer"), false, false)]];
    }
    let mut rows = vec![Vec::new(); 7];
    rows[0].push((0, m.heading("dialogs.output.printer"), true, false));
    let printer = if f.query.is_some() || (!f.queried && f.paths.is_empty()) {
        tr("menu.loading")
    } else {
        f.printer_name.clone()
    };
    rows[1].push((0, printer, false, true));
    if f.destinations
        .iter()
        .any(|d| d.name == f.printer && d.default)
    {
        rows[1].push((
            text_width(&f.printer_name) + 2,
            tr("dialogs.output.default"),
            true,
            true,
        ));
    }
    let chevron = rows[1]
        .iter()
        .map(|(x, s, _, _)| x + text_width(s))
        .max()
        .unwrap_or(0)
        + 2;
    rows[1].push((chevron, "⌄".into(), true, true));
    rows[3].push((0, m.heading("dialogs.output.copies"), true, false));
    rows[4].push((0, f.copies.clone(), false, true));
    if f.paged {
        rows[3].push((
            column,
            m.heading("dialogs.output.pages"),
            true,
            false,
        ));
        rows[4].push((
            column,
            if f.pages.is_empty() {
                tr("dialogs.output.all")
            } else {
                f.pages.clone()
            },
            f.pages.is_empty(),
            true,
        ));
    }
    rows[6].push((
        0,
        format!(
            "[{}] {}",
            if f.two_sided { "x" } else { " " },
            tr("dialogs.output.two_sided")
        ),
        !f.caps.duplex || f.query.is_some(),
        false,
    ));
    rows[6].push((
        column,
        format!(
            "[{}] {}",
            if f.grayscale { "x" } else { " " },
            tr("dialogs.output.grayscale")
        ),
        !f.caps.color || f.query.is_some(),
        false,
    ));
    if !f.error.is_empty() {
        rows.push(vec![(0, f.error.clone(), false, false)]);
    }
    rows
}
pub fn geometry(m: &Model) -> Card {
    let f = m.printing.card.as_ref().unwrap();
    let rows = body(m);
    let widths: Vec<_> = labels(m).iter().map(|b| text_width(&b.label) + 4).collect();
    let need = rows
        .iter()
        .flatten()
        .map(|(x, s, _, _)| x + text_width(s))
        .max()
        .unwrap_or(0)
        .max(text_width(&title(m)) + text_width(&m.i18n.tr("card.esc")) + 2)
        .max(widths.iter().sum::<usize>() + 2)
        .max(2 * field_column(m, cards::room(m.columns)))
        .max(MIN_CARD_WIDTH);
    cards::direction(
        m,
        cards::layout(
            m.columns,
            m.height + crate::tui::layout::CHROME_ROWS,
            need,
            rows.len(),
            &widths,
            f.scroll,
        ),
    )
}
pub fn draw(out: &mut String, m: &Model, theme: &Theme) {
    let f = m.printing.card.as_ref().unwrap();
    let card = geometry(m);
    cards::frame(out, m, theme, &card, &title(m));
    let base = theme.base();
    let column = field_column(m, cards::room(m.columns));
    for (i, row) in body(m).iter().skip(card.top).take(card.shown).enumerate() {
        for (x, text, muted, field) in row {
            if *x >= card.content() {
                continue;
            }
            let width = if *x == 0 && matches!(i + card.top, 3 | 4 | 6) {
                column.saturating_sub(2)
            } else {
                card.content() - x
            };
            let focused = match (i + card.top, *x) {
                (1, 0) => f.focus == 0,
                (4, 0) => f.focus == 1,
                (4, x) if x == column => f.focus == 2,
                (6, 0) => f.focus == 3,
                (6, x) if x == column => f.focus == 4,
                _ => false,
            };
            let ink = if *muted {
                &theme.muted
            } else if !f.error.is_empty() && i + card.top == 7 {
                &theme.error
            } else {
                &theme.foreground
            };
            let fill = if *field { &theme.field } else { "" };
            let enabled = i + card.top != OPTION_ROW || !*muted;
            let attribute = theme.depth.control_attribute(enabled, false, false);
            let mark = if focused { "\x1b[4m" } else { "" };
            let width = (text_width(text) + usize::from(*field)).min(width);
            let shown = cards::field_text(m, text, width);
            let shown = if *field {
                crate::tui::cells::isolate(
                    if i + card.top == 1 {
                        crate::tui::cells::Run::Name
                    } else {
                        crate::tui::cells::Run::Number
                    },
                    &shown,
                )
            } else {
                shown
            };
            out.push_str(&format!(
                "\x1b[{};{}H{base}{fill}{ink}{attribute}{mark}{}{base}",
                card.y + 4 + i,
                cards::column(m, &card, *x, width) + 1,
                shown
            ));
        }
    }
    cards::buttons(out, theme, &card, &labels(m));
    if f.dropdown && !f.destinations.is_empty() {
        let shown = f.destinations.len().min(card.shown.saturating_sub(2));
        let top = f.choice.saturating_sub(shown.saturating_sub(1));
        for (i, d) in f.destinations.iter().enumerate().skip(top).take(shown) {
            let selected = i == f.choice;
            let attribute = theme.depth.overlay_attribute(selected, false);
            let fill = if selected {
                &theme.cursor
            } else {
                &theme.background
            };
            let suffix = if d.default {
                format!("  {}", m.i18n.tr("dialogs.output.default"))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "\x1b[{};{}H{base}{fill}{attribute}{}{}{base}",
                card.y + 6 + i - top,
                card.left() + 1,
                theme.foreground,
                fit(&format!("{}{suffix}", d.name), card.content())
            ));
        }
    }
}
