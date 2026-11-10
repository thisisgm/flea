//! The Trash rows as one custom Element: the list's rows at the text size, a glyph or its thumbnail, the name, the location and the date.
use super::board::{Board, HOVER, WASH};
use super::elide;
use super::ink::Ink;
use crate::row::marks::Mark;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Corners, Element, ElementId, Font,
    GlobalElementId, GlyphRun, Hsla, ImageAssetLoader, InspectorElementId, IntoElement, LayoutId,
    ObjectFit, Pixels, RenderImage, Resource, Style, TextRun, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// One held row's paint inputs, written by the pane when its view changes, never per frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub name: String,
    pub location: String,
    pub deleted: String,
    pub mark: Mark,
    pub marked: bool,
    pub thumb: Option<Resource>,
}

struct Shaped {
    name: GlyphRun,
    location: GlyphRun,
    deleted: GlyphRun,
}

/// Paint state the pane keeps across frames.
pub struct ListPaint {
    pub font: Font,
    pub board: Board,
    pub ink: Ink,
    pub first: usize,
    pub total: usize,
    pub bytes: Option<(u64, bool)>,
    pub error_text: Option<String>,
    pub lines: Vec<Line>,
    /// Bumped whenever a line's text changes, so shaped text is rebuilt only then; marks and images never reshape.
    pub revision: u64,
    pub source_request: u64,
    /// Bumped when layout inputs change; prepaint copies it into laid_out, so equal means the frame drew them.
    pub committed: u64,
    pub laid_out: u64,
    surface_drawn: bool,
    pub scroll_y: f32,
    pub cursor: usize,
    pub hover: Option<usize>,
    pub hover_wash: bool,
    /// The cursor's accent edge shows only while the list holds the keyboard.
    pub edge: bool,
    shaped: Vec<Option<Shaped>>,
    shaped_for: (u64, usize),
    advance: Option<Pixels>,
    images: Vec<Option<Arc<RenderImage>>>,
    /// Rows the last prepaint laid out, and which of them drew a decoded image.
    pub painted: Vec<usize>,
    pub bounds: Option<Bounds<Pixels>>,
    pub visible_bounds: Option<Bounds<Pixels>>,
    pub drawn_images: Vec<usize>,
    pub shapes: u64,
}

impl ListPaint {
    pub fn new(font: Font, board: Board, ink: Ink) -> Self {
        Self {
            font,
            board,
            ink,
            first: 0,
            total: 0,
            bytes: None,
            error_text: None,
            lines: Vec::new(),
            revision: 0,
            source_request: 0,
            committed: 0,
            laid_out: 0,
            surface_drawn: false,
            scroll_y: 0.,
            cursor: 0,
            hover: None,
            hover_wash: false,
            edge: true,
            shaped: Vec::new(),
            shaped_for: (u64::MAX, 0),
            advance: None,
            images: Vec::new(),
            painted: Vec::new(),
            bounds: None,
            visible_bounds: None,
            drawn_images: Vec::new(),
            shapes: 0,
        }
    }

    /// Whether this surface has prepainted the committed snapshot since its last removal or opening.
    pub fn drawn(&self) -> bool {
        self.surface_drawn && self.committed == self.laid_out
    }

    pub(super) fn invalidate_surface(&mut self) {
        self.surface_drawn = false;
    }

    pub fn line(&self, index: usize) -> Option<&Line> {
        index
            .checked_sub(self.first)
            .and_then(|at| self.lines.get(at))
    }

    /// The marked cursor row's rung and accent edge, drawn only while the list has the keyboard.
    pub fn edge_at(&self, index: usize) -> bool {
        // TrashView.qml grounds marked rows only, so a cursor row v unmarked draws no rung (EY-388).
        index == self.cursor && self.edge && self.line(index).is_some_and(|line| line.marked)
    }

    /// The row's ground: the cursor rung while the list has the keyboard, the marked plate, or the pointer wash.
    pub fn ground(&self, index: usize) -> Option<Hsla> {
        let marked = self.line(index).is_some_and(|line| line.marked);
        if self.edge_at(index) {
            Some(self.ink.accent.opacity(WASH))
        } else if marked {
            Some(self.ink.mark_fill)
        } else if self.hover_wash && self.hover == Some(index) {
            Some(self.ink.foreground.opacity(HOVER))
        } else {
            None
        }
    }

    /// The window's system font and text size; a change reshapes every row and measures the digit advance again.
    pub fn set_face(&mut self, font: Font, board: Board) {
        if self.font != font || self.board != board {
            self.font = font;
            self.board = board;
            self.revision += 1;
            self.advance = None;
        }
    }

    fn advance(&mut self, window: &Window) -> Pixels {
        if let Some(advance) = self.advance {
            return advance;
        }
        let text = window.text_system();
        let id = text.resolve_font(&self.font);
        let advance = text
            .advance(id, self.board.body, '0')
            .map_or(self.board.body * 0.6, |a| a.width);
        self.advance = Some(advance);
        advance
    }
}

/// Columns measured from the row's left edge, as `.trashv .row` lays them out.
pub fn columns(board: &Board, width: f32) -> (f32, f32, f32, f32) {
    let deleted = width - board.pad_x - board.deleted_w;
    let location = deleted - board.gap - board.location_w;
    let name = board.pad_x + board.slot + board.gap;
    (
        name,
        (location - board.gap - name).max(0.),
        location,
        deleted,
    )
}

pub struct TrashList(pub Rc<RefCell<ListPaint>>);

impl IntoElement for TrashList {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn shape(text: &str, font: &Font, body: Pixels, window: &Window) -> GlyphRun {
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, body, &[run], None))
}

