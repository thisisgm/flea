//! Confirmation, transfer and Jobs decisions shared by the GPUI window and the TUI; no I/O, no paint.
pub mod card;
pub mod collide;
pub mod confirm;
mod fat;
pub mod host;
pub mod jobs;
pub mod lines;
pub mod transfer;
pub mod trash;
mod wire;

pub use wire::{parse, Collision, Event, JobRow, Rename, Reply, TransferDone};

use crate::i18n::I18n;

// One run of a drawn line: figures in the foreground, the words between them muted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub figure: bool,
}

// Sample input: parts("%1 of %2", ["1 kB", "2 kB"]) is [1 kB][ of ][2 kB], arguments as figures.
pub fn parts(template: &str, args: &[&str]) -> Vec<Part> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut chars = template.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        let digit = chars
            .peek()
            .and_then(|&(_, d)| d.to_digit(10))
            .filter(|d| *d >= 1);
        match (c, digit) {
            ('%', Some(n)) if (n as usize) <= args.len() => {
                chars.next();
                if !word.is_empty() {
                    out.push(Part {
                        text: std::mem::take(&mut word),
                        figure: false,
                    });
                }
                out.push(Part {
                    text: args[n as usize - 1].to_owned(),
                    figure: true,
                });
            }
            _ => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(Part {
            text: word,
            figure: false,
        });
    }
    out
}

pub fn joined(parts: &[Part]) -> String {
    parts.iter().map(|part| part.text.as_str()).collect()
}

// The running preset's key for an action inside a hint template, or nothing where the preset binds none.
pub fn hint(catalog: &I18n, preset: &str, template: &str, action: &str) -> String {
    crate::keys::presentation::hinted(catalog, template, preset, action)
}

pub fn undo_hint(catalog: &I18n, preset: &str) -> String {
    hint(catalog, preset, "jobs.hint.undo", "undo")
}

pub fn count(catalog: &I18n, n: u64) -> String {
    catalog
        .locale()
        .format_count(i64::try_from(n).unwrap_or(i64::MAX))
}

pub fn size(catalog: &I18n, bytes: u64) -> String {
    catalog.locale().format_size(bytes)
}

pub fn n(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

// "0:04", "16:40" or "2:14:48" for whole seconds, the form every remaining-time figure takes.
pub fn duration(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

// A raw JSON string member read back as its text; anything else reads as empty.
pub fn wire_text(raw: Option<&str>) -> String {
    raw.and_then(|r| crate::jsondoc::parse(r).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

// The name a line shows for a path: the part after the last separator.
pub fn leaf(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, leaf)| leaf)
}

#[cfg(test)]
pub(crate) mod testcat;
#[cfg(test)]
mod tests;
