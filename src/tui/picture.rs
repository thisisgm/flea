use super::capabilities::Protocol;
use super::chafa::Grid;
use super::model::Model;
use super::pane::Area;
use super::pane_state::{self, Kind};

// One row of a picture's stack: a picture row, then the player, the page line and controls, and the details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Picture(usize),
    Blank,
    Player,
    Page,
    Controls,
    Detail(usize),
}

// The picture's size in cells and the rows under it, laid out by kind so the box never moves as decoders start.
pub fn stack(m: &Model, area: &Area) -> ((usize, usize), Vec<Slot>) {
    let (cells, _, tail) = laid(m, area);
    (cells, tail)
}

// The stack and the room its picture fits into, the room counted before an empty picture gives up its blank row.
fn laid(m: &Model, area: &Area) -> ((usize, usize), usize, Vec<Slot>) {
    let Some(row) = m.rows.get(&m.cursor) else { return ((0, 0), area.height.saturating_sub(usize::from(area.card)), Vec::new()) };
    let kind = pane_state::kind(row);
    let details: Vec<Slot> = if area.card { Vec::new() } else {
        let limit = if kind == Kind::Model3d { area.height.saturating_sub(5) } else { 3 };
        m.preview_metadata.iter().enumerate().filter(|(index, line)| super::pane::metadata_visible(m, *index, line)).map(|(index, _)| Slot::Detail(index)).take(limit).collect()
    };
    let lead = |mut tail: Vec<Slot>, rest: Vec<Slot>| {
        if !rest.is_empty() {
            tail.push(Slot::Blank);
            tail.extend(rest);
        }
        tail
    };
    let mut tail = match kind {
        Kind::Video => lead(vec![Slot::Blank, Slot::Player], details),
        Kind::Audio => lead(vec![Slot::Player], details),
        Kind::Pdf => vec![Slot::Blank, Slot::Page, Slot::Controls],
        Kind::Office => lead(vec![Slot::Blank, Slot::Page, Slot::Controls], details),
        _ => lead(Vec::new(), details),
    };
    let room = area.height.saturating_sub(tail.len() + usize::from(area.card));
    let room = model_room(m, kind, area.width(), room, area.card);
    let cells = picture(m, row, kind, (area.width(), room));
    if cells.1 == 0 && tail.first() == Some(&Slot::Blank) {
        tail.remove(0);
    }
    (cells, room, tail)
}

fn model_room(m: &Model, kind: Kind, columns: usize, rows: usize, card: bool) -> usize {
    if kind != Kind::Model3d || card { return rows; }
    let cell = m.preview_state.cell.unwrap_or(super::graphics::FALLBACK_CELL);
    rows.min(columns.saturating_mul(cell.0).saturating_mul(3) / cell.1.max(1) / 4)
}

pub fn slot(m: &Model, i: usize, area: &Area) -> Slot {
    let ((_, rows), tail) = stack(m, area);
    let slot = if i < rows { Slot::Picture(i) } else { tail.get(i - rows).copied().unwrap_or(Slot::Blank) };
    if shows(m, slot) { slot } else { Slot::Blank }
}

// A page line and its controls wait for the first page, so nothing moves when it lands; a player waits for its player.
fn shows(m: &Model, slot: Slot) -> bool {
    match slot {
        Slot::Page | Slot::Controls => m.pdf.as_ref().is_some_and(|pdf| pdf.settled),
        Slot::Player => m.player.is_some(),
        _ => true,
    }
}

// The half-block grid for the shown preview only, so a stale picture never draws for the next file.
pub fn grid(m: &Model) -> Option<&Grid> {
    let still = &m.preview_state.still;
    let key = still.key().filter(|key| key.generation == m.preview_generation && Some(&key.path) == pane_state::content(m).as_ref())?;
    still.grid(key)
}

