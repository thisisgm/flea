//! Right-to-left hint units: a Latin run stays whole on a line and breaks only at its own spaces when longer than one.
use super::*;
use crate::g2::settings_pages::geometry::hint_line_count;

// One cell per character, so a width reads as a character count.
fn cells(unit: &str) -> f32 {
    unit.chars().count() as f32
}

const WIDE: f32 = 1000.;

#[test]
fn a_left_to_right_hint_keeps_its_units() {
    let raw = "Ctrl+Shift++ and Ctrl+Shift+- step through them.";
    assert_eq!(hint_units_in(raw, false, WIDE, cells), hint_units(raw));
}

#[test]
fn a_right_to_left_hint_keeps_each_latin_run_whole_and_drops_the_paragraph_mark() {
    let fsi = |text: &str| format!("\u{2068}{text}\u{2069}");
    let chord = |text: &str| fsi(&format!("\u{2066}{text}\u{2069}"));
    let raw = format!(
        "\u{200f}{} ו-{} עוברים בין הגדלים; {} מחזיר",
        chord("Ctrl+Shift++"),
        chord("Ctrl+Shift+-"),
        chord("Ctrl+Shift+0"),
    );
    let units = hint_units_in(&raw, true, WIDE, cells);
    assert_eq!(units[0], format!("{} ", chord("Ctrl+Shift++")));
    assert!(
        units.iter().all(|unit| !unit.contains('\u{200f}')),
        "{units:?}"
    );
    assert_eq!(units.concat(), raw.trim_start_matches('\u{200f}'));
    let latin = format!("Flea פועל לפי {} ואינו", fsi("Keyboard Shortcuts"));
    let units = hint_units_in(&latin, true, WIDE, cells);
    assert!(
        units.contains(&format!("{} ", fsi("Keyboard Shortcuts"))),
        "{units:?}"
    );
    assert_eq!(units[0], "Flea ");
    let plain = hint_units_in("לחצו על Keyboard Shortcuts עכשיו", true, WIDE, cells);
    assert_eq!(plain, ["לחצו ", "על ", "Keyboard Shortcuts ", "עכשיו"]);
}

#[test]
fn a_latin_run_longer_than_a_line_breaks_at_its_own_spaces() {
    // An English fallback string in a right-to-left window is one left-to-right run from end to end.
    let raw = "Ctrl+Shift++ and Ctrl+Shift+- step through them.";
    assert_eq!(hint_units_in(raw, true, WIDE, cells), [raw]);
    let width = 20.;
    let units = hint_units_in(raw, true, width, cells);
    assert_eq!(
        units,
        ["Ctrl+Shift++ and ", "Ctrl+Shift+- step ", "through them."]
    );
    let widths: Vec<f32> = units.iter().map(|unit| cells(unit)).collect();
    assert_eq!(hint_line_count(&widths, width), 3);
    // A run that starts mid-line fills that line first, then wraps, word order kept.
    let mixed = "לחצו על Keyboard Shortcuts and then Settings עכשיו";
    let units = hint_units_in(mixed, true, 20., cells);
    assert_eq!(
        units,
        [
            "לחצו ",
            "על ",
            "Keyboard ",
            "Shortcuts and then ",
            "Settings ",
            "עכשיו"
        ]
    );
    assert_eq!(units.concat(), mixed);
    // A single word wider than the line keeps its own line, as hint_line_count places it.
    let long = hint_units_in("א Supercalifragilistic ב", true, 10., cells);
    assert_eq!(long, ["א ", "Supercalifragilistic ", "ב"]);
}

#[test]
fn a_spaced_unit_splits_its_space_off() {
    assert_eq!(spaced("Hyprland "), ("Hyprland", true));
    assert_eq!(spaced("אותו."), ("אותו.", false));
}
