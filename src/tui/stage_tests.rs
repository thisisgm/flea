use super::*;
use crate::tui::chafa::ColorMode;
use crate::tui::render::frame_tests::model;

fn shown(icon: &str, name: &str) -> Model {
    let mut m = model();
    let row = m.rows.get_mut(&7).unwrap();
    (row.icon, row.name, row.thumbnail) = (icon.into(), name.into(), true);
    m.preview_path = m.path.join(name);
    (m.preview_loaded, m.preview_body) = (true, String::new());
    m.capabilities.graphics = Protocol::None;
    m.preview_state.colors = Some(ColorMode::Truecolor);
    m
}

#[test]
fn a_terminal_without_graphics_asks_chafa_for_the_shown_picture_only() {
    let mut m = shown("image-png", "photo.png");
    still(&mut m, Protocol::None, true, (48, 21));
    let asked = Key { path: m.path.join("photo.png"), generation: m.preview_generation, page: (0, 0, 0), cells: (48, 21), colors: ColorMode::Truecolor };
    assert_eq!(m.preview_state.still.key(), Some(&asked));
    m.preview_generation += 1;
    still(&mut m, Protocol::None, true, (48, 21));
    assert_eq!(m.preview_state.still.key().map(|key| key.generation), Some(m.preview_generation), "navigation asks for the next preview");
    still(&mut m, Protocol::Kitty, true, (48, 21));
    assert!(m.preview_state.still.key().is_none(), "half blocks drew where the terminal shows pictures");
    still(&mut m, Protocol::None, false, (48, 21));
    assert!(m.preview_state.still.key().is_none(), "a hidden preview kept its picture");
    m.preview_state.colors = None;
    still(&mut m, Protocol::None, true, (48, 21));
    assert!(m.preview_state.still.key().is_none(), "a sixteen-colour terminal drew half blocks");
}

#[test]
fn a_video_waits_for_its_probe_and_a_missing_poster_tool_draws_nothing() {
    let mut m = shown("video-mp4", "clip.mp4");
    still(&mut m, Protocol::None, true, (48, 19));
    assert!(m.preview_state.still.key().is_none(), "a poster was asked for before the probe");
    m.media_probe = Some((800, 600, 10_000));
    still(&mut m, Protocol::None, true, (48, 19));
    assert_eq!(m.preview_state.still.key().is_some(), Path::new(graphics::POSTER).exists());
}

#[test]
fn a_page_is_asked_for_once_per_decoded_revision() {
    let mut m = shown("text-x-generic", "paper.pdf");
    m.pdf = Some(pdf::Pdf::new(m.path.join("paper.pdf"), Protocol::None, 48, 21, (0, 0), "#101315", false));
    still(&mut m, Protocol::None, true, (48, 21));
    assert!(m.preview_state.still.key().is_none(), "a page was asked for before it was decoded");
    let page = m.pdf.as_mut().unwrap();
    (page.png, page.revision) = (b"page".to_vec(), 2);
    still(&mut m, Protocol::None, true, (48, 21));
    assert_eq!(m.preview_state.still.key().map(|key| key.page), Some((2, 0, 0)));
    m.pdf.as_mut().unwrap().revision = 3;
    still(&mut m, Protocol::None, true, (48, 21));
    assert_eq!(m.preview_state.still.key().map(|key| key.page), Some((3, 0, 0)), "a new page kept the last page's cells");
}

#[test]
fn the_player_starts_once_the_probe_fixes_its_box_and_an_overlay_stops_it() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = shown("video-mp4", "clip.mp4");
    let mut graphics = Graphics::new();
    let (mut shown, mut frame) = (Shown::default(), String::new());
    step(&mut m, &mut graphics, &mut shown, &mut wire, (130, 31), false, &mut frame).unwrap();
    assert!(m.player.is_none(), "the player started before the probe fixed its box");
    m.media_probe = Some((800, 600, 10_000));
    step(&mut m, &mut graphics, &mut shown, &mut wire, (130, 31), false, &mut frame).unwrap();
    let place = picture::place(&m);
    let player = m.player.as_ref().expect("the probe started the player");
    assert!(player.at_rest(), "a plain terminal runs no mpv until play");
    assert_eq!(player.geometry, (place.cells.0, place.cells.1, place.left, place.top));
    step(&mut m, &mut graphics, &mut shown, &mut wire, (130, 31), true, &mut frame).unwrap();
    assert!(m.player.is_none() && m.playback.is_none(), "an overlay left the player up");
    wire::finish(wire, reader);
}

// A picture that cannot be shown says so through the catalog, in the status row's own sentence.
#[test]
fn an_unshown_picture_is_reported_through_the_catalog() {
    use std::time::{Duration, Instant};
    let mut m = shown("image-png", "photo.png");
    let i18n = crate::i18n::test_catalogs::english();
    for key in ["preview.unshown", "preview.imageError"] {
        assert_ne!(i18n.tr(key), key, "the English catalog has {key}");
    }
    // The picture starts a file job for a missing file, so wait until it answers and drop its error.
    still(&mut m, Protocol::None, true, (48, 21));
    let start = Instant::now();
    while m.preview_state.still.running() {
        m.preview_state.still.poll();
        assert!(start.elapsed() < Duration::from_secs(15), "the file job did not answer");
        std::thread::sleep(Duration::from_millis(5));
    }
    m.preview_state.still.poll();
    let _ = std::mem::take(&mut m.preview_state.still.error);
    m.error.clear();
    m.errors.clear();
    // The same key asks for nothing new, so the planted error survives the poll.
    m.preview_state.still.error = crate::tui::still::UNSHOWN.into();
    still(&mut m, Protocol::None, true, (48, 21));
    assert_eq!(m.error, i18n.tr_args("preview.imageError", &[&i18n.tr("preview.unshown")]));
}
