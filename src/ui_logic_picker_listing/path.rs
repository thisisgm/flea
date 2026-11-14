//! Picker path arithmetic and list/grid geometry (Picker.js port).
pub const RECENT: &str = "flea:recent";

pub fn is_recent(location: &str) -> bool {
    location == RECENT
}

pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

// A row's own identity, which is what a mark holds.
pub fn row_path(location: &str, name: &str) -> String {
    if is_recent(location) {
        format!("/{name}")
    } else {
        join(location, name)
    }
}

pub fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        None => "/",
        Some(0) => "/",
        Some(cut) => &path[..cut],
    }
}

// A folder symlink stays navigable; a file kind stays a file.
pub fn directory(is_dir: bool, mode: u32, kind: &str) -> bool {
    // Port of Picker.directory: row.d, or a symlink (0o120xxx) with folder kind.
    is_dir || (mode & 0o170000 == 0o120000 && kind == "folder")
}

// A launch opens only a drawn view; the rest opens the list.
pub fn remembered_view(stored: &str) -> &str {
    if stored == "grid" {
        "grid"
    } else {
        "list"
    }
}

// Only a view-moving switch writes the state file.
pub fn view_switch<'a>(current: &str, next: &'a str) -> Option<&'a str> {
    if (next == "list" || next == "grid") && next != current {
        Some(next)
    } else {
        None
    }
}

// Visible tiles as listing rows; an empty listing has none.
pub fn tile_range(content_y: f64, cell_h: f64, tile_rows: usize, columns: usize, total: usize) -> Option<(usize, usize)> {
    if total == 0 {
        return None;
    }
    let columns = columns.max(1);
    let cell = cell_h.max(1.0);
    let rows = total.div_ceil(columns);
    let first_row = (content_y / cell).floor().max(0.0) as usize;
    let bottom = content_y + tile_rows.max(1) as f64 * cell;
    let last_row = rows.saturating_sub(1).min(((bottom / cell).ceil() as usize).saturating_sub(1));
    let first = (first_row.min(rows - 1) * columns).min(total - 1);
    let last = ((last_row + 1) * columns).saturating_sub(1).min(total - 1).max(first);
    Some((first, last))
}

// One grid step keeps the main edge rule.
pub fn grid_target(index: usize, delta: isize, columns: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let end = total - 1;
    // A cursor past the end answers the last row.
    let next = index as isize + delta;
    if next < 0 || next > end as isize {
        return index.min(end);
    }
    let at = index.min(end);
    let next = next as usize;
    if columns > 0 && ((delta == -1 && at.is_multiple_of(columns)) || (delta == 1 && next.is_multiple_of(columns))) {
        return at;
    }
    next
}

// Page steps clamp to the nearer end; single steps refuse instead.
pub fn page_target(index: usize, delta: isize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    (index as isize + delta).clamp(0, total as isize - 1) as usize
}

pub const WINDOW_COVER: usize = 2;
pub const WINDOW_SLACK: usize = 60;

// Twice the wider screen plus slack.
pub fn window_size(list_rows: usize, tile_rows: usize, columns: usize) -> usize {
    let tiles = tile_rows.max(1) * columns.max(1);
    WINDOW_COVER * list_rows.max(tiles) + WINDOW_SLACK
}
