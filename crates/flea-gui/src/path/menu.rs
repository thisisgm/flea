//! The suggestion popup: the menu recipe on the field's edges, rows at the text size, a cursor on the 8% wash and fades where it clips.
use super::paint::{line, paint_run, Ink, Paint, Size};
use super::scale::Scale;
use super::suggestions::{Popup, Suggestion, FRAME};
use super::view::PathView;
use crate::list::Typeface;
use crate::{list::Palette, palette::Theme, row::marks::Mark};
use gpui::{
    fill, linear_color_stop, linear_gradient, point, px, quad, relative, size, App, BorderStyle,
    Bounds, ContentMask, Element, ElementId, Entity, GlobalElementId, GlyphRun, InspectorElementId,
    IntoElement, LayoutId, Pixels, Point, Style, TransformationMatrix, Window,
};
use std::rc::Rc;

// A non-cursor row's typed run sits on the 14% wash; the cursor's turns bold instead, no box inside a highlight.
const RUN_WASH: f32 = 0.14;
const S_IFDIR: u32 = 0o040000;

/// One shaped piece of a row's label: its row, its left edge after the row's earlier pieces, whether it is the leaf or the match run, and the run.
pub struct Piece {
    pub row: usize,
    pub x: f32,
    pub leaf: bool,
    pub wash: bool,
    pub run: Rc<GlyphRun>,
}

impl Piece {
    pub fn width(&self) -> f32 {
        f32::from(self.run.line().width)
    }
}

pub struct MenuElement {
    view: Entity<PathView>,
}

impl MenuElement {
    pub fn new(view: Entity<PathView>) -> Self {
        Self { view }
    }
}

impl Paint {
    /// Refills the kept pieces of rows first..last, each laid after the row's earlier pieces on their shaped widths.
    pub fn lay_menu(
        &mut self,
        rows: &[Suggestion],
        cursor: Option<usize>,
        (first, last): (usize, usize),
        mut shape: impl FnMut(&Typeface, &str, bool) -> GlyphRun,
    ) {
        let Self {
            typeface,
            shapes,
            pieces,
            ..
        } = self;
        pieces.clear();
        for (index, row) in rows.iter().enumerate().take(last).skip(first) {
            let bold = layers(row, cursor == Some(index)).bold_run;
            let mut x = 0.;
            for pair in cuts(row).windows(2) {
                let (from, to) = (pair[0], pair[1]);
                if to <= from || !row.text.is_char_boundary(from) || !row.text.is_char_boundary(to)
                {
                    continue;
                }
                let wash = !row.wash.is_empty() && from >= row.wash.start && to <= row.wash.end;
                let text = &row.text[from..to];
                let run = shapes.get(text, Size::Body, bold && wash, || {
                    shape(typeface, text, bold && wash)
                });
                let piece = Piece {
                    row: index,
                    x,
                    leaf: from >= row.leaf_start,
                    wash,
                    run,
                };
                x += piece.width();
                pieces.push(piece);
            }
        }
    }
}

// Jump.segments: the cuts are the leaf's start and the match run's two edges, so a row is at most four pieces.
pub fn cuts(row: &Suggestion) -> [usize; 5] {
    let len = row.text.len();
    let clamp = |at: usize| at.min(len);
    let mut cuts = [0, clamp(row.leaf_start), len, len, len];
    if !row.wash.is_empty() {
        cuts[3] = clamp(row.wash.start);
        cuts[4] = clamp(row.wash.end);
    }
    cuts.sort_unstable();
    cuts
}

/// The mark's box in the row whose top is top inside the popup's inner origin: at the pad, centred on whole pixels.
pub fn mark_box(scale: &Scale, inner: Point<Pixels>, top: f32) -> Bounds<Pixels> {
    Bounds::new(
        point(
            inner.x + px(scale.pad_x),
            inner.y + px(top + scale.mark_top()),
        ),
        size(px(scale.mark), px(scale.mark)),
    )
}

/// The label's line top in that row, centred on whole pixels; its glyphs and run wash sit on it.
pub fn line_top(scale: &Scale, inner: Point<Pixels>, top: f32) -> Pixels {
    inner.y + px(top + scale.label_top())
}

/// A typed run's wash behind one piece of a row's label: from x across width, one line box tall.
pub fn run_wash(scale: &Scale, x: Pixels, line_top: Pixels, width: Pixels) -> Bounds<Pixels> {
    Bounds::new(point(x, line_top), size(width, px(scale.wash_line())))
}

/// What a row paints under its label: the cursor's band with a bold run, or the run's own wash, never a wash inside the band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layers {
    pub band: bool,
    pub run_wash: bool,
    pub bold_run: bool,
}

