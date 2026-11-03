//! The media strip: one Element paints play, seek track, clock, mute and the volume track, and routes their input.
use super::glyphs::Glyph;
use super::ink::{body_small, size, MediaInk};
use crate::list::Typeface;
use flea::ui_logic_pv3::{
    duration,
    strip::{self as geometry, Layout, Metrics, Span},
};
use gpui::{
    fill, point, px, relative, App, Bounds, CursorStyle, Element, ElementId, Font, FontFeatures,
    GlobalElementId, GlyphRun, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement,
    LayoutId, Pixels, Style, TextRun, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};

mod pointer;

/// What the strip shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripView {
    pub playing: bool,
    pub muted: bool,
    /// Framed in the preview column, unframed under Quick Look with a volume track.
    pub framed: bool,
    pub position: i64,
    pub duration: i64,
    /// The seek fraction a drag in flight draws instead of the player's own.
    pub scrub: Option<f32>,
    pub level: u8,
}

/// The parts a press can land on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Play,
    Seek,
    Mute,
    Volume,
}

/// What the strip asks its player for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StripInput {
    Toggle,
    Mute,
    ScrubPress(f32),
    ScrubMove(f32),
    ScrubRelease,
    Wheel {
        up: bool,
    },
    LevelPress(u8),
    LevelMove(u8),
    LevelRelease,
    /// Any pointer activity over the strip, which re-arms a host's hide timer.
    Touched,
}

/// Paint state kept across frames: the shaped clock and the press in flight.
#[derive(Default)]
pub struct StripCache {
    clock: Option<((i64, i64), GlyphRun)>,
    pressed: Option<Part>,
    /// The play mark's box as the last paint drew it, which the column readers take as the play button.
    pub play: Option<Bounds<Pixels>>,
    /// The seek track and the mute mark as the last paint drew them, which the Quick Look readers take as their centres.
    pub seek: Option<Bounds<Pixels>>,
    pub mute: Option<Bounds<Pixels>>,
}

impl StripCache {
    pub(crate) fn held(&self) -> bool {
        matches!(self.pressed, Some(Part::Seek | Part::Volume))
    }
}

pub type OnInput = Rc<dyn Fn(StripInput, &mut Window, &mut App)>;

pub struct StripElement {
    pub view: StripView,
    pub ink: MediaInk,
    pub typeface: Typeface,
    pub cache: Rc<RefCell<StripCache>>,
    pub on_input: OnInput,
}

/// The strip's parts in window coordinates, and the hitbox the seek cursor follows.
pub struct Placed {
    layout: Layout,
    content: Bounds<Pixels>,
    seek: Hitbox,
}

pub fn metrics(typeface: &Typeface) -> Metrics {
    let body_small = body_small(typeface);
    Metrics {
        gap: size::GAP,
        slot: size::SLOT.max(body_small),
        body_small,
    }
}

/// The part under a strip-relative x, if any.
pub fn part_at(layout: &Layout, x: f32) -> Option<Part> {
    let parts = [
        (layout.play, Part::Play),
        (layout.seek, Part::Seek),
        (layout.mute, Part::Mute),
    ];
    let found = parts
        .into_iter()
        .find(|(span, _)| span.contains(x))
        .map(|(_, part)| part);
    found.or_else(|| {
        layout
            .volume
            .filter(|span| span.contains(x))
            .map(|_| Part::Volume)
    })
}

fn span_bounds(span: Span, content: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(content.origin.x + px(span.x), content.origin.y),
        gpui::size(px(span.w), content.size.height),
    )
}

