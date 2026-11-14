//! The panel and sheet state, driven by key names the way the window delivers them.
use super::geometry::{fade_shown, reveal, Scale, FADE};
use super::panel::{Panel, Press, Side};
use super::seams::{merge, Facts, MemoryStore, Request};
use super::sheet::{Sheet, Sources};
use flea::i18n::{I18n, Paths};
use flea::jsondoc::{parse, render, Json};
use flea::ui_logic_settings_pages::{
    pages::SECTIONS,
    rows::{focusable, Kind},
    sheet::Decision,
};
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn english() -> Arc<I18n> {
    let missing = PathBuf::from("/nonexistent/flea-st1-test");
    Arc::new(I18n::load(Paths {
        omarchy: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/i18n/fixtures/omarchy"),
        language_file: missing.join("language"),
        catalogs: Vec::new(),
    }))
}

pub(super) fn panel(document: &str) -> Panel {
    let store = MemoryStore::new(parse(document).expect("fixture parses"));
    Panel::new(Box::new(store), Facts::default(), english())
}

pub(super) fn press(panel: &mut Panel, keys: &[&str]) {
    for key in keys {
        let text = if key.chars().count() == 1 { *key } else { "" };
        panel.key(key, text, false);
    }
}

fn label(panel: &Panel) -> String {
    panel.rows[panel.cursor].label.resolve(&panel.catalog)
}

#[test]
fn opens_on_view_at_its_first_control() {
    let panel = panel("{}");
    assert_eq!(
        (panel.section, panel.side, panel.title().as_str()),
        ("view", Side::Pane, "View")
    );
    assert!(focusable(&panel.rows[panel.cursor]));
    assert_eq!(label(&panel), "Last-used view");
}

#[test]
fn j_and_k_walk_controls_and_skip_headings_and_hints() {
    let mut panel = panel("{}");
    press(&mut panel, &["j", "down", "j"]);
    assert_eq!(label(&panel), "Columns");
    press(&mut panel, &["k"]);
    assert_eq!(label(&panel), "Row density");
    press(&mut panel, &["up", "up", "up"]);
    assert_eq!(
        label(&panel),
        "Last-used view",
        "the walk stops at the first control"
    );
}

#[test]
fn tab_hands_the_keys_to_the_rail_which_steps_sections() {
    let mut panel = panel("{}");
    press(&mut panel, &["tab", "down", "down"]);
    assert_eq!((panel.side, panel.section), (Side::Rail, "plugins"));
    press(&mut panel, &["l", "h"]);
    assert_eq!(
        panel.section, "plugins",
        "h and l never reach across from the rail"
    );
    press(&mut panel, &["up", "up", "up", "up"]);
    assert_eq!(panel.section, SECTIONS[0].id);
    press(&mut panel, &["enter"]);
    assert_eq!(panel.side, Side::Pane);
}

#[test]
fn space_toggles_a_check_and_enter_walks_a_choice() {
    let mut panel = panel("{}");
    while label(&panel) != "Folders first" {
        press(&mut panel, &["j"]);
    }
    press(&mut panel, &["space"]);
    assert_eq!(
        panel.document().get("foldersFirst").and_then(Json::as_bool),
        Some(false)
    );
    press(&mut panel, &["k", "space"]);
    assert_eq!(
        panel.document().get("sort"),
        None,
        "space never walks a choice"
    );
    press(&mut panel, &["enter"]);
    assert_eq!(
        panel
            .document()
            .get("sort")
            .and_then(|s| s.get("key"))
            .and_then(Json::as_str),
        Some("size")
    );
    press(&mut panel, &["h", "h"]);
    assert_eq!(
        panel
            .document()
            .get("sort")
            .and_then(|s| s.get("key"))
            .and_then(Json::as_str),
        Some("kind")
    );
}

