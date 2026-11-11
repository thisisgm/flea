//! The archive cards' own sentences and leaves; every title is a whole template filled once.
use crate::i18n::I18n;

// Sample input: "/d/backup.7z" reads "backup.7z"; "" reads "".
pub fn leaf_of(path: &str) -> String {
    match path.rfind('/') {
        Some(cut) => path[cut + 1..].to_string(),
        None => path.to_string(),
    }
}

pub fn path_leaves(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| leaf_of(path)).collect()
}

// Sample input: one item titles "Compress 1 Item"; three title "Compress 3 Items".
pub fn compress_title(i18n: &I18n, count: usize) -> String {
    i18n.trn("archive.compress.title", count as i64)
}

// Sample input: "backup.7z" titles "backup.7z is encrypted."
pub fn encrypted_title(i18n: &I18n, leaf: &str) -> String {
    i18n.tr_args("archive.extract.title", &[leaf])
}

// Sample input: "backup.7z" refuses "That password did not open backup.7z."
pub fn wrong_password_sentence(i18n: &I18n, leaf: &str) -> String {
    i18n.tr_args("archive.extract.wrong", &[leaf])
}

// Sample input: three items to 7z stick "Compressing 3 items to .7z".
pub fn compress_sticky(i18n: &I18n, count: usize, format: &str) -> String {
    let items = i18n.trn("items.count", count as i64);
    i18n.tr_args("archive.compressing", &[&items, format])
}

/// What the compress card's commit does: the name is free, and only a matched non-empty pair reaches the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Commit {
    Accept,
    Mismatch,
    EmptyPassword,
    EmptyName,
}

// Sample input: a matched pair with a name accepts; a mismatch names itself before an empty password does.
pub fn compress_commit(matched: bool, password_empty: bool, name_empty: bool) -> Commit {
    if !matched {
        return Commit::Mismatch;
    }
    if password_empty {
        return Commit::EmptyPassword;
    }
    if name_empty {
        return Commit::EmptyName;
    }
    Commit::Accept
}

// Sample input: a mismatch reads "Passwords do not match."; an empty password asks for one first.
pub fn compress_refusal(i18n: &I18n, commit: &Commit) -> String {
    match commit {
        Commit::Accept => String::new(),
        Commit::Mismatch => i18n.tr("archive.passwords_mismatch"),
        Commit::EmptyPassword => i18n.tr("archive.type_password"),
        Commit::EmptyName => String::new(),
    }
}

// Sample input: an empty field asks for the password; a typed one accepts.
pub fn extract_commit(password_empty: bool) -> Commit {
    if password_empty { Commit::EmptyPassword } else { Commit::Accept }
}

// Sample input: 7 of 18 heads "Extracting 7 of 18"; an unknown total names no count, and the last sample holds it.
pub fn extracting_head(i18n: &I18n, done_1based: u64, total: u64) -> String {
    let locale = i18n.locale();
    if total == 0 {
        let done = done_1based.saturating_sub(1);
        if done == 0 {
            return i18n.tr("archive.extracting.head_unknown");
        }
        return i18n.trn("archive.extracting.head_unknown_count", count(done));
    }
    let at = done_1based.min(total);
    i18n.tr_args("archive.extracting.head", &[&locale.format_count(count(at)), &locale.format_count(count(total))])
}

// A backend count wider than i64 saturates at the format's own limit, never wrapping negative.
fn count(n: u64) -> i64 {
    n.min(i64::MAX as u64) as i64
}

// Sample input: the entry in flight reads "DSC_4907.jpg · 586.6 kB".
pub fn extracting_file(i18n: &I18n, name: &str, size: u64) -> String {
    let size = i18n.locale().format_size(size);
    i18n.tr_args("archive.extracting.file", &[name, &size])
}

/// The extracting card op1's Jobs card draws: the head, the file line, the bar and the byte line.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtractBlock {
    pub head: String,
    pub file: String,
    pub byte: Vec<super::flashcard::Part>,
    pub share: f64,
    pub cancel: bool,
}

// Sample input: done 6 of 18 heads "Extracting 7 of 18", the unpacked bytes lead the byte line.
pub fn extracting_block(i18n: &I18n, sample: &super::wire::ExtractSample) -> ExtractBlock {
    let locale = i18n.locale();
    let head = extracting_head(i18n, (sample.done as u64).saturating_add(1), sample.total as u64);
    let file = extracting_file(i18n, &sample.file, sample.item_bytes);
    let figure = |text: String| super::flashcard::Part { text, figure: true };
    let moved = figure(locale.format_size(sample.bytes));
    let total = figure(locale.format_size(sample.total_bytes));
    let rate = figure(locale.format_size(sample.rate));
    let unit = figure("/s".into());
    let left = sample
        .left
        .or_else(|| {
            (sample.total_bytes > 0 && sample.rate > 0)
                .then(|| sample.total_bytes.saturating_sub(sample.bytes) / sample.rate)
        })
        .filter(|_| sample.total_bytes > 0)
        .map(|seconds| figure(super::flashcard::duration(seconds.saturating_mul(1000))));
    let byte = if sample.total_bytes == 0 {
        super::flashcard::lined(&i18n.tr("archive.extracting.progress_no_total"), &[moved, rate, unit])
    } else {
        match left {
            Some(left) => super::flashcard::lined(&i18n.tr("archive.extracting.progress"), &[moved, total, rate, unit, left]),
            None => super::flashcard::lined(&i18n.tr("archive.extracting.progress_stalled"), &[moved, total, rate, unit]),
        }
    };
    let share = if sample.total_bytes > 0 {
        sample.bytes as f64 / sample.total_bytes as f64
    } else if sample.total > 0 {
        sample.done as f64 / sample.total as f64
    } else {
        0.0
    };
    ExtractBlock { head, file, byte, share: share.clamp(0.0, 1.0), cancel: true }
}
