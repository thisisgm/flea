// Each Settings board's 460px pane, string for string in drawing order, against the panel's rows (JetBrains Mono at 0.6 em).
use super::super::about::{self, Facts};
use super::super::pages::{self, State};
use super::super::rows::{Kind, Master, Row};
use super::super::test_support::english;
use super::super::update::{State as Update, Status};
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
        update: Status {
            state: Update::Unchecked,
            kind: "unowned".into(),
            ..Status::default()
        },
    }
}

fn drawn(section: &str, fixture: &str) -> Vec<String> {
    let i18n = english();
    let data = parse(fixture).expect("fixture parses");
    let state = State {
        home: "/home/gm",
        ..State::new(&data)
    };
    // Make Flea the default is its owner's row, handed in; the stub stands in its place.
    let make_default = vec![Row::new(Kind::Check, Text::data("Make Flea the default"))];
    let rows = pages::rows(section, &state, Vec::new(), || {
        about::rows(&board_facts(), true, "English", make_default)
    });
    rows.iter()
        .flat_map(|row| {
            let options: Vec<f32> = row
                .labels
                .iter()
                .map(|label| width(&label.resolve(&i18n), 12.))
                .collect();
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
        })
        .collect()
}

type Board = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

#[path = "visible_boards_tests.rs"]
mod boards;
#[path = "visible_boards_more_tests.rs"]
mod more_boards;

#[test]
fn every_settings_board_reads_as_the_panel_draws_it() {
    let mut wrong = Vec::new();
    for (board, section, fixture, want) in boards::BOARDS.iter().chain(more_boards::BOARDS) {
        let got = drawn(section, fixture);
        // Menus, Favorites and About show the window of a pane taller than the view, so they match a run of it.
        let found = if matches!(
            *board,
            "settings-menus" | "settings-menus-extras" | "settings-favorites" | "settings-about"
        ) {
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
    assert!(segment_fits(
        width("Row density", 14.),
        &["Tight", "Compact", "Normal", "Comfortable"].map(|o| width(o, 12.)),
        PANE_W
    ));
    assert!(!segment_fits(
        width("New tabs open in", 14.),
        &["Current folder", "Home", "Start folder"].map(|o| width(o, 12.)),
        PANE_W
    ));
    assert!(segment_fits(0., &[0.], 74. + CHIP_MIN));
    assert!(!segment_fits(0., &[0.], 74. + CHIP_MIN - 1.));
}

#[test]
fn a_walked_choice_shows_its_value_and_marks_show_no_words() {
    let row = super::super::rows::choice(
        "newTab",
        "settings.row.new_tab",
        "",
        &["current", "home"],
        &["settings.choice.current_folder", "settings.choice.home"],
        "home",
    );
    assert_eq!(said(&row, false, &english()), ["New tabs open in", "Home"]);
    let view = super::super::rows::choice(
        "view",
        "settings.row.view",
        "",
        &["list"],
        &["settings.choice.view_list"],
        "list",
    );
    assert_eq!(said(&view, true, &english()), ["Last-used view"]);
    assert!(texts(&super::super::rows::Row::hint(Text::Empty), true).is_empty());
}

#[test]
fn the_window_bar_row_reads_the_saved_choice() {
    for (saved, label) in [
        ("mixed", "As shipped"),
        ("list", "Over the list"),
        ("window", "Across the window"),
    ] {
        let got = drawn("view", &format!(r#"{{"windowBar":"{saved}"}}"#));
        let at = got
            .iter()
            .position(|text| text == "Window bar")
            .expect("the View board has the row");
        assert_eq!(got.get(at + 1).map(String::as_str), Some(label), "{saved}");
    }
}