#[test]
fn a_refused_write_names_itself_in_the_footer() {
    let mut store = MemoryStore::new(parse("{}").unwrap());
    store.refuse = true;
    let mut panel = Panel::new(Box::new(store), Facts::default(), english());
    press(&mut panel, &["j", "l"]);
    assert!(
        panel
            .rows
            .iter()
            .any(|row| row.footer && row.label.resolve(&panel.catalog) == "Saved"),
        "l never toggles a check"
    );
    press(&mut panel, &["space"]);
    let footer = panel.rows.iter().find(|row| row.footer).expect("footer");
    assert_eq!(
        footer.label.resolve(&panel.catalog),
        "That setting could not be saved."
    );
    assert_eq!(panel.document().get("rememberViewPerFolder"), None);
}

#[test]
fn columns_opens_its_page_and_back_returns_to_view() {
    let mut panel = panel("{}");
    while label(&panel) != "Columns" {
        press(&mut panel, &["j"]);
    }
    press(&mut panel, &["enter"]);
    assert_eq!((panel.section, panel.title().as_str()), ("columns", "View"));
    while panel.rows[panel.cursor].id != "backView" {
        press(&mut panel, &["j"]);
    }
    press(&mut panel, &["enter"]);
    assert_eq!(panel.section, "view");
}

#[test]
fn escape_closes_and_the_keyboard_row_hands_over_to_the_sheet() {
    let mut panel = panel("{}");
    press(&mut panel, &["escape"]);
    assert_eq!(panel.take_requests(), [Request::Close]);
    panel
        .rows
        .push(flea::ui_logic_settings_pages::rows::Row::action(
            "keyboardSheet",
            "settings.title",
            "",
            Default::default(),
        ));
    panel.activate(panel.rows.len() - 1);
    assert_eq!(panel.take_requests(), [Request::Close, Request::OpenSheet]);
}

#[test]
fn slash_searches_every_section_and_enter_opens_the_row() {
    let mut panel = panel("{}");
    press(&mut panel, &["/"]);
    assert_eq!(panel.query.as_deref(), Some(""));
    assert_eq!(panel.title(), "View", "an empty query keeps the section");
    press(&mut panel, &["t", "r", "a", "s", "h"]);
    assert_eq!(
        panel.title(),
        "Settings",
        "no section is current while a query stands"
    );
    let sections: Vec<&str> = panel
        .results
        .iter()
        .map(|group| SECTIONS[group.section].id)
        .collect();
    assert!(
        sections.contains(&"places") && sections.contains(&"menus"),
        "{sections:?}"
    );
    press(&mut panel, &["enter"]);
    assert_eq!((panel.query.as_deref(), panel.section), (None, "places"));
    assert!(label(&panel).contains("Trash"), "{}", label(&panel));
}

#[test]
fn escape_and_backspace_leave_the_search_before_the_panel() {
    let mut panel = panel("{}");
    press(&mut panel, &["/", "z", "escape"]);
    assert_eq!(panel.take_requests().len(), 0);
    assert_eq!(panel.query, None);
    press(&mut panel, &["/", "backspace"]);
    assert_eq!(panel.query, None);
    press(&mut panel, &["/", "q", "backspace"]);
    assert_eq!(panel.query.as_deref(), Some(""));
}

// EY-197: Shift+J reorders with shifted, unshifted or missing text; plain j still walks the cursor.
#[test]
fn shift_j_moves_a_favourite_whatever_text_the_press_carries() {
    for text in ["J", "j", ""] {
        let mut panel = panel(
            r#"{"places":{"favourites":[{"label":"P","path":"/p"},{"label":"Q","path":"/q"}]}}"#,
        );
        panel.show_section("places");
        press(&mut panel, &["j"]);
        assert_eq!(panel.rows[panel.cursor].favourite, Some(0));
        panel.key("j", text, true);
        assert_eq!(
            panel.take_requests(),
            [Request::MoveFavourite { from: 0, to: 1 }],
            "text {text:?}"
        );
        assert_eq!(
            panel.rows[panel.cursor].favourite,
            Some(0),
            "text {text:?} moved the cursor instead"
        );
        panel.key("k", &text.replace('J', "K").replace('j', "k"), true);
        assert!(
            panel.take_requests().is_empty(),
            "the first favourite moves no higher"
        );
    }
}