impl IntoElement for StripElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for StripElement {
    type RequestLayoutState = ();
    type PrepaintState = Placed;

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
            size: gpui::size(relative(1.).into(), px(size::STRIP).into()),
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
    ) -> Placed {
        // The clock reads whole seconds, so it is shaped again only when one ticks over.
        let key = (
            self.view.position.max(0) / 1000,
            self.view.duration.max(0) / 1000,
        );
        let mut cache = self.cache.borrow_mut();
        if cache.clock.as_ref().is_none_or(|(shown, _)| *shown != key) {
            let text = format!(
                "{} / {}",
                duration(self.view.position),
                duration(self.view.duration)
            );
            // The boards set the clock in tabular figures, so its width holds while it counts.
            let font = Font {
                features: FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])),
                ..self.typeface.font.clone()
            };
            let run = TextRun {
                len: text.len(),
                font,
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window
                .text_system()
                .layout_line(&text, self.typeface.caption, &[run], None);
            cache.clock = Some((key, GlyphRun::new(line)));
        }
        let clock_width = cache
            .clock
            .as_ref()
            .map_or(0., |(_, run)| f32::from(run.line().width));
        let hair = px(size::HAIRLINE);
        // The framed strip's border takes a hairline from every side; the unframed one only its top divider.
        let content = if self.view.framed {
            Bounds::new(
                point(bounds.origin.x, bounds.origin.y + hair),
                gpui::size(bounds.size.width, bounds.size.height - hair * 2.),
            )
        } else {
            Bounds::new(
                point(bounds.origin.x, bounds.origin.y + hair),
                gpui::size(bounds.size.width, bounds.size.height - hair),
            )
        };
        let layout = geometry::layout(
            f32::from(bounds.size.width),
            self.view.framed,
            clock_width,
            metrics(&self.typeface),
        );
        let seek = window.insert_hitbox(span_bounds(layout.seek, content), HitboxBehavior::Normal);
        Placed {
            layout,
            content,
            seek,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        placed: &mut Placed,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (ink, view, caption) = (self.ink, self.view, f32::from(self.typeface.caption));
        let Placed {
            layout, content, ..
        } = *placed;
        {
            let mut cache = self.cache.borrow_mut();
            cache.play = Some(span_bounds(layout.play, content));
            cache.seek = Some(span_bounds(layout.seek, content));
            cache.mute = Some(span_bounds(layout.mute, content));
        }
        paint_ground(bounds, view.framed, &ink, window);
        let glyph = |span: Span, glyph: Glyph, window: &mut Window, cx: &mut App| {
            let side = px(body_small(&self.typeface));
            let x = content.origin.x + px(span.x) + (px(span.w) - side) / 2.;
            let y = content.origin.y + (content.size.height - side) / 2.;
            let (key, data) = glyph.svg();
            let mark = Bounds::new(
                point(window.pixel_snap(x), window.pixel_snap(y)),
                gpui::size(side, side),
            );
            let _ = window.paint_svg(
                mark,
                key,
                Some(data),
                TransformationMatrix::unit(),
                ink.foreground,
                cx,
            );
        };
        glyph(
            layout.play,
            if view.playing {
                Glyph::Pause
            } else {
                Glyph::Play
            },
            window,
            cx,
        );
        glyph(
            layout.mute,
            if view.muted {
                Glyph::VolumeX
            } else {
                Glyph::Volume
            },
            window,
            cx,
        );
        let played = view
            .scrub
            .unwrap_or(geometry::fraction(view.position, view.duration));
        paint_track(
            span_bounds(layout.seek, content),
            played,
            caption,
            &ink,
            window,
        );
        if let Some(volume) = layout.volume {
            paint_track(
                span_bounds(volume, content),
                f32::from(view.level) / 100.,
                caption,
                &ink,
                window,
            );
        }
        if let Some((_, run)) = self.cache.borrow().clock.as_ref() {
            let line = run.line();
            let top = content.origin.y + (content.size.height - line.ascent - line.descent) / 2.;
            let origin = point(
                window.pixel_snap(content.origin.x + px(layout.clock.x)),
                window.pixel_snap(top + line.ascent),
            );
            let _ = window.paint_glyph_run(origin, run, ink.muted);
        }
        window.set_cursor_style(CursorStyle::PointingHand, &placed.seek);
        self.listen(bounds, layout, content, window);
    }
}

fn paint_ground(bounds: Bounds<Pixels>, framed: bool, ink: &MediaInk, window: &mut Window) {
    window.paint_quad(fill(bounds, ink.surface));
    let hair = px(size::HAIRLINE);
    let edge = |x, y, w, h| Bounds::new(point(x, y), gpui::size(w, h));
    let (o, s) = (bounds.origin, bounds.size);
    if framed {
        for side in [
            edge(o.x, o.y, s.width, hair),
            edge(o.x, o.y + s.height - hair, s.width, hair),
            edge(o.x, o.y, hair, s.height),
            edge(o.x + s.width - hair, o.y, hair, s.height),
        ] {
            window.paint_quad(fill(side, ink.muted_frame));
        }
    } else {
        window.paint_quad(fill(edge(o.x, o.y, s.width, hair), ink.hair));
    }
}

/// MediaTrack: a 2 px track in the frame ink, the played part in the accent frame, a square handle on its end.
fn paint_track(
    span: Bounds<Pixels>,
    fraction: f32,
    caption: f32,
    ink: &MediaInk,
    window: &mut Window,
) {
    let thick = px(size::TRACK);
    let top = window.pixel_snap(span.origin.y + (span.size.height - thick) / 2.);
    let width = f32::from(span.size.width);
    let track = Bounds::new(
        point(span.origin.x, top),
        gpui::size(span.size.width, thick),
    );
    window.paint_quad(fill(track, ink.muted_frame));
    let played = px((width * fraction).round());
    window.paint_quad(fill(
        Bounds::new(track.origin, gpui::size(played, thick)),
        ink.accent_frame,
    ));
    let handle = geometry::handle(caption);
    let x = span.origin.x + px(geometry::handle_x(width, fraction, handle));
    let mark = Bounds::new(
        point(x, top - px((handle - size::TRACK) / 2.)),
        gpui::size(px(handle), px(handle)),
    );
    window.paint_quad(fill(mark, ink.accent_frame));
}
