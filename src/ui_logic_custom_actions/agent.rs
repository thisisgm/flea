//! Open in Agent (the background menu row and the A key): with no default agent chosen plain `omarchy agent` only errors, so both open Omarchy's default agent in the folder through `omarchy agent --pick`, spawned directly and detached with the folder as its working directory, never through the OEM terminal route, with no shell interpolation anywhere.

// The stored id Settings > Menus hides by; shipped off, as the board draws its switch unticked.
pub const ID: &str = "openAgent";
pub const LABEL_KEY: &str = "actions.row.open_agent";
// Lucide's bot cut to the Omarchy edge, as the board source draws it.
pub const GLYPH: &str = "bot";

// Pressing A in the listing does the same with no menu.
pub const KEY: &str = "A";

// The argv the launcher runs in the background menu's folder.
pub fn argv() -> Vec<String> {
    vec!["omarchy".into(), "agent".into(), "--pick".into()]
}

// The spawn pair the join hands to its own detached spawn (the same pair as `tui::launch::agent`): argv with no shell interpolation, and the folder as its working directory; the spawn itself stays with the join because the core crate keeps zero dependencies and no process handles.
pub fn spawn(folder: &str) -> (&str, Vec<String>) {
    (folder, argv())
}
