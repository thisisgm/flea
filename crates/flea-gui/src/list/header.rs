//! The column header: one custom Element paints the ground, the hairline, the titles and the resize zones' accent line, after ui/Header.qml.
use super::{
    columns::{self, Cell, Column, Layout},
    element::paint_line,
    pane::Held,
    ListState, ListView, Metrics, Palette, HAIRLINE, NORMAL,
};
use crate::{navigation::folder_state::Order, palette::Theme};
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, Font,
    GlobalElementId, GlyphRun, InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun,
    Window,
};
use std::{cell::RefCell, rc::Rc};

/// A drag in flight, or the last one: Header.qml's dragKey, dragStartX, dragLastX, dragStartWidth, dragPreview and dragMoved.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drag {
    pub column: Option<Column>,
    pub start_x: f32,
    pub last_x: f32,
    pub start_width: f32,
    pub preview: f32,
    pub moved: bool,
}

impl Drag {
    /// Press on a zone: the drawn width is where the preview starts, so the edge never jumps under the pointer.
    pub fn begin(&mut self, column: Column, x: f32, width: f32) {
        *self = Self {
            column: Some(column),
            start_x: x,
            last_x: x,
            start_width: width,
            preview: width,
            moved: false,
        };
    }

    /// A move under one pixel is still a click; past it the edge follows, and widening moves it left: start minus dx.
    pub fn move_to(&mut self, column: Column, x: f32) -> bool {
        if self.column != Some(column) || (x - self.start_x).abs() < 1. {
            return false;
        }
        self.moved = true;
        self.last_x = x;
        self.preview =
            columns::clamp_width(f64::from(self.start_width - (x - self.start_x))) as f32;
        true
    }

    /// Release: the one width to write, only after a real move; the trace stays readable until the next press.
    pub fn end(&mut self) -> Option<(Column, u32)> {
        let column = self.column.take()?;
        let moved = std::mem::take(&mut self.moved);
        moved.then(|| (column, columns::clamp_width(f64::from(self.preview))))
    }

    /// A double click or a lost grab ends the drag with nothing to write.
    pub fn cancel(&mut self) {
        self.column = None;
        self.moved = false;
    }

    /// Ipc.headerDragTrace(): "startX|lastX|startWidth|preview".
    pub fn trace(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.start_x, self.last_x, self.start_width, self.preview
        )
    }
}

#[path = "header_handles.rs"]
mod handles;
pub use handles::{handle_at, handles, Handle};

/// The patch one remembered edge writes through `flea --ui-state`, merged into ui.json's columnWidths map.
pub fn width_patch(column: Column, width: u32) -> String {
    format!("{{\"columnWidths\":{{\"{}\":{width}}}}}", column.key())
}

// What the shaped titles were shaped for; a language change drops them through forget_titles().
#[derive(Clone, PartialEq)]
struct TitleKey {
    order: Order,
    recent: bool,
    font: Font,
    caption: Pixels,
    scale: f32,
}

const ELLIPSIS: &str = "\u{2026}";

struct Title {
    run: GlyphRun,
    text: String,
    // The title cut to the cell it is wider than, or None while the whole title fits.
    cut: Option<Cut>,
}

// A title cut for one cell width: the head and an ellipsis, shaped once for that width.
struct Cut {
    width: f32,
    run: GlyphRun,
    text: String,
}

impl Title {
    /// The run and text a cell draws: the whole title, or its cut head once the cell is narrower.
    fn drawn(&self) -> (&GlyphRun, &str) {
        match &self.cut {
            Some(cut) => (&cut.run, &cut.text),
            None => (&self.run, &self.text),
        }
    }

