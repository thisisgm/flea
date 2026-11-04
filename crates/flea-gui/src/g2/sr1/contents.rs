//! Contents results: one fixed-height block per match, the file's row then its first matching line, painted from prepared runs.
use super::run::RUN_WASH;
#[path = "contents_text.rs"]
mod text;
use crate::list::{mark_bounds, RowStates, Typeface, END_W, LABEL_X, PAD_X};
use crate::row;
use flea::protocol::RowFacts;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    GlyphRun, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Style, TransformationMatrix,
    Window,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use text::prepare;

#[cfg(test)]
type PaintedLine = (usize, u64, String, Bounds<Pixels>, bool, bool, bool);

#[cfg(test)]
thread_local! {
    pub(crate) static PAINTED_LINES: RefCell<Option<Vec<PaintedLine>>> = const { RefCell::new(None) };
}

// boards_find.py: the result row 31, its matching line 20 at caption 12, 4 of air under the block.
pub const ROW_H: f32 = 31.;
pub const LINE_H: f32 = 20.;
pub const BLOCK_PAD: f32 = 4.;
pub const BLOCK_H: f32 = ROW_H + LINE_H + BLOCK_PAD;
// The line number sits right-aligned in the 23 mark column, the line on the label column.
const NUMBER_W: f32 = 23.;
const CURSOR_WASH: f32 = 0.18;
// ui.py .row.hover: the pointer's row is the foreground at 8%, and its mark lifts to the foreground.
const HOVER: f32 = 0.08;
const EDGE_W: f32 = 2.;
const ELLIPSIS: char = '\u{2026}';
// The run's wash is its inline box, the board's 1.2 line height at caption size.
const RUN_LINE: f32 = 1.2;

/// The roles a contents block draws with, from the live theme at the join.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub marked: Hsla,
}

impl Palette {
    /// Reads the row and caption roles from the shared window palette.
    pub fn from_list(list: &crate::list::Palette) -> Self {
        Self {
            background: list.background,
            foreground: list.foreground,
            metadata: list.metadata,
            metadata_frame: list.metadata_frame,
            muted: list.muted,
            accent: list.accent,
            accent_frame: list.accent_frame,
            marked: list.marked,
        }
    }
    /// The lifted block's edge, drawn in the accent frame.
    pub fn edge(&self) -> Hsla {
        self.accent_frame
    }
}

/// One contents match as its producer will hand it over: the row's facts, the line's number and text.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub row: RowFacts,
    pub location: String,
    pub line_no: u64,
    pub line: String,
}

/// The held window of blocks, the integer-count model of every list.
#[derive(Clone, Debug, Default)]
pub struct Held {
    pub listing: usize,
    pub total: usize,
    pub start: usize,
    pub blocks: Vec<Block>,
}

impl Held {
    pub fn block(&self, index: usize) -> Option<&Block> {
        index
            .checked_sub(self.start)
            .and_then(|at| self.blocks.get(at))
    }
}

/// Visible blocks for a viewport: every block any pixel of the viewport crosses, and none past the count.
pub fn visible(scroll_y: f32, height: f32, total: usize) -> std::ops::Range<usize> {
    let first = (scroll_y.max(0.) / BLOCK_H).floor() as usize;
    let last = (((scroll_y.max(0.) + height) / BLOCK_H).ceil() as usize).min(total);
    first.min(last)..last
}

pub struct Prepared {
    pub name: GlyphRun,
    pub location: GlyphRun,
    pub size: GlyphRun,
    pub number: GlyphRun,
    pub line: GlyphRun,
    // The run's span on the line, absent inside the cursor's highlight where the run is bold instead.
    pub wash: Option<(Pixels, Pixels)>,
    pub mark: row::marks::Mark,
}

#[derive(PartialEq)]
struct Key {
    listing: usize,
    lifted: bool,
    cells: usize,
    scale: u32,
    languages: bool,
}

