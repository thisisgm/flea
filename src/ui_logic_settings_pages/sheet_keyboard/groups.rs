//! The rest list's group each binding reads under, so the keyboard's lists head their rows as the rest list does.
use crate::keys::{action_group, SHEET_GROUPS};

/// Actions the rest list leaves out, by the start of their names, and a sibling it lists whose group they share:
/// copy and paste variants and tags change, zoom, media and preview toggles look, a digit's tab moves.
const SIBLINGS: [(&str, &str); 11] = [
    ("copy", "copy"),
    ("paste", "paste"),
    ("tag", "tags"),
    ("menu", "menu"),
    ("zoom", "zoomIn"),
    ("seek", "mute"),
    ("playPause", "mute"),
    ("markdownView", "togglePreview"),
    ("toggleWrap", "togglePreview"),
    ("nextSheet", "previewPageForward"),
    ("previousSheet", "previewPageBack"),
];

/// The rest list's group an action reads under, by its own name, its group's or a sibling's; none for plugin and
/// user keys.
pub fn group_of(action: &str) -> Option<usize> {
    let digit = crate::keys::DIGITS
        .iter()
        .any(|digit| digit.action == action);
    let sibling = SIBLINGS
        .iter()
        .find(|(start, _)| action.starts_with(start))
        .map_or(if digit { "tabNext" } else { action }, |(_, sibling)| {
            sibling
        });
    let names = [action, action_group(action), sibling];
    SHEET_GROUPS
        .iter()
        .position(|group| names.iter().any(|name| group.actions.contains(name)))
}
