//! Facts use the listing, matching metadata and mounted document counts.
use super::selection_preview::*;
impl SelectionPreview {
    pub(crate) fn frame_size(&self, kind: Kind, facts: usize) -> (f32, f32) {
        let metrics = crate::list::Metrics::for_typeface(&self.face);
        let width = (self.width - 2. * metrics.pad).max(1.);
        let caption = f32::from(self.face.caption) * 1.4;
        let below = caption * (facts + 1) as f32 + metrics.gap * 2. + 2. * metrics.pad;
        let height = if kind.tall() {
            (self.height - below).max(1.)
        } else {
            (width * 10. / 16.).round()
        };
        (width, height)
    }

    pub(crate) fn facts(&self, frame: &Frame, cx: &gpui::App) -> Vec<Vec<(String, String)>> {
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
            return vec![rows];
        }
        let row = &frame.subject.row;
        let words = crate::row::format::Words::from_catalog(&self.catalog);
        let size = words.locale.format_size(row.size);
        let modified = crate::row::format::date_text(row.mtime, &words);
        let kind = &row.icon;
        if let Mounted::Model(model) = &frame.mounted {
            let groups = model.read(cx).facts(flea::ui_logic_pv4::facts::FileFacts {
                kind,
                size: &size,
                modified: &modified,
            });
            if !groups.is_empty() {
                return groups;
            }
        }
        let mut rows = vec![
            (self.catalog.tr("properties.kind"), kind.clone()),
            (self.catalog.tr("properties.size"), size),
            (self.catalog.tr("properties.modified"), modified),
        ];
        if frame.kind == Kind::Text && self.full_facts {
            rows.push((
                self.catalog.tr("list.column_mode"),
                crate::row::format::permissions(row.mode),
            ));
        }
        vec![rows]
    }

    pub(crate) fn compact(&self, frame: &Frame, cx: &gpui::App) -> String {
        let mut facts = vec![
            frame.subject.row.icon.clone(),
            self.catalog.locale().format_size(frame.subject.row.size),
        ];
        if let Mounted::Pdf(pdf) = &frame.mounted {
            let pages = pdf.read(cx).surface.page_count();
            if pages > 0 {
                facts.push(self.catalog.trn("quicklook.pages", pages as i64));
            }
        }
        if frame.kind == Kind::Text && frame.text_answered {
            facts.push(
                self.catalog
                    .trn("quicklook.lines", number(frame.meta.as_ref(), "lines")),
            );
        }
        facts.join(&self.catalog.tr("media.separator"))
    }
}