    /// Text.ElideRight for a cell `width` wide: the longest head that fits beside an ellipsis, shaped only when the width changes.
    fn fit(&mut self, width: f32, key: &TitleKey, window: &Window) {
        if f32::from(self.run.line().width) <= width {
            self.cut = None;
            return;
        }
        if self.cut.as_ref().is_some_and(|cut| cut.width == width) {
            return;
        }
        let shape = |text: &str| {
            GlyphRun::new(window.text_system().layout_line(
                text,
                key.caption,
                &[title_run(text.len(), &key.font)],
                None,
            ))
        };
        let line = self.run.line();
        let ellipsis = f32::from(shape(ELLIPSIS).line().width);
        let head = self
            .text
            .char_indices()
            .map(|(at, _)| at)
            .take_while(|at| f32::from(line.x_for_index(*at)) + ellipsis <= width)
            .last()
            .unwrap_or(0);
        let text = format!("{}{ELLIPSIS}", &self.text[..head]);
        self.cut = Some(Cut {
            width,
            run: shape(&text),
            text,
        });
    }
}

struct Titles {
    key: TitleKey,
    name: Title,
    columns: [Title; 4],
}

/// What the last header frame drew, read by IPC and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeaderDrawn {
    pub bounds: Bounds<Pixels>,
    pub layout: Layout,
    pub frames: u64,
    /// Title sets shaped so far, so a test can hold a scroll or a hover to zero reshapes.
    pub shaped: u64,
}

/// The header's state across frames: shaped titles, the drag, the zone under the pointer and the last frame.
#[derive(Default)]
pub struct HeaderState {
    titles: Option<Titles>,
    pub painted_titles: String,
    pub drag: Drag,
    /// The zone the pointer is over, whose accent line shows at rest.
    pub hot: Option<Column>,
    /// A title pressed and not yet released: its sort key and the pointer's window position, a tap if it stays put.
    pub press: Option<(&'static str, f32, f32)>,
    pub drawn: HeaderDrawn,
}

impl HeaderState {
    /// The Name title as last shaped, its sort arrow included, for tests.
    #[cfg(test)]
    pub fn name_title(&self) -> Option<&str> {
        self.titles.as_ref().map(|titles| titles.name.text.as_str())
    }

    pub fn forget_titles(&mut self) {
        self.titles = None;
    }

    /// The title cell Header.qml anchors: the name from the padding (in Dual from the row labels) to the first drawn column's gap, each column its own cell.
    pub fn title_cell(
        layout: &Layout,
        metrics: &Metrics,
        column: Option<Column>,
        dual: bool,
    ) -> Option<Cell> {
        match column {
            None => {
                // A Dual pane sets the name over the row labels, not the padding.
                let x = if dual {
                    layout.name.x
                } else {
                    metrics.pad + (layout.name.x - metrics.label_x()).max(0.)
                };
                Some(Cell {
                    x,
                    width: (layout.name.right() - x).max(0.),
                })
            }
            Some(column) => layout.cell(column),
        }
    }

    /// The span a title paints inside its cell, the whole cell until titles are shaped.
    pub fn title_span(&self, column: Option<Column>, cell: Cell) -> Cell {
        let width = self.titles.as_ref().map_or(cell.width, |titles| {
            let title = column.map_or(&titles.name, |column| &titles.columns[column.slot()]);
            f32::from(title.drawn().0.line().width).min(cell.width)
        });
        let x = if right_aligned(column) {
            cell.right() - width
        } else {
            cell.x
        };
        Cell { x, width }
    }

    /// Fits each drawn title to its own cell, so a title too wide for its cell ends in an ellipsis instead of clipping its head.
    fn fit_titles(&mut self, layout: &Layout, metrics: &Metrics, dual: bool, window: &Window) {
        let Some(Titles { key, name, columns }) = self.titles.as_mut() else {
            return;
        };
        for column in words::CELLS {
            let Some(cell) = Self::title_cell(layout, metrics, column, dual) else {
                continue;
            };
            let title = match column {
                None => &mut *name,
                Some(column) => &mut columns[column.slot()],
            };
            title.fit(cell.width, key, window);
        }
    }
}

impl ListState {
    /// One remembered edge: clamped to the rails, applied to the stored widths at once, and the patch to write, or None when nothing changed.
    pub fn write_width(&mut self, column: Column, width: u32) -> Option<String> {
        let next = columns::clamp_width(f64::from(width));
        if self.view.stored.get(column) == Some(next) {
            return None;
        }
        self.view.stored = self.view.stored.with(column, next);
        Some(width_patch(column, next))
    }

