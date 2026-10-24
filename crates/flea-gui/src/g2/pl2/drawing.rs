//! The cold form as one Element; text is shaped in prepaint and reused by paint.
use super::{dialog::Focus, view::NetworkView};
use crate::g2::op1::palette::CardPalette;
use gpui::{
    point, px, quad, relative, size, App, BorderStyle, Bounds, Element, ElementId, Entity,
    GlobalElementId, GlyphRun, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Style,
    TextRun, Window,
};

pub const WIDTH: f32 = 499.;
pub const HEIGHT: f32 = 610.;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    fn bounds(self) -> Bounds<Pixels> {
        Bounds::new(point(px(self.x), px(self.y)), size(px(self.w), px(self.h)))
    }
}
pub fn field(focus: Focus) -> Option<Rect> {
    let (x, y, w) = match focus {
        Focus::Label => (17., 126., 465.),
        Focus::Host => (17., 191., 226.5),
        Focus::Port => (255.5, 191., 226.5),
        Focus::Path => (17., 256., 465.),
        Focus::Domain => (17., 321., 465.),
        Focus::User => (17., 386., 465.),
        Focus::Password => (17., 451., 465.),
        _ => return None,
    };
    Some(Rect { x, y, w, h: 30. })
}

pub struct Text {
    pub field: Option<Focus>,
    pub rect: Rect,
    pub run: GlyphRun,
    pub ink: Hsla,
    pub clip: Option<Rect>,
}
pub struct BoxPaint {
    pub rect: Rect,
    pub fill: Hsla,
    pub border: Hsla,
    pub radius: f32,
    pub border_width: f32,
    pub clip: Option<Rect>,
}
#[derive(Default)]
pub struct Drawn {
    pub tracking: f32,
    pub revision: u64,
    pub text: Vec<Text>,
    pub boxes: Vec<BoxPaint>,
    pub hits: Vec<(Focus, Rect)>,
    pub origin: (f32, f32),
    pub card: Option<Rect>,
    pub body_height: f32,
}

impl Drawn {
    pub(super) fn label(
        &mut self,
        text: &str,
        rect: Rect,
        ink: Hsla,
        font_size: f32,
        bold: bool,
        window: &Window,
    ) {
        let mut font = window.text_style().font();
        if bold {
            font.weight = gpui::FontWeight::BOLD;
        }
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
            .layout_line(text, px(font_size), &[run], None);
        let line = if rect.h == 14. && self.tracking != 0. {
            let mut runs = line.runs.clone();
            for run in &mut runs {
                for glyph in &mut run.glyphs {
                    glyph.position.x +=
                        px(text[..glyph.index].chars().count() as f32 * self.tracking);
                }
            }
            std::sync::Arc::new(gpui::LineLayout {
                font_size: line.font_size,
                width: line.width
                    + px(text.chars().count().saturating_sub(1) as f32 * self.tracking),
                ascent: line.ascent,
                descent: line.descent,
                runs,
                len: line.len,
            })
        } else {
            line
        };
        self.text.push(Text {
            field: None,
            rect,
            run: GlyphRun::new(line),
            ink,
            clip: None,
        });
    }
    pub(super) fn caret(&mut self, focus: Focus, rect: Rect, view: &NetworkView) {
        if let Some(text) = self.text.last_mut() {
            text.field = Some(focus);
        }
        if view.model.focus != focus || view.model.busy() || view.model.committed {
            return;
        }
        let Some(text) = self.text.last_mut() else {
            return;
        };
        let selected = if focus == Focus::Password {
            super::secret_edit::display_index(
                &view.model.password,
                view.password_selection.start,
                view.model.reveal,
            )
                ..super::secret_edit::display_index(
                    &view.model.password,
                    view.password_selection.end,
                    view.model.reveal,
                )
        } else {
            view.input.selected.clone()
        };
        let cursor_at = if focus == Focus::Password {
            if view.password_reversed {
                selected.start
            } else {
                selected.end
            }
        } else {
            view.input.cursor()
        };
        let cursor = f32::from(text.run.line().x_for_index(cursor_at));
        let missing = (WIDTH - self.card.map_or(WIDTH, |card| card.w)).max(0.);
        let adjustment = if rect.w == 226.5 {
            missing / 2.
        } else {
            missing
        };
        let room = (rect.w - if focus == Focus::Password { 42. } else { 20. } - adjustment).max(0.);
        let scroll = (cursor - room).max(0.);
        text.clip = Some(Rect {
            x: rect.x + 1.,
            w: rect.w - 2.,
            ..rect
        });
        text.rect.x -= scroll;
        let mut add = |x: f32, w: f32, h: f32, y: f32, ink| {
            self.boxes.push(BoxPaint {
                rect: Rect { x, y, w, h },
                fill: ink,
                border: ink,
                radius: 0.,
                border_width: 1.,
                clip: None,
            });
        };
        if !selected.is_empty() {
            let from = f32::from(text.run.line().x_for_index(selected.start)) - scroll;
            let to = f32::from(text.run.line().x_for_index(selected.end)) - scroll;
            let from = from.max(0.);
            let to = to.min(room);
            if to > from {
                add(
                    rect.x + 10. + from,
                    to - from,
                    17.,
                    rect.y + 6.,
                    view.palette.accent.opacity(0.24),
                );
            }
        } else {
            add(
                rect.x + 10. + cursor - scroll,
                1.,
                17.,
                rect.y + 6.,
                view.palette.foreground,
            );
        }
    }
    pub(super) fn control(&mut self, focus: Focus, rect: Rect, selected: bool, view: &NetworkView) {
        let p = view.palette;
        let current = view.model.focus == focus;
        let fill = if field(focus).is_some() {
            p.background
        } else if view.pressed == Some(focus) {
            p.foreground.opacity(0.22)
        } else if view.hovered == Some(focus) {
            p.foreground.opacity(0.08)
        } else {
            p.background
        };
        let border = if current {
            p.accent_frame
        } else if selected {
            p.foreground
        } else {
            p.muted_frame
        };
        self.boxes.push(BoxPaint {
            rect,
            fill,
            border,
            radius: p.radius,
            border_width: 1.,
            clip: None,
        });
        self.hits.push((focus, rect));
    }
}

