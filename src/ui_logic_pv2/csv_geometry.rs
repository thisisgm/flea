//! The CSV table's faces and the board geometry it lays cells out in.
use super::csv;

/// The face a cell is measured and drawn in: the bold header, a body cell, or a number in tabular figures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Head,
    Body,
    Figure,
}

/// The face a body cell takes.
pub fn body_face(cell: &str) -> Face {
    if csv::is_number(cell) {
        Face::Figure
    } else {
        Face::Body
    }
}

/// The board's table geometry at a caption size: its pixel values scale with the caption, as the board sizes do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Quick Look's grid, with row numbers and both axes scrolling; the column fits its frame.
    pub look: bool,
    /// The inset of the first cell: the row padding in Quick Look, 12 in the column.
    pub edge: f64,
    /// The column's gap between cells.
    pub gap: f64,
    /// Quick Look's row number column.
    pub number_width: f64,
    /// Quick Look's padding on each side of a cell.
    pub pad_x: f64,
    pub body_pad_y: f64,
    pub head_pad_y: f64,
    pub row_height: f64,
    pub head_height: f64,
}

impl Geometry {
    /// `line` is the caption's line height, `row_padding_x` the theme's row padding, `hairline` the rule's width.
    pub fn new(look: bool, caption: f64, line: f64, row_padding_x: f64, hairline: f64) -> Self {
        let scale = caption / 12.;
        let board = |px: f64| (px * scale).round();
        let (body_pad_y, head_pad_y) = if look {
            (board(6.), board(8.))
        } else {
            (board(3.), board(3.))
        };
        Geometry {
            look,
            edge: if look { row_padding_x } else { board(12.) },
            gap: board(6.),
            number_width: board(42.),
            pad_x: row_padding_x,
            body_pad_y,
            head_pad_y,
            row_height: line + 2. * body_pad_y,
            head_height: line + 2. * head_pad_y + hairline,
        }
    }

    /// Body rows a frame of this height shows, never fewer than one.
    pub fn view_rows(&self, body_height: f64) -> usize {
        (body_height / self.row_height.max(1.)).ceil().max(1.) as usize
    }
}
