//! The Open With and Tags flyouts: the leaves each row opens, built from the app registry and the tag names.
use super::entry::{Entry, Text};
use super::rows::OPEN_WITH_OTHER;

// The desktop's default alone first with the muted caption, a separator, every other capable app alphabetical, a separator, then the tail.
pub fn open_with_entries(apps: &[super::build::App]) -> Vec<Entry> {
    let leaf = |app: &super::build::App, default: bool| Entry {
        icon: Some(app.icon.clone()),
        hint: default.then_some(Text::Key("menu.open_with.default")),
        ..Entry::leaf(app.id.clone(), Text::Data(app.label.clone()))
    };
    let default_at = apps.iter().position(|app| app.default);
    let mut rest: Vec<&super::build::App> = apps
        .iter()
        .enumerate()
        .filter(|(at, _)| Some(*at) != default_at)
        .map(|(_, app)| app)
        .collect();
    rest.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    let mut rows: Vec<Entry> = Vec::new();
    if let Some(default) = default_at.map(|at| &apps[at]) {
        rows.push(leaf(default, true));
        rows.push(Entry::separator());
    }
    rows.extend(rest.iter().map(|app| leaf(app, false)));
    if rows.last().is_some_and(|row| !row.separator) {
        rows.push(Entry::separator());
    }
    rows.push(
        Entry::leaf(OPEN_WITH_OTHER, Text::Key("menu.open_with.other")).with_glyph("app-window"),
    );
    rows
}

// The Tags flyout's seven leaves in menu order, each with its colour slot and digit hint.
pub fn tag_entries() -> Vec<Entry> {
    crate::ui_logic_pl4::tags::NAMES
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let slot = u8::try_from(at + 1).unwrap_or(1);
            Entry {
                tag: Some(slot),
                ..Entry::leaf(
                    format!("tag{slot}"),
                    Text::Key(crate::ui_logic_pl4::tags::label_key(name)),
                )
            }
            .with_key_hint(crate::ui_logic_pl4::tags::key_for(name))
        })
        .collect()
}
