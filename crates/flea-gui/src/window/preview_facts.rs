//! Facts use the listing, the meta reply and the mounted view's counts, as Facts.js reads them.
use super::selection_preview::*;
use flea::ui_logic_pv1::{facts, kinds::State};
impl SelectionPreview {
    /// One caption row of the facts table, and the name's own row.
    pub(crate) fn fact_row(&self) -> (f32, f32) {
        (
            (f32::from(self.face.caption) * 1.3).round(),
            (f32::from(self.face.body) * 1.3).round(),
        )
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
                (row * facts as f32) as f64,
                metrics.gap as f64,
            )
            .max(1.) as f32
        } else {
            (width * 10. / 16.).round()
        };
        (width, height)
    }

    /// The state Facts.js draws: an unreadable file is Error, whatever the row is.
    pub(crate) fn state(&self, frame: &Frame, cx: &gpui::App) -> State {
        if self.failure(frame, cx).is_some() {
            State::Error
        } else {
            frame.kind.state()
        }
    }

    /// PreviewColumn.qml failure: the mounted view's sentence, an unreadable archive, or a renderer that failed.
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
        frame
            .failed
            .then(|| self.catalog.tr("document.pdf_unreadable"))
    }

    fn extra(&self, frame: &Frame, cx: &gpui::App) -> facts::Extra {
        let pages = match &frame.mounted {
            Mounted::Pdf(pdf) => pdf.read(cx).surface.page_count(),
            _ => 0,
        };
        facts::Extra {
            pages: if pages > 0 { pages.to_string() } else { String::new() },
            owner: frame
                .facts
                .as_ref()
                .map_or(String::new(), |m| m.owner.clone()),
            ..Default::default()
        }
    }

    /// Facts.js facts, or the count and combined size for a selection; a model or an office document adds its own under Kind.
    pub(crate) fn facts(&self, frame: &Frame, cx: &gpui::App) -> Vec<(String, String)> {
        if frame.kind == Kind::Multi {
            let subject = &frame.subject;
            let mut rows = vec![(
                self.catalog.tr("dialogs.file.properties.contains"),
                self.catalog
                    .trn("items.count", subject.selected_count as i64),
            )];
            if subject.selected.len() == subject.selected_count
                && subject.selected.iter().all(|row| !row.dir)
            {
                if let Some(bytes) = subject
                    .selected
                    .iter()
                    .try_fold(0_u64, |sum, row| sum.checked_add(row.size))
                {
                    rows.push((
                        self.catalog.tr("dialogs.file.properties.combined"),
                        self.catalog.locale().format_size(bytes),
                    ));
                }
            }
            return rows;
        }
        let row = &frame.subject.row;
        if let Mounted::Model(model) = &frame.mounted {
            let words = crate::row::format::Words::from_catalog(&self.catalog);
            let size = words.locale.format_size(row.size);
            let modified = crate::row::format::date_text(row.mtime, &words);
            let kind = if row.kind.is_empty() { &row.icon } else { &row.kind };
            let groups = model.read(cx).facts(flea::ui_logic_pv4::facts::FileFacts {
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

    /// Facts.js compactLine: the Kind apart, so it alone elides, then the size and the count.
    pub(crate) fn compact(&self, frame: &Frame, cx: &gpui::App) -> (String, String) {
        facts::compact_line(
            frame.kind.state(),
            Some(&fact_row(&frame.subject.row)),
            frame.facts.as_ref(),
            &frame.subject.row.kind,
            &self.extra(frame, cx),
            &self.catalog,
        )
    }
}
