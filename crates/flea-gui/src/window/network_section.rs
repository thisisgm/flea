//! The network service rows as the shared rail draws them.
use super::*;

/// The rows the rail draws for the listing's shares; fstype and release identity stay on the join's own rows.
pub fn section(rows: &[RailEntry], shown: bool) -> Vec<RailSection> {
    if !shown || rows.is_empty() {
        return Vec::new();
    }
    vec![RailSection {
        heading: Heading::Key("places.heading.network"),
        rows: rows
            .iter()
            .map(|row| rail::RailEntry {
                group: Group::Network,
                kind: match row.kind {
                    flea::ui_logic_device_rail::Kind::Dropbox => Kind::Dropbox,
                    flea::ui_logic_device_rail::Kind::Cloud => Kind::Cloud,
                    _ => Kind::Share,
                },
                label: row.label.clone(),
                label_key: if row.kind == flea::ui_logic_device_rail::Kind::Dropbox {
                    "plugins.builtin.dropbox.name"
                } else {
                    ""
                },
                path: row.path.clone(),
                uri: row.uri.clone(),
                mounted: Some(row.mounted),
                releasable: flea::ui_logic_device_rail::eject::releasable(row),
                glyph: if row.kind == flea::ui_logic_device_rail::Kind::Dropbox {
                    "dropbox"
                } else {
                    "server"
                },
                ..rail::RailEntry::default()
            })
            .collect(),
    }]
}