pub struct ContentsState {
    languages: bool,
    pub typeface: Typeface,
    pub query: String,
    entries: HashMap<usize, (Key, Rc<Prepared>)>,
    // The query the held entries were shaped for; a new query drops them all.
    shaped_for: String,
    pub visible: Vec<(usize, Rc<Prepared>)>,
    pub misses: u64,
    // The localized words the size cell prints, the same ones the list reads.
    words: row::format::Words,
}

impl ContentsState {
    pub fn apply_settings(&mut self, data: &flea::jsondoc::Json) {
        self.languages =
            data.get("langMarks").and_then(flea::jsondoc::Json::as_bool) != Some(false);
    }

    pub fn new(typeface: Typeface, words: row::format::Words) -> Self {
        Self {
            languages: true,
            typeface,
            query: String::new(),
            entries: HashMap::new(),
            shaped_for: String::new(),
            visible: Vec::new(),
            misses: 0,
            words,
        }
    }
}

pub struct ContentsElement {
    held: Rc<Held>,
    scroll_y: f32,
    rows: RowStates,
    palette: Palette,
    state: Rc<RefCell<ContentsState>>,
}

impl ContentsElement {
    pub fn new(
        held: Rc<Held>,
        scroll_y: f32,
        cursor: usize,
        palette: Palette,
        state: Rc<RefCell<ContentsState>>,
    ) -> Self {
        let rows = RowStates::cursor_only(held.listing, cursor);
        Self {
            held,
            scroll_y,
            rows,
            palette,
            state,
        }
    }

    /// The same listing-scoped selection, focus and pointer snapshot as the underlying list.
    pub fn with_states(mut self, rows: RowStates) -> Self {
        self.rows = rows;
        self
    }

    /// The block under the pointer, which the window's hover tracking names at the join.
    pub fn hovered(mut self, index: Option<usize>) -> Self {
        self.rows.hovered = index;
        self
    }
}

// A line cut on the right to the cells it has, the board's text-overflow ellipsis on a monospaced face.
fn right_elide(text: &str, cells: usize) -> String {
    if text.chars().count() <= cells {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(cells.saturating_sub(1)).collect();
    cut.push(ELLIPSIS);
    cut
}

fn paint_at(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    height: f32,
    color: Hsla,
    window: &mut Window,
) -> bool {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(height) - line.ascent - line.descent) / 2. + line.ascent);
    window
        .paint_glyph_run(point(x, baseline), run, color)
        .is_ok()
}

