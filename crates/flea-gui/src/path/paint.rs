//! What the path surfaces draw with: the inks, the face's advances, a bounded cache of shaped pieces and the crumb strip Element.
use super::{menu::Piece, scale::Scale, strip::Strip, suggestions::Popup, view::PathView};
use crate::list::{Palette, Typeface};
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
    pub error: Hsla,
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
            muted: palette.muted,
            muted_frame: palette.muted_frame,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error: palette.error,
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
    /// The suggestion rows the last frame laid and painted, 0 while no popup draws.
    pub drawn: usize,
    pub field_origin: Pixels,
    /// The face and size the field's line was shaped in; set_typeface leaves PathView.line alone, so prepaint reshapes on a change.
    pub(super) field_style: Option<(gpui::Font, Pixels)>,
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
            drawn: 0,
            field_origin: px(0.),
            field_style: None,
            runs: Default::default(),
            pieces: Vec::new(),
        }
    }

    /// The window's system font and text size; a change drops every measured advance and shaped crumb.
    pub fn set_typeface(&mut self, typeface: Typeface) -> bool {
        if self.typeface == typeface {
            return false;
        }
        self.typeface = typeface;
        self.advances = [None; 3];
        self.shapes = Shapes::new(SHAPES);
        self.runs = Default::default();
        true
    }

    pub fn pixels(&self, size: Size) -> Pixels {
        pixels(&self.typeface, size)
    }

    /// The text size's geometry for the field, the popup and its rows.
    pub fn scale(&self) -> Scale {
        Scale::at(&self.typeface)
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
        Size::Field => typeface.body_small,
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

/// The room between the path's last crumb and its Git label, in pixels.
pub const GIT_GAP: f32 = 9.;

/// A strip's first-crumb inset: none on the chrome, a dual pane's row padding (PanePath's rowPaddingX) on a pane.
pub fn strip_pad(pane: bool, scale: &Scale) -> f32 {
    if pane {
        scale.pad_x
    } else {
        0.
    }
}

pub struct CrumbElement {
    view: Entity<PathView>,
    place: Place,
}

impl CrumbElement {
    pub fn new(view: Entity<PathView>, place: Place) -> Self {
        Self { view, place }
    }

    fn pad(&self, scale: &Scale) -> f32 {
        strip_pad(self.place != Place::Chrome, scale)
    }
}

impl IntoElement for CrumbElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[path = "paint_element.rs"]
mod element;
