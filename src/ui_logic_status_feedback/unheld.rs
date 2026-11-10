//! The fixed unsupported-entry reasons and summaries emitted by backend/unheld.rs.
use crate::i18n::I18n;

const REASONS: [(&str, &str); 5] = [
    ("this drive cannot hold links", "feedback.unheld.links"),
    (
        "this drive cannot hold special files",
        "feedback.unheld.nodes",
    ),
    (
        "this drive does not allow that name",
        "feedback.unheld.name",
    ),
    (
        "that name is too long for this drive",
        "feedback.unheld.long",
    ),
    (
        "this drive treats that name as the same as another in the folder",
        "feedback.unheld.taken",
    ),
];

pub(super) fn reason(message: &str) -> Option<&'static str> {
    REASONS
        .iter()
        .find_map(|(source, key)| (*source == message).then_some(*key))
}

struct Summary<'a> {
    key: &'static str,
    count: Option<i64>,
    path: &'a str,
    why: &'a str,
}

// Sample input: "a could not be moved: this drive cannot hold links", or "2 items inside could not be copied; the first is a: this drive cannot hold links".
fn summary(message: &str) -> Option<Summary<'_>> {
    // Only a whole backend reason establishes the boundary after a path that may contain colons.
    let (summary, why) = message.rsplit_once(": ")?;
    if reason(why).is_none() && super::drive_refusal(why).is_none() {
        return None;
    }
    for (verb, single, multiple) in [
        (
            "copied",
            "feedback.unheld.copy_one",
            "feedback.unheld.copy_many",
        ),
        (
            "moved",
            "feedback.unheld.move_one",
            "feedback.unheld.move_many",
        ),
    ] {
        if let Some(path) = summary.strip_suffix(&format!(" could not be {verb}")) {
            return Some(Summary {
                key: single,
                count: None,
                path,
                why,
            });
        }
        let marker = format!(" items inside could not be {verb}; the first is ");
        if let Some((count, path)) = summary.split_once(&marker) {
            let count = count.parse::<i64>().ok().filter(|count| *count >= 2)?;
            return Some(Summary {
                key: multiple,
                count: Some(count),
                path,
                why,
            });
        }
    }
    None
}

pub(super) fn recognizes(message: &str) -> bool {
    reason(message).is_some() || summary(message).is_some()
}

pub(super) fn translated(message: &str, catalog: &I18n) -> Option<String> {
    if let Some(key) = reason(message) {
        return Some(catalog.tr(key));
    }
    let summary = summary(message)?;
    let why = super::clause(summary.why, catalog);
    let args = &[summary.path, why.as_str()];
    Some(match summary.count {
        Some(count) => catalog.trn_args(summary.key, count, args),
        None => catalog.tr_args(summary.key, args),
    })
}