    /// A release over the header (`over`) or off it: writes once after a real move, never on a click.
    pub fn end_drag(&mut self, over: bool) -> Option<String> {
        // A release off the header lights no zone, even after a drag that moved no edge.
        if !over {
            self.header.hot = None;
        }
        let (column, width) = self.header.drag.end()?;
        let drag = self.header.drag;
        // ResizeHandle.qml stays lit under a still pointer only while the edge followed it, never once the rails clamped it.
        let followed =
            (drag.preview - (drag.start_width - (drag.last_x - drag.start_x))).abs() < 1.;
        self.header.hot = (over && followed).then_some(column);
        self.write_width(column, width)
    }

    /// A double click or F4 ends any drag, then fits an enabled column over the held rows only.
    pub fn autofit(&mut self, column: Column, held: &Held, window: &Window) -> Option<String> {
        self.header.drag.cancel();
        if self.view.dual
            || self.view.searching
            || !self.view.switched.on(column)
            || (self.view.recent && matches!(column, Column::Mode | Column::Kind))
        {
            return None;
        }
        // A fit moves the edges under a still pointer, so no zone stays lit from before it.
        self.header.hot = None;
        let (_, tokens) = self.measure(window);
        let current = tokens.wanted(column, self.view.stored);
        let text = window.text_system();
        let caption = self.typeface.caption;
        let font = self.typeface.font.clone();
        let fitted = fitted_width(column, held, &self.view, current, |value| {
            f32::from(
                text.layout_line(value, caption, &[title_run(value.len(), &font)], None)
                    .width,
            )
        })?;
        self.write_width(column, fitted)
    }
}

/// Size, Modified and Kind set their titles on the cell's right edge, the others on its left.
pub fn right_aligned(column: Option<Column>) -> bool {
    matches!(column, Some(Column::Size | Column::Date | Column::Kind))
}

fn title_run(len: usize, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color: gpui::Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// Header.title(): the sorted column's title carries the direction arrow; Mode is not a sort key.
fn sort_key(column: Option<Column>) -> Option<&'static str> {
    match column {
        None => Some("name"),
        Some(Column::Size) => Some("size"),
        Some(Column::Date) => Some("mtime"),
        Some(Column::Kind) => Some("kind"),
        Some(Column::Mode) => None,
    }
}

#[path = "header_hit.rs"]
mod hit;
pub use hit::{title_key, TAP_SLOP};

#[path = "header_words.rs"]
mod words;
pub use words::titles;

#[path = "header_fit.rs"]
mod fit;
pub use fit::{cell_text, fitted_width};

#[path = "header_probe.rs"]
mod probe;
pub(crate) use probe::element as probe_element;

/// The header slot's own height source (window/composition.rs), so the element never asks for more than the slot holds.
pub(crate) fn element_height(face: &super::Typeface) -> f32 {
    Metrics::for_typeface(face).header
}

pub struct HeaderElement {
    state: Rc<RefCell<ListState>>,
}

impl HeaderElement {
    pub fn new(state: Rc<RefCell<ListState>>) -> Self {
        Self { state }
    }
}

/// What prepaint resolved for paint.
#[derive(Clone, Copy)]
pub struct HeaderFrame {
    palette: Palette,
    metrics: Metrics,
    layout: Layout,
    line: Option<Column>,
}

impl IntoElement for HeaderElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[path = "header_element.rs"]
mod element;
