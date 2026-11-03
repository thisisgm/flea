//! Convert's format cursor, option row and output line follow the supplied cell boards.
use super::*;
use crate::tui::{
    cells::{fit, text_width, Wrapped},
    render::cards::{self, Button, Card},
    theme::Theme,
};

pub fn title(m: &Model) -> String {
    let f = m.converting.card.as_ref().unwrap();
    if m.menu_count > 1 {
        return m
            .i18n
            .trn("dialogs.output.title.convert_many", m.menu_count as i64);
    }
    let path = f
        .paths
        .first()
        .map(String::as_str)
        .unwrap_or_else(|| m.menu_path.to_str().unwrap_or(""));
    m.i18n.tr_args(
        if f.kind == Kind::Office {
            "dialogs.output.title.office"
        } else {
            "dialogs.output.title.convert"
        },
        &[path.rsplit('/').next().unwrap_or(path)],
    )
}
fn labels(m: &Model) -> [Button; 2] {
    let f = m.converting.card.as_ref().unwrap();
    [
        Button {
            label: m.i18n.tr("dialogs.output.cancel"),
            default: false,
            focused: f.focus == 2,
            enabled: true,
        },
        Button {
            label: m.i18n.tr("dialogs.output.convert"),
            default: true,
            focused: f.focus == 3,
            enabled: !f.paths.is_empty()
                && f.error.is_empty()
                && (f.kind != Kind::Pdf || f.count.is_some()),
        },
    ]
}
pub fn option_row(f: &Form) -> usize {
    f.kind.formats().len() + 1
}
pub fn output_paths(f: &Form) -> Vec<String> {
    let Some(first) = f.paths.first() else {
        return Vec::new();
    };
    let path = Path::new(first);
    let parent = path.parent().unwrap_or(Path::new("/"));
    if f.kind == Kind::Pdf {
        let Some(pages) = f
            .count
            .and_then(|n| Pages::parse(&f.pages).ok()?.selected(n).ok())
        else {
            return Vec::new();
        };
        let mut result = vec![parent
            .join(output::page_name(path, pages[0], f.format()))
            .to_string_lossy()
            .into_owned()];
        if pages.len() > 1 {
            result.push(
                parent
                    .join(output::page_name(path, *pages.last().unwrap(), f.format()))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        result
    } else {
        let sources = if f.combine_option() && f.combine {
            &f.paths[..1]
        } else {
            &f.paths
        };
        sources
            .iter()
            .map(|source| {
                parent
                    .join(output::converted_name(Path::new(source), f.format()))
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }
}
struct Line {
    text: String,
    kind: u8,
}
fn body(m: &Model, width: usize) -> Vec<Line> {
    let f = m.converting.card.as_ref().unwrap();
    let mut rows = Vec::new();
    if f.kind != Kind::Office {
        for format in f.kind.formats() {
            rows.push(Line {
                text: m.i18n.tr(&format!("dialogs.output.choice.{format}")),
                kind: 1,
            });
        }
        rows.push(Line {
            text: String::new(),
            kind: 2,
        });
        if f.kind == Kind::Pdf {
            rows.push(Line {
                text: m.heading("dialogs.output.pages"),
                kind: 3,
            });
            rows.push(Line {
                text: if f.pages.is_empty() {
                    m.i18n.tr("dialogs.output.all")
                } else {
                    f.pages.clone()
                },
                kind: 4,
            });
        } else {
            rows.push(Line {
                text: m.i18n.tr(if f.combine_option() {
                    "dialogs.output.combine_pdf"
                } else {
                    "dialogs.output.metadata"
                }),
                kind: 5,
            });
        }
        rows.push(Line {
            text: String::new(),
            kind: 0,
        });
    }
    let output = m.i18n.tr("dialogs.output.output");
    let lead = text_width(&output).min(width.saturating_sub(1));
    let paths = output_paths(f);
    let range = if f.kind == Kind::Pdf && paths.len() == 2 {
        vec![
            paths[0].clone(),
            m.i18n
                .tr_args("dialogs.output.output_range", &["", &last_name(&paths[1])])
                .trim_start()
                .to_owned(),
        ]
    } else {
        paths
    };
    for (i, line) in range
        .iter()
        .flat_map(|p| Wrapped::new(p, width.saturating_sub(lead).max(1)))
        .enumerate()
    {
        rows.push(Line {
            text: line.into(),
            kind: if i == 0 { 6 } else { 8 },
        });
    }
    if !f.error.is_empty() {
        for text in Wrapped::new(&f.error, width) {
            rows.push(Line {
                text: text.into(),
                kind: 7,
            });
        }
    }
    rows
}
// The range's last file goes by its name alone, under the first path, as the convert-pdf board prints it.
fn last_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
pub fn geometry(m: &Model) -> Card {
    let f = m.converting.card.as_ref().unwrap();
    let widths: Vec<_> = labels(m).iter().map(|b| text_width(&b.label) + 4).collect();
    let need = (text_width(&title(m)) + text_width(&m.i18n.tr("card.esc")) + 2)
        .max(50)
        .max(
            f.kind
                .formats()
                .iter()
                .map(|format| {
                    text_width(&m.i18n.tr(&format!("dialogs.output.choice.{format}"))) + 3
                })
                .max()
                .unwrap_or(0),
        )
        .max(
            text_width(&m.i18n.tr(if f.combine_option() {
                "dialogs.output.combine_pdf"
            } else {
                "dialogs.output.metadata"
            })) + 4,
        )
        .max(widths.iter().sum::<usize>() + 2);
    let rows = body(m, need.min(cards::room(m.columns)));
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
    let f = m.converting.card.as_ref().unwrap();
    let card = geometry(m);
    cards::frame(out, m, theme, &card, &title(m));
    let base = theme.base();
    for (i, row) in body(m, card.content())
        .iter()
        .enumerate()
        .skip(card.top)
        .take(card.shown)
    {
        // A format row's wash spans the card's air and its ▎ edge takes the first air cell, so its mark sits on Output's column as the boards draw it.
        let column = if row.kind == 1 {
            card.left() + 1 - card.air
        } else {
            card.left() + 1
        };
        let at = format!("\x1b[{};{}H", card.y + 4 + i - card.top, column);
        out.push_str(&format!("{at}{base}"));
        match row.kind {
            1 => {
                let chosen = i == f.choice;
                let mark = if f.kind.formats()[i] == "pdf" {
                    "\u{eaeb}"
                } else {
                    "\u{eaea}"
                };
                let fill = if chosen {
                    &theme.cursor
                } else {
                    &theme.background
                };
                let ink = if chosen {
                    &theme.foreground
                } else {
                    &theme.frame
                };
                let edge = match (chosen && f.focus == 0, cards::rtl(m)) {
                    (false, _) => " ",
                    (true, false) => "▎",
                    (true, true) => "▕",
                };
                let lead = card.air.saturating_sub(1);
                if cards::rtl(m) {
                    out.push_str(&format!(
                        "{fill}{}{} {ink}{}{}{}{edge}",
                        theme.foreground,
                        cards::field_text(
                            m,
                            &row.text,
                            (card.content() + 2 * card.air).saturating_sub(3 + lead)
                        ),
                        if m.marks() { mark } else { " " },
                        " ".repeat(lead),
                        theme.accent
                    ));
                } else {
                    out.push_str(&format!(
                        "{fill}{}{edge}{}{ink}{} {}{}",
                        theme.accent,
                        " ".repeat(lead),
                        if m.marks() { mark } else { " " },
                        theme.foreground,
                        fit(
                            &row.text,
                            (card.content() + 2 * card.air).saturating_sub(3 + lead)
                        )
                    ));
                }
            }
            2 => out.push_str(&format!("{}{}", theme.rule, "─".repeat(card.content()))),
            3 => out.push_str(&format!(
                "{}{}",
                theme.muted,
                cards::field_text(m, &row.text, card.content())
            )),
            4 => out.push_str(&format!(
                "\x1b[{};{}H{}{}{}{}",
                card.y + 4 + i - card.top,
                cards::column(m, &card, 0, (text_width(&row.text) + 1).min(card.content())) + 1,
                theme.field,
                if f.pages.is_empty() {
                    &theme.muted
                } else {
                    &theme.foreground
                },
                if f.focus == 1 { "\x1b[4m" } else { "" },
                crate::tui::cells::isolate(
                    crate::tui::cells::Run::Number,
                    &cards::field_text(
                        m,
                        &row.text,
                        (text_width(&row.text) + 1).min(card.content())
                    )
                )
            )),
            5 => {
                let checked = if f.combine_option() {
                    f.combine
                } else {
                    f.strip
                };
                let ink = if f.option_enabled() {
                    &theme.foreground
                } else {
                    &theme.muted
                };
                if cards::rtl(m) {
                    let text = cards::field_text(m, &row.text, card.content().saturating_sub(4));
                    let label = text.trim_start();
                    // The underline marks the label alone, as in the left-to-right row.
                    out.push_str(&format!(
                        "{ink}{}{}{label}\x1b[24m{} [{}]",
                        &text[..text.len() - label.len()],
                        if f.focus == 1 { "\x1b[4m" } else { "" },
                        if f.option_enabled() {
                            &theme.frame
                        } else {
                            &theme.muted
                        },
                        if checked { "x" } else { " " }
                    ));
                } else {
                    out.push_str(&format!(
                        "{}[{}] {ink}{}{}",
                        if f.option_enabled() {
                            &theme.frame
                        } else {
                            &theme.muted
                        },
                        if checked { "x" } else { " " },
                        if f.focus == 1 { "\x1b[4m" } else { "" },
                        fit(&row.text, card.content().saturating_sub(4))
                    ));
                }
            }
            6 | 8 => {
                let output = m.i18n.tr("dialogs.output.output");
                let lead = text_width(&output).min(card.content().saturating_sub(1));
                let prefix = if row.kind == 6 {
                    output
                } else {
                    " ".repeat(lead)
                };
                if cards::rtl(m) {
                    out.push_str(&format!(
                        "{}{}{}{}",
                        theme.foreground,
                        crate::tui::cells::isolate(
                            crate::tui::cells::Run::Name,
                            &cards::field_text(m, &row.text, card.content().saturating_sub(lead))
                        ),
                        theme.muted,
                        // Trimmed so the catalog's trailing separator pads between the name and the label, not at the border.
                        cards::field_text(m, prefix.trim_end(), lead)
                    ));
                } else {
                    out.push_str(&format!(
                        "{}{}{base}{}{}",
                        theme.muted,
                        fit(&prefix, lead),
                        theme.foreground,
                        crate::tui::cells::isolate(
                            crate::tui::cells::Run::Name,
                            &fit(&row.text, card.content().saturating_sub(lead))
                        )
                    ));
                }
            }
            7 => out.push_str(&format!(
                "{}{}",
                theme.error,
                fit(&row.text, card.content())
            )),
            _ => {}
        }
        out.push_str(&base);
    }
    cards::buttons(out, theme, &card, &labels(m));
}
