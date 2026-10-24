//! The mn2 rows as `mn1` entries (Open in Agent, the Actions flyout, the plugin menu-action rows), spliced into the inventory order the boards draw; switches arrive as the effective hidden set the menu reads.
use super::agent;
use super::plugins::{self, Contribution};
use super::wire::{Action, Listing};
use crate::ui_logic_mn1::entry::{Entry, Text};

// The stored id Settings > Menus hides by; it ships hidden with Run Script's place (DESIGN-040 Defaults ledger).
pub const ID: &str = "actions";
pub const LABEL_KEY: &str = "actions.row.actions";
// Lucide's zap cut to the Omarchy edge, as the board source draws it.
pub const GLYPH: &str = "zap";
// The flyout's own mark for leaves that name no kind: scripts keep the terminal mark their runScript submenu gave them.
pub const FLYOUT_GLYPH: &str = "terminal";
// A chosen action or script fires under this verb with its id as the leaf; a flyout leaf fires under its parent row's verb, so the leaf's mode decides the route.
pub const FIRE: &str = "actions";

// The background inspect group after Open in Terminal: Open in Agent joins it with the agent mark, and ships off, switched on here in Settings > Menus.
pub fn agent_row(hidden: &[String]) -> Option<Entry> {
    if crate::ui_logic_mn1::rows::is_hidden(hidden, agent::ID) {
        return None;
    }
    let mut entry = Entry::row(agent::ID, Text::Key(agent::LABEL_KEY), agent::GLYPH);
    entry.action = agent::ID.into();
    Some(entry)
}

// The mark a flyout leaf draws for the kind the action takes: the board's three image actions each carry the image mark, and any other kind leaves the leaf on the flyout's own mark.
pub fn kind_mark(top: &str) -> Option<&'static str> {
    match top {
        "image" => Some("image"),
        _ => None,
    }
}

// The file menu's Actions row: hidden at defaults, shown whenever an action or script matches the selection; `kind` is the selection's shared MIME top type when the host knows every selected path shares one.
pub fn actions_row(hidden: &[String], listing: &Listing, kind: Option<&str>) -> Option<Entry> {
    if crate::ui_logic_mn1::rows::is_hidden(hidden, ID) || listing.actions.is_empty() {
        return None;
    }
    let mut entry = Entry::row(ID, Text::Key(LABEL_KEY), GLYPH);
    entry.action = FIRE.into();
    let mark = kind.and_then(kind_mark);
    entry.submenu = Some(listing.actions.iter().map(|a| leaf(a, mark)).collect());
    Some(entry)
}

fn leaf(action: &Action, mark: Option<&'static str>) -> Entry {
    let mut entry = Entry::leaf(action.id.clone(), Text::Data(action.name.clone()));
    entry.action = FIRE.into();
    // A manifest action is marked with the kind it takes; scripts and leaves with no boarded kind keep the flyout's terminal mark.
    entry.glyph = if action.script {
        Some(FLYOUT_GLYPH)
    } else {
        mark.or(Some(FLYOUT_GLYPH))
    };
    entry
}

// Every mark the window can draw by name: mn1's own cut table, plus the three mn2 cuts handed to the join as data (MARKS); a plugin row names any of them and mn1's draw filters, and any other mark stays with the plugin's own SVG, which the host draws from its mark path at join.
pub fn static_glyph(name: &str) -> Option<&'static str> {
    Some(match name {
        "app-window" => "app-window",
        "archive" => "archive",
        "archive-out" => "archive-out",
        "bot" => "bot",
        "check" => "check",
        "checkmark" => "checkmark",
        "chevron-right" => "chevron-right",
        "clipboard" => "clipboard",
        "code" => "code",
        "columns" => "columns",
        "contrast" => "contrast",
        "copy" => "copy",
        "download" => "download",
        "drive" => "drive",
        "eject" => "eject",
        "eye" => "eye",
        "eye-off" => "eye-off",
        "file" => "file",
        "file-plus" => "file-plus",
        "file-text" => "file-text",
        "folder" => "folder",
        "folder-open" => "folder-open",
        "folder-plus" => "folder-plus",
        "globe" => "globe",
        "hash" => "hash",
        "image" => "image",
        "images" => "images",
        "info" => "info",
        "key" => "key",
        "lock" => "lock",
        "minus" => "minus",
        "network" => "network",
        "play" => "play",
        "plus" => "plus",
        "printer" => "printer",
        "rename" => "rename",
        "scissors" => "scissors",
        "server" => "server",
        "shelf" => "shelf",
        "sliders" => "sliders",
        "sort" => "sort",
        "star" => "star",
        "symlink" => "symlink",
        "tag" => "tag",
        "terminal" => "terminal",
        "trash" => "trash",
        "type" => "type",
        "undo" => "undo",
        "zap" => "zap",
        _ => return None,
    })
}

// The cut paths mn2 hands the join so it can register what mn1 does not draw yet: zap and bot from the board source (`design-hig/source/ui.py`, lucide's zap and bot cut to the Omarchy edge), hash as the plugin board draws Verify Checksum (`boards/plugin-menu-rows.html`, lucide's hash).
pub const MARKS: &[(&str, &str)] = &[
    ("zap", "M13 2 3 14h9l-1 8 10-12h-9l1-8z"),
    ("bot", "M12 8V4H8 M4 8h16v12H4z M2 14h2 M20 14h2 M15 13v2 M9 13v2"),
    ("hash", "M4 9h16 M4 15h16 M10 3 8 21 M16 3l-2 18"),
];

// The handed cut path for a mark mn2 owns, if it owns one.
pub fn mark_path(name: &str) -> Option<&'static str> {
    MARKS.iter().find(|(id, _)| *id == name).map(|(_, path)| *path)
}

// Whether a chosen leaf previews rather than runs, read off the listing the menu was built from; None when that listing no longer names the leaf, so the host routes nothing.
pub fn leaf_previews(listing: &Listing, leaf: &str) -> Option<bool> {
    listing
        .actions
        .iter()
        .find(|action| action.id == leaf)
        .map(|action| action.mode == "preview")
}

// A plugin's row in the file menu, drawn exactly like a built-in row with its own mark and no badge, its ellipsis saying it asks for more; a top-level row fires its own action with no leaf, so the action is `plugin:action`, the host names the plugin from it, and two plugin rows never share a verb.
pub fn plugin_rows(contributions: &[Contribution]) -> Vec<Entry> {
    contributions
        .iter()
        .map(|c| {
            let mut entry = Entry::leaf(plugins::leaf(c), Text::Data(c.label.clone()));
            entry.action = plugins::leaf(c).into();
            entry.glyph = static_glyph(&c.glyph);
            entry
        })
        .collect()
}