impl IntoElement for ContentsElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ContentsElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
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
        _: &mut App,
    ) {
        let mut state = self.state.borrow_mut();
        let advance = f32::from(state.typeface.caption) * 0.6;
        let room = f32::from(bounds.size.width) - LABEL_X - END_W;
        let cells = (room / advance).floor().max(0.) as usize;
        let range = visible(
            self.scroll_y,
            f32::from(bounds.size.height),
            self.held.total,
        );
        // The bound follows the viewport: entries outside three screens of it go.
        let keep = range.start.saturating_sub(range.len() * 2)..range.end + range.len() * 2;
        state.entries.retain(|index, _| keep.contains(index));
        if state.shaped_for != state.query {
            state.entries.clear();
            let query = state.query.clone();
            state.shaped_for = query;
        }
        state.visible.clear();
        for index in range {
            let Some(block) = self.held.block(index) else {
                continue;
            };
            let key = Key {
                listing: self.held.listing,
                lifted: self.rows.row(index, self.held.listing).cursor,
                cells,
                scale: window.scale_factor().to_bits(),
                languages: state.languages,
            };
            let hit = state
                .entries
                .get(&index)
                .filter(|(held, _)| *held == key)
                .map(|(_, prepared)| Rc::clone(prepared));
            let prepared = match hit {
                Some(prepared) => prepared,
                None => {
                    state.misses += 1;
                    let prepared = Rc::new(prepare(block, &state, key.lifted, cells, window));
                    state.entries.insert(index, (key, Rc::clone(&prepared)));
                    prepared
                }
            };
            state.visible.push((index, prepared));
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = self.state.borrow();
        let palette = self.palette;
        let (left, right) = (bounds.origin.x, bounds.origin.x + bounds.size.width);
        window.paint_quad(fill(bounds, palette.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (index, prepared) in &state.visible {
                let row = self.rows.row(*index, self.held.listing);
                let lifted = row.lifted();
                let top = bounds.origin.y + px(*index as f32 * BLOCK_H - self.scroll_y);
                let wash = if row.drop_target || (row.cursor && self.rows.focused) {
                    Some(palette.accent.opacity(CURSOR_WASH))
                } else if row.cursor {
                    Some(palette.foreground.opacity(crate::list::GREY_CURSOR))
                } else if row.selected {
                    Some(if self.rows.focused {
                        palette.marked
                    } else {
                        palette.foreground.opacity(crate::list::TRACK)
                    })
                } else if row.hovered {
                    Some(palette.foreground.opacity(HOVER))
                } else {
                    None
                };
                if let Some(wash) = wash {
                    let block = Bounds::new(point(left, top), size(bounds.size.width, px(BLOCK_H)));
                    window.paint_quad(fill(block, wash));
                }
                if row.drop_target || (row.cursor && self.rows.focused) {
                    window.paint_quad(fill(
                        Bounds::new(point(left, top), size(px(EDGE_W), px(BLOCK_H))),
                        palette.edge(),
                    ));
                }
                let (ink, meta) = if lifted {
                    (palette.foreground, palette.foreground)
                } else {
                    (palette.foreground, palette.metadata)
                };
                let gutter = if lifted {
                    palette.foreground
                } else {
                    palette.muted
                };
                let (key, data) = prepared.mark.svg();
                let mark_ink = if lifted {
                    palette.foreground
                } else {
                    palette.metadata_frame
                };
                let _ = window.paint_svg(
                    mark_bounds(left, top),
                    key,
                    Some(data),
                    TransformationMatrix::unit(),
                    mark_ink,
                    cx,
                );
                let name_x = left + px(LABEL_X);
                paint_at(&prepared.name, name_x, top, ROW_H, ink, window);
                let location_x = name_x + prepared.name.line().width;
                paint_at(&prepared.location, location_x, top, ROW_H, meta, window);
                let size_right = right - px(END_W);
                paint_at(
                    &prepared.size,
                    size_right - prepared.size.line().width,
                    top,
                    ROW_H,
                    meta,
                    window,
                );
                let line_top = top + px(ROW_H);
                let number_right = left + px(PAD_X + NUMBER_W);
                let _number_painted = paint_at(
                    &prepared.number,
                    number_right - prepared.number.line().width,
                    line_top,
                    LINE_H,
                    gutter,
                    window,
                );
                if let Some((start, end)) = prepared.wash {
                    let tall = f32::from(state.typeface.caption) * RUN_LINE;
                    let wash = Bounds::new(
                        point(name_x + start, line_top + px((LINE_H - tall) / 2.)),
                        size(end - start, px(tall)),
                    );
                    window.paint_quad(fill(wash, palette.accent.opacity(RUN_WASH)));
                }
                let _line_painted = paint_at(&prepared.line, name_x, line_top, LINE_H, ink, window);
                #[cfg(test)]
                if let Some(block) = self.held.block(*index) {
                    PAINTED_LINES.with(|lines| {
                        let mut lines = lines.borrow_mut();
                        let Some(lines) = lines.as_mut() else { return };
                        lines.push((
                            *index,
                            block.line_no,
                            block.line.clone(),
                            Bounds::new(point(left, line_top), size(bounds.size.width, px(LINE_H))),
                            _number_painted,
                            _line_painted,
                            row.selected,
                        ))
                    });
                }
            }
        });
    }
}
