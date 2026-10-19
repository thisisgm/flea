use super::*;
use crate::tui::capabilities::Protocol;
use crate::tui::chafa::{Cell, Color, ColorMode, Grid};
use crate::tui::input::Pointer;
use crate::tui::keymap::Map;
use crate::tui::render::frame_tests::{grid, model, plain};

const BENCH: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\nmuted='#68544b'";

fn cells(row: &str) -> Vec<char> {
    row.chars().collect()
}

fn press(m: &mut Model, x: usize, y: usize, motion: bool) -> bool {
    super::press(m, &Pointer { button: 0, x, y, released: false, motion })
}

#[test]
fn the_card_sits_nine_cells_in_with_a_bar_a_rule_and_air_over_a_dimmed_screen() {
    let mut m = model();
    m.quicklook = true;
    let theme = Theme::from_text(BENCH);
    let map = Map::load();
    let out = frame(&m, &theme, &map, 130, 31, Duration::ZERO);
    let screen = grid(&out, 130, 31);
    let (top, bar, rule, air, text, bottom) = (cells(&screen[2]), cells(&screen[3]), cells(&screen[4]), cells(&screen[5]), &screen[6], cells(&screen[28]));
    assert_eq!((top[9], top[10], top[119], top[120]), ('┌', '─', '─', '┐'), "{}", screen[2]);
    assert_eq!((bottom[9], bottom[120]), ('└', '┘'));
    assert!((3..28).all(|y| cells(&screen[y])[9] == '│' && cells(&screen[y])[120] == '│'));
    assert_eq!(bar[10..12].iter().collect::<String>(), "  ");
    assert!(screen[3][..].contains("notes.txt  4 lines"), "{}", screen[3]);
    assert_eq!((bar[116], bar[117], bar[118], bar[119]), ('\u{ea76}', ' ', ' ', ' '));
    assert!(rule[12..118].iter().all(|c| *c == '─') && rule[10..12] == [' ', ' '] && rule[118..120] == [' ', ' ']);
    assert!(air[10..120].iter().all(|c| *c == ' '));
    assert!(text.contains("1 Visual gate notes"), "{text}");
    // Everything behind the card is the same frame drawn in the scrim's inks.
    let behind = crate::tui::render::frame(&m, theme.dim.as_deref().unwrap(), &map, 130, 31, Duration::ZERO);
    assert!(out.starts_with(&behind));
    assert!(!behind.contains(&theme.foreground), "an undimmed ink drew behind the card");
    assert!(out[behind.len()..].contains(&theme.frame));
    m.rounded.store(true, std::sync::atomic::Ordering::Relaxed);
    let screen = grid(&frame(&m, &theme, &map, 130, 31, Duration::ZERO), 130, 31);
    assert_eq!((cells(&screen[2])[9], cells(&screen[2])[120], cells(&screen[28])[9], cells(&screen[28])[120]), ('╭', '╮', '╰', '╯'));
}

#[test]
fn the_pane_picture_is_cleared_behind_the_card_and_the_card_draws_it_centred() {
    let mut m = model();
    let row = m.rows.get_mut(&7).unwrap();
    (row.icon, row.name, row.thumbnail) = ("image-png".into(), "photo.png".into(), true);
    m.preview_path = m.path.join("photo.png");
    m.image_file = Some(m.path.join("photo.png"));
    m.capabilities.graphics = Protocol::None;
    let picture = Grid { columns: 4, rows: 2, cells: vec![Cell { symbol: '▀', foreground: Color::Rgb(9, 9, 9), background: Color::Default }; 8] };
    let key = crate::tui::still::Key { path: m.path.join("photo.png"), generation: m.preview_generation, page: (0, 0, 0), cells: (106, 21), colors: ColorMode::Truecolor };
    m.preview_state.still.hold(key, picture);
    let theme = Theme::from_text(BENCH);
    let pane = Area { columns: 50, padding: 1, height: 24, card: false };
    assert!(plain(&crate::tui::pane::body(&m, 0, &pane, &theme)).contains('▀'));
    m.quicklook = true;
    assert!(!plain(&crate::tui::pane::body(&m, 0, &pane, &theme)).contains('▀'), "the pane picture drew behind the card");
    let card = plain(&crate::tui::pane::body(&m, 0, &area(130, m.height), &theme));
    assert_eq!(card.find('▀'), Some(2 + 51), "{card:?}");
}

#[test]
fn a_press_on_the_close_mark_or_outside_the_card_closes_quick_look() {
    let mut m = model();
    m.quicklook = true;
    m.preview_focus = true;
    assert!(!press(&mut m, 60, 10, false) && m.quicklook, "a press inside the card keeps it open");
    assert!(!press(&mut m, 116, 4, false) && m.quicklook);
    assert!(!press(&mut m, 5, 10, true) && m.quicklook, "pointer motion never closes the card");
    assert!(press(&mut m, 117, 4, false) && !m.quicklook && !m.preview_focus);
    for (x, y) in [(5, 10), (125, 10), (60, 2), (60, 30)] {
        m.quicklook = true;
        assert!(press(&mut m, x, y, false) && !m.quicklook, "a press at {x},{y} outside the card");
    }
}

#[test]
fn the_bar_names_the_one_fact_its_kind_has() {
    let mut m = model();
    assert_eq!(fact(&m, &m.rows[&7]), "4 lines");
    m.rows.get_mut(&7).unwrap().icon = "image-png".into();
    m.media_probe = Some((800, 600, 0));
    assert_eq!(fact(&m, &m.rows[&7]), "800 × 600");
    m.rows.get_mut(&7).unwrap().icon = "audio-x-flac".into();
    m.media_probe = Some((0, 0, 75_000));
    assert_eq!(fact(&m, &m.rows[&7]), "1:15");
    m.rows.get_mut(&7).unwrap().icon = "package-x-generic".into();
    crate::tui::pane_state::meta(&mut m, &crate::jsondoc::parse(r#"{"entries":1,"unpacked":5,"names":[]}"#).unwrap());
    assert_eq!(fact(&m, &m.rows[&7]), "1 entry");
}
