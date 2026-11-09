//! Keymap board fidelity: the resting sheet draws every board string, and each board query lists the board's rows in its order.
use super::sheet::{Sheet, Sources};
use super::tests::english;
use flea::keys::SHEET_GROUPS;
use flea::ui_logic_settings_pages::{
    caps,
    sheet::{MenuEntry, MenuLeaf, RailEntry},
    Text,
};

// The menu, rail and recent rows the boards were drawn over; in the window they come from the menu, rail and recents seams.
fn board_sources() -> Sources {
    let entry = |label: &str, action: &str, reason: &str| MenuEntry {
        label: Text::data(label),
        action: action.into(),
        disabled: !reason.is_empty(),
        reason: if reason.is_empty() {
            Text::Empty
        } else {
            Text::data(reason)
        },
        ..Default::default()
    };
    let leaf = |id: &str, label: &str| MenuLeaf {
        id: id.into(),
        label: Text::data(label),
        ..Default::default()
    };
    let compress = MenuEntry {
        submenu: vec![leaf("password", "With Password…")],
        ..entry("Compress", "compress", "")
    };
    let copy_as = MenuEntry {
        submenu: vec![
            leaf("path", "Path"),
            leaf("folder", "Folder Path"),
            leaf("stem", "Name Without Extension"),
        ],
        ..entry("Copy As", "copyAs", "")
    };
    Sources {
        menus: vec![
            entry("Paste", "paste", "Nothing to paste"),
            entry("Paste As", "pasteAs", "Nothing to paste"),
            entry("Permissions…", "permissions", ""),
            compress,
            copy_as,
        ],
        rail: vec![RailEntry {
            label: "Home".into(),
            group: "places".into(),
            kind: "home".into(),
            identity: "home".into(),
        }],
        recent: vec!["/fx/v/work/Media/photo.png".into()],
        home: "/home/gm".into(),
    }
}

fn queried(query: &str) -> Vec<String> {
    let mut sheet = Sheet::new("default", english(), board_sources());
    for letter in query.chars() {
        let text = letter.to_string();
        sheet.key(&text, &text);
    }
    sheet
        .ranked
        .iter()
        .map(|row| {
            let place = row.where_.resolve(&sheet.catalog);
            let place = if place.is_empty() {
                String::new()
            } else {
                format!(" {}", sheet.catalog.tr_args("settings.sheet.in", &[&place]))
            };
            let dead = if row.disabled {
                format!(" [{}]", row.reason.resolve(&sheet.catalog))
            } else {
                String::new()
            };
            format!(
                "{}|{}{place}{dead}",
                row.keys,
                row.label.resolve(&sheet.catalog)
            )
        })
        .collect()
}

// Headings match in caps::heading's capitals; A opens the agent and ctrl-g searches contents.
#[test]
fn the_resting_sheet_draws_every_keymap_board_string() {
    let sheet = Sheet::new("default", english(), Sources::default());
    let mut drawn = vec![
        sheet.catalog.tr("keys.action.keymapSheet"),
        "esc".into(),
        "/".into(),
    ];
    let headings: Vec<String> = SHEET_GROUPS
        .iter()
        .map(|group| caps::heading(&sheet.catalog.tr(group.label_key), sheet.catalog.language()))
        .collect();
    for row in &sheet.rows {
        drawn.push(row.label.clone());
        drawn.extend(row.keys.split(" / ").map(str::to_owned));
    }
    let left: Vec<&str> = super::board_strings::KEYMAP
        .iter()
        .copied()
        .filter(|text| {
            !drawn.iter().any(|piece| piece == text) && !headings.contains(&text.to_uppercase())
        })
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn the_perm_query_lists_the_two_board_rows() {
    assert_eq!(
        queried("perm"),
        ["shift-delete|Delete permanently", "|Permissions…"]
    );
}

#[test]
fn the_pa_query_lists_the_dead_key_board_in_its_order() {
    assert_eq!(
        queried("pa"),
        [
            "ctrl-4|Dual pane",
            "F|Page preview forward",
            "B|Page preview back",
            "ctrl-shift-c|Copy path",
            "ctrl-d|Page down",
            "ctrl-u|Page up",
            "p / ctrl-v|Paste [Nothing to paste]",
            "h / left|Parent folder",
            "Y|Copy folder path",
            "P|Paste as [Nothing to paste]",
            "e|Expand",
            "|With Password… in Compress",
            "|Path in Copy As",
            "|Folder Path in Copy As",
            "ctrl-space|Preview",
            "ctrl-pagedown|Next tab",
            "ctrl-pageup|Previous tab",
            "space|Quick Look",
        ]
    );
}

#[test]
fn the_ho_query_lists_the_field_board_in_its_order() {
    assert_eq!(
        queried("ho"),
        [
            ". / ctrl-shift-.|Show hidden files",
            "|Name Without Extension in Copy As",
            "|Open Home in Places",
            "|Open photo.png in /fx/v/work/Media",
            "g / home|First item",
        ]
    );
}
