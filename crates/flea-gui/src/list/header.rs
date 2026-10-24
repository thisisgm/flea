//! The column header: one custom Element paints the ground, the hairline, the titles and the resize zones' accent line, after ui/Header.qml.
use super::{
    columns::{self, Cell, Column, Layout},
    element::paint_line,
    pane::Held,
    ListState, ListView, Metrics, Palette, HAIRLINE, NORMAL,
};
use crate::{
    navigation::folder_state::Order,
    palette::Theme,
    row::{format, is_symlink},
};
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

/// One grab zone over a drawn column's left edge: ResizeHandle.qml's nine pixels, offset by half so the hairline sits in its middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub column: Column,
    pub x: f32,
    pub width: f32,
}

/// Header.qml's handles Loader: zones exist only in a single pane's list header with no search on, one per drawn column.
pub fn handles(
    layout: &Layout,
    metrics: &Metrics,
    view: &ListView,
) -> impl Iterator<Item = Handle> {
    let active = !view.dual && !view.searching;
    let width = metrics.handle;
    let half = (width / 2.).floor();
    let layout = *layout;
    Column::ALL.into_iter().filter_map(move |column| {
        let cell = layout.cell(column).filter(|_| active)?;
        Some(Handle {
            column,
            x: cell.x - half,
            width,
        })
    })
}

/// The zone under a header-local x, if any.
pub fn handle_at(layout: &Layout, metrics: &Metrics, view: &ListView, x: f32) -> Option<Column> {
    handles(layout, metrics, view)
        .find(|handle| x >= handle.x && x < handle.x + handle.width)
        .map(|handle| handle.column)
}

/// The patch one remembered edge writes through `flea --ui-state`, merged into ui.json's columnWidths map.
pub fn width_patch(column: Column, width: u32) -> String {
    format!("{{\"columnWidths\":{{\"{}\":{width}}}}}", column.key())
}

/// ColumnFit.cellText(): the string a row draws in one column, measured by an autofit.
pub fn cell_text(column: Column, held: &Held, index: usize, view: &ListView) -> Option<String> {
    let facts = held.row(index)?;
    // Before the catalog words a date and a link's size word print nothing, so a fit there has nothing to measure.
    let worded = column == Column::Date || (column == Column::Size && is_symlink(facts.mode));
    if worded && !view.words.ready {
        return None;
    }
    Some(match column {
        Column::Mode => format::permissions(facts.mode),
        Column::Size => {
            let walked = if facts.dir {
                view.walked_for(held.listing, index)
            } else {
                None
            };
            format::size_text(facts, walked, &view.words)
        }
        Column::Date => format::date_text(facts.mtime, &view.words),
        // The rows reply carries no kind names, so the held row's painted Kind cell is empty.
        Column::Kind => String::new(),
    })
}

