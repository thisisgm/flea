// The Compare overlay's text and rows: the view draws what these return and keeps no wording of its own.
use super::meta::CompareState;
use crate::i18n::I18n;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hunk,
    Context,
    Removed,
    Added,
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: Kind,
    pub old_no: u64,
    pub new_no: u64,
    pub text: String,
}

// The board's sentence for a pair with no patch, by the backend's state; a patch has none.
pub fn sentence_key(state: &CompareState) -> Option<&'static str> {
    match state {
        CompareState::Diff(_) => None,
        CompareState::Identical => Some("preview.compare.identical"),
        CompareState::Binary => Some("preview.compare.binary"),
        CompareState::Over => Some("preview.compare.over"),
        CompareState::Failed => Some("preview.compare.failed"),
    }
}

// The compare-large board names the limit in the decimal unit every other size uses.
pub const OVER_BYTES: u64 = 1_048_576;

pub fn sentence(state: &CompareState, i18n: &I18n) -> String {
    match sentence_key(state) {
        Some(key @ "preview.compare.over") => i18n.tr_args(key, &[&i18n.locale().format_size(OVER_BYTES)]),
        Some(key) => i18n.tr(key),
        None => String::new(),
    }
}

// "notes-a.md and notes-b.md · 12 lines differ" with a patch; without one, the two names alone.
pub fn title(name_a: &str, name_b: &str, differing: Option<u64>, i18n: &I18n) -> String {
    let names = i18n.tr_args("preview.compare.names", &[name_a, name_b]);
    match differing {
        None => names,
        Some(n) => i18n.tr_args("preview.compare.title", &[&names, &i18n.trn("preview.compare.differ", n as i64)]),
    }
}

// Sample input: "@@ -9,12 +9,12 @@" starts the old side at 9 and the new side at 9.
fn hunk_start(line: &str) -> Option<(u64, u64)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let first = |side: &str| -> Option<u64> {
        let (start, len) = side.split_once(',').map_or((side, None), |(a, b)| (a, Some(b)));
        if len.is_some_and(|l| l.is_empty() || !l.bytes().all(|b| b.is_ascii_digit())) || start.is_empty() {
            return None;
        }
        start.parse().ok()
    };
    Some((first(old)?, first(new)?))
}

// What diff -u prints, as the rows the overlay draws; the --- and +++ lines are the copied patch's, not rows.
pub fn rows(patch: &str) -> Vec<DiffRow> {
    let mut out = Vec::new();
    let (mut old_at, mut new_at, mut in_hunk) = (0u64, 0u64, false);
    let mut lines: Vec<&str> = patch.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    for line in lines {
        if let Some((old, new)) = hunk_start(line) {
            (old_at, new_at, in_hunk) = (old, new, true);
            out.push(DiffRow { kind: Kind::Hunk, old_no: 0, new_no: 0, text: line.into() });
            continue;
        }
        if !in_hunk {
            continue;
        }
        let rest = line.get(1..).unwrap_or("").to_string();
        let row = match line.chars().next() {
            Some('-') => {
                old_at += 1;
                DiffRow { kind: Kind::Removed, old_no: old_at - 1, new_no: 0, text: rest }
            }
            Some('+') => {
                new_at += 1;
                DiffRow { kind: Kind::Added, old_no: 0, new_no: new_at - 1, text: rest }
            }
            Some('\\') => DiffRow { kind: Kind::Note, old_no: 0, new_no: 0, text: line.into() },
            _ => {
                (old_at, new_at) = (old_at + 1, new_at + 1);
                DiffRow { kind: Kind::Context, old_no: old_at - 1, new_no: new_at - 1, text: rest }
            }
        };
        out.push(row);
    }
    out
}

// The removed plus added lines, which is what the title counts.
pub fn changed(rows: &[DiffRow]) -> u64 { rows.iter().filter(|r| matches!(r.kind, Kind::Removed | Kind::Added)).count() as u64 }

// The file name a title shows for a path.
pub fn leaf(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(path)
}

// Two regular files, and only two, offer Compare; a link stays a link.
pub fn can_compare(modes: &[u32]) -> bool { modes.len() == 2 && modes.iter().all(|m| m & 0o170000 == 0o100000) }
