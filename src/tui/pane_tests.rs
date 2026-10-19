use super::*;
use crate::tui::pane_state::{meta, Unshown};
use crate::tui::render::frame_tests::{model, plain};

fn rows(m: &Model, columns: usize, theme: &Theme) -> Vec<String> {
    (0..m.height).map(|y| preview(m, y, columns, m.height, theme)).collect()
}

fn archive() -> Model {
    let mut m = model();
    let row = m.rows.get_mut(&7).unwrap();
    (row.name, row.icon, row.kind) = ("bundle.zip".into(), "package-x-generic".into(), "ZIP archive".into());
    m.preview_path = m.path.join("bundle.zip");
    m.preview_body.clear();
    meta(&mut m, &crate::jsondoc::parse(r#"{"t":"meta","entries":3,"unpacked":2048,"names":[{"n":"src","d":true},{"n":"a.txt","d":false,"s":12}]}"#).unwrap());
    super::prepare(&mut m);
    m
}

#[test]
fn a_link_head_names_its_target_in_the_meta_ink() {
    let mut m = model();
    m.rows.get_mut(&7).unwrap().link = "../shared/notes.txt".into();
    let theme = Theme::from_text("");
    let head = preview(&m, 0, 50, m.height, &theme);
    assert!(plain(&head).contains("notes.txt → ../shared/notes.txt"), "{}", plain(&head));
    assert!(head.contains(&format!("{} → ../shared", theme.meta)));
}

#[test]
fn an_archive_draws_its_count_line_then_a_blank_then_its_entries() {
    let m = archive();
    let theme = Theme::from_text("");
    let lines: Vec<String> = rows(&m, 50, &theme).iter().map(|row| plain(row)).collect();
    assert_eq!(lines[2].trim_end(), " 3 entries · 2.0 kB unpacked");
    assert_eq!(lines[3].trim(), "");
    assert!(lines[4].contains("src"), "{:?}", lines[4]);
    assert!(lines[5].contains("a.txt") && lines[5].ends_with("12 B  "), "{:?}", lines[5]);
    assert_eq!(lines[6].trim(), "+ 1 more");
    assert_eq!(m.preview_line_count, 4, "the count's blank row, two entries and the more line scroll together");
}

#[test]
fn a_file_that_cannot_be_shown_centres_its_mark_sentence_and_open_with() {
    let mut m = model();
    m.preview_state.unshown = Some(Unshown { sentence: "This file can't be shown.".into(), open_with: true });
    super::prepare(&mut m);
    let theme = Theme::from_text("");
    let drawn = rows(&m, 50, &theme);
    let lines: Vec<String> = drawn.iter().map(|row| plain(row)).collect();
    let top = 3 + (m.height - 3 - 4) / 2;
    assert!(lines[3..top].iter().all(|row| row.trim().is_empty()));
    assert_eq!(lines[top].trim(), "\u{ea6c}");
    assert!(drawn[top].contains(&theme.error));
    assert_eq!(lines[top + 1].trim(), "This file can't be shown.");
    assert_eq!(lines[top + 2].trim(), "");
    assert_eq!(lines[top + 3].trim(), "[ Open With… ]");
    assert!(drawn[top + 3].contains(&format!("{}[ {}Open With…{} ]", theme.frame, theme.foreground, theme.frame)));
    m.preview_state.unshown.as_mut().unwrap().open_with = false;
    let lines: Vec<String> = rows(&m, 50, &theme).iter().map(|row| plain(row)).collect();
    assert_eq!(lines[3 + (m.height - 3 - 2) / 2 + 1].trim(), "This file can't be shown.");
    assert!(!lines.iter().any(|row| row.contains("Open With")));
}

#[test]
fn every_preview_row_fills_its_width_at_every_size() {
    let theme = Theme::from_text("");
    let mut unshown = model();
    unshown.preview_state.unshown = Some(Unshown { sentence: "This file can't be shown.".into(), open_with: true });
    let mut linked = model();
    linked.rows.get_mut(&7).unwrap().link = "a/very/long/target/name.txt".into();
    for mut m in [model(), archive(), unshown, linked] {
        // Six columns is the narrowest pane the twelve-column floor draws.
        for columns in 6..70 {
            m.preview_width = usize::MAX;
            super::prepare(&mut m);
            for (y, row) in rows(&m, columns, &theme).iter().enumerate() {
                assert_eq!(text_width(&plain(row)), columns, "row {y} at {columns} columns: {:?}", plain(row));
            }
        }
    }
}

#[test]
fn control_runs_paint_in_their_own_inks_and_fill_the_width() {
    let theme = Theme::from_text("background='#1c120d'\nforeground='#c6b395'\nmuted='#68544b'");
    let runs = [(Ink::Frame, "──".to_string()), (Ink::Muted, "x".to_string())];
    assert_eq!(paint(&runs, 4, &theme), format!("{}──{}x{} ", theme.track, theme.muted, theme.foreground));
    assert_eq!(plain(&paint(&runs, 1, &theme)), "─");
}

// A half-block picture is cleared under every card, the keys sheet, Properties and the delete card as well as Quick Look, and comes back after.
#[test]
fn the_half_block_picture_is_cleared_under_every_card() {
    use crate::tui::capabilities::Protocol;
    use crate::tui::chafa::{Cell, Color, ColorMode, Grid};
    let theme = Theme::from_text("");
    let map = crate::tui::keymap::Map::load();
    let mut m = model();
    m.preview_visible = true;
    let row = m.rows.get_mut(&7).unwrap();
    (row.icon, row.name, row.thumbnail) = ("image-png".into(), "photo.png".into(), true);
    m.preview_path = m.path.join("photo.png");
    m.image_file = Some(m.path.join("photo.png"));
    m.capabilities.graphics = Protocol::None;
    let picture = Grid { columns: 4, rows: 2, cells: vec![Cell { symbol: '▀', foreground: Color::Rgb(201, 9, 9), background: Color::Default }; 8] };
    let key = crate::tui::still::Key { path: m.path.join("photo.png"), generation: m.preview_generation, page: (0, 0, 0), cells: (48, 21), colors: ColorMode::Truecolor };
    m.preview_state.still.hold(key, picture);
    let lit = "\x1b[38;2;201;9;9m";
    let drawn = |m: &Model| crate::tui::render::frame(m, &theme, &map, 130, 31, std::time::Duration::ZERO).contains(lit);
    assert!(drawn(&m), "the fixture draws its half-block picture");
    m.sheet = true;
    assert!(!drawn(&m), "the keys sheet left the picture lit");
    m.properties = Some(vec![(String::new(), "Loading properties…".into())]);
    assert!(!drawn(&m), "Properties left the picture lit");
    (m.sheet, m.properties) = (false, None);
    m.deletion = Some(Default::default());
    assert!(!drawn(&m), "the delete card left the picture lit");
    m.deletion = None;
    assert!(drawn(&m), "the picture comes back once the card closes");
}
