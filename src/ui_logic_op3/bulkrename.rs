// The bulk rename card's preview: find and replace over the old names, each line's reason, the counts and the Tab walk.
use crate::i18n::I18n;

/// Linux filename components hold at most 255 bytes of UTF-8, the bound the backend refuses past.
pub const NAME_BYTES: usize = 255;
/// The edit file's ceiling per name line, so a runaway edit is refused before it is split.
pub const NAME_LINE_BYTES: usize = 256;
/// The private edit directory's leaf prefix, so a cleanup never walks elsewhere.
pub const SCRATCH_STEM: &str = "flea-rename-";

pub fn apply_find_replace(old_names: &[String], find: &str, replace: &str) -> Vec<String> {
    old_names.iter().map(|name| if find.is_empty() { name.clone() } else { name.replace(find, replace) }).collect()
}

/// Why one new name cannot be used, or None when it can.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Empty,
    Slash,
    Invalid,
    TooLong,
    /// Same as the zero-based line named, the first later line holding the name.
    Same(usize),
}

/// Sample input: "trip/4123.HEIC" at line 4 names Slash; a name repeated later points at that later line.
pub fn reasons(new_names: &[String]) -> Vec<Option<Reason>> {
    let mut out = vec![None; new_names.len()];
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, text) in new_names.iter().enumerate().rev() {
        out[i] = if text.is_empty() {
            Some(Reason::Empty)
        } else if text.contains('/') {
            Some(Reason::Slash)
        } else if text == "." || text == ".." || text.contains(['\0', '\r', '\n']) {
            Some(Reason::Invalid)
        } else if text.len() > NAME_BYTES {
            Some(Reason::TooLong)
        } else {
            seen.get(text.as_str()).map(|line| Reason::Same(*line))
        };
        seen.insert(text, i);
    }
    out
}

pub fn reason_text(i18n: &I18n, reason: Reason) -> String {
    match reason {
        Reason::Empty => i18n.tr("dialogs.file.bulk.reason_empty"),
        Reason::Slash => i18n.tr("dialogs.file.bulk.reason_slash"),
        Reason::Invalid => i18n.tr("dialogs.file.bulk.reason_invalid"),
        Reason::TooLong => i18n.tr("dialogs.file.bulk.reason_long"),
        Reason::Same(line) => i18n.tr_args("dialogs.file.bulk.reason_same", &[&i18n.locale().format_count(line as i64 + 1)]),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub renamed: usize,
    pub unchanged: usize,
    pub invalid: usize,
}

/// The live rules alone; the backend refuses a name taken outside the batch.
pub fn counts(old_names: &[String], new_names: &[String], reasons: &[Option<Reason>]) -> Counts {
    let mut out = Counts::default();
    for (i, name) in new_names.iter().enumerate() {
        if reasons.get(i).copied().flatten().is_some() {
            out.invalid += 1;
        } else if old_names.get(i) == Some(name) {
            out.unchanged += 1;
        } else {
            out.renamed += 1;
        }
    }
    out
}

/// The summary drops the invalid count; a clash is said once under the new names instead.
pub fn footer(i18n: &I18n, counts: Counts) -> String {
    let locale = i18n.locale();
    i18n.tr_args("dialogs.file.bulk.footer", &[&locale.format_count(counts.renamed as i64), &locale.format_count(counts.unchanged as i64)])
}

/// The one error line under the new names, held empty until two names clash.
pub fn clash_line(i18n: &I18n, reasons: &[Option<Reason>]) -> String {
    if reasons.iter().any(|r| matches!(r, Some(Reason::Same(_)))) {
        i18n.tr("dialogs.file.bulk.clash")
    } else {
        String::new()
    }
}

/// Rename alone until a new name differs, then Rename with the live count.
pub fn rename_label(i18n: &I18n, renamed: usize) -> String {
    if renamed == 0 {
        i18n.tr("dialogs.file.bulk.rename")
    } else {
        i18n.tr_args("dialogs.file.bulk.rename_count", &[&i18n.locale().format_count(renamed as i64)])
    }
}

pub fn title(i18n: &I18n, count: usize) -> String {
    i18n.trn("dialogs.file.bulk.title", count as i64)
}

pub fn can_rename(counts: Counts) -> bool {
    counts.invalid == 0 && counts.renamed > 0
}

/// A short list reads "" past its end, so a line is never drawn from a stale index.
pub fn name_at(new_names: &[String], index: usize) -> &str {
    new_names.get(index).map_or("", String::as_str)
}

/// One Tab stop of the card, in its walk order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Find,
    Replace,
    Line(usize),
    Edit,
    Cancel,
    Rename,
}

pub fn focus_stops(lines: usize) -> Vec<Stop> {
    let mut stops = vec![Stop::Find, Stop::Replace];
    stops.extend((0..lines).map(Stop::Line));
    stops.extend([Stop::Edit, Stop::Cancel, Stop::Rename]);
    stops
}

/// An unknown stop enters at the first or last stop; the walk comes round both ways.
pub fn next_stop(current: Option<Stop>, back: bool, lines: usize) -> Stop {
    let stops = focus_stops(lines);
    let len = stops.len();
    match current.and_then(|c| stops.iter().position(|s| *s == c)) {
        None => if back { stops[len - 1] } else { stops[0] },
        Some(at) => stops[(at + if back { len - 1 } else { 1 }) % len],
    }
}

/// The edit file read back: one name per line with a single trailing newline, the same count as before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reloaded {
    Names(Vec<String>),
    TooLarge,
    CountChanged,
}

pub fn reload(text: &str, lines: usize) -> Reloaded {
    if text.len() > lines * NAME_LINE_BYTES {
        return Reloaded::TooLarge;
    }
    let mut names: Vec<String> = text.split('\n').map(str::to_string).collect();
    if names.last().is_some_and(String::is_empty) {
        names.pop();
    }
    if names.len() != lines {
        return Reloaded::CountChanged;
    }
    Reloaded::Names(names)
}

/// The names file the editor opens, one per line.
pub fn edit_text(new_names: &[String]) -> String {
    let mut text = new_names.join("\n");
    text.push('\n');
    text
}

/// A scratch dir is the base plus one flea-rename leaf, so a cleanup never walks elsewhere.
pub fn is_scratch_dir(base: &str, dir: &str) -> bool {
    if base.is_empty() || dir.len() <= base.len() + SCRATCH_STEM.len() + 1 {
        return false;
    }
    let Some(leaf) = dir.strip_prefix(base).and_then(|rest| rest.strip_prefix('/')) else { return false };
    !leaf.contains('/') && !leaf.contains("..") && leaf.starts_with(SCRATCH_STEM)
}

/// The editor's argv: the file rides as an argument and never inside the shell string.
pub fn editor_argv(folder: &str, editor: &str, file: &str) -> Vec<String> {
    ["xdg-terminal-exec", &format!("--dir={folder}"), "--", "/bin/sh", "-c", &format!("exec {editor} \"$1\""), "flea-bulk-rename", file]
        .iter().map(|s| s.to_string()).collect()
}

/// The folder the batch lives in, "/" for a root entry.
pub fn folder_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(cut) => &path[..cut],
    }
}

pub fn request(paths: &[String], names: &[String]) -> Option<String> {
    if paths.is_empty() || paths.len() != names.len() {
        return None;
    }
    Some(format!(r#"{{"c":"bulkrename","paths":{},"names":{}}}"#, crate::json::string_array(paths), crate::json::string_array(names)))
}
