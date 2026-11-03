//! The mn2 rows as `mn1` entries (Open in Agent, the Actions flyout, the plugin menu-action rows), spliced into the inventory order the boards draw; switches arrive as the effective hidden set the menu reads.
use super::agent;
use super::plugins::{self, Contribution};
use super::wire::{Action, Listing};
use crate::ui_logic_mn1::entry::{Entry, Text, MARK_NAMES};

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

// The mark a flyout leaf draws for the MIME globs its action takes: an action whose every glob is one image type or image/* carries the image mark, as the board's three image actions do, and any other leaf keeps the flyout's own mark.
pub fn kind_mark(globs: &[String]) -> Option<&'static str> {
    let top = globs.first()?.split('/').next()?;
    if !globs.iter().all(|glob| glob.split('/').next() == Some(top)) {
        return None;
    }
    match top {
        "image" => Some("image"),
        _ => None,
    }
}

// The file menu's Actions row: hidden at defaults, shown whenever an action or script matches the selection.
pub fn actions_row(hidden: &[String], listing: &Listing) -> Option<Entry> {
    if crate::ui_logic_mn1::rows::is_hidden(hidden, ID) || listing.actions.is_empty() {
        return None;
    }
    let mut entry = Entry::row(ID, Text::Key(LABEL_KEY), GLYPH);
    entry.action = FIRE.into();
    entry.submenu = Some(listing.actions.iter().map(leaf).collect());
    Some(entry)
}

fn leaf(action: &Action) -> Entry {
    let mut entry = Entry::leaf(action.id.clone(), Text::Data(action.name.clone()));
    entry.action = FIRE.into();
    // A manifest action is marked with the kind its own globs take; scripts and leaves with no boarded kind keep the flyout's terminal mark.
    entry.glyph = if action.script {
        Some(FLYOUT_GLYPH)
    } else {
        kind_mark(&action.mime).or(Some(FLYOUT_GLYPH))
    };
    entry
}

// Every mark the window can draw by name: mn1's one list (MARK_NAMES), plus the mn2 cuts handed to the join as data (MARKS); a plugin row names any of them and mn1's draw filters, and any other mark stays with the plugin's own SVG, which the host draws from its mark path at join.
pub fn static_glyph(name: &str) -> Option<&'static str> {
    MARK_NAMES
        .iter()
        .copied()
        .chain(MARKS.iter().map(|(id, _)| *id))
        .find(|id| *id == name)
}

// The cut paths mn2 hands the join for marks mn1's cut table does not hold: hash as the plugin board draws Verify Checksum (`boards/plugin-menu-rows.html`, lucide's hash); zap and bot live only in that table.
pub const MARKS: &[(&str, &str)] = &[
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