fn paint_text(run: &GlyphRun, x: Pixels, top: Pixels, row: f32, ink: Hsla, window: &mut Window) {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(row) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, ink);
}

impl Element for TrashList {
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
        cx: &mut App,
    ) {
        let mut state = self.0.borrow_mut();
        state.hover_wash = crate::list::hover_wash::HoverWash::on(cx);
        state.bounds = Some(bounds);
        let visible = bounds
            .intersect(&window.content_mask().bounds)
            .intersect(&Bounds::new(point(px(0.), px(0.)), window.viewport_size()));
        state.visible_bounds = Some(visible);
        state.surface_drawn = visible.size.width > px(0.) && visible.size.height > px(0.);
        if !state.surface_drawn {
            state.painted.clear();
            state.images.clear();
            state.drawn_images.clear();
            return;
        }
        state.laid_out = state.committed;
        let advance = state.advance(window);
        let board = state.board;
        let (_, name_w, _, _) = columns(&board, f32::from(bounds.size.width));
        let name_cells = (px(name_w) / advance).floor().max(0.) as usize;
        if state.shaped_for != (state.revision, name_cells) {
            state.shaped_for = (state.revision, name_cells);
            state.shaped.clear();
            let count = state.lines.len();
            state.shaped.resize_with(count, || None);
        }
        let first = (state.scroll_y / board.row_h).floor() as usize;
        let last = (((state.scroll_y + f32::from(bounds.size.height)) / board.row_h).ceil()
            as usize)
            .min(state.total);
        let (location_cells, deleted_cells) = (
            (px(board.location_w) / advance) as usize,
            (px(board.deleted_w) / advance) as usize,
        );
        let ListPaint {
            lines,
            shaped,
            painted,
            images,
            drawn_images,
            font,
            first: held_first,
            shapes,
            ..
        } = &mut *state;
        painted.clear();
        images.clear();
        drawn_images.clear();
        for index in first..last.max(first) {
            let Some(at) = index
                .checked_sub(*held_first)
                .filter(|at| *at < lines.len())
            else {
                continue;
            };
            painted.push(index);
            let line = &lines[at];
            if shaped[at].is_none() {
                *shapes += 1;
                shaped[at] = Some(Shaped {
                    name: shape(
                        &elide::right(&line.name, name_cells),
                        font,
                        board.body,
                        window,
                    ),
                    location: shape(
                        &elide::left(&line.location, location_cells),
                        font,
                        board.body,
                        window,
                    ),
                    deleted: shape(
                        &elide::right(&line.deleted, deleted_cells),
                        font,
                        board.body,
                        window,
                    ),
                });
            }
            let image = line
                .thumb
                .as_ref()
                .and_then(|source| window.use_asset::<ImageAssetLoader>(source, cx))
                .and_then(Result::ok);
            if image.is_some() {
                drawn_images.push(index);
            }
            images.push(image);
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
        let state = self.0.borrow();
        let ink = state.ink;
        let (left, width) = (bounds.origin.x, f32::from(bounds.size.width));
        let board = state.board;
        let (name_x, name_w, location_x, deleted_x) = columns(&board, width);
        window.paint_quad(fill(bounds, ink.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (slot, index) in state.painted.iter().enumerate() {
                let top = bounds.origin.y + px(*index as f32 * board.row_h - state.scroll_y);
                let row = Bounds::new(point(left, top), size(bounds.size.width, px(board.row_h)));
                if let Some(ground) = state.ground(*index) {
                    window.paint_quad(fill(row, ground));
                }
                if state.edge_at(*index) {
                    window.paint_quad(fill(
                        Bounds::new(point(left, top), size(px(board.edge), px(board.row_h))),
                        ink.edge(),
                    ));
                }
                let Some(line) = state.line(*index) else {
                    continue;
                };
                let glyph = Bounds::new(
                    point(
                        left + px(board.pad_x + (board.slot - board.glyph) / 2.),
                        top + px((board.row_h - board.glyph) / 2.),
                    ),
                    size(px(board.glyph), px(board.glyph)),
                );
                match &state.images[slot] {
                    Some(image) => {
                        let fit = ObjectFit::Contain.get_bounds(glyph, image.size(0));
                        let _ = window.paint_image(
                            glyph,
                            fit,
                            Corners::default(),
                            Arc::clone(image),
                            0,
                            false,
                        );
                    }
                    None => {
                        let (key, data) = line.mark.svg();
                        let _ = window.paint_svg(
                            glyph,
                            key,
                            Some(data),
                            TransformationMatrix::unit(),
                            ink.foreground,
                            cx,
                        );
                    }
                }
                let Some(shaped) = state
                    .shaped
                    .get(*index - state.first)
                    .and_then(Option::as_ref)
                else {
                    continue;
                };
                let names = Bounds::new(
                    point(left + px(name_x), top),
                    size(px(name_w), px(board.row_h)),
                );
                window.with_content_mask(Some(ContentMask { bounds: names }), |window| {
                    let x = names.origin.x;
                    paint_text(&shaped.name, x, top, board.row_h, ink.foreground, window);
                });
                let location_right = left + px(location_x + board.location_w);
                paint_text(
                    &shaped.location,
                    location_right - shaped.location.line().width,
                    top,
                    board.row_h,
                    ink.foreground,
                    window,
                );
                let deleted_right = left + px(deleted_x + board.deleted_w);
                paint_text(
                    &shaped.deleted,
                    deleted_right - shaped.deleted.line().width,
                    top,
                    board.row_h,
                    ink.foreground,
                    window,
                );
            }
        });
    }
}
