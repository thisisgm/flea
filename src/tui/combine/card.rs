//! Card drawing and pointer targets share one cell geometry.
#[path = "card_layout.rs"]
mod layout;
use super::*;
use crate::tui::{
    cells::{fit, text_width},
    render::cards::{self, Button, Card},
    theme::Theme,
};
use layout::{add, column_needs, splits};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Box {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}
impl Box {
    pub fn hit(&self, x: usize, y: usize) -> bool {
        x > self.x && y > self.y && x <= self.x + self.width && y <= self.y + self.height
    }
}
pub struct Geometry {
    pub card: Card,
    pub preview: Box,
    pub tiles: Vec<Box>,
    pub fields: Vec<Field>,
}
pub struct Field {
    pub focus: usize,
    pub choice: usize,
    pub rect: Box,
    pub text: String,
    pub chosen: bool,
    pub enabled: bool,
}
fn buttons(m: &Model) -> [Button; 2] {
    let f = m.combining.card.as_ref().unwrap();
    [
        Button {
            label: m.i18n.tr("dialogs.output.cancel"),
            default: false,
            focused: f.focus == 7,
            enabled: true,
        },
        Button {
            label: m.i18n.tr("dialogs.output.combine"),
            default: true,
            focused: f.focus == 8,
            enabled: !f.paths.is_empty(),
        },
    ]
}
const SETTING_GROUPS: usize = 3;
const SETTING_ROWS: usize = SETTING_GROUPS * 2;
fn field_rows(card: &Card, preview: usize) -> [usize; 3] {
    let gaps = card
        .shown
        .saturating_sub(preview + SETTING_ROWS)
        .min(SETTING_GROUPS);
    let first = preview + 1 + usize::from(gaps > 0);
    let second = first + 2 + usize::from(gaps > 1);
    [first, second, second + 2 + usize::from(gaps > 2)]
}
fn preview_rows(m: &Model) -> usize {
    (m.height + super::super::layout::CHROME_ROWS)
        .saturating_sub(20)
        .clamp(3, 24)
}
pub fn geometry(m: &Model) -> Geometry {
    let f = m.combining.card.as_ref().unwrap();
    let widths: Vec<_> = buttons(m)
        .iter()
        .map(|b| text_width(&b.label) + 4)
        .collect();
    let (left, middle, right) = column_needs(m);
    let need = (left + middle + right)
        .max(
            text_width(
                &m.i18n
                    .trn("dialogs.output.title.combine", m.menu_count as i64),
            ) + 2
                + text_width(&m.i18n.tr("card.esc")),
        )
        .max(widths.iter().sum::<usize>() + 2);
    let mut card = cards::direction(
        m,
        cards::layout(
            m.columns,
            m.height + super::super::layout::CHROME_ROWS,
            need,
            preview_rows(m) + 9,
            &widths,
            0,
        ),
    );
    let preview = Box {
        x: card.left(),
        y: card.y + 3,
        width: card.content(),
        height: preview_rows(m).min(card.shown.saturating_sub(SETTING_ROWS)),
    };
    let rows = field_rows(&card, preview.height);
    let focused = match f.focus {
        1..=3 => rows[0],
        4 | 5 => rows[1],
        6 => rows[2],
        _ => 0,
    };
    card.top = focused.saturating_add(1).saturating_sub(card.shown);
    let cell = m
        .preview_state
        .cell
        .unwrap_or(super::super::graphics::FALLBACK_CELL);
    let tiles = f
        .composition()
        .map(|c| {
            c.fitted(
                preview.width.saturating_sub(2) as f64 * cell.0 as f64,
                preview.height.saturating_sub(2) as f64 * cell.1 as f64,
                0.,
            )
        })
        .map(|c| {
            c.tiles
                .into_iter()
                .map(|r| {
                    let x = (r.x / cell.0 as f64).floor() as usize;
                    let y = (r.y / cell.1 as f64).floor() as usize;
                    Box {
                        x: preview.x + 1 + x,
                        y: preview.y + 1 + y,
                        width: ((r.x + r.width) / cell.0 as f64).ceil() as usize - x,
                        height: ((r.y + r.height) / cell.1 as f64).ceil() as usize - y,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let mut g = Geometry {
        card,
        preview,
        tiles,
        fields: Vec::new(),
    };
    let [row, spacing_row, name_row] = rows;
    let (split, format) = splits(m, g.card.content());
    add(
        m,
        &mut g,
        1,
        row,
        0,
        split,
        &["Row", "Column", "Grid"],
        layout_name(f.settings.layout),
        true,
    );
    add(
        m,
        &mut g,
        2,
        row,
        split,
        format,
        &[&f.columns],
        &f.columns,
        f.settings.layout == Layout::Grid,
    );
    add(
        m,
        &mut g,
        3,
        row,
        format,
        usize::MAX,
        &["png", "jpg", "webp"],
        &f.format,
        true,
    );
    add(
        m,
        &mut g,
        4,
        spacing_row,
        0,
        split,
        &["None", "Small", "Large"],
        spacing_name(f.settings.spacing),
        true,
    );
    let backgrounds: &[&str] = if f.format == "jpg" {
        &["White", "Black"]
    } else {
        &["Transparent", "White", "Black"]
    };
    add(
        m,
        &mut g,
        5,
        spacing_row,
        split,
        usize::MAX,
        backgrounds,
        background_name(f.settings.background),
        f.background_enabled(),
    );
    add(
        m,
        &mut g,
        6,
        name_row,
        0,
        usize::MAX,
        &[&f.name],
        &f.name,
        true,
    );
    if cards::rtl(m) {
        for field in &mut g.fields {
            field.rect.x =
                cards::column(m, &g.card, field.rect.x - g.card.left(), field.rect.width);
        }
    }
    g
}
fn outline(out: &mut String, b: Box, ink: &str, base: &str) {
    if b.width < 2 || b.height < 2 {
        return;
    }
    out.push_str(&format!(
        "\x1b[{};{}H{ink}┌{}┐",
        b.y + 1,
        b.x + 1,
        "─".repeat(b.width - 2)
    ));
    for y in b.y + 1..b.y + b.height - 1 {
        out.push_str(&format!(
            "\x1b[{};{}H{ink}│\x1b[{};{}H│",
            y + 1,
            b.x + 1,
            y + 1,
            b.x + b.width
        ));
    }
    out.push_str(&format!(
        "\x1b[{};{}H{ink}└{}┘{base}",
        b.y + b.height,
        b.x + 1,
        "─".repeat(b.width - 2)
    ));
}
pub fn draw(out: &mut String, m: &Model, theme: &Theme) {
    let f = m.combining.card.as_ref().unwrap();
    let g = geometry(m);
    let title = m
        .i18n
        .trn("dialogs.output.title.combine", m.menu_count as i64);
    cards::frame(out, m, theme, &g.card, &title);
    let base = theme.base();
    outline(out, g.preview, &theme.frame, &base);
    if m.capabilities.graphics == super::super::graphics::Protocol::None {
        for (i, rect) in g.tiles.iter().enumerate() {
            let attribute = theme
                .depth
                .overlay_attribute(f.focus == 0 && f.image == i, false);
            let ink = if f.focus == 0 && f.image == i {
                &theme.foreground
            } else {
                &theme.frame
            };
            outline(out, *rect, &format!("{attribute}{ink}"), &base);
            let number = m.i18n.locale().format_count((i + 1) as i64);
            if rect.width > 0 && rect.height > 0 {
                out.push_str(&format!(
                    "\x1b[{};{}H{}{attribute}{number}{base}",
                    rect.y + 1,
                    rect.x + 1,
                    theme.muted
                ));
            }
        }
    }
    let (split, format) = splits(m, g.card.content());
    let [row, spacing_row, name_row] = field_rows(&g.card, g.preview.height);
    for (r, x, key, enabled, end) in [
        (row - 1, 0, "layout", true, split),
        (
            row - 1,
            split,
            "columns",
            f.settings.layout == Layout::Grid,
            format,
        ),
        (row - 1, format, "format", true, g.card.content()),
        (spacing_row - 1, 0, "spacing", true, split),
        (
            spacing_row - 1,
            split,
            "background",
            f.background_enabled(),
            g.card.content(),
        ),
        (name_row - 1, 0, "name", true, g.card.content()),
    ] {
        if r >= g.card.top && r < g.card.top + g.card.shown {
            let label = m.i18n.tr(&format!("dialogs.output.{key}")).to_uppercase();
            out.push_str(&format!(
                "\x1b[{};{}H{}{}{}{base}",
                g.card.y + 4 + r - g.card.top,
                cards::column(m, &g.card, x, end.saturating_sub(x)) + 1,
                theme.depth.control_attribute(enabled, false, false),
                if enabled {
                    &theme.muted
                } else {
                    &theme.unavailable_word
                },
                cards::field_text(m, &label, end.saturating_sub(x))
            ));
        }
    }
    for field in &g.fields {
        let attribute = theme.depth.control_attribute(
            field.enabled,
            field.enabled && f.focus == field.focus && field.chosen,
            field.chosen,
        );
        let ink = if !field.enabled {
            &theme.unavailable_word
        } else if field.chosen {
            &theme.foreground
        } else {
            &theme.muted
        };
        out.push_str(&format!(
            "\x1b[{};{}H{base}{attribute}{}{}{}{}{base}",
            field.rect.y + 1,
            field.rect.x + 1,
            if matches!(field.focus, 2 | 6) {
                &theme.field
            } else {
                ""
            },
            ink,
            if f.focus == field.focus && field.chosen {
                "\x1b[4m"
            } else {
                ""
            },
            if matches!(field.focus, 2 | 6) {
                crate::tui::cells::isolate(
                    if field.focus == 2 {
                        crate::tui::cells::Run::Number
                    } else {
                        crate::tui::cells::Run::Name
                    },
                    &cards::field_text(m, &field.text, field.rect.width),
                )
            } else {
                cards::field_text(m, &field.text, field.rect.width)
            }
        ));
    }
    if !f.error.is_empty() {
        out.push_str(&format!(
            "\x1b[{};{}H{}{}{base}",
            g.card.y + 4,
            g.card.left() + 1,
            theme.error,
            fit(&f.error, g.card.content())
        ));
    }
    cards::buttons(out, theme, &g.card, &buttons(m));
}
