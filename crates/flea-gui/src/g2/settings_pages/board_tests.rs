//! Board fidelity: every string a settings board draws inside its card is a string the card draws from the same state.
use super::board_strings as board;
use super::panel::Panel;
use super::seams::{merge, Facts, MemoryStore};
use super::tests::english;
use flea::jsondoc::parse;
use flea::ui_logic_settings_pages::{
    about,
    pages::SECTIONS,
    rows::Row,
    update::{self, Status},
    Text,
};

// The shipped menu.hidden of src/uischema.rs DEFAULTS, which the boards draw; the store hands Settings normalised state.
const SHIPPED_HIDDEN: &str = r#"{"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent"]}}"#;

fn open(document: &str, facts: Facts, section: &'static str) -> Panel {
    let mut state = parse(SHIPPED_HIDDEN).expect("defaults parse");
    merge(&mut state, &parse(document).expect("fixture parses"));
    let store = MemoryStore::new(state);
    let mut panel = Panel::new(Box::new(store), facts, english());
    panel.show_section(section);
    panel
}

// The rows the pane draws: the open section's, or every result group's heading and hits while a query stands.
fn drawn_rows(panel: &Panel) -> Vec<(Row, Option<&'static str>)> {
    if panel.query.as_deref().is_none_or(str::is_empty) {
        return panel.rows.iter().map(|row| (row.clone(), None)).collect();
    }
    let mut out = Vec::new();
    for group in &panel.results {
        let rows = panel.everything_rows(group.section);
        if let Some(heading) = group.heading {
            out.push((rows[heading].clone(), Some(SECTIONS[group.section].label)));
        }
        out.extend(
            group
                .hits
                .iter()
                .map(|found| (rows[found.row].clone(), None)),
        );
    }
    out
}

// Every string the card sets: head, rail, the field's query and each drawn row's wording.
fn drawn(panel: &Panel) -> Vec<String> {
    let catalog = &panel.catalog;
    let mut out = vec![panel.title(), "esc".to_owned()];
    out.extend(SECTIONS.iter().map(|section| catalog.tr(section.label)));
    out.extend(panel.query.clone());
    for (row, section) in drawn_rows(panel) {
        out.extend(section.map(|key| catalog.tr(key)));
        out.extend([
            row.label.resolve(catalog),
            row.value.resolve(catalog),
            row.caption.resolve(catalog),
            row.error.resolve(catalog),
            row.display.clone(),
        ]);
        out.extend(row.labels.iter().map(|label| label.resolve(catalog)));
        out.extend(row.values.iter().cloned());
        out.extend(row.stops.iter().map(u32::to_string));
        for item in &row.items {
            out.extend([item.keys.clone(), item.label.resolve(catalog)]);
        }
    }
    out.retain(|text| !text.is_empty());
    out
}

fn missing(panel: &Panel, strings: &[&str]) -> Vec<String> {
    let drawn = drawn(panel);
    strings
        .iter()
        .filter(|text| !drawn.iter().any(|piece| piece.contains(*text)))
        .map(|text| (*text).to_owned())
        .collect()
}

#[test]
fn the_view_boards_draw_every_string_of_view() {
    let panel = open("{}", Facts::default(), "view");
    for strings in [board::VIEW, board::GIT, board::OPENING] {
        assert_eq!(missing(&panel, strings), Vec::<String>::new());
    }
}

#[test]
fn the_places_boards_draw_every_string_of_places() {
    let panel = open("{}", Facts::default(), "places");
    assert_eq!(missing(&panel, board::PLACES), Vec::<String>::new());
    let favourites = r#"{"places":{"favourites":[{"label":"Docs","path":"/fx/v/work/Docs"},{"label":"Media","path":"/fx/v/work/Media"}]}}"#;
    let panel = open(favourites, Facts::default(), "places");
    assert_eq!(missing(&panel, board::FAVORITES), Vec::<String>::new());
}

// Open in Agent ships unticked: the agent contract (mn2) ships it in the default hidden set, so this document and the defaults draw the same count.
const AGENT_SHIPPED: &str = r#"{"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent"]}}"#;

#[test]
fn the_preview_keys_and_menus_boards_draw_every_string() {
    for (document, section, strings) in [
        ("{}", "preview", board::PREVIEW),
        ("{}", "keys", board::KEYS),
        (AGENT_SHIPPED, "menus", board::MENUS),
        (AGENT_SHIPPED, "menus", board::MENUS_EXTRAS),
    ] {
        let panel = open(document, Facts::default(), section);
        assert_eq!(missing(&panel, strings), Vec::<String>::new(), "{section}");
    }
    let panel = open("{}", Facts::default(), "menus");
    assert_eq!(
        missing(&panel, board::MENUS),
        Vec::<String>::new(),
        "the defaults count Open in Agent as off"
    );
    assert!(!drawn(&panel).iter().any(|text| text == "6 of 14"));
}

