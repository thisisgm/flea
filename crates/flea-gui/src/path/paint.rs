//! What the path surfaces draw with: the inks, the face's advances, a bounded cache of shaped pieces and the crumb strip Element.
use super::{menu::Piece, strip::Strip, suggestions::Popup, view::PathView};
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, CursorStyle, Element, ElementId,
    Entity, FontWeight, GlobalElementId, GlyphRun, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun, Window,
};
use std::rc::Rc;

/// The roles the path surfaces draw with, interim from the inherited style until the theme tokens land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub background: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
}

// Board inks: hover is the foreground at 8%, the hairline 12%.
const HOVER: f32 = 0.08;
const HAIR: f32 = 0.12;

impl Ink {
    pub fn of(palette: Palette) -> Self {
        Self {
            background: palette.background,
            surface: palette.background,
            foreground: palette.foreground,
            muted: palette.metadata,
            muted_frame: palette.metadata_frame,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
        }
    }

    // Board inks: hover is the foreground at 8%, the washes the accent at 14% and 24%, the hairline 12%.
    pub fn hover(&self) -> Hsla {
        self.foreground.opacity(HOVER)
    }
    pub fn wash(&self, share: f32) -> Hsla {
        self.accent.opacity(share)
    }
    pub fn hair(&self) -> Hsla {
        self.foreground.opacity(HAIR)
    }
}

/// The face at the three sizes the path draws: crumbs at the caption, the field at 13 and suggestions at the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Caption,
    Field,
    Body,
}

struct Shape {
    text: String,
    size: Size,
    bold: bool,
    run: Rc<GlyphRun>,
    used: u64,
}

/// A bounded LRU of shaped pieces: a hit compares in place and allocates nothing.
pub struct Shapes {
    kept: Vec<Shape>,
    limit: usize,
    clock: u64,
    pub hits: u64,
    pub misses: u64,
}