#[test]
fn a_favourite_row_hands_its_moves_to_the_host() {
    let mut panel =
        panel(r#"{"places":{"favourites":[{"label":"P","path":"/p"},{"label":"Q","path":"/q"}]}}"#);
    panel.show_section("places");
    press(&mut panel, &["j"]);
    assert_eq!(panel.rows[panel.cursor].kind, Kind::Favourite);
    panel.key("k", "K", true);
    assert!(
        panel.take_requests().is_empty(),
        "the first favourite moves no higher"
    );
    panel.key("j", "J", true);
    press(&mut panel, &["x", "enter"]);
    assert_eq!(
        panel.take_requests(),
        [
            Request::MoveFavourite { from: 0, to: 1 },
            Request::RemoveFavourite(0),
            Request::Host("favourite:0".into()),
        ]
    );
    // The pointer reaches the same writers: the grip dragged past the end stops at the last row, the x removes.
    panel.press(panel.cursor, Press::Drag(5));
    panel.press(panel.cursor, Press::Remove);
    assert_eq!(
        panel.take_requests(),
        [
            Request::MoveFavourite { from: 0, to: 1 },
            Request::RemoveFavourite(0)
        ]
    );
}

// tests/settings-tail.qml: every step keeps the cursor row inside the pane and clear of the fade; the last control rises to the top.
#[test]
fn every_section_keeps_its_cursor_inside_the_pane_and_clear_of_the_fade() {
    let mut panel = panel(
        r#"{"places":{"favourites":[{"label":"A","path":"/a"},{"label":"B","path":"/b"},{"label":"C","path":"/c"},{"label":"D","path":"/d"},{"label":"E","path":"/e"}]}}"#,
    );
    let mut scrolled = Vec::new();
    for section in SECTIONS.iter().map(|section| section.id).chain(["columns"]) {
        panel.show_section(section);
        assert_eq!(panel.scroll, 0., "{section} opens at its top");
        let viewport = panel.pane.1;
        for step in 0..panel.rows.len() {
            let heights = panel.heights();
            let top: f32 = heights.iter().take(panel.cursor).sum();
            let bottom = top + heights[panel.cursor];
            assert!(
                top >= panel.scroll && bottom <= panel.scroll + viewport,
                "{section} step {step}: cursor inside"
            );
            if fade_shown(&heights, panel.scroll, viewport) {
                assert!(
                    bottom <= panel.scroll + viewport - FADE,
                    "{section} step {step}: fade over cursor"
                );
            }
            press(&mut panel, &["j"]);
        }
        let heights = panel.heights();
        let total: f32 = heights.iter().sum();
        if total > viewport {
            scrolled.push(section);
            let top: f32 = heights.iter().take(panel.cursor).sum();
            assert_eq!(
                panel.scroll,
                top.min(total - viewport),
                "{section}: the last control rises as far as the rows allow"
            );
            assert_eq!(
                panel.scroll,
                total - viewport,
                "{section}: revealed to its end"
            );
            assert!(
                !fade_shown(&heights, panel.scroll, viewport),
                "{section}: no fade at the end"
            );
        }
    }
    assert!(
        scrolled.contains(&"view") && scrolled.contains(&"menus"),
        "{scrolled:?}"
    );
}

#[test]
fn reveal_moves_only_as_far_as_the_cursor_needs() {
    let panel = panel("{}");
    let (rows, heights, viewport) = (&panel.rows, panel.heights(), panel.pane.1);
    assert_eq!(
        reveal(rows, &heights, 1, 0., viewport, Scale::default()),
        0.
    );
    assert_eq!(
        reveal(rows, &heights, 1, 200., viewport, Scale::default()),
        heights[0]
    );
}

#[test]
fn the_memory_store_merges_leaf_by_leaf() {
    let mut held =
        parse(r#"{"places":{"rail":"shown","driveSize":false},"columns":["name"]}"#).unwrap();
    merge(
        &mut held,
        &parse(r#"{"places":{"driveSize":true},"columns":["size"]}"#).unwrap(),
    );
    assert_eq!(
        render(&held).replace(['\n', ' '], ""),
        r#"{"places":{"rail":"shown","driveSize":true},"columns":["size"]}"#
    );
}

#[test]
fn the_sheet_query_types_ranks_steps_and_runs() {
    let mut sheet = Sheet::new("default", english(), Sources::default());
    assert!(!sheet.rows.is_empty());
    for key in ["p", "e", "r", "m"] {
        sheet.key(key, key);
    }
    let shown: Vec<String> = sheet
        .ranked
        .iter()
        .map(|row| format!("{} {}", row.keys, row.label.resolve(&sheet.catalog)))
        .collect();
    assert_eq!(shown, ["shift-delete Delete permanently"]);
    sheet.key("down", "");
    assert_eq!(sheet.cursor, 0, "one row wraps onto itself");
    sheet.key("enter", "");
    assert_eq!(
        sheet.decided,
        Some(Decision::Action {
            action: "deletePermanently".into()
        })
    );
    assert_eq!(std::mem::take(&mut sheet.requests), [Request::Close]);
}

#[test]
fn the_sheet_closes_on_escape_and_on_backspace_when_empty() {
    let mut sheet = Sheet::new("vim", english(), Sources::default());
    sheet.key("shift", "");
    sheet.key("delete", "");
    assert!(
        sheet.requests.is_empty(),
        "a bare modifier and Delete never close"
    );
    sheet.key("a", "a");
    sheet.key("backspace", "");
    assert!(sheet.requests.is_empty() && sheet.query.is_empty());
    sheet.key("backspace", "");
    assert_eq!(std::mem::take(&mut sheet.requests), [Request::Close]);
    sheet.key("escape", "");
    assert_eq!(sheet.requests, [Request::Close]);
}

// ui/SettingsPanel.qml:117-118: Chosen folder writes only when a pane stands behind the panel.
#[test]
fn chosen_folder_with_no_pane_behind_the_panel_writes_nothing() {
    let mut panel = panel("{}");
    panel.show_section("view");
    let at = panel
        .rows
        .iter()
        .position(|row| row.id == "startFolder")
        .expect("Chosen folder row");
    panel.press(at, Press::Row);
    panel.choose_start_folder("");
    let word = |key: &str| {
        panel
            .document()
            .get(key)
            .and_then(Json::as_str)
            .map(str::to_owned)
    };
    assert_eq!((word("startIn"), word("startFolder")), (None, None));
    assert!(
        !panel
            .requests
            .contains(&Request::Host("startFolder".into())),
        "the panel decides it, no host request"
    );
}

#[test]
fn chosen_folder_stores_the_focused_pane_and_reads_it_back() {
    let store = MemoryStore::new(parse("{}").expect("fixture parses"));
    let mut panel = Panel::new(
        Box::new(store),
        Facts {
            pane_path: "/home/gm/Work".into(),
            ..Facts::default()
        },
        english(),
    );
    panel.show_section("view");
    let at = panel
        .rows
        .iter()
        .position(|row| row.id == "startFolder")
        .expect("Chosen folder row");
    panel.press(at, Press::Row);
    assert_eq!(
        panel.document().get("startFolder").and_then(Json::as_str),
        Some("/home/gm/Work")
    );
    assert_eq!(
        panel.rows[at].value.resolve(&panel.catalog),
        "/home/gm/Work"
    );
    panel.choose_start_folder("/srv/archive");
    assert_eq!(
        panel.document().get("startFolder").and_then(Json::as_str),
        Some("/srv/archive")
    );
}
