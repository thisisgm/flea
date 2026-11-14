//! Shared expectations driven through mounted fields, including selection direction and Unicode byte offsets.
use super::Entry;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Snapshot(
    pub String,
    pub Range<usize>,
    pub bool,
    pub usize,
    pub Option<Range<usize>>,
);

// Each chord is checked alone during a composition, so a move and its reverse cannot cancel out.
pub(crate) const COMPOSING: [&str; 6] = [
    "ctrl-left",
    "ctrl-right",
    "ctrl-shift-left",
    "ctrl-shift-right",
    "ctrl-up",
    "ctrl-shift-down",
];

pub(crate) fn snapshot(entry: &Entry) -> Snapshot {
    Snapshot(
        entry.text.clone(),
        entry.selected.clone(),
        entry.reversed,
        entry.cursor(),
        entry.marked.clone(),
    )
}

pub(crate) fn check(mut drive: impl FnMut(&str, &str) -> Snapshot) {
    let text = "alpha beta-gamma.txt";
    for (keys, range, reversed) in [
        ("end ctrl-left", 11..11, false),
        ("end ctrl-left ctrl-left", 6..6, false),
        ("end ctrl-left ctrl-left ctrl-left ctrl-left", 0..0, false),
        ("ctrl-right", 5..5, false),
        ("ctrl-right ctrl-right", 10..10, false),
        (
            "ctrl-right ctrl-right ctrl-right ctrl-right",
            text.len()..text.len(),
            false,
        ),
        ("end ctrl-shift-left", 11..text.len(), true),
        ("end ctrl-shift-left ctrl-shift-left", 6..text.len(), true),
        (
            "end ctrl-shift-left ctrl-shift-right",
            text.len()..text.len(),
            true,
        ),
        (
            "end ctrl-left ctrl-shift-left ctrl-shift-right ctrl-shift-right",
            11..text.len(),
            false,
        ),
        ("ctrl-shift-right ctrl-shift-right", 0..10, false),
        ("end shift-home ctrl-left", 0..0, false),
        ("ctrl-shift-right ctrl-right", 5..5, false),
        ("end ctrl-shift-left ctrl-left", 11..11, false),
        (
            "end ctrl-shift-left ctrl-right",
            text.len()..text.len(),
            false,
        ),
        ("end ctrl-left ctrl-shift-left ctrl-right", 11..11, false),
        ("right right shift-right ctrl-left", 2..2, false),
        ("right right shift-right ctrl-right", 3..3, false),
        ("end ctrl-up", 0..0, false),
        ("ctrl-down", text.len()..text.len(), false),
        ("end ctrl-shift-up ctrl-shift-up", 0..text.len(), true),
        ("ctrl-shift-down ctrl-shift-down", 0..text.len(), false),
        ("end shift-left left", text.len() - 1..text.len() - 1, false),
        ("end shift-left right", text.len()..text.len(), false),
        ("end shift-home shift-end", text.len()..text.len(), true),
        ("end ctrl-alt-left", text.len() - 1..text.len() - 1, false),
        ("end cmd-left", text.len() - 1..text.len() - 1, false),
    ] {
        let cursor = if reversed { range.start } else { range.end };
        assert_eq!(
            drive(text, keys),
            Snapshot(text.into(), range, reversed, cursor, None),
            "{keys}"
        );
    }
    for (text, keys, range, reversed) in [
        (
            "/a folder/second-part/file.txt",
            "end ctrl-left",
            22..22,
            false,
        ),
        (
            "/a folder/second-part/file.txt",
            "end ctrl-shift-left ctrl-shift-left",
            17..30,
            true,
        ),
        ("  alpha / - ", "end ctrl-left", 2..2, false),
        ("  alpha / - ", "ctrl-right ctrl-right", 12..12, false),
        ("e\u{301}cole résumé", "ctrl-right", 7..7, false),
        ("e\u{301}cole résumé", "end ctrl-shift-left", 8..16, true),
        (
            "a \u{1f469}\u{200d}\u{1f4bb} café",
            "ctrl-right ctrl-right",
            19..19,
            false,
        ),
        ("日本 語", "end ctrl-left", 7..7, false),
        ("", "ctrl-left ctrl-right ctrl-up ctrl-down", 0..0, false),
    ] {
        let cursor = if reversed { range.start } else { range.end };
        assert_eq!(
            drive(text, keys),
            Snapshot(text.into(), range, reversed, cursor, None),
            "{text:?}: {keys}"
        );
    }
}