#[test]
fn the_display_board_draws_every_string_of_display() {
    let facts = Facts {
        base_size: 14,
        monitor_scale: 1.,
        ..Facts::default()
    };
    let panel = open("{}", facts, "display");
    assert_eq!(missing(&panel, board::DISPLAY), Vec::<String>::new());
}

// A source build: no package owns it and the updater is not asked; the make-default row is the make-default contract's, which the host supplies.
#[test]
fn the_about_board_draws_every_string_but_the_make_default_row() {
    let about = about::Facts {
        version: "0.4.0".into(),
        source: Text::Key("settings.about.source_unowned"),
        package: Some(String::new()),
        handler: "kitty-open.desktop".into(),
        update: Status {
            state: update::State::Unchecked,
            ..update::idle()
        },
        ..about::Facts::default()
    };
    let panel = open(
        "{}",
        Facts {
            language: "English".into(),
            about,
            ..Facts::default()
        },
        "about",
    );
    assert_eq!(missing(&panel, board::ABOUT), ["Make Flea the default"]);
}

#[test]
fn the_search_board_draws_every_string_of_its_results() {
    let mut panel = open("{}", Facts::default(), "view");
    for key in ["/", "t", "h", "u", "m", "b"] {
        let text = if key.len() == 1 { key } else { "" };
        panel.key(key, text, false);
    }
    assert_eq!(missing(&panel, board::SEARCH), Vec::<String>::new());
}

// Every mark the card can set for this state: the head, field and box chrome, the rail, each row's own mark, and the view strip.
fn drawn_marks(panel: &Panel) -> Vec<&'static str> {
    let mut out = vec![
        "sliders",
        "x",
        "search",
        "chevron-right",
        "checkmark-bold",
        "plus",
        "lock",
        "flea",
        "folder",
        "list",
        "columns",
        "grid",
        "dual",
    ];
    out.extend(SECTIONS.iter().map(|section| section.glyph));
    for (row, _) in drawn_rows(panel) {
        out.extend([row.glyph, row.mark]);
        out.extend(row.items.iter().map(|item| item.glyph));
    }
    out
}

#[test]
fn every_settings_board_mark_is_a_mark_the_card_draws() {
    let favourites = r#"{"places":{"favourites":[{"label":"Docs","path":"/fx/v/work/Docs"},{"label":"Media","path":"/fx/v/work/Media"}]}}"#;
    let about = Facts {
        about: about::Facts {
            update: Status {
                state: update::State::Unchecked,
                ..update::idle()
            },
            ..about::Facts::default()
        },
        ..Facts::default()
    };
    let boards: [(&str, &'static str, Facts, &[&str]); 11] = [
        ("{}", "view", Facts::default(), board::MARKS_VIEW),
        ("{}", "view", Facts::default(), board::MARKS_GIT),
        ("{}", "view", Facts::default(), board::MARKS_OPENING),
        ("{}", "places", Facts::default(), board::MARKS_PLACES),
        (
            favourites,
            "places",
            Facts::default(),
            board::MARKS_FAVORITES,
        ),
        ("{}", "preview", Facts::default(), board::MARKS_PREVIEW),
        ("{}", "keys", Facts::default(), board::MARKS_KEYS),
        ("{}", "display", Facts::default(), board::MARKS_DISPLAY),
        ("{}", "menus", Facts::default(), board::MARKS_MENUS),
        ("{}", "menus", Facts::default(), board::MARKS_MENUS_EXTRAS),
        ("{}", "about", about, board::MARKS_ABOUT),
    ];
    for (document, section, facts, marks) in boards {
        let panel = open(document, facts, section);
        let drawn = drawn_marks(&panel);
        let left: Vec<&str> = marks
            .iter()
            .copied()
            .filter(|mark| !drawn.contains(mark))
            .collect();
        assert_eq!(left, Vec::<&str>::new(), "{section}");
    }
    let mut panel = open("{}", Facts::default(), "view");
    for key in ["/", "t", "h", "u", "m", "b"] {
        panel.key(key, if key.len() == 1 { key } else { "" }, false);
    }
    let drawn = drawn_marks(&panel);
    assert_eq!(
        board::MARKS_SEARCH
            .iter()
            .copied()
            .filter(|mark| !drawn.contains(mark))
            .collect::<Vec<_>>(),
        Vec::<&str>::new()
    );
}