pub fn layers(row: &Suggestion, cursor: bool) -> Layers {
    let run = !row.wash.is_empty();
    Layers {
        band: cursor,
        run_wash: run && !cursor,
        bold_run: run && cursor,
    }
}

impl IntoElement for MenuElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for MenuElement {
    type RequestLayoutState = ();
    type PrepaintState = Ink;

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
    ) -> Ink {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let view = self.view.read(cx);
        let mut paint = view.paint.borrow_mut();
        let rows = view.jump.entries.len();
        let popup = Popup::framed(
            paint.scale(),
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            rows,
        );
        paint.popup = Some(popup);
        let visible = popup.visible(view.scroll, rows);
        paint.drawn = visible.1 - visible.0;
        paint.lay_menu(
            &view.jump.entries,
            view.jump.cursor,
            visible,
            |typeface, text, bold| line(typeface, text, Size::Body, bold, window),
        );
        Ink::of(Palette::inherited(&window.text_style(), theme))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        ink: &mut Ink,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        let paint = view.paint.borrow();
        let Some(popup) = paint.popup else { return };
        let scale = paint.scale();
        let scroll = view.scroll;
        window.paint_quad(quad(
            bounds,
            px(view.corner),
            ink.surface,
            px(FRAME),
            ink.muted_frame,
            BorderStyle::Solid,
        ));
        let inner = Bounds::new(
            point(bounds.origin.x + px(FRAME), bounds.origin.y + px(FRAME)),
            size(
                (bounds.size.width - px(2. * FRAME)).max(px(0.)),
                (bounds.size.height - px(2. * FRAME)).max(px(0.)),
            ),
        );
        let label_x = inner.origin.x + px(scale.label_x());
        let label_w =
            (inner.size.width - px(scale.pad_x + scale.mark + scale.gap + scale.pad_x)).max(px(0.));
        window.with_content_mask(Some(ContentMask { bounds: inner }), |window| {
            let (first, last) = popup.visible(scroll, view.jump.entries.len());
            for index in first..last {
                let top = scale.row_top(index, scroll);
                let layers = layers(&view.jump.entries[index], view.jump.cursor == Some(index));
                if layers.band {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(inner.origin.x, inner.origin.y + px(top)),
                            size(inner.size.width, px(scale.row)),
                        ),
                        ink.hover(),
                    ));
                }
                let (key, data) = Mark::for_row("folder", S_IFDIR).svg();
                let _ = window.paint_svg(
                    mark_box(&scale, inner.origin, top),
                    key,
                    Some(data),
                    TransformationMatrix::unit(),
                    ink.muted_frame,
                    cx,
                );
                let pieces = paint.pieces.iter().filter(|piece| piece.row == index);
                let drawn = pieces
                    .clone()
                    .next_back()
                    .map_or(0., |piece| piece.x + piece.width());
                // A path too long for the slot slides left inside it, because the tail names the folder.
                let slide = (label_w - px(drawn)).min(px(0.));
                let band = Bounds::new(
                    point(label_x, inner.origin.y + px(top)),
                    size(label_w, px(scale.row)),
                );
                let line = line_top(&scale, inner.origin, top);
                window.with_content_mask(Some(ContentMask { bounds: band }), |window| {
                    for piece in pieces {
                        let x = label_x + slide + px(piece.x);
                        if piece.wash && layers.run_wash {
                            window.paint_quad(fill(
                                run_wash(&scale, x, line, piece.run.line().width),
                                ink.wash(RUN_WASH),
                            ));
                        }
                        let color = if piece.leaf {
                            ink.foreground
                        } else {
                            ink.muted
                        };
                        paint_run(&piece.run, x, line, px(scale.line), color, window);
                    }
                });
            }
            let (above, below) = popup.fades(scroll);
            let clear = ink.surface.opacity(0.);
            if above {
                let band = Bounds::new(inner.origin, size(inner.size.width, px(scale.fade)));
                window.paint_quad(fill(
                    band,
                    linear_gradient(
                        180.,
                        linear_color_stop(ink.surface, 0.),
                        linear_color_stop(clear, 1.),
                    ),
                ));
            }
            if below {
                let band = Bounds::new(
                    point(inner.origin.x, inner.bottom() - px(scale.fade)),
                    size(inner.size.width, px(scale.fade)),
                );
                window.paint_quad(fill(
                    band,
                    linear_gradient(
                        0.,
                        linear_color_stop(ink.surface, 0.),
                        linear_color_stop(clear, 1.),
                    ),
                ));
            }
        });
    }
}
