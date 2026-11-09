//! Facts use the listing, the meta reply and the mounted view's counts, as Facts.js reads them.
use super::selection_preview::*;
use flea::ui_logic_preview_core::{facts, kinds::State};

#[cfg(test)]
#[path = "muted_ink_tests.rs"]
mod muted_ink_tests;

// cols-preview-video.html, drawn at base and body 14 and caption 12; each value scales with its text size.
const BOARD_BASE: f32 = 14.;
const BOARD_BODY: f32 = 14.;
const BOARD_CAPTION: f32 = 12.;
/// The state mark, at base 14.
const STATE_MARK: f32 = 40.;
/// A fact row's pitch, as a multiple of the caption size (18 px at caption 12).
const FACT_PITCH: f32 = 1.5;
/// The name's line, at body 14.
const NAME_LINE: f32 = 20.;
/// A fact's text line and its label column, at caption 12.
const FACT_TEXT: f32 = 16.;
const FACT_LABEL: f32 = 71.;
/// FactsTable.qml's row gap, three hairlines.
const FACT_GAP: f32 = 3.;

/// The column's state mark, scaled with the text size.
pub(crate) fn state_mark_size(face: &crate::list::Typeface) -> f32 {
    (STATE_MARK * f32::from(face.base) / BOARD_BASE).round()
}

/// FactsTable.qml: a fact's name in the muted caption beside its value in the foreground.
pub(crate) fn label_ink(roles: &crate::list::Palette) -> gpui::Hsla {
    roles.caption(false)
}

/// PreviewFactsLine.qml: a tall kind's one line, the Kind and its tail, in the foreground.
pub(crate) fn line_ink(roles: &crate::list::Palette) -> gpui::Hsla {
    roles.foreground
}

impl SelectionPreview {
    /// cols-preview-video.html: an 18 px fact row at caption 12, the name's 20 px line at body 14.
    pub(crate) fn fact_row(&self) -> (f32, f32) {
        (
            (f32::from(self.face.caption) * FACT_PITCH).round(),
            (f32::from(self.face.body) * NAME_LINE / BOARD_BODY).round(),
        )
    }

    /// The board's 16 px text line inside a fact row, its 71 px label column, and FactsTable.qml's hairline * 3 row gap.
    /// A 3D model takes cols-preview-3mf.html's wider column, as wide as Layer height and its 15 px gap.
    pub(crate) fn fact_grid(&self, kind: Kind) -> (f32, f32, f32) {
        let caption = f32::from(self.face.caption);
        let label = if kind == Kind::Model {
            crate::g2::model_preview::parts::LABEL_COLUMN
        } else {
            FACT_LABEL
        };
        (
            (caption * FACT_TEXT / BOARD_CAPTION).round(),
            (caption * label / BOARD_CAPTION).round(),
            FACT_GAP,
        )
    }

    /// The board's label column, widened to the widest label and a space when one passes it, at most half the table and never under the board's.
    pub(crate) fn label_column(
        &self,
        kind: Kind,
        facts: &[(String, String)],
        table: f32,
        window: &gpui::Window,
    ) -> f32 {
        let measure = |text: &str| {
            let font = self.face.font.clone();
            crate::g2::status_feedback::caption::measure(
                text.to_owned().into(),
                font,
                self.face.caption,
                gpui::px(0.),
                window,
            )
        };
        let board = self.fact_grid(kind).1;
        let widest = facts
            .iter()
            .map(|(label, _)| measure(label))
            .fold(0., f32::max);
        if widest <= board {
            return board;
        }
        (widest + measure(" "))
            .ceil()
            .min((table / 2.).floor())
            .max(board)
    }

    /// Facts.js tallHeight for a tall kind, else the 16 by 10 frame every other state shares.
    pub(crate) fn frame_size(&self, kind: Kind, facts: usize) -> (f32, f32) {
        let metrics = crate::list::Metrics::for_typeface(&self.face);
        let width = (self.width - 2. * metrics.pad).max(1.);
        let (row, name) = self.fact_row();
        let height = if kind.tall() {
            facts::tall_height(
                self.height as f64,
                metrics.pad as f64,
                name as f64,
                (row * facts as f32 + self.fact_grid(kind).2 * facts.saturating_sub(1) as f32)
                    as f64,
                metrics.gap as f64,
            )
            .max(1.) as f32
        } else {
            (width * 10. / 16.).round()
        };
        (width, height)
    }

    /// The state Facts.js draws: Error only for PreviewColumn.qml's failure; a picture or model that failed keeps its kind.
    pub(crate) fn state(&self, frame: &Frame, cx: &gpui::App) -> State {
        if self.loading_frame(frame) {
            State::Loading
        } else if self.failure(frame, cx).is_some() {
            State::Error
        } else {
            frame.kind.state()
        }
    }

    /// PreviewColumn.qml failure: the mounted view's sentence, an unreadable archive, or a PDF that failed; never a picture or model.
    pub(crate) fn failure(&self, frame: &Frame, cx: &gpui::App) -> Option<String> {
        if frame.manual || frame.kind == Kind::Multi {
            return None;
        }
        if let Mounted::Doc(doc) = &frame.mounted {
            if let Some(sentence) = doc.failure(&self.catalog, cx) {
                return Some(sentence);
            }
        }
        if frame.facts.as_ref().is_some_and(|m| m.archive_failed) {
            return Some(self.catalog.tr("preview.archiveUnreadable"));
        }
        (frame.failed && matches!(frame.mounted, Mounted::Pdf(_)))
            .then(|| self.catalog.tr("document.pdf_unreadable"))
    }

