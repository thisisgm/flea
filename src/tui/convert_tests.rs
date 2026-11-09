use super::{
    action_key_tests::drain,
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    render::{
        self,
        frame_tests::{caps_language, grid, model},
    },
    theme::Theme,
    wire::{self, Wire},
};
use crate::jsondoc;
use std::time::Duration;

pub(super) fn opened(w: &mut Wire, name: &str) -> Model {
    let mut m = model();
    m.rows.get_mut(&7).unwrap().name = name.into();
    actions::key(&mut m, &Key::character('F', ""), &Map::load(), w).unwrap();
    drain(w);
    for line in [
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true,"count":1}"#,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"convert","id":ID,"ok":true,"paths":["/fx/v/work/{name}"]}}"#
        ),
    ] {
        let line = line.replace("ID", &m.action_id.to_string());
        m.receive(jsondoc::parse(&line).unwrap(), w).unwrap();
        drain(w);
    }
    m
}
pub(super) fn frame(m: &Model) -> String {
    grid(
        &render::frame(
            m,
            &Theme::from_text(""),
            &Map::load(),
            130,
            31,
            Duration::ZERO,
        ),
        130,
        31,
    )
    .join("\n")
}
#[test]
fn tui_convert_all_formats_follow_shared_contract() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::convert_process_tests::scenario("formats");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "photo.png");
    let shown = frame(&m);
    for label in [
        "Convert photo.png",
        "JPEG",
        "PNG",
        "WebP",
        "AVIF",
        "HEIC",
        "TIFF",
        "BMP",
        "PDF",
        "Remove metadata",
        "[ Convert ]",
    ] {
        assert!(shown.contains(label), "missing {label}: {shown}");
    }
    wire::finish(w, reader);
}
#[test]
fn tui_convert_pdf_ranges_and_outputs_are_real() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::convert_process_tests::scenario("pdf");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "garden-report.pdf");
    assert!(frame(&m).contains("PAGES"));
    wire::finish(w, reader);
}
#[test]
fn tui_convert_pdf_heading_drops_the_greek_accent() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::convert_process_tests::scenario("pdf");
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "garden-report.pdf");
    m.i18n = caps_language("el");
    let shown = frame(&m);
    // Plain to_uppercase draws ΣΕΛΊΔΕΣ.
    assert!(shown.contains("ΣΕΛΙΔΕΣ"), "{shown}");
    wire::finish(w, reader);
}
#[test]
fn tui_convert_multiple_images_keep_listing_order() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::convert_process_tests::scenario("order");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "photo.png");
    assert_eq!(m.menu_action, "convertCard");
    wire::finish(w, reader);
}
#[test]
fn tui_convert_office_reuses_preview_cache() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::convert_process_tests::scenario("office");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "report.docx");
    assert!(frame(&m).contains("Convert report.docx to PDF"));
    wire::finish(w, reader);
}
#[test]
fn tui_convert_collision_cancel_preserve_sources() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::convert_process_tests::scenario("safe");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "photo.png");
    assert!(!super::layout::focused(&m));
    wire::finish(w, reader);
}
// The card's borders on the option row, and whether a format row above draws its edge against the left or the right one.
fn edge_sides(rows: &[String]) -> (bool, bool) {
    let cells: Vec<Vec<char>> = rows.iter().map(|row| row.chars().collect()).collect();
    let y = rows
        .iter()
        .position(|row| row.contains("[ ]") || row.contains("[x]"))
        .unwrap();
    let x = cells[y]
        .windows(3)
        .position(|run| run[0] == '[' && run[2] == ']')
        .unwrap();
    let left = (0..x).rev().find(|&i| cells[y][i] == '│').unwrap();
    let right = (x..cells[y].len()).find(|&i| cells[y][i] == '│').unwrap();
    let above = &cells[..y];
    (
        above.iter().any(|row| row[left + 1] == '▎'),
        above.iter().any(|row| row[right - 1] == '▕'),
    )
}
// Cells from the card's left border to the word, and from the word's end to the right border, on the word's first row.
fn border_gaps(rows: &[String], word: &str) -> (usize, usize) {
    let y = rows.iter().position(|row| row.contains(word)).unwrap();
    let cells: Vec<char> = rows[y].chars().collect();
    let x = rows[y][..rows[y].find(word).unwrap()].chars().count();
    let end = x + word.chars().count();
    let left = (0..x).rev().find(|&i| cells[i] == '│').unwrap();
    let right = (end..cells.len()).find(|&i| cells[i] == '│').unwrap();
    (x - left - 1, right - end)
}
#[test]
fn tui_convert_rtl_rows_mirror_the_format_edge_and_the_option_brackets() {
    let root = crate::backend::testdir::TestDir::new("convert-rtl");
    root.dir("catalog/en");
    root.dir("omarchy/default/i18n");
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    root.file(
        "catalog/en/flea.json",
        include_str!("../../i18n/en/flea.json"),
    );
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "photo.png");
    let theme = Theme::from_text(render::cards::tests::PALETTE);
    let draw = |m: &Model| {
        let out = render::frame(m, &theme, &Map::load(), 130, 31, Duration::ZERO);
        (
            grid(&out, 130, 31),
            render::cards::tests::inks(&out, 130, 31),
            out,
        )
    };
    let bracket = |rows: &[String], ink: &[Vec<render::cards::tests::Ink>]| {
        let y = rows
            .iter()
            .position(|row| row.contains("[ ]") || row.contains("[x]"))
            .unwrap();
        let x = rows[y]
            .chars()
            .collect::<Vec<_>>()
            .windows(3)
            .position(|run| run[0] == '[' && run[2] == ']')
            .unwrap();
        ink[y][x].0.clone()
    };
    let (rows, ink, _) = draw(&m);
    assert_eq!(
        edge_sides(&rows),
        (true, false),
        "left to right, the chosen format's edge takes the outer air cell"
    );
    let ltr_bracket = bracket(&rows, &ink);
    // An enabled option inks its label apart from its brackets, so only the frame branch keeps them equal.
    assert!(m.converting.card.as_ref().unwrap().option_enabled());
    let y = rows.iter().position(|row| row.contains("[ ]")).unwrap();
    let label = rows[y][..rows[y].find("Remove").unwrap()].chars().count();
    assert_ne!(ink[y][label].0, ltr_bracket);
    let ltr_output = border_gaps(&rows, "Output");
    let arabic = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.file("language", "ar"),
        catalogs: vec![root.join("catalog")],
    });
    m.i18n = arabic.clone();
    assert!(m.i18n.locale().right_to_left);
    assert_eq!(
        border_gaps(&draw(&m).0, "Output").1,
        ltr_output.0,
        "the output label mirrors onto the right border"
    );
    let mut pdf = opened(&mut w, "garden-report.pdf");
    let ltr_pages = border_gaps(&draw(&pdf).0, "All");
    pdf.i18n = arabic;
    let rows = draw(&pdf).0;
    assert_eq!(
        border_gaps(&rows, "All").1,
        ltr_pages.0,
        "the pages field mirrors onto the right border\n{}",
        rows.join("\n")
    );
    // Before the fix the right-to-left edge sat one cell in from the border, behind the lead air.
    assert_eq!(
        edge_sides(&draw(&m).0),
        (false, true),
        "right to left, the edge mirrors onto the outer air cell"
    );
    m.converting.card.as_mut().unwrap().focus = 1;
    let (rows, ink, out) = draw(&m);
    assert_eq!(
        bracket(&rows, &ink),
        ltr_bracket,
        "the brackets keep the left-to-right ink"
    );
    let checkbox = out.find(" [ ]").or_else(|| out.find(" [x]")).unwrap();
    let underline = out[..checkbox].rfind("\x1b[4m").unwrap();
    assert!(
        out[underline..checkbox].contains("\x1b[24m"),
        "the underline ends with the label"
    );
    assert!(
        !out[underline + "\x1b[4m".len()..].starts_with(' '),
        "the underline starts with the label, not its padding"
    );
    // A catalog output label without the trailing separator still fits its lead whole.
    let catalog = include_str!("../../i18n/en/flea.json");
    let en = catalog.replace("\"Output \"", "\"Output\"");
    assert_ne!(en, catalog, "the fixture drops the trailing separator");
    root.file("catalog/en/flea.json", &en);
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    assert_eq!(
        border_gaps(&draw(&m).0, "Output").1,
        ltr_output.0,
        "the bare label keeps every cell"
    );
    wire::finish(w, reader);
}

// Only a plain Space toggles the focused option; Ctrl+Space leaves it.
#[test]
fn tui_convert_space_toggles_only_without_a_modifier() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "photo.png");
    actions::key(&mut m, &Key::named("Tab", ""), &Map::load(), &mut w).unwrap();
    assert_eq!(m.converting.card.as_ref().unwrap().focus, 1);
    actions::key(&mut m, &Key::named("Space", "ctrl"), &Map::load(), &mut w).unwrap();
    assert!(
        !m.converting.card.as_ref().unwrap().strip,
        "Ctrl+Space leaves the option"
    );
    actions::key(&mut m, &Key::named("Space", ""), &Map::load(), &mut w).unwrap();
    assert!(
        m.converting.card.as_ref().unwrap().strip,
        "Space toggles the option"
    );
    drain(&mut w);
    wire::finish(w, reader);
}
