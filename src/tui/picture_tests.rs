use super::*;
use crate::tui::chafa::{Cell, Color, ColorMode};
use crate::tui::render::frame_tests::model;
use crate::tui::still::Key;

// The board's cell: 7 by 18 pixels, so an 800 by 600 picture takes 48 by 14 cells in the 130-column pane.
const CELL: (usize, usize) = (7, 18);

fn shown(icon: &str, name: &str) -> Model {
    let mut m = model();
    let row = m.rows.get_mut(&7).unwrap();
    (row.icon, row.name, row.thumbnail) = (icon.into(), name.into(), true);
    m.preview_path = m.path.join(name);
    m.capabilities.graphics = Protocol::Kitty;
    m.preview_state.cell = Some(CELL);
    m.preview_metadata = vec!["Dimensions · 800 × 600".into(), "Modified · today".into(), "Duration · 0:10".into()];
    m.preview_state.facts = [("model3d.fact.dimensions", "800 × 600"), ("preview.fact.modified", "today"), ("preview.fact.duration", "0:10")].into_iter().map(|(key, value)| crate::tui::preview::words::Fact { key, value: value.into(), number: true }).collect();
    m
}

fn pane(m: &Model) -> Area {
    frame(m).0
}

fn grid(columns: usize, rows: usize) -> Grid {
    Grid { columns, rows, cells: vec![Cell { symbol: '▀', foreground: Color::Rgb(1, 2, 3), background: Color::Default }; columns * rows] }
}

#[test]
fn fit_takes_the_cells_magick_fits_a_picture_into() {
    assert_eq!(fit((800, 600), (48, 22), CELL), (48, 14));
    assert_eq!(fit((600, 800), (48, 22), CELL), (43, 22));
    assert_eq!(fit((4000, 10), (48, 22), CELL), (48, 1));
    assert_eq!(fit((0, 600), (48, 22), CELL), (48, 22));
}

#[test]
fn every_kind_keeps_its_board_rows_under_the_picture() {
    let mut m = shown("image-png", "photo.png");
    assert_eq!(stack(&m, &pane(&m)), ((0, 0), vec![Slot::Detail(0), Slot::Detail(2)]), "nothing is reserved before the probe");
    m.media_probe = Some((800, 600, 0));
    assert_eq!(stack(&m, &pane(&m)), ((48, 14), vec![Slot::Blank, Slot::Detail(0), Slot::Detail(2)]));
    let mut m = shown("video-mp4", "clip.mp4");
    m.media_probe = Some((800, 600, 10_000));
    assert_eq!(stack(&m, &pane(&m)), ((48, 14), vec![Slot::Blank, Slot::Player, Slot::Blank, Slot::Detail(0), Slot::Detail(2)]));
    let mut m = shown("audio-x-flac", "song.flac");
    m.media_probe = Some((0, 0, 10_000));
    assert_eq!(stack(&m, &pane(&m)), ((0, 0), vec![Slot::Player, Slot::Blank, Slot::Detail(0), Slot::Detail(2)]));
    let m = shown("text-x-generic", "paper.pdf");
    assert_eq!(stack(&m, &pane(&m)), ((0, 0), vec![Slot::Page, Slot::Controls]));
    let mut m = shown("image-png", "photo.png");
    (m.quicklook, m.media_probe) = (true, Some((800, 600, 0)));
    assert_eq!(stack(&m, &pane(&m)), ((72, 21), Vec::new()), "the card shows no details and centres the picture");
}

#[test]
fn a_picture_sits_on_the_pane_body_and_centred_in_the_card() {
    let mut m = shown("image-png", "photo.png");
    m.media_probe = Some((800, 600, 0));
    let place = place(&m);
    assert_eq!((place.cells, place.room, place.left, place.top), ((48, 14), (48, 21), 82, 6));
    m.quicklook = true;
    let place = super::place(&m);
    assert_eq!((place.cells, place.room, place.left, place.top), ((72, 21), (106, 21), 30, 7));
}

#[test]
fn a_half_block_picture_is_drawn_only_for_the_shown_preview() {
    let mut m = shown("image-png", "photo.png");
    m.capabilities.graphics = Protocol::None;
    let key = Key { path: m.path.join("photo.png"), generation: m.preview_generation, page: (0, 0, 0), cells: (48, 22), colors: ColorMode::Truecolor };
    m.preview_state.still.hold(key, grid(48, 14));
    assert_eq!(stack(&m, &pane(&m)).0, (48, 14));
    m.preview_generation += 1;
    assert!(super::grid(&m).is_none(), "the last preview's picture was offered for the next one");
    m.preview_generation -= 1;
    m.rows.get_mut(&7).unwrap().link = "elsewhere.png".into();
    assert!(super::grid(&m).is_none(), "a picture of another file was offered");
}

#[test]
fn the_page_controls_wait_for_the_first_page_and_take_the_pointer_there() {
    let mut m = shown("text-x-generic", "paper.pdf");
    m.capabilities.graphics = Protocol::None;
    assert_eq!(control_row(&m), None);
    m.pdf = Some(crate::tui::pdf::Pdf::new(m.path.join("paper.pdf"), Protocol::None, 48, 21, (0, 0), "#101315", false));
    assert!(m.pdf.as_ref().unwrap().settled);
    assert_eq!(control_row(&m), Some((7, 81, 48)));
    m.quicklook = true;
    assert_eq!(control_row(&m), Some((8, 12, 106)));
}

// D8: the half-block decode is keyed by the room, so the room must not shrink by the blank row the landed grid brings back.
#[test]
fn a_half_block_picture_keeps_the_room_its_decode_was_keyed_by() {
    let mut m = shown("image-png", "photo.png");
    m.capabilities.graphics = Protocol::None;
    let before = place(&m).room;
    let key = Key { path: m.path.join("photo.png"), generation: m.preview_generation, page: (0, 0, 0), cells: before, colors: ColorMode::Truecolor };
    m.preview_state.still.hold(key, grid(48, 14));
    assert_eq!(stack(&m, &pane(&m)).0, (48, 14));
    assert_eq!(place(&m).room, before, "the room moved when the grid landed, so the decode restarts and the picture never stays");
}
