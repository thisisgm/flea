use super::{Depth, Theme};
use crate::tui::{
    actions, combine, convert,
    input::Key,
    keymap::Map,
    layout, look,
    model::{Deletion, Model, Tab},
    print,
    render::{self, frame_tests::model, menu_layout},
    wire,
};
use flea::ui_logic_print_convert::composition::Layout;
use std::time::Duration;

#[path = "mono_test_support.rs"]
mod support;
use support::*;

#[test]
fn mono_menu_cursor_moves_without_marking_neighbours_or_frame() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let map = Map::load();
    for sub in [
        menu_layout::Sub::Root,
        menu_layout::Sub::OpenWith,
        menu_layout::Sub::Compress,
        menu_layout::Sub::Taildrop,
    ] {
        let mut m = menu_model();
        m.menu_view.sub = sub;
        let (mut w, reader) = wire::echo_wire();
        for _ in 0..2 {
            let rows = menu_layout::entries(&m, &map);
            let p = menu_layout::place(&m, &rows);
            let y = p.y + 1 + usize::from(sub != menu_layout::Sub::Root) + m.menu_cursor - p.top;
            let cells = attributes(&frame(&m, &theme), m.columns, m.height + 4);
            assert!(
                (p.x + 1..=p.x + p.width).all(|x| cells[y][x].reverse),
                "{sub:?}: cursor interior"
            );
            assert!(
                !cells[y][p.x].reverse && !cells[y][p.x + p.width + 1].reverse,
                "{sub:?}: frame"
            );
            assert!(!cells[y + 1][p.x + 1].reverse, "{sub:?}: next row");
            actions::key(&mut m, &Key::named("Down", ""), &map, &mut w).unwrap();
        }
        wire::finish(w, reader);
    }
}

#[test]
fn mono_active_tab_keeps_a_mark_and_resets_before_the_next_tab() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = model();
    m.tabs.push(Tab {
        path: "/fx/Docs".into(),
        cursor: 0,
        back: Vec::new(),
        forward: Vec::new(),
    });
    for tab in 0..2 {
        m.tab = tab;
        let cells = attributes(&layout::header(&m, &theme, m.columns), m.columns, 1);
        let mut x = 1;
        for (index, text) in layout::tab_layout(&m, m.columns - 2) {
            for cell in &cells[0][x..x + crate::tui::cells::text_width(&text)] {
                assert_eq!(cell.reverse, index == tab, "tab {tab}, span {index}");
            }
            x += crate::tui::cells::text_width(&text);
        }
    }
}

#[test]
fn mono_card_button_focus_moves_and_default_stays_marked() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = model();
    m.deletion = Some(Deletion {
        token: 1,
        count: 1,
        ..Default::default()
    });
    let (mut w, reader) = wire::echo_wire();
    for focused in 0..2 {
        let card = render::cards::delete_card(&m);
        let cells = attributes(&frame(&m, &theme), m.columns, m.height + 4);
        for (i, &(x, y, width)) in card.buttons.iter().enumerate() {
            for cell in &cells[y][x..x + width] {
                assert_eq!(cell.reverse, i == focused, "button {i}, focus {focused}");
                if focused == 1 && i == 0 {
                    assert!(cell.underline, "default Cancel lost its mark");
                }
            }
            assert!(!cells[y][x - 1].reverse, "button focus leaked");
        }
        actions::key(&mut m, &Key::named("Tab", ""), &Map::load(), &mut w).unwrap();
    }
    wire::finish(w, reader);
}

#[test]
fn mono_printer_picker_marks_the_current_destination() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = opened("print");
    for choice in 0..2 {
        m.printing.card.as_mut().unwrap().choice = choice;
        let stream = frame(&m, &theme);
        let rows = render::frame_tests::grid(&stream, m.columns, m.height + 4);
        let cells = attributes(&stream, m.columns, m.height + 4);
        for (i, name) in ["First", "Second"].iter().enumerate() {
            let y = rows.iter().position(|line| line.contains(name)).unwrap();
            let x = rows[y]
                .chars()
                .position(|c| c == name.chars().next().unwrap())
                .unwrap();
            assert_eq!(
                cells[y][x].reverse,
                i == choice,
                "printer {i}, choice {choice}"
            );
        }
    }
}

