// Each Settings board's 460px pane, string for string in drawing order, against the panel's rows (JetBrains Mono at 0.6 em).
use super::super::about::{self, Facts};
use super::super::pages::{self, State};
use super::super::test_support::english;
use super::super::update::{State as Update, Status};
use super::super::rows::{Kind, Master, Row};
use super::super::Text;
use super::*;
use crate::jsondoc::parse;

const PANE_W: f32 = 460.;

fn width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.6
}

fn board_facts() -> Facts {
    Facts {
        version: "0.4.0".into(),
        built: String::new(),
        source: about::installed_from("", false),
        package: Some(String::new()),
        handler: "kitty-open.desktop".into(),
        update: Status { state: Update::Unchecked, kind: "local".into(), ..Status::default() },
    }
}

fn drawn(section: &str, fixture: &str) -> Vec<String> {
    let i18n = english();
    let data = parse(fixture).expect("fixture parses");
    let state = State { home: "/home/gm", ..State::new(&data) };
    // Make Flea the default is its owner's row, handed in; the stub stands in its place.
    let make_default = vec![Row::new(Kind::Check, Text::data("Make Flea the default"))];
    let rows = pages::rows(section, &state, Vec::new(), || about::rows(&board_facts(), true, "English", make_default));
    rows.iter().flat_map(|row| {
        let options: Vec<f32> = row.labels.iter().map(|label| width(&label.resolve(&i18n), 12.)).collect();
        let segment = segment_fits(width(&row.label.resolve(&i18n), 14.), &options, PANE_W);
        let mut out = said(row, segment, &i18n);
        // The board's own box states, so a tick that drifts fails here too.
        let master = matches!(row.kind, Kind::Group) && row.master;
        if row.kind == Kind::Check || master {
            let state = match (master, row.state, row.on) {
                (true, Master::All, _) | (false, _, true) => "[on]",
                (true, Master::Some, _) => "[some]",
                _ => "[off]",
            };
            out.push(state.to_owned());
        }
        out
    }).collect()
}