/// Header.fittedWidth(): the widest held row's cell in the caption face, or None with no held row, which writes nothing.
pub fn fitted_width(
    column: Column,
    held: &Held,
    view: &ListView,
    current: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Option<u32> {
    let widths: Vec<f32> = (held.start..held.start + held.rows.len())
        .filter_map(|index| cell_text(column, held, index, view))
        .map(|text| measure(&text).ceil())
        .collect();
    (!widths.is_empty()).then(|| columns::autofit_width(&widths, f64::from(current)))
}

// What the shaped titles were shaped for; a language change drops them through forget_titles().
#[derive(Clone, PartialEq)]
struct TitleKey {
    order: Order,
    font: Font,
    caption: Pixels,
    scale: f32,
}

struct Title {
    run: GlyphRun,
    text: String,
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
    pub fn forget_titles(&mut self) {
        self.titles = None;
    }

    /// The title cell Header.qml anchors: the name from the padding to the first drawn column's gap, each column its own cell.
    pub fn title_cell(layout: &Layout, metrics: &Metrics, column: Option<Column>) -> Option<Cell> {
        match column {
            None => Some(Cell {
                x: metrics.pad,
                width: (layout.name.right() - metrics.pad).max(0.),
            }),
            Some(column) => layout.cell(column),
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

    /// A release: writes once after a real move, never on a click.
    pub fn end_drag(&mut self) -> Option<String> {
        let (column, width) = self.header.drag.end()?;
        self.write_width(column, width)
    }

    /// Header.autofitColumn(): a double click ends any drag, then fits one drawn column over the held rows only.
    pub fn autofit(&mut self, column: Column, held: &Held, window: &Window) -> Option<String> {
        self.header.drag.cancel();
        if self.view.dual || self.view.searching || !self.drawn.layout.shown(column) {
            return None;
        }
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

#[path = "header_words.rs"]
mod words;
pub use words::titles;

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

impl Element for HeaderElement {
    type RequestLayoutState = ();
    type PrepaintState = HeaderFrame;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let height = element_height(&self.state.borrow().typeface);
        let style = Style {
            size: size(relative(1.).into(), px(height).into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> HeaderFrame {
        let mut state = self.state.borrow_mut();
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = state.palette(window, theme);
        let width = f32::from(bounds.size.width);
        let (metrics, tokens, mut layout) = state.layout(width, window);
        let drag = state.header.drag;
        if let Some(column) = drag.column {
            // The preview moves the header alone; the rows keep the stored layout until the one write on release.
            layout = layout.previewing(
                column,
                drag.preview,
                &tokens,
                (width - metrics.lane).max(0.),
            );
        }
        let key = TitleKey {
            order: state.view.order,
            font: state.typeface.font.clone().bold(),
            caption: state.typeface.caption,
            scale: window.scale_factor(),
        };
        let ListState { view, header, .. } = &mut *state;
        if header
            .titles
            .as_ref()
            .is_none_or(|titles| titles.key != key)
        {
            let text = window.text_system();
            let shape = |column: Option<Column>| {
                let title = words::painted(view, column);
                Title {
                    run: GlyphRun::new(text.layout_line(
                        &title,
                        key.caption,
                        &[title_run(title.len(), &key.font)],
                        None,
                    )),
                    text: title,
                }
            };
            header.painted_titles = titles(view);
            header.titles = Some(Titles {
                name: shape(None),
                columns: Column::ALL.map(|column| shape(Some(column))),
                key,
            });
            header.drawn.shaped += 1;
        }
        header.drawn = HeaderDrawn {
            bounds,
            layout,
            frames: header.drawn.frames + 1,
            shaped: header.drawn.shaped,
        };
        let active = !view.dual && !view.searching;
        HeaderFrame {
            palette,
            metrics,
            layout,
            line: drag
                .column
                .or(header.hot)
                .filter(|column| active && layout.shown(*column)),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut HeaderFrame,
        window: &mut Window,
        _: &mut App,
    ) {
        let HeaderFrame {
            palette,
            metrics,
            layout,
            line,
        } = *frame;
        let state = self.state.borrow();
        let Some(titles) = &state.header.titles else {
            return;
        };
        let left = bounds.origin.x;
        let top = bounds.origin.y;
        window.paint_quad(fill(bounds, palette.foreground.opacity(NORMAL)));
        let rule = Bounds::new(
            point(left, top + bounds.size.height - px(metrics.hairline)),
            size(bounds.size.width, px(metrics.hairline)),
        );
        window.paint_quad(fill(rule, palette.foreground.opacity(HAIRLINE)));
        // The board's padding sits above the titles, so their line box is the header less that padding.
        let text_top = top + px(metrics.header_top);
        let text_height = metrics.header - metrics.header_top;
        window.paint_layer(bounds, |window| {
            for column in words::CELLS {
                let Some(cell) = HeaderState::title_cell(&layout, &metrics, column) else {
                    continue;
                };
                let title = column.map_or(&titles.name, |column| &titles.columns[column.slot()]);
                let run = &title.run;
                let x = match column {
                    Some(Column::Size | Column::Date) => left + px(cell.right()) - run.line().width,
                    _ => left + px(cell.x),
                };
                let mask = Bounds::new(
                    point(left + px(cell.x), top),
                    size(px(cell.width), bounds.size.height),
                );
                window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                    paint_line(run, x, text_top, text_height, palette.metadata, window);
                });
            }
        });
        // ResizeHandle.qml's accent line: one hairline on the column's left edge while its zone is hovered or dragged.
        if let Some(cell) = line.and_then(|column| layout.cell(column)) {
            let edge = Bounds::new(
                point(left + px(cell.x), top),
                size(px(metrics.hairline), bounds.size.height),
            );
            window.paint_quad(fill(edge, palette.accent));
        }
    }
}