#[test]
fn mono_convert_format_stays_marked_after_focus_leaves_the_list() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = opened("convert");
    m.converting.card.as_mut().unwrap().focus = 1;
    for choice in 0..2 {
        m.converting.card.as_mut().unwrap().choice = choice;
        let stream = frame(&m, &theme);
        let rows = render::frame_tests::grid(&stream, m.columns, m.height + 4);
        let cells = attributes(&stream, m.columns, m.height + 4);
        for (i, name) in ["JPEG", "PNG"].iter().enumerate() {
            let y = rows.iter().position(|line| line.contains(name)).unwrap();
            let x = rows[y]
                .chars()
                .position(|c| c == name.chars().next().unwrap())
                .unwrap();
            assert_eq!(
                cells[y][x].reverse,
                i == choice,
                "format {i}, choice {choice}"
            );
        }
    }
}

#[test]
fn mono_combine_tile_focus_moves_between_outlines() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = opened("combine");
    for image in 0..2 {
        m.combining.card.as_mut().unwrap().image = image;
        let g = combine::geometry(&m);
        let cells = attributes(&frame(&m, &theme), m.columns, m.height + 4);
        for (i, rect) in g.tiles.iter().enumerate() {
            assert_eq!(
                cells[rect.y][rect.x + 1].reverse,
                i == image,
                "tile {i}, image {image}"
            );
            assert_eq!(cells[rect.y + rect.height - 1][rect.x].reverse, i == image);
        }
        assert!(
            !cells[g.preview.y][g.preview.x].reverse,
            "preview frame inherited tile focus"
        );
    }
}

#[test]
fn mono_combine_choices_stay_marked_as_focus_moves() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = opened("combine");
    // Tab reaches Columns (focus 2) only in Grid, so Row walks the other groups.
    for (layout, focuses) in [
        (Layout::Row, &[1, 3, 4, 5, 6][..]),
        (Layout::Grid, &[1, 2, 3, 4, 5, 6][..]),
    ] {
        m.combining.card.as_mut().unwrap().settings.layout = layout;
        for &focus in focuses {
            m.combining.card.as_mut().unwrap().focus = focus;
            let g = combine::geometry(&m);
            let cells = attributes(&frame(&m, &theme), m.columns, m.height + 4);
            for field in &g.fields {
                let ink = cells[field.rect.y][field.rect.x];
                let focused = field.enabled && field.focus == focus && field.chosen;
                let at = format!("{layout:?}, focus {focus}, field {}", field.focus);
                assert_eq!(ink.reverse, focused, "{at}");
                if field.chosen && !focused {
                    assert!(ink.underline, "{at}: lost its selection");
                }
                if !field.chosen {
                    assert!(!ink.underline, "{at}: unchosen field acquired a mark");
                }
            }
        }
    }
}

#[test]
fn mono_quick_look_plate_selection_moves_without_marking_other_plates() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = model();
    m.quicklook = true;
    m.rows.get_mut(&7).unwrap().name = "plates.3mf".into();
    m.model3d.data = Some(Box::new(flea::ui_logic_model_preview::wire::Data {
        plates: vec![Default::default(), Default::default()],
        ..Default::default()
    }));
    for plate in 0..2 {
        m.model3d.controls.plate = plate;
        let text = look::card(&m, &theme, m.columns);
        let rows = render::frame_tests::grid(&text, m.columns, m.height + 4);
        let cells = attributes(&text, m.columns, m.height + 4);
        for i in 0..2 {
            let name = format!("Plate {}", i + 1);
            let y = rows.iter().position(|line| line.contains(&name)).unwrap();
            let offset = rows[y].find(&name).unwrap();
            let x = crate::tui::cells::text_width(&rows[y][..offset]);
            assert_eq!(cells[y][x].reverse, i == plate, "plate {i}, active {plate}");
        }
        let y = rows
            .iter()
            .position(|line| line.contains("Plate 1"))
            .unwrap();
        let close = rows[y]
            .rfind(['\u{ea76}', '\u{d7}'])
            .expect("Quick Look bar close mark");
        let x = crate::tui::cells::text_width(&rows[y][..close]);
        assert!(
            !cells[y][x].reverse,
            "plate selection leaked into the close mark"
        );
    }
}

#[path = "mono_snapshot_tests.rs"]
mod snapshot_tests;
