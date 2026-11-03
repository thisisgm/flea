use super::*;
use super::tests::{long, opened};
use crate::tui::render::{cards::tests::{at, role, screen, PALETTE}, frame_tests::entry};

#[test]
fn placement_anchors_under_the_row_and_flips_or_clamps_at_every_edge() {
    let map = Map::load();
    let theme = Theme::from_text(PALETTE);
    for (columns, height, cursor, rows) in [(130, 27, 7, 9), (130, 27, 24, 30), (130, 27, 0, 9), (130, 10, 7, 9), (40, 27, 7, 9),
        (30, 8, 5, 9), (24, 6, 0, 9), (130, 3, 1, 9), (12, 1, 0, 9)] {
        let mut m = opened();
        long(&mut m, rows);
        (m.columns, m.height, m.cursor) = (columns, height, cursor);
        m.menu_path = m.row_path(m.rows.get(&cursor).unwrap());
        let entries = entries(&m, &map);
        let p = place(&m, &entries);
        let anchor = crate::tui::layout::BODY_ROW - 1 + cursor;
        let case = format!("{columns}x{height} cursor {cursor}");
        assert!(p.x + p.width + 2 <= columns, "{case}: right edge");
        assert!(p.y + 1 >= crate::tui::layout::BODY_ROW && p.y + p.rows + 2 <= crate::tui::layout::BODY_ROW - 1 + height.max(p.rows + 2), "{case}: inside the body");
        if p.y > anchor { assert_eq!(p.y, anchor + 1, "{case}: under its row"); }
        else if p.y + p.rows + 2 <= anchor { assert_eq!(p.y + p.rows + 2, anchor, "{case}: above its row"); }
        let below = anchor + 1 + entries.len() + 2 <= crate::tui::layout::BODY_ROW - 1 + height;
        assert_eq!(p.y == anchor + 1, below, "{case}: under whenever it fits");
        let (grid, _, _) = screen(&m, &theme);
        assert_eq!(at(&grid, p.y, p.x, 1), "┌", "{case}");
        assert_eq!(at(&grid, p.y + p.rows + 1, p.x + p.width + 1, 1), "┘", "{case}");
    }
    let mut m = opened();
    long(&mut m, 30);
    m.cursor = 24;
    m.menu_path = m.row_path(m.rows.get(&24).unwrap());
    let p = place(&m, &entries(&m, &map));
    assert_eq!((p.y, p.rows), (13, 11), "on the body's last rows it opens above, its bottom edge touching the row");
    m.columns = 34;
    assert_eq!(place(&m, &entries(&m, &map)).x, 7, "at the right edge it slides left to fit");
}

#[test]
fn hit_boxes_follow_the_drawn_rows_through_resize_scroll_and_submenus() {
    let map = Map::load();
    let theme = Theme::from_text(PALETTE);
    for (columns, height, cursor, sub) in [(130, 27, 0, Sub::Root), (60, 12, 10, Sub::Root), (40, 8, 6, Sub::Root), (130, 6, 10, Sub::Root),
        (130, 27, 1, Sub::Compress), (50, 5, 0, Sub::OpenWith)] {
        let mut m = opened();
        (m.columns, m.height, m.menu_cursor, m.menu_view.sub) = (columns, height, cursor, sub);
        let rows = entries(&m, &map);
        m.menu_top = place(&m, &rows).top;
        let p = place(&m, &rows);
        let (grid, ink, _) = screen(&m, &theme);
        let case = format!("{columns}x{height} cursor {cursor} {sub:?}");
        let first = p.y + 1 + usize::from(sub != Sub::Root);
        assert!((first..=p.y + p.rows).any(|y| ink[y][p.x + 1].1 == role(&theme.tab)), "{case}: the cursor row is on screen");
        for (y, inks) in ink.iter().enumerate().take(p.y + p.rows + 2).skip(p.y) {
            for x in p.x..=p.x + p.width + 1 {
                let found = hit(&m, &map, x + 1, y + 1);
                let interior = x > p.x && x <= p.x + p.width && y >= first && y <= p.y + p.rows;
                match found {
                    Some(Some(index)) => {
                        assert!(interior, "{case}: {x},{y} is a frame cell");
                        let label = &rows[index].as_ref().unwrap().label;
                        let shown: String = at(&grid, y, p.x + 4, label.chars().count().min(p.width.saturating_sub(4)));
                        assert!(label.starts_with(shown.trim_end()), "{case}: {x},{y} hits {label:?} but draws {shown:?}");
                        assert_eq!(index == m.menu_cursor, inks[p.x + 1].1 == role(&theme.tab), "{case}: wash and hit agree at {y}");
                    }
                    Some(None) => assert!(!interior || at(&grid, y, p.x + 2, 1) == "─" || p.width < 4, "{case}: {x},{y} is a row"),
                    None => panic!("{case}: {x},{y} lies on the frame"),
                }
            }
        }
        assert_eq!(hit(&m, &map, p.x, p.y + 2), None, "{case}: left of the frame");
        assert_eq!(hit(&m, &map, p.x + p.width + 3, p.y + 2), None, "{case}: right of the frame");
    }
}

#[test]
fn the_menu_follows_its_target_row_when_the_listing_changes_under_it() {
    let map = Map::load();
    let mut m = opened();
    let before = place(&m, &entries(&m, &map));
    let rows: Vec<_> = m.rows.values().cloned().collect();
    m.rows = std::iter::once(entry("aaa.txt", false, 1)).chain(rows).enumerate().collect();
    m.total = 10;
    assert_eq!(place(&m, &entries(&m, &map)).y, before.y + 1, "notes.txt moved down one row, and so does its menu");
    m.rows.retain(|_, row| row.name != "notes.txt");
    assert_eq!(place(&m, &entries(&m, &map)).y, before.y, "a target that left the listing keeps the row it opened on");
}
