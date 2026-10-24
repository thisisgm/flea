//! Port of the Recent/path/row-identity and grid-geometry assertions.
use super::path::*;

#[test]
fn recent_is_a_token_never_a_path() {
    assert!(!RECENT.starts_with('/'));
    assert!(is_recent(RECENT));
    assert!(!is_recent("/home/gm"));
}

#[test]
fn rows_join_but_recent_rows_name_themselves() {
    assert_eq!(row_path("/home/gm", "a.png"), "/home/gm/a.png");
    assert_eq!(row_path(RECENT, "home/gm/Pictures/a.png"), "/home/gm/Pictures/a.png");
    assert_eq!(row_path("/", "etc"), "/etc");
    assert_eq!(parent_of("/a/b.txt"), "/a");
    assert_eq!(parent_of("/a"), "/");
    assert_eq!(parent_of("/"), "/");
}

#[test]
fn symlink_folders_stay_navigable_files_stay_files() {
    assert!(directory(false, 0o120777, "folder"));
    assert!(!directory(false, 0o100644, "folder"));
    assert!(directory(true, 0, ""));
}

#[test]
fn only_a_drawn_view_is_remembered() {
    assert_eq!(remembered_view("grid"), "grid");
    assert_eq!(remembered_view("list"), "list");
    assert_eq!(remembered_view("columns"), "list");
    assert_eq!(view_switch("list", "grid"), Some("grid"));
    assert_eq!(view_switch("grid", "grid"), None);
    assert_eq!(view_switch("list", "columns"), None);
}

#[test]
fn grid_steps_keep_the_edge_rule_pages_clamp() {
    assert_eq!(grid_target(0, -1, 4, 9), 0);
    assert_eq!(grid_target(7, 1, 4, 9), 7);
    assert_eq!(grid_target(4, 1, 4, 9), 5);
    assert_eq!(grid_target(0, 1, 4, 9), 1);
    assert_eq!(grid_target(8, 1, 4, 9), 8);
    assert_eq!(page_target(0, -5, 9), 0);
    assert_eq!(page_target(8, 5, 9), 8);
    assert_eq!(page_target(3, 2, 9), 5);
}

#[test]
fn window_covers_both_views_plus_slack() {
    assert_eq!(window_size(10, 4, 5), 100);
    assert_eq!(window_size(30, 2, 5), 120);
    assert_eq!(tile_range(0.0, 10.0, 4, 5, 100), Some((0, 19)));
    assert_eq!(tile_range(0.0, 10.0, 4, 5, 0), None, "an empty listing has no visible tile");
}

#[test]
fn a_grid_step_with_no_columns_moves_by_one_and_never_panics() {
    assert_eq!(grid_target(1, 1, 0, 5), 2);
    assert_eq!(grid_target(4, -1, 0, 5), 3);
    assert_eq!(grid_target(0, 1, 0, 0), 0, "an empty listing stays at zero");
}

#[test]
fn a_step_from_a_cursor_past_the_end_stays_on_the_last_row() {
    // Picker.js gridTarget: past total answers the last row.
    assert_eq!(grid_target(10, -1, 3, 5), 4);
    assert_eq!(grid_target(10, 1, 3, 5), 4);
    assert_eq!(grid_target(9, 1, 3, 5), 4);
}