impl Shapes {
    pub fn new(limit: usize) -> Self {
        Self {
            kept: Vec::with_capacity(limit),
            limit,
            clock: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.kept.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    pub fn get(
        &mut self,
        text: &str,
        size: Size,
        bold: bool,
        shape: impl FnOnce() -> GlyphRun,
    ) -> Rc<GlyphRun> {
        self.clock += 1;
        if let Some(kept) = self
            .kept
            .iter_mut()
            .find(|kept| kept.size == size && kept.bold == bold && kept.text == text)
        {
            kept.used = self.clock;
            self.hits += 1;
            return Rc::clone(&kept.run);
        }
        self.misses += 1;
        let run = Rc::new(shape());
        let shape = Shape {
            text: text.to_owned(),
            size,
            bold,
            run: Rc::clone(&run),
            used: self.clock,
        };
        if self.kept.len() < self.limit {
            self.kept.push(shape);
        } else if let Some(oldest) = self.kept.iter_mut().min_by_key(|kept| kept.used) {
            *oldest = shape;
        }
        run
    }
}

pub struct Paint {
    pub typeface: Typeface,
    advances: [Option<Pixels>; 3],
    pub shapes: Shapes,
    /// Where each surface last drew, for the pointer and the IPC readers: the chrome's path slot, the field's frame, each strip.
    pub area: Option<Bounds<Pixels>>,
    pub frame: Option<Bounds<Pixels>>,
    pub strips: [Option<Bounds<Pixels>>; 3],
    pub popup: Option<Popup>,
    pub field_origin: Pixels,
    /// Each strip's shaped crumbs and the popup's shaped pieces, kept across frames so a redraw reuses their buffers.
    pub runs: [Vec<Rc<GlyphRun>>; 3],
    pub pieces: Vec<Piece>,
}

// Crumbs of both strips, a field and a screen of suggestion pieces fit with room to scroll back.
const SHAPES: usize = 160;

impl Paint {
    pub fn new() -> Self {
        Self {
            typeface: Typeface::omarchy_default(),
            advances: [None; 3],
            shapes: Shapes::new(SHAPES),
            area: None,
            frame: None,
            strips: [None; 3],
            popup: None,
            field_origin: px(0.),
            runs: Default::default(),
            pieces: Vec::new(),
        }
    }

    pub fn pixels(&self, size: Size) -> Pixels {
        pixels(&self.typeface, size)
    }

    // One advance of "0" per size, measured once: it turns a strip's room into the fit's character budget.
    pub fn advance(&mut self, size: Size, window: &Window) -> Pixels {
        let slot = size as usize;
        if let Some(advance) = self.advances[slot] {
            return advance;
        }
        let pixels = self.pixels(size);
        let text = window.text_system();
        let id = text.resolve_font(&self.typeface.font);
        let advance = text
            .advance(id, pixels, '0')
            .map_or(pixels * 0.6, |advance| advance.width);
        self.advances[slot] = Some(advance);
        advance
    }

    /// A known advance for a reader with no window, zero until the first paint measured it.
    pub fn measured(&self, size: Size) -> Pixels {
        self.advances[size as usize].unwrap_or(px(0.))
    }

    pub fn shape(&mut self, text: &str, size: Size, bold: bool, window: &Window) -> Rc<GlyphRun> {
        let Self {
            typeface, shapes, ..
        } = self;
        shapes.get(text, size, bold, || {
            line(typeface, text, size, bold, window)
        })
    }

    /// Refills slot's kept runs from the cache and places the strip on their shaped widths; shape runs only on a miss.
    pub fn lay_strip(
        &mut self,
        slot: usize,
        strip: &mut Strip,
        mut shape: impl FnMut(&Typeface, &str) -> GlyphRun,
    ) {
        let Self {
            typeface,
            shapes,
            runs,
            ..
        } = self;
        let runs = &mut runs[slot];
        runs.clear();
        for crumb in strip.crumbs() {
            runs.push(shapes.get(&crumb.text, Size::Caption, false, || {
                shape(typeface, &crumb.text)
            }));
        }
        strip.place(runs.iter().map(|run| f32::from(run.line().width)));
    }
}

fn pixels(typeface: &Typeface, size: Size) -> Pixels {
    match size {
        Size::Caption => typeface.caption,
        Size::Field => px(13.),
        Size::Body => typeface.body,
    }
}

/// Shapes one piece in the face at size, for a cache miss.
pub fn line(typeface: &Typeface, text: &str, size: Size, bold: bool, window: &Window) -> GlyphRun {
    let mut font = typeface.font.clone();
    if bold {
        font.weight = FontWeight::BOLD;
    }
    let run = TextRun {
        len: text.len(),
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(
        window
            .text_system()
            .layout_line(text, pixels(typeface, size), &[run], None),
    )
}

impl Default for Paint {
    fn default() -> Self {
        Self::new()
    }
}

/// Paints a shaped piece with its glyphs centred in a band height tall.
pub fn paint_run(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    height: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (height - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

/// Which strip an Element draws: the chrome's, or a dual pane's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Chrome,
    Pane(usize),
}

impl Place {
    pub fn slot(self) -> usize {
        match self {
            Self::Chrome => 0,
            Self::Pane(side) => 1 + side.min(1),
        }
    }
}

// A dual pane's strip starts its first crumb a row's padding in, the chrome's at its slot's edge.
pub const STRIP_PAD: f32 = 14.;

pub struct CrumbElement {
    view: Entity<PathView>,
    place: Place,
}

impl CrumbElement {
    pub fn new(view: Entity<PathView>, place: Place) -> Self {
        Self { view, place }
    }

    fn pad(&self) -> f32 {
        if self.place == Place::Chrome {
            0.
        } else {
            STRIP_PAD
        }
    }
}

impl IntoElement for CrumbElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for CrumbElement {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, Ink, bool);

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
    ) -> (Hitbox, Ink, bool) {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let slot = self.place.slot();
        let pad = self.pad();
        let paint = self.view.read(cx).paint.clone();
        let mut paint = paint.borrow_mut();
        let ink = Ink::of(Palette::inherited(&window.text_style(), theme));
        // Dual draws each path on its own pane, so the chrome keeps its slot and builds no crumbs.
        if self.place == Place::Chrome && self.view.read(cx).inputs.dual {
            paint.area = Some(bounds);
            paint.runs[slot].clear();
            return (
                window.insert_hitbox(bounds, HitboxBehavior::Normal),
                ink,
                false,
            );
        }
        let advance = paint.advance(Size::Caption, window);
        let budget = ((f32::from(bounds.size.width) - 2. * pad) / f32::from(advance))
            .floor()
            .max(0.) as usize;
        let focused = self.view.update(cx, |view, _| view.fit(slot, budget));
        paint.strips[slot] = Some(bounds);
        if self.place == Place::Chrome {
            paint.area = Some(bounds);
        }
        self.view.update(cx, |view, _| {
            paint.lay_strip(slot, &mut view.strips[slot], |typeface, text| {
                line(typeface, text, Size::Caption, false, window)
            });
        });
        (
            window.insert_hitbox(bounds, HitboxBehavior::Normal),
            ink,
            focused,
        )
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (hitbox, ink, focused): &mut (Hitbox, Ink, bool),
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        if self.place == Place::Chrome && view.inputs.dual {
            return;
        }
        let slot = self.place.slot();
        let strip = &view.strips[slot];
        let paint = view.paint.borrow();
        let pane = self.place != Place::Chrome;
        if pane {
            window.paint_quad(fill(
                bounds,
                if *focused {
                    ink.surface
                } else {
                    ink.background
                },
            ));
            let rule = Bounds::new(
                point(bounds.origin.x, bounds.bottom() - px(1.)),
                size(bounds.size.width, px(1.)),
            );
            window.paint_quad(fill(rule, ink.muted_frame));
        }
        // The ancestors rest muted in the chrome and in the foreground on a dual pane; the leaf and a lit parent draw in the foreground.
        let rest = if pane { ink.foreground } else { ink.muted };
        let style = match strip.hovered.and_then(|at| strip.crumbs().get(at)) {
            Some(crumb) if crumb.parent() => CursorStyle::PointingHand,
            _ => CursorStyle::IBeam,
        };
        window.set_cursor_style(style, hitbox);
        let left = bounds.origin.x + px(self.pad());
        let height = if pane {
            bounds.size.height - px(1.)
        } else {
            bounds.size.height
        };
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (index, (crumb, run)) in strip.crumbs().iter().zip(&paint.runs[slot]).enumerate() {
                let lit = crumb.last || (strip.hovered == Some(index) && !crumb.elided);
                let x = left + px(strip.extent(index).map_or(0., |(left, _)| left));
                paint_run(
                    run,
                    x,
                    bounds.origin.y,
                    height,
                    if lit { ink.foreground } else { rest },
                    window,
                );
            }
        });
    }
}
