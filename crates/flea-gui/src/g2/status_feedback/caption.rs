//! A one-line caption drawn glyph by glyph with letter spacing, which GPUI's text style cannot express.
use gpui::{
    point, px, size, App, Bounds, ContentMask, Element, ElementId, Font, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, LineLayout, Pixels, Point, ShapedLine, SharedString,
    Style, TextRun, Window,
};
use std::{cell::Cell, rc::Rc};

/// The hero caption's letter spacing, `.statecap` at 1.2px on every hero board.
pub const TRACKING: f32 = 1.2;

/// The scripts the boards draw with no capitals and no tracking, `.statecap:is(:lang(ja), ...)`.
pub const CASELESS: [&str; 12] = [
    "ja", "zh", "ko", "ar", "fa", "he", "ur", "th", "hi", "bn", "ta", "si",
];

/// Whether a language code's primary subtag is one of the boards' caseless scripts.
pub fn caseless(language: &str) -> bool {
    CASELESS.contains(&language.split(['-', '_']).next().unwrap_or(""))
}

/// The caption tracking for a language: 1.2px, or none for a caseless script, which tracking would also cut apart.
pub fn tracking_for(language: &str) -> f32 {
    if caseless(language) {
        0.
    } else {
        TRACKING
    }
}

/// The boards leave caseless scripts unchanged, including Latin words embedded in their captions.
pub fn heading_for(text: &str, language: &str) -> String {
    if caseless(language) {
        text.to_owned()
    } else {
        flea::ui_logic_settings_pages::caps::heading(text, language)
    }
}

/// What a Tracked caption painted: its text, ink, tracking and box, for tests.
#[cfg(test)]
pub type Inked = (SharedString, Hsla, Pixels, Bounds<Pixels>);

#[cfg(test)]
thread_local! {
    /// Test hook: while armed, paint records each glyph's x from the caption's left edge.
    pub static PAINTED: std::cell::RefCell<Option<Vec<Pixels>>> = const { std::cell::RefCell::new(None) };
    /// Test hook: while armed, every Tracked caption records its text, ink, tracking and box as it paints.
    pub static INKED: std::cell::RefCell<Option<Vec<Inked>>> = const { std::cell::RefCell::new(None) };
}

/// One line of `text` with `tracking` after every glyph, as CSS letter-spacing lays it out, in a `line` box.
pub struct Tracked {
    pub text: SharedString,
    pub font: Font,
    pub size: Pixels,
    pub line: Pixels,
    pub tracking: Pixels,
    pub color: Hsla,
}

impl Tracked {
    /// A field or section caption at its live size, using the board's locale and 1.2px tracking at 12px.
    pub fn caption(
        text: &str,
        language: &str,
        font: Font,
        size: Pixels,
        line: Pixels,
        color: Hsla,
    ) -> Self {
        Self {
            text: heading_for(text, language).into(),
            font,
            size,
            line,
            tracking: px(tracking_for(language) * f32::from(size) / 12.),
            color,
        }
    }
}

/// Each glyph's x in the line once `tracking` follows every glyph before it, for tests.
#[cfg(test)]
pub fn origins(layout: &LineLayout, tracking: Pixels) -> Vec<Pixels> {
    let glyphs = layout.runs.iter().flat_map(|run| run.glyphs.iter());
    glyphs
        .enumerate()
        .map(|(i, glyph)| glyph.position.x + tracking * i as f32)
        .collect()
}

/// The tracked line's width: the shaped advance plus one tracking step per glyph, the last included.
pub fn width(layout: &LineLayout, tracking: Pixels) -> Pixels {
    let glyphs: usize = layout.runs.iter().map(|run| run.glyphs.len()).sum();
    layout.width + tracking * glyphs as f32
}

/// Measure a producer's own text with the same shaping and tracking as its painted line.
pub fn measure(
    text: SharedString,
    font: Font,
    size: Pixels,
    tracking: Pixels,
    window: &Window,
) -> f32 {
    let run = TextRun {
        len: text.len(),
        font,
        color: window.text_style().color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(text, size, &[run], None);
    f32::from(width(&shaped, tracking))
}

impl IntoElement for Tracked {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Tracked {
    type RequestLayoutState = ShapedLine;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    // Shaped once per frame here and reused by paint; GPUI's line cache carries the shape across frames.
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ShapedLine) {
        let run = TextRun {
            len: self.text.len(),
            font: self.font.clone(),
            color: self.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window
            .text_system()
            .shape_line(self.text.clone(), self.size, &[run], None);
        let style = Style {
            size: size(width(&shaped, self.tracking).into(), self.line.into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), shaped)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut ShapedLine,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        shaped: &mut ShapedLine,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        #[cfg(test)]
        INKED.with_borrow_mut(|inked| {
            if let Some(inked) = inked {
                inked.push((self.text.clone(), self.color, self.tracking, bounds));
            }
        });
        paint_line(
            shaped,
            bounds.origin,
            self.line,
            self.tracking,
            self.color,
            window,
        );
    }
}

/// Paints a shaped line's glyphs with `tracking` after each one, vertically centred in a `line` box at `origin`.
pub fn paint_line(
    shaped: &ShapedLine,
    origin: Point<Pixels>,
    line: Pixels,
    tracking: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let baseline = origin.y + (line - shaped.ascent - shaped.descent) / 2. + shaped.ascent;
    let mut i = 0;
    for run in shaped.runs.iter() {
        for glyph in run.glyphs.iter() {
            let x = glyph.position.x + tracking * i as f32;
            i += 1;
            #[cfg(test)]
            PAINTED.with(|painted| {
                if let Some(xs) = painted.borrow_mut().as_mut() {
                    xs.push(x);
                }
            });
            let at = point(origin.x + x, baseline + glyph.position.y);
            let _ = if glyph.is_emoji {
                window.paint_emoji(at, run.font_id, glyph.id, shaped.font_size)
            } else {
                window.paint_glyph(at, run.font_id, glyph.id, shaped.font_size, color)
            };
        }
    }
}

/// The caption box and its pane's clip, as the last layout placed them.
pub type Place = Rc<Cell<Option<(Bounds<Pixels>, ContentMask<Pixels>)>>>;

/// Where the hero's caption goes: lays out the widest phrase's box and records it with its clip, painting nothing itself.
pub struct Slot {
    pub place: Place,
    pub width: f32,
    pub line: Pixels,
}

impl IntoElement for Slot {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Slot {
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
            size: size(px(self.width).into(), self.line.into()),
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
        _: &mut App,
    ) {
        self.place.set(Some((bounds, window.content_mask())));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}