pub struct NetworkElement(pub Entity<NetworkView>);
impl IntoElement for NetworkElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for NetworkElement {
    type RequestLayoutState = ();
    type PrepaintState = CardPalette;
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
        (
            window.request_layout(
                Style {
                    size: size(relative(1.).into(), relative(1.).into()),
                    ..Style::default()
                },
                None,
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> CardPalette {
        let view = self.0.read(cx);
        let mut drawn = view.drawn.borrow_mut();
        if drawn.revision != view.revision || drawn.card.is_none() {
            *drawn = Drawn::build(
                view,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                window,
            );
        }
        view.palette
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        p: &mut CardPalette,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.0.read(cx);
        let drawn = view.drawn.borrow();
        let origin = bounds.origin + point(px(drawn.origin.0), px(drawn.origin.1));
        for b in &drawn.boxes {
            let mut rect = b.rect.bounds();
            rect.origin += origin;
            let paint = quad(
                rect,
                px(b.radius),
                b.fill,
                px(b.border_width),
                b.border,
                BorderStyle::Solid,
            );
            let clip = b.clip.map(|clip| {
                let mut bounds = clip.bounds();
                bounds.origin += origin;
                gpui::ContentMask { bounds }
            });
            window.with_content_mask(clip, |window| window.paint_quad(paint));
        }
        for text in &drawn.text {
            let line = text.run.line();
            let at = origin
                + point(
                    px(text.rect.x),
                    px(text.rect.y)
                        + (px(text.rect.h) - line.ascent - line.descent) / 2.
                        + line.ascent,
                );
            let clip = text.clip.unwrap_or(text.rect);
            let mask = Bounds::new(
                origin + point(px(clip.x), px(clip.y)),
                size(px(clip.w), px(clip.h)),
            );
            window.with_content_mask(Some(gpui::ContentMask { bounds: mask }), |window| {
                let _ = window.paint_glyph_run(at, &text.run, text.ink);
            });
        }
        super::marks::paint(view, &drawn, origin, p, window, cx);
        if view.model.opened
            && !view.model.busy()
            && (view.input.editing || view.model.focus == Focus::Password)
        {
            if let Some((_, rect)) = drawn
                .hits
                .iter()
                .find(|(focus, _)| *focus == view.model.focus)
            {
                let bounds = Bounds::new(
                    origin + point(px(rect.x), px(rect.y)),
                    size(px(rect.w), px(rect.h)),
                );
                window.handle_input(
                    &view.focus,
                    gpui::ElementInputHandler::new(bounds, self.0.clone()),
                    cx,
                );
            }
        }
    }
}
