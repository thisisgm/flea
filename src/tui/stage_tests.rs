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

// The picture's file job for a missing file answers first; its error is dropped and a refusal planted in its place.
fn unshown(m: &mut Model) {
    use std::time::{Duration, Instant};
    still(m, Protocol::None, true, (48, 21));
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
    still(m, Protocol::None, true, (48, 21));
}

// A picture that cannot be shown says so through the catalog, in the status row's own sentence.
#[test]
fn an_unshown_picture_is_reported_through_the_catalog() {
    let mut m = shown("image-png", "photo.png");
    let i18n = crate::i18n::test_catalogs::english();
    for key in ["preview.unshown", "preview.imageError"] {
        assert_ne!(i18n.tr(key), key, "the English catalog has {key}");
    }
    unshown(&mut m);
    assert_eq!(m.error, i18n.tr_args("preview.imageError", &[&i18n.tr("preview.unshown")]));
}

// A preview's failure leaves the footer with its file, so the next preview never draws under the last one's error.
#[test]
fn a_preview_failure_leaves_the_footer_when_the_preview_moves_on() {
    for hide in [false, true] {
        let mut m = shown("image-png", "photo.png");
        m.preview_visible = true;
        unshown(&mut m);
        assert!(!m.error.is_empty());
        m.fail("Denied".into());
        if hide { m.preview_visible = false; } else { m.cursor = 8; }
        crate::tui::preview::load(&mut m, false);
        assert_eq!((m.error.as_str(), m.errors.len()), ("Denied", 0), "hidden: {hide}");
    }
    for reload in [false, true] {
        let mut m = shown("image-png", "photo.png");
        m.preview_visible = true;
        unshown(&mut m);
        m.dismiss_error();
        m.fail("Denied".into());
        m.preview_state.still.error = crate::tui::still::UNSHOWN.into();
        still(&mut m, Protocol::None, true, (48, 21));
        assert_eq!((m.error.as_str(), m.errors.len()), ("Denied", 1), "the preview's error queued behind the shown one");
        let asked = m.preview_state.still.key().map(|key| key.generation);
        // A reload of the same file asks the picture again, so the picture's earlier failure goes too.
        if reload { assert_eq!(m.current_path(), Some(m.preview_path.clone()), "the reload is of another file"); } else { m.cursor = 8; }
        crate::tui::preview::load(&mut m, reload);
        assert_eq!((m.error.as_str(), m.errors.len()), ("Denied", 0), "the queued preview error outlived its picture, reloaded: {reload}");
        if reload {
            still(&mut m, Protocol::None, true, (48, 21));
            let now = m.preview_state.still.key().map(|key| key.generation);
            assert!(asked.is_some() && now == Some(m.preview_generation) && now != asked, "the reload did not ask the picture again: {asked:?} then {now:?}");
        }
    }
}

// A page that cannot be read is the preview's own failure, so it leaves the footer with its file too.
#[test]
fn a_page_failure_leaves_the_footer_when_the_preview_moves_on() {
    use std::time::{Duration, Instant};
    let (mut wire, reader) = wire::echo_wire();
    let mut m = shown("application-pdf", "paper.pdf");
    m.preview_visible = true;
    let (mut graphics, mut shown, mut frame) = (Graphics::new(), Shown::default(), String::new());
    let start = Instant::now();
    while m.error.is_empty() {
        step(&mut m, &mut graphics, &mut shown, &mut wire, (130, 31), false, &mut frame).unwrap();
        assert!(start.elapsed() < Duration::from_secs(15), "the page of a missing file never failed");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Only the page posts a job's own words; the still, graphics and media sites each wrap theirs in a sentence.
    assert_eq!(m.error, crate::error::io_message(&std::io::ErrorKind::NotFound.into()), "the error came from somewhere other than the page");
    // A reload of the same page keeps the page, and the page posts its error once, so the error stays.
    let failed = m.error.clone();
    crate::tui::preview::load(&mut m, true);
    step(&mut m, &mut graphics, &mut shown, &mut wire, (130, 31), false, &mut frame).unwrap();
    assert_eq!(m.error, failed, "a reload of the same page took its error back");
    m.cursor = 8;
    crate::tui::preview::load(&mut m, false);
    assert!(m.error.is_empty(), "the page's error outlived its file: {}", m.error);
    wire::finish(wire, reader);
}

// Dropping a player that drew through kitty deletes every placement, so the frame redraws and the TUI's own picture is placed again.
#[test]
fn a_dropped_kitty_player_makes_the_next_draw_place_the_picture_again() {
    let (mut wire, reader) = wire::echo_wire();
    for (protocol, cleared) in [(Protocol::Kitty, true), (Protocol::Sixel, false)] {
        let mut m = shown("video-mp4", "clip.mp4");
        let mut player = media::Player::resting(&m.path.join("clip.mp4"), protocol, true, (40, 30, 81, 3), (130, 31), (800, 600));
        player.force_starting_for_test();
        m.player = Some(player);
        let (mut graphics, mut shown, mut frame) = (Graphics::new(), Shown::default(), String::from("drawn"));
        step(&mut m, &mut graphics, &mut shown, &mut wire, (130, 31), true, &mut frame).unwrap();
        assert!(m.player.is_none(), "the overlay left the player up");
        assert_eq!(frame.is_empty(), cleared, "{protocol:?}");
    }
    wire::finish(wire, reader);
}