const BOARDS: &[(&str, &str, &str, &[&str])] = &[
    ("settings-view", "view", r#"{}"#, &["View", "Last-used view", "Remember each folder's view", "[off]", "Row density", "Tight", "Compact", "Normal", "Comfortable", "Compact stays the default.", "Columns", "Name, Size, Modified", "Address bar", "Path", "Breadcrumb", "Window bar", "As shipped", "Columns view limit", "2", "3", "4", "5", "Show Git status", "[off]", "Highlight today's dates", "[off]", "Open folders while dragging", "[off]", "Sorting", "Sort by", "Name", "Size", "Modified", "Kind", "Folders first", "[on]", "Group by kind", "folders, photos, files", "[off]", "Show hidden files", "[off]", "Hidden files last", "[off]", "Show code icons", "[on]", "Remember each folder's sort", "[on]", "Cursor", "Wrap at list ends", "up at the top", "[off]", "Escape goes up a folder", "with nothing to close", "[off]", "Opening", "Flea opens in", "Home", "Last", "Chosen", "Chosen folder", "Use this folder", "New tabs open in", "Current folder", "Open items with", "Double Click", "Single Click", "Click a selected name to rename", "[on]", "Saved"]),
    ("settings-display", "display", r#"{}"#, &["Text Size", "Text size", "Follow Omarchy", "Override", "9", "10", "11", "12", "14", "16", "20", "Ctrl+Shift++ and Ctrl+Shift+- step through them; Ctrl+Shift+0 follows Omarchy again.", "Effective", "14 px", "Omarchy's own size.", "Scale", "Scale", "1x", "Flea follows Hyprland's scale and does not change it.", "Appearance", "Hyprland-aware icons", "[off]"]),
    ("settings-keys", "keys", r#"{}"#, &["Preset", "Keybinding preset", "Default", "Vim", "Mac", "Windows", "Keys change, actions do not.", "This Preset", "arrows", "Move cursor", "enter", "Open", "space", "Quick Look", "y", "Copy", "p", "Paste", "dd", "Move to Trash", "Press ? for Keyboard Shortcuts."]),
    ("settings-places", "places", r#"{}"#, &["Favorites", "Add This Folder", "Built In", "Home", "[on]", "Recent", "[off]", "Network", "[on]", "Devices", "[on]", "Trash", "[on]", "Sidebar", "Show drive size", "[off]", "Show Trash count", "[off]", "Show unmounted drives", "[on]", "Show tags", "[off]", "Auto-hide sidebar", "[off]", "Show sidebar", "[on]", "Sidebar width", "160 px", "192 px", "224 px", "256 px", "Trash", "Empty after 30 days", "permanently", "[off]"]),
    ("settings-favorites", "places", r#"{"places":{"favourites":[{"label":"Docs","path":"/fx/v/work/Docs"},{"label":"Media","path":"/fx/v/work/Media"}]}}"#, &["Favorites", "Add This Folder", "Docs", "/fx/v/work/Docs", "Media", "/fx/v/work/Media", "Built In", "Home", "[on]", "Recent", "[off]", "Network", "[on]", "Devices", "[on]", "Trash", "[on]", "Sidebar", "Show drive size", "[off]", "Show Trash count", "[off]", "Show unmounted drives", "[on]", "Show tags", "[off]", "Auto-hide sidebar", "[off]", "Show sidebar", "[on]", "Sidebar width", "160 px", "192 px", "224 px", "256 px"]),
    ("settings-menus", "menus", r#"{"menu":{"hidden":["delete","openTerminal","placeMenu","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent"]}}"#, &["Basic File Actions", "6 of 6", "[on]", "Cut", "[on]", "Copy", "[on]", "Paste", "[on]", "Duplicate", "[on]", "Rename", "[on]", "Move to Trash", "[on]", "Destructive", "Delete Permanently…", "off by default", "[off]", "Open and Inspect", "5 of 14", "[some]", "Open With", "[on]", "Print…", "[on]", "Run", "[off]", "Open as Text", "[off]", "Open in Terminal", "[off]", "Open in Agent", "[off]", "Move To…", "[off]", "Copy To…", "[off]", "Properties", "[off]"]),
    ("settings-menus-extras", "menus", r#"{"menu":{"hidden":["delete","openTerminal","placeMenu","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent"]}}"#, &["Send with Taildrop", "[on]", "Send with LocalSend", "[on]", "Send with rclone", "[on]", "Move to Dropbox", "[on]", "Copy Share Link", "[on]", "Actions", "[on]", "Places row menu", "[off]", "Update Flea", "[on]", "Show Code Icons", "[on]", "Show Thumbnails", "[off]", "Paste As", "[off]", "Invert Selection", "[off]", "Shortcuts", "Show keyboard hints", "[off]", "Every key stays bound either way.", "Always Shown", "Open", "Show Hidden Files"]),
    ("settings-preview", "preview", r#"{}"#, &["Preview Column", "Show the preview column", "[on]", "Load", "Automatic", "Manual", "Automatic follows the cursor.", "Thumbnails", "Thumbnails", "Off", "Images", "Everything", "Thumbnail size", "Medium", "On network shares", "[off]", "On phones", "[off]", "On USB drives", "[on]", "Off still shows thumbnails that were already made.", "Zoom the grid with ctrl and scroll", "[on]"]),
    ("settings-about", "about", r#"{}"#, &["Flea", "A file manager for Omarchy", "Version", "0.4.0", "Built", "Not recorded in this build", "Installed from", "Not from a package", "Package", "Not owned by a package", "License", "MIT, © 2026 GM", "Language", "Language", "English", "Updates", "Update Flea", "Built from source", "Check automatically", "when About opens, every 6 h", "[on]", "Rebuild from the checkout to update.", "This Computer", "File manager", "kitty-open.desktop", "Make Flea the default", "[off]"]),
];

#[test]
fn every_settings_board_reads_as_the_panel_draws_it() {
    let mut wrong = Vec::new();
    for (board, section, fixture, want) in BOARDS {
        let got = drawn(section, fixture);
        // Menus, Favorites and About show the window of a pane taller than the view, so they match a run of it.
        let found = if matches!(*board, "settings-menus" | "settings-menus-extras" | "settings-favorites" | "settings-about") {
            got.windows(want.len()).any(|run| run == *want)
        } else {
            got == *want
        };
        if !found {
            wrong.push(format!("{board}\n  board: {want:?}\n  drawn: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_choice_too_wide_for_its_row_walks() {
    assert!(segment_fits(width("Row density", 14.), &["Tight", "Compact", "Normal", "Comfortable"].map(|o| width(o, 12.)), PANE_W));
    assert!(!segment_fits(width("New tabs open in", 14.), &["Current folder", "Home", "Start folder"].map(|o| width(o, 12.)), PANE_W));
    assert!(segment_fits(0., &[0.], 74. + CHIP_MIN));
    assert!(!segment_fits(0., &[0.], 74. + CHIP_MIN - 1.));
}

#[test]
fn a_walked_choice_shows_its_value_and_marks_show_no_words() {
    let row = super::super::rows::choice("newTab", "settings.row.new_tab", "", &["current", "home"], &["settings.choice.current_folder", "settings.choice.home"], "home");
    assert_eq!(said(&row, false, &english()), ["New tabs open in", "Home"]);
    let view = super::super::rows::choice("view", "settings.row.view", "", &["list"], &["settings.choice.view_list"], "list");
    assert_eq!(said(&view, true, &english()), ["Last-used view"]);
    assert!(texts(&super::super::rows::Row::hint(Text::Empty), true).is_empty());
}
