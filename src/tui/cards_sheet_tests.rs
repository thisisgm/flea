use super::*;
use super::tests::{at, deleting, pointer, press, role, screen, Ink, PALETTE};
use crate::tui::{model::Deletion, render::frame_tests::model, wire};

#[test]
fn properties_card_matches_its_board_with_the_kind_word() {
    let theme = Theme::from_text(PALETTE);
    let mut m = model();
    let reply = crate::jsondoc::parse(r#"{"t":"menuaction","op":"properties","ok":true,"path":"/fx/v/work/notes.txt","kind":"Plain text document","bytes":99,"modified":"2025-06-15T10:00:00","mode":"0644","owner":"gm","uid":1000,"gid":1000}"#).unwrap();
    m.properties = Some(properties(&reply, &m.i18n));
    m.sheet = true;
    let (rows, ink, _) = screen(&m, &theme);
    assert_eq!([at(&rows, 9, 40, 1), at(&rows, 9, 89, 1), at(&rows, 21, 40, 1), at(&rows, 21, 89, 1)], ["┌", "┐", "└", "┘"]);
    assert_eq!((at(&rows, 10, 43, 10), at(&rows, 10, 84, 3)), ("Properties".into(), "esc".into()));
    for (y, label, value) in [(12, "Path", "/fx/v/work/notes.txt"), (13, "Kind", "Plain text document"), (14, "Size", "99 B (99 bytes)"),
        (15, "Modified", "2025-06-15 10:00"), (16, "Permissions", "0644"), (17, "Owner", "gm (uid 1000)"), (18, "Group", "gid 1000")] {
        assert_eq!(at(&rows, y, 43, 13).trim_end(), label);
        assert_eq!(at(&rows, y, 56, value.chars().count()), value);
        assert_eq!((ink[y][43].0.clone(), ink[y][56].0.clone()), (role(&theme.muted), role(&theme.foreground)));
    }
    assert_eq!(at(&rows, 20, 78, 9), "[ Close ]");
    assert_eq!((ink[20][78].0.clone(), ink[20][81].1.clone()), (role(&theme.foreground), role(&theme.wash)), "Close is focused and default");
}

#[test]
fn sheets_close_on_escape_enter_question_mark_close_and_outside_and_scroll_within_bounds() {
    let map = Map::load();
    let (mut w, reader) = wire::echo_wire();
    for key in [Key::named("Escape", ""), Key::named("Return", ""), Key::character('?', "")] {
        let mut m = model();
        m.sheet = true;
        m.properties = Some(vec![("Path".into(), "/fx/v/work/notes.txt".into())]);
        press(&mut m, key, &mut w);
        assert!(!m.sheet && m.properties.is_none());
    }
    let mut m = model();
    m.sheet = true;
    m.properties = Some((0..40).map(|i| (format!("Row {i}"), "value".into())).collect());
    let card = properties_card(&m);
    let most = 40 - card.shown;
    for _ in 0..60 { press(&mut m, Key::named("Down", ""), &mut w); }
    assert_eq!(m.sheet_top, most);
    press(&mut m, pointer(64, card.left() + 1, card.y + 4, false), &mut w);
    assert_eq!(m.sheet_top, most - 1);
    press(&mut m, pointer(0, card.left() + 1, card.y + 4, false), &mut w);
    assert!(m.sheet, "a click inside the card keeps it open");
    let (x, y, _) = properties_card(&m).buttons[0];
    press(&mut m, pointer(0, x + 1, y + 1, false), &mut w);
    assert!(!m.sheet, "Close closes");
    let mut m = model();
    m.sheet = true;
    press(&mut m, pointer(0, 1, 1, false), &mut w);
    assert!(!m.sheet, "a click outside the keys sheet closes it");
    let mut m = model();
    m.sheet = true;
    m.height = 8;
    for _ in 0..80 {
        sheet_key(&mut m, &Key::named("Down", ""), &map, "").unwrap();
    }
    assert_eq!(m.sheet_top, super::super::key_sheet::limit(&m, &map), "the keys sheet scrolls to its last row and stops");
    assert!(m.sheet_top > 0);
    wire::finish(w, reader);
}

#[test]
fn a_card_dims_everything_behind_it_and_closing_restores_the_same_frame() {
    let theme = Theme::from_text(PALETTE);
    let dim = theme.dim.as_deref().unwrap();
    let pairs = |pick: fn(&Theme) -> [&String; 17]| pick(&theme).into_iter().zip(pick(dim)).map(|(lit, dim)| (role(lit), role(dim))).collect::<Vec<_>>();
    let scrim = pairs(|t| [&t.foreground, &t.muted, &t.meta, &t.accent, &t.error, &t.symlink, &t.executable, &t.border, &t.frame, &t.rule,
        &t.background, &t.selected, &t.cursor, &t.marked, &t.tab, &t.match_fill, &t.wash]);
    let mut m = model();
    let (_, before, plain) = screen(&m, &theme);
    m.deletion = Some(Deletion { token: 1, count: 1, ..Default::default() });
    // Each cell outside the card in the scrim's ink for its lit role, counting all of them and those in the preview pane; row 9 holds the cursor, which an open card unfocuses as every overlay does.
    let behind = |after: &[Vec<Ink>], card: &Card| {
        let (mut cells, mut preview) = (0, 0);
        for (y, row) in before.iter().enumerate() {
            for (x, lit) in row.iter().enumerate().filter(|(x, _)| y != 9 && (!(card.x..=card.x + card.width + 1).contains(x) || !(card.y..=card.y + card.height + 1).contains(&y))) {
                for (ink, dimmed) in [(&lit.0, &after[y][x].0), (&lit.1, &after[y][x].1)] {
                    if let Some((_, expected)) = scrim.iter().find(|(lit, _)| lit == ink) {
                        assert_eq!(dimmed, expected, "cell {x},{y} keeps its role behind the scrim");
                        cells += 1;
                        preview += usize::from(x > 80);
                    }
                }
            }
        }
        (cells, preview)
    };
    let card = delete_card(&m);
    let (_, after, open) = screen(&m, &theme);
    assert!(behind(&after, &card).1 > 100, "the preview pane under the card redraws in the scrim's inks");
    assert_eq!(after[card.y + 3][card.left()].0, role(&theme.foreground), "the card stays lit");
    assert_ne!(open, plain, "an open card changes the frame, so the picture under it is dropped");
    m.deletion = None;
    assert_eq!(screen(&m, &theme).2, plain, "closing draws the frame the picture was placed on");
    m.menu = true;
    m.menu_ready = true;
    let (_, menu, _) = screen(&m, &theme);
    assert_eq!(menu[4][120], before[4][120], "a menu never dims what is behind it");
    m.menu = false;
    m.sheet = true;
    let (_, sheet, _) = screen(&m, &theme);
    assert!(behind(&sheet, &crate::tui::render::key_sheet::card(&m, &Map::load())).0 > 100, "the keys sheet dims what is behind it too");
    assert_eq!(dim.background, theme.background);
}

#[test]
fn corners_round_only_while_hyprland_rounds() {
    let theme = Theme::from_text(PALETTE);
    let m = deleting(1, 1);
    m.rounded.store(true, Ordering::Relaxed);
    let (rows, _, _) = screen(&m, &theme);
    assert_eq!([at(&rows, 12, 35, 1), at(&rows, 12, 94, 1), at(&rows, 18, 35, 1), at(&rows, 18, 94, 1)], ["╭", "╮", "╰", "╯"]);
    for (answer, rounds) in [(r#"{"option":"decoration:rounding","int":8,"set":true}"#, true), (r#"{"int":0}"#, false), ("", false), (r#"{"int":"8"}"#, false)] {
        assert_eq!(rounding(answer), rounds, "{answer}");
    }
}

// The sheet and the card page, jump and close with the listing's own keys in every preset, as the listing does.
#[test]
fn sheets_take_the_listing_page_jump_and_q_keys_in_every_preset() {
    let map = Map::load();
    let (mut w, reader) = wire::echo_wire();
    for preset in ["default", "windows", "mac", "vim", "yazi"] {
        for properties in [false, true] {
            let fresh = || {
                let mut m = model();
                m.preset = preset.into();
                m.sheet = true;
                m.height = 8;
                if properties {
                    m.properties = Some(
                        (0..40)
                            .map(|i| (format!("Row {i}"), "value".into()))
                            .collect(),
                    );
                }
                m
            };
            let mut m = fresh();
            let limit = if properties {
                40 - properties_card(&m).shown
            } else {
                super::super::key_sheet::limit(&m, &map)
            };
            let page = if properties {
                properties_card(&m).shown
            } else {
                super::super::key_sheet::card(&m, &map).shown
            };
            assert!(limit > page, "{preset}: the sheet is taller than the card");
            press(&mut m, Key::named("PageDown", ""), &mut w);
            assert_eq!(
                m.sheet_top, page,
                "{preset} {properties}: PageDown turns one card of rows"
            );
            press(&mut m, Key::named("PageUp", ""), &mut w);
            assert_eq!(m.sheet_top, 0, "{preset} {properties}: PageUp turns back");
            press(&mut m, Key::named("End", ""), &mut w);
            assert_eq!(
                m.sheet_top, limit,
                "{preset} {properties}: End reaches the last row"
            );
            press(&mut m, Key::named("Home", ""), &mut w);
            assert_eq!(
                m.sheet_top, 0,
                "{preset} {properties}: Home returns to the first row"
            );
            press(&mut m, Key::character('G', "shift"), &mut w);
            assert_eq!(
                m.sheet_top, limit,
                "{preset} {properties}: G reaches the last row"
            );
            if preset != "default" && preset != "windows" && preset != "mac" {
                press(&mut m, Key::character('g', ""), &mut w);
                press(&mut m, Key::character('g', ""), &mut w);
                assert_eq!(
                    m.sheet_top, 0,
                    "{preset} {properties}: g g returns to the first row"
                );
            }
            assert!(
                m.sheet,
                "{preset} {properties}: motion keeps the sheet open"
            );
            press(&mut m, Key::character('q', ""), &mut w);
            assert!(
                !m.sheet && m.properties.is_none() && !m.quit,
                "{preset} {properties}: q closes the sheet and keeps Flea running"
            );
        }
    }
    wire::finish(w, reader);
}

// An 8-row terminal leaves the keys sheet room for its title and rule only, so no row is drawn over the bottom border.
#[test]
fn a_keys_sheet_too_short_for_rows_keeps_its_bottom_border_whole() {
    let theme = Theme::from_text(PALETTE);
    let map = Map::load();
    let mut m = model();
    m.columns = 30;
    m.height = crate::tui::layout::body_height(8);
    m.sheet = true;
    let card = super::super::key_sheet::card(&m, &map);
    let (rows, _, _) = screen(&m, &theme);
    let bottom = at(&rows, card.y + card.height + 1, card.x, card.width + 2);
    assert!(
        bottom.chars().all(|c| "└─┘╰╯".contains(c)),
        "{bottom} {rows:#?}"
    );
    assert_eq!(card.shown, 0);
    for height in 5..12 {
        m.height = crate::tui::layout::body_height(height);
        let card = super::super::key_sheet::card(&m, &map);
        let (rows, _, _) = screen(&m, &theme);
        assert!(
            at(&rows, card.y + card.height + 1, card.x, card.width + 2)
                .chars()
                .all(|c| "└─┘╰╯".contains(c)),
            "{height} rows: {rows:#?}"
        );
    }
}
