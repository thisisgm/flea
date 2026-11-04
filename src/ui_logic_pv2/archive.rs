//! Quick Look's archive page (ui/Preview.qml archivePane, PreviewArchive.qml): the name, the count the archive's own
//! index gave, then as many entries as the frame has room for and the "+ N more" line, which is always visible.
use crate::i18n::I18n;

/// The `meta` answer's archive fields: exact totals whenever non-zero, the names capped by the backend.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub entries: u64,
    pub unpacked: u64,
    pub failed: bool,
    pub names: Vec<(String, bool)>,
}

/// The rows the frame lists: one per line box less the line kept for the "+ N more" count.
pub fn shown(height: f64, line_height: f64) -> usize {
    ((height / line_height.max(1.)).floor() as usize).saturating_sub(1)
}

/// The entries listed above the count, bounded twice: by what the wire carried and by what the frame can draw.
pub fn entries(meta: &Meta, shown: usize) -> &[(String, bool)] {
    &meta.names[..meta.names.len().min(shown)]
}

/// Every entry the frame did not name, whether the wire left it out or the frame had no room.
pub fn more(meta: &Meta, shown: usize) -> u64 {
    meta.entries
        .saturating_sub(entries(meta, shown).len() as u64)
}

/// "214 entries · 3.4 GB unpacked"; an empty index reads "0 entries", and a failed one states nothing.
pub fn line(i18n: &I18n, meta: &Meta) -> String {
    if meta.failed {
        return String::new();
    }
    let count = i18n.trn("document.archive_entries", meta.entries as i64);
    if meta.entries > 0 && meta.unpacked > 0 {
        let unpacked = i18n.tr_args(
            "document.archive_unpacked",
            &[&i18n.locale().format_size(meta.unpacked)],
        );
        i18n.tr_args("document.facts_pair", &[&count, &unpacked])
    } else {
        count
    }
}

/// The "+ N more" line, None when every entry is listed.
pub fn more_line(i18n: &I18n, meta: &Meta, shown: usize) -> Option<String> {
    let rest = more(meta, shown);
    (rest > 0).then(|| {
        i18n.tr_args(
            "document.archive_more",
            &[&i18n.locale().format_count(rest as i64)],
        )
    })
}

pub fn refusal(meta: Option<&Meta>) -> Option<super::refusal::Refusal> {
    meta.filter(|meta| meta.failed)
        .map(|_| super::refusal::Refusal::ArchiveUnreadable)
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