// A picture's cells: chafa's grid, a page's decoded pixels, or the file's own size fitted as magick and mpv fit it.
fn picture(m: &Model, row: &super::model::Row, kind: Kind, room: (usize, usize)) -> (usize, usize) {
    if room.0 == 0 || room.1 == 0 {
        return (0, 0);
    }
    let cell = m.preview_state.cell.filter(|cell| cell.0 > 0 && cell.1 > 0);
    match (m.capabilities.graphics, cell) {
        (Protocol::None, _) => grid(m).map_or((0, 0), |grid| (grid.columns, grid.rows)),
        (_, None) => (0, 0),
        (_, Some(cell)) => match kind {
            // Nothing is reserved before the probe answers, and the decode waits for it too, so a picture is decoded once.
            Kind::Image if !pane_state::pictured(row) => (0, 0),
            Kind::Image | Kind::Video => m.media_probe.map_or((0, 0), |(w, h, _)| fit((w, h), room, cell)),
            // An embedded thumbnail holds the box until the converted page replaces it; without one the kind mark stands.
            Kind::Office if !m.pdf.as_ref().is_some_and(super::office::page_drawn) => match (m.image_file.is_some(), m.media_probe) {
                (true, Some((w, h, _))) => fit((w, h), room, cell),
                _ => (0, 0),
            },
            Kind::Pdf | Kind::Office => m.pdf.as_ref().and_then(|pdf| pdf.shown).map_or((0, 0), |size| {
                ((size.0.div_ceil(cell.0)).min(room.0), ((size.1 + cell.1 / 2) / cell.1).clamp(1, room.1))
            }),
            Kind::Model3d => super::model3d::pixels(room, cell).map_or((0, 0), |p| (p.0.div_ceil(cell.0), p.1.div_ceil(cell.1))),
            _ => (0, 0),
        },
    }
}

// The cells a w by h picture takes fitted into a box of cells, as magick -resize fits it into the box's pixels.
pub fn fit(size: (usize, usize), cells: (usize, usize), cell: (usize, usize)) -> (usize, usize) {
    let (width, height) = (cells.0 * cell.0, cells.1 * cell.1);
    if size.0 == 0 || size.1 == 0 || width == 0 || height == 0 {
        return cells;
    }
    let (w, h) = if width * size.1 <= height * size.0 {
        (width, (size.1 * width + size.0 / 2) / size.0)
    } else {
        ((size.0 * height + size.1 / 2) / size.1, height)
    };
    (w.div_ceil(cell.0).clamp(1, cells.0), ((h + cell.1 / 2) / cell.1).clamp(1, cells.1))
}

// Where decoders draw: the picture's cells, the box they fit into, and the picture's top-left cell, one-based.
pub struct Place {
    pub cells: (usize, usize),
    pub room: (usize, usize),
    pub left: usize,
    pub top: usize,
}

pub fn place(m: &Model) -> Place {
    let (area, x0, top) = frame(m);
    // The room stays put when a half-block grid lands, so the decode it keys is never asked again for a room one row off.
    let ((columns, rows), room, _) = laid(m, &area);
    let room = (area.width(), room);
    let (columns, rows) = if rows == 0 { room } else { (columns, rows) };
    let offset = if area.card { room.0.saturating_sub(columns) / 2 } else { 0 };
    let left = if m.quicklook { x0 + offset } else { super::layout::visual_start(m, x0 + offset, columns) };
    Place { cells: (columns, rows), room, left: left + 1, top }
}

// The body area, its first content column (zero-based) and its first body row (one-based) in the pane or the card.
fn frame(m: &Model) -> (Area, usize, usize) {
    if m.quicklook {
        let card = super::look::area(m.columns, m.height);
        let x0 = super::look::left(m.columns) + card.padding;
        return (Area { height: super::look::text_rows(m.height), ..card }, x0, super::layout::BODY_ROW + 4);
    }
    let (start, _) = super::render::preview_area(m.columns, false);
    let columns = super::layout::panes(m.columns, true).2;
    let padding = super::layout::pane_padding(columns);
    (Area { columns, padding, height: m.height.saturating_sub(3), card: false }, start, super::layout::BODY_ROW + 3)
}

// The control row a pointer can press: its one-based screen row, its first content column and its width.
pub fn control_row(m: &Model) -> Option<(usize, usize, usize)> {
    let (area, x0, top) = frame(m);
    let ((_, rows), tail) = stack(m, &area);
    let index = tail.iter().position(|slot| matches!(slot, Slot::Player | Slot::Controls) && shows(m, *slot))?;
    Some((top + rows + index, x0, area.width()))
}

#[cfg(test)]
#[path = "picture_tests.rs"]
mod tests;