    fn extra(&self, frame: &Frame, cx: &gpui::App) -> facts::Extra {
        let pages = match &frame.mounted {
            Mounted::Pdf(pdf) => pdf.read(cx).surface.page_count(),
            _ => 0,
        };
        facts::Extra {
            pages: if pages > 0 {
                pages.to_string()
            } else {
                String::new()
            },
            owner: frame
                .facts
                .as_ref()
                .map_or(String::new(), |m| m.owner.clone()),
            ..Default::default()
        }
    }

    /// Facts.js facts, or the count and combined size for a selection; a model or an office document adds its own under Kind.
    pub(crate) fn facts(&self, frame: &Frame, cx: &gpui::App) -> Vec<(String, String)> {
        if self.manual_empty(frame) {
            return Vec::new();
        }
        if frame.kind == Kind::Multi {
            return facts::multi_facts(
                &super::selection_preview::multi::rows(&frame.subject),
                Some(frame.subject.selected_count),
                &self.catalog,
            )
            .into_iter()
            .map(|fact| (fact.label, fact.value))
            .collect();
        }
        let row = &frame.subject.row;
        if let Mounted::Model(model) = &frame.mounted {
            let words = crate::row::format::Words::from_catalog(&self.catalog);
            let size = words.locale.format_size(row.size);
            let modified = crate::row::format::date_text(row.mtime, &words);
            let kind = if row.kind.is_empty() {
                &row.icon
            } else {
                &row.kind
            };
            let groups = model
                .read(cx)
                .facts(flea::ui_logic_model_preview::facts::FileFacts {
                    kind,
                    size: &size,
                    modified: &modified,
                });
            if !groups.is_empty() {
                return groups.into_iter().flatten().collect();
            }
        }
        let mut rows: Vec<_> = facts::facts(
            self.state(frame, cx),
            &fact_row(row),
            frame.facts.as_ref(),
            &row.kind,
            &self.extra(frame, cx),
            &self.catalog,
        )
        .into_iter()
        .map(|f| (f.label, f.value))
        .collect();
        if let Mounted::Doc(docs::Doc::Office(office)) = &frame.mounted {
            let at = usize::from(!row.kind.is_empty()).min(rows.len());
            rows.splice(at..at, office.read(cx).facts(&self.catalog, cx));
        }
        rows
    }

    /// Keep compact facts where they fit; overflow uses the existing full-facts table.
    pub(crate) fn show_full_facts(
        &self,
        frame: &Frame,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> bool {
        if self.full_facts || !frame.kind.tall() {
            return true;
        }
        let (kind, tail) = self.compact(frame, cx);
        let measure = |text: String| {
            crate::g2::status_feedback::caption::measure(
                text.into(),
                self.face.font.clone(),
                self.face.caption,
                gpui::px(0.),
                window,
            )
        };
        let pad = crate::list::Metrics::for_typeface(&self.face).pad;
        measure(kind) + measure(tail) > (self.width - 2. * pad).max(1.)
    }

    /// A mirrored line reads its tail toward the kind; the complete size stays LTR.
    pub(crate) fn compact(&self, frame: &Frame, cx: &gpui::App) -> (String, String) {
        let rtl = self.catalog.locale().right_to_left;
        let kind = frame.subject.row.kind.clone();
        let size = facts::size(&self.catalog, frame.subject.row.size);
        let mut parts = vec![if rtl {
            format!("\u{2066}{size}\u{2069}")
        } else {
            size
        }];
        let count = facts::compact_count(
            frame.kind.state(),
            frame.facts.as_ref(),
            &self.extra(frame, cx),
            &self.catalog,
        );
        if !count.is_empty() {
            parts.push(count);
        }
        let tail = parts.join(flea::ui_logic_preview_core::FACT_SEP);
        let tail = if kind.is_empty() {
            tail
        } else {
            format!("{}{tail}", flea::ui_logic_preview_core::FACT_SEP)
        };
        (kind, if rtl { format!("\u{200f}{tail}") } else { tail })
    }
}

/// The compact facts line, in the name's ink as the board and v0.3.12 draw it.
pub(super) fn compact_facts_line(
    kind: String,
    tail: String,
    row: f32,
    line: f32,
    roles: &crate::list::Palette,
) -> gpui::Div {
    use gpui::{div, prelude::*, px};
    div()
        .debug_selector(|| "column-preview-facts".into())
        .flex()
        .flex_none()
        .w_full()
        .h(px(row))
        .line_height(px(line))
        .text_color(line_ink(roles))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(kind),
        )
        .child(div().flex_none().whitespace_nowrap().child(tail))
}

#[test]
fn vf4_r1_compact_facts_line_draws_in_the_name_ink() {
    use gpui::Styled;
    let theme = crate::palette::Theme::default();
    let roles = crate::list::Palette::inherited(&gpui::TextStyle::default(), theme);
    assert_ne!(roles.foreground, roles.metadata);
    let mut line = compact_facts_line(
        "Markdown document".into(),
        " · 193 B".into(),
        17.,
        17.,
        &roles,
    );
    assert_eq!(line.style().text.color, Some(roles.foreground));
}
