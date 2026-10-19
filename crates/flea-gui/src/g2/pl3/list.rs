//! The Trash rows as one custom Element: fixed 31 rows, a glyph or its thumbnail, the name, the location and the date.
use super::board::{DELETED_W, EDGE, GAP, GLYPH, HOVER, LOCATION_W, PAD_X, ROW_H, SLOT, WASH};
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
    pub body: Pixels,
    pub ink: Ink,
    pub first: usize,
    pub total: usize,
    pub lines: Vec<Line>,
    /// Bumped whenever a line's text changes, so shaped text is rebuilt only then; marks and images never reshape.
    pub revision: u64,
    pub scroll_y: f32,
    pub cursor: usize,
    pub hover: Option<usize>,
    /// The cursor's accent edge shows only while the list holds the keyboard.
    pub edge: bool,
    shaped: Vec<Option<Shaped>>,
    shaped_for: (u64, usize),
    advance: Option<Pixels>,
    images: Vec<Option<Arc<RenderImage>>>,
    /// Rows the last prepaint laid out, and which of them drew a decoded image.
    pub painted: Vec<usize>,
    pub drawn_images: Vec<usize>,
    pub shapes: u64,
}

impl ListPaint {
    pub fn new(font: Font, body: Pixels, ink: Ink) -> Self {
        Self {
            font,
            body,
            ink,
            first: 0,
            total: 0,
            lines: Vec::new(),
            revision: 0,
            scroll_y: 0.,
            cursor: 0,
            hover: None,
            edge: true,
            shaped: Vec::new(),
            shaped_for: (u64::MAX, 0),
            advance: None,
            images: Vec::new(),
            painted: Vec::new(),
            drawn_images: Vec::new(),
            shapes: 0,
        }
    }

    pub fn line(&self, index: usize) -> Option<&Line> {
        index
            .checked_sub(self.first)
            .and_then(|at| self.lines.get(at))
    }

    /// The row's ground: the cursor rung while the list has the keyboard, the marked plate, or the pointer wash.
    pub fn ground(&self, index: usize) -> Option<Hsla> {
        let marked = self.line(index).is_some_and(|line| line.marked);
        if index == self.cursor && self.edge {
            Some(self.ink.accent.opacity(WASH))
        } else if marked {
            Some(self.ink.mark_fill)
        } else if self.hover == Some(index) {
            Some(self.ink.foreground.opacity(HOVER))
        } else {
            None
        }
    }

    fn advance(&mut self, window: &Window) -> Pixels {
        if let Some(advance) = self.advance {
            return advance;
        }
        let text = window.text_system();
        let id = text.resolve_font(&self.font);
        let advance = text
            .advance(id, self.body, '0')
            .map_or(self.body * 0.6, |a| a.width);
        self.advance = Some(advance);
        advance
    }
}

/// Columns measured from the row's left edge, as `.trashv .row` lays them out.
pub fn columns(width: f32) -> (f32, f32, f32, f32) {
    let deleted = width - PAD_X - DELETED_W;
    let location = deleted - GAP - LOCATION_W;
    let name = PAD_X + SLOT + GAP;
    (name, (location - GAP - name).max(0.), location, deleted)
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

fn paint_text(run: &GlyphRun, x: Pixels, top: Pixels, ink: Hsla, window: &mut Window) {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(ROW_H) - line.ascent - line.descent) / 2. + line.ascent);
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
        let advance = state.advance(window);
        let (_, name_w, _, _) = columns(f32::from(bounds.size.width));
        let name_cells = (px(name_w) / advance).floor().max(0.) as usize;
        if state.shaped_for != (state.revision, name_cells) {
            state.shaped_for = (state.revision, name_cells);
            state.shaped.clear();
            let count = state.lines.len();
            state.shaped.resize_with(count, || None);
        }
        let first = (state.scroll_y / ROW_H).floor() as usize;
        let last = (((state.scroll_y + f32::from(bounds.size.height)) / ROW_H).ceil() as usize)
            .min(state.total);
        let (location_cells, deleted_cells) = (
            (px(LOCATION_W) / advance) as usize,
            (px(DELETED_W) / advance) as usize,
        );
        let ListPaint {
            lines,
            shaped,
            painted,
            images,
            drawn_images,
            font,
            body,
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
                    name: shape(&elide::right(&line.name, name_cells), font, *body, window),
                    location: shape(
                        &elide::left(&line.location, location_cells),
                        font,
                        *body,
                        window,
                    ),
                    deleted: shape(
                        &elide::right(&line.deleted, deleted_cells),
                        font,
                        *body,
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
        let (name_x, name_w, location_x, deleted_x) = columns(width);
        window.paint_quad(fill(bounds, ink.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (slot, index) in state.painted.iter().enumerate() {
                let top = bounds.origin.y + px(*index as f32 * ROW_H - state.scroll_y);
                let row = Bounds::new(point(left, top), size(bounds.size.width, px(ROW_H)));
                if let Some(ground) = state.ground(*index) {
                    window.paint_quad(fill(row, ground));
                }
                if *index == state.cursor && state.edge {
                    window.paint_quad(fill(
                        Bounds::new(point(left, top), size(px(EDGE), px(ROW_H))),
                        ink.accent_frame,
                    ));
                }
                let Some(line) = state.line(*index) else {
                    continue;
                };
                let glyph = Bounds::new(
                    point(
                        left + px(PAD_X + (SLOT - GLYPH) / 2.),
                        top + px((ROW_H - GLYPH) / 2.),
                    ),
                    size(px(GLYPH), px(GLYPH)),
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
                let names = Bounds::new(point(left + px(name_x), top), size(px(name_w), px(ROW_H)));
                window.with_content_mask(Some(ContentMask { bounds: names }), |window| {
                    paint_text(&shaped.name, names.origin.x, top, ink.foreground, window);
                });
                let location_right = left + px(location_x + LOCATION_W);
                paint_text(
                    &shaped.location,
                    location_right - shaped.location.line().width,
                    top,
                    ink.foreground,
                    window,
                );
                let deleted_right = left + px(deleted_x + DELETED_W);
                paint_text(
                    &shaped.deleted,
                    deleted_right - shaped.deleted.line().width,
                    top,
                    ink.foreground,
                    window,
                );
            }
        });
    }
}
