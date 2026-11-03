//! Board keymap: every group draws its rows in the board's order, in the board's column, under the board's heading.
use super::sheet::{Sheet, Sources};
use super::tests::english;

fn drawn() -> Vec<(usize, String, Vec<String>)> {
    let sheet = Sheet::new("default", english(), Sources::default());
    sheet
        .groups()
        .into_iter()
        .map(|(side, group, rows)| {
            let rows = rows
                .iter()
                .map(|row| format!("{}|{}", row.keys, row.label))
                .collect();
            (side, sheet.catalog.tr(group.label_key), rows)
        })
        .collect()
}

// The sheet follows keymap.html, including the List outline actions.
const MOVE: &[&str] = &[
    "j / down|Cursor down",
    "k / up|Cursor up",
    "g / home|First item",
    "G / end|Last item",
    "ctrl-d|Page down",
    "ctrl-u|Page up",
    "enter|Open",
    "h / left|Parent folder",
    "l / right|Browse in",
    "H|Back",
    "L|Forward",
    "tab|Next area",
    "shift-tab|Previous area",
    "ctrl-tab|Focus the preview",
    "t|New tab",
    "ctrl-enter|Open in new tab",
    "w / ctrl-w|Close tab",
    "ctrl-pagedown|Next tab",
    "ctrl-pageup|Previous tab",
    "{|Move tab left",
    "}|Move tab right",
    "ctrl-n|New window",
    "escape|Close or go back",
    "X|Carry to another app",
];

const LOOK: &[&str] = &[
    "ctrl-1|List view",
    "ctrl-2|Columns view",
    "ctrl-3|Grid view",
    "ctrl-4|Dual pane",
    "alt-p|Preview column",
    "i|Facts",
    "ctrl-space|Preview",
    "space|Quick Look",
    "F|Page preview forward",
    "B|Page preview back",
    ". / ctrl-shift-.|Show hidden files",
    "ctrl-b|Sidebar",
    "+|Zoom in",
    "-|Zoom out",
    "e|Expand",
    "ctrl-right|Open in place",
    "ctrl-left|Close in place",
    "m|Mute",
    "ctrl-shift-plus|Larger text",
    "ctrl-shift-minus|Smaller text",
    "ctrl-shift-0|Reset text size",
    "?|Keyboard Shortcuts",
];

const FIND: &[&str] = &[
    "/|Filter",
    "f / ctrl-f|Find",
    "ctrl-g|Search contents",
    "o|Reveal",
    "s|Sort",
    "S|Reverse sort",
    ": / ctrl-l|Go to folder",
    "ctrl-r / f5|Reload",
];

// The board's CHANGE column is cut by the card's foot after Open agent here, so the rows it draws open the group.
const CHANGE_TOP: &[&str] = &[
    "y / ctrl-c|Copy",
    "x / ctrl-x|Cut",
    "p / ctrl-v|Paste",
    "r / f2|Rename",
    "dd / delete|Move to Trash",
    "shift-delete|Delete permanently",
    "z / ctrl-z|Undo",
    "ctrl-shift-z|Redo",
    "ctrl-shift-n|New folder",
    "Y|Copy folder path",
    "ctrl-shift-c|Copy path",
    "A|Open agent here",
];

#[test]
fn keymap_sheet_groups_draw_in_the_board_order_and_columns() {
    let groups = drawn();
    let heads: Vec<(usize, &str)> = groups
        .iter()
        .map(|(side, head, _)| (*side, head.as_str()))
        .collect();
    assert_eq!(
        heads,
        [(0, "Move"), (1, "Look"), (1, "Find"), (1, "Change")],
        "MOVE fills the first column and LOOK opens the second whole"
    );
    assert_eq!(groups[0].2, MOVE);
    assert_eq!(groups[1].2, LOOK);
    assert_eq!(groups[2].2, FIND);
    assert_eq!(&groups[3].2[..CHANGE_TOP.len()], CHANGE_TOP);
}

#[test]
fn keymap_sheet_rows_reader_follows_the_drawn_order() {
    let sheet = Sheet::new("default", english(), Sources::default());
    let mut drawn = Vec::new();
    for (_, group, rows) in sheet.groups() {
        drawn.push(sheet.catalog.tr(group.label_key));
        drawn.extend(rows.iter().map(|row| format!("{} {}", row.keys, row.label)));
    }
    let read = crate::ipc::settings::sheet_rows(&sheet);
    assert_eq!(read.lines().collect::<Vec<_>>(), drawn);
    assert!(read.contains("Move\nj / down Cursor down\n"));
    assert!(read.contains("escape Close or go back\nX Carry to another app\nLook\n"));
}

#[test]
fn keymap_sheet_draws_every_row_once_in_every_preset() {
    for preset in flea::keys::PRESETS {
        let sheet = Sheet::new(preset, english(), Sources::default());
        let mut drawn: Vec<&str> = sheet
            .groups()
            .iter()
            .flat_map(|(_, _, rows)| rows.iter().map(|row| row.action))
            .collect();
        let mut held: Vec<&str> = sheet.rows.iter().map(|row| row.action).collect();
        drawn.sort_unstable();
        held.sort_unstable();
        assert_eq!(drawn, held, "{preset}");
    }
}
