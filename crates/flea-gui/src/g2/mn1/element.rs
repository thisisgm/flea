//! The menu card as one custom Element: the frame, each row's wash, mark, label and trailing parts, and the edge fades.
use super::glyphs;
use super::lines::{Ink, Lead, Line, Shaped};
use super::menu::{Card, Menu};
use crate::list::Palette;
use crate::palette::Theme;
use flea::ui_logic_mn1::fit::{self, CHEVRON, CLEAR, GAP, PAD, PAD_X, ROW, SEP, SLOT};
use gpui::{
    fill, linear_color_stop, linear_gradient, point, px, quad, size, App, BorderStyle, Bounds,
    ContentMask, Element, ElementId, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, Pixels, Position, Style, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};

// The cursor and a lit flyout parent take the foreground at 8%, with no edge.
const WASH: f32 = 0.08;
const RULE: f32 = 0.4;
const DIM: f32 = 0.55;
// The fade crosses the row padding and reaches round(37 x 0.7) into the cut row, inside the 1 pixel frame.
const FADE_H: i32 = PAD + CLEAR - 1;
const TAG: f32 = 14.;
const CHECK: f32 = 16.;

/// The roles a menu paints with, read from the pane's palette until the theme tokens land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuInk {
    pub surface: Hsla,
    pub frame: Hsla,
    pub rule: Hsla,
    pub wash: Hsla,
    pub label: Hsla,
    pub muted: Hsla,
    pub mark: Hsla,
    pub error: Hsla,
    pub tag: Hsla,
}

impl MenuInk {
    pub fn from_palette(palette: &Palette) -> Self {
        Self {
            surface: palette.background,
            frame: palette.muted_frame,
            rule: palette.muted_frame.opacity(RULE),
            wash: palette.foreground.opacity(WASH),
            label: palette.foreground,
            muted: palette.muted,
            mark: palette.metadata_frame,
            error: palette.error,
            tag: palette.accent,
        }
    }
}

pub struct MenuElement {
    menu: Rc<RefCell<Menu>>,
    // The compositor's corner radius, which the window module reads from Hyprland.
    radius: Pixels,
}

impl MenuElement {
    pub fn new(menu: Rc<RefCell<Menu>>, radius: Pixels) -> Self {
        Self { menu, radius }
    }
}

impl IntoElement for MenuElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn paint_text(shaped: &Shaped, x: Pixels, top: Pixels, color: Hsla, window: &mut Window) {
    let line = shaped.run.line();
    let baseline =
        window.pixel_snap(top + (px(ROW as f32) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), &shaped.run, color);
}

fn paint_mark(
    key: gpui::SharedString,
    data: &'static [u8],
    bounds: Bounds<Pixels>,
    color: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let _ = window.paint_svg(
        bounds,
        key,
        Some(data),
        TransformationMatrix::unit(),
        color,
        cx,
    );
}

fn slot(left: Pixels, top: Pixels, w: f32, h: f32) -> Bounds<Pixels> {
    let x = left + px(PAD_X + (SLOT - w) / 2.);
    Bounds::new(
        point(x, top + px((ROW as f32 - h) / 2.)),
        size(px(w), px(h)),
    )
}

fn paint_lead(
    lead: &Lead,
    mark: Hsla,
    ink: &MenuInk,
    left: Pixels,
    top: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    match lead {
        Lead::None | Lead::Check(false) => {}
        Lead::Cut(name) => {
            let (key, data) = glyphs::mark(name);
            paint_mark(key, data, slot(left, top, SLOT, SLOT), mark, window, cx);
        }
        Lead::Check(true) => {
            let (key, data) = glyphs::mark("checkmark");
            paint_mark(key, data, slot(left, top, CHECK, CHECK), mark, window, cx);
        }
        Lead::Brand(brand) => {
            let (key, data, ratio) = glyphs::brand(*brand);
            paint_mark(
                key,
                data,
                slot(left, top, (SLOT * ratio).round(), SLOT),
                mark,
                window,
                cx,
            );
        }
        Lead::Tag(_) => window.paint_quad(fill(slot(left, top, TAG, TAG), ink.tag)),
        // An application's own icon belongs to the icon theme unit; until then the window mark stands in.
        Lead::Icon(_) => {
            let (key, data) = glyphs::mark("app-window");
            paint_mark(key, data, slot(left, top, SLOT, SLOT), mark, window, cx);
        }
    }
}

fn paint_row(
    line: &Line,
    lit: bool,
    ink: &MenuInk,
    at: gpui::Point<Pixels>,
    width: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let (left, top, right) = (at.x, at.y, at.x + width);
    if line.separator {
        let y = top + px(((SEP - 1) / 2) as f32);
        window.paint_quad(fill(
            Bounds::new(point(left, y), size(width, px(1.))),
            ink.rule,
        ));
        return;
    }
    if lit {
        window.paint_quad(fill(
            Bounds::new(point(left, top), size(width, px(ROW as f32))),
            ink.wash,
        ));
    }
    let dim = if line.disabled { DIM } else { 1. };
    let frame = px(1. - line.frame_inset());
    let (left, right) = (left - frame, right + frame);
    let red = matches!(line.ink, Ink::Errored | Ink::Danger);
    let label_ink = if red { ink.error } else { ink.label }.opacity(dim);
    let mark_ink = if red { ink.error } else { ink.mark }.opacity(dim);
    paint_lead(&line.lead, mark_ink, ink, left, top, window, cx);
    // Trailing parts end on one edge: the chevron's column, which a state word keeps, then the key, then the word.
    let mut edge = right - px(PAD_X);
    if line.has_trailing_column() {
        if line.chevron {
            let (key, data) = glyphs::mark("chevron-right");
            let bounds = Bounds::new(
                point(edge - px(CHEVRON), top + px((ROW as f32 - CHEVRON) / 2.)),
                size(px(CHEVRON), px(CHEVRON)),
            );
            paint_mark(key, data, bounds, ink.frame.opacity(dim), window, cx);
        }
        edge -= px(CHEVRON + GAP);
    }
    for (part, width) in line
        .key
        .iter()
        .map(|key| (key, key.width()))
        .chain(line.word.iter().map(|word| (word, line.word_width())))
    {
        let x = edge - px(width);
        if let Some(caption) = &line.wrapped {
            caption.paint(
                point(x, top),
                if line.disabled { ink.label } else { ink.muted },
                window,
            );
        } else {
            paint_text(part, x, top, ink.muted.opacity(dim), window);
        }
        edge = x - px(GAP);
    }
    let Some(label) = &line.label else { return };
    let start = left + px(PAD_X + SLOT + GAP);
    let column = Bounds::new(
        point(start, top),
        size(px(line.label_room(width.into())), px(ROW as f32)),
    );
    window.with_content_mask(Some(ContentMask { bounds: column }), |window| {
        paint_text(label, start, top, label_ink, window)
    });
}

fn paint_card(
    card: &Card,
    lit: Option<usize>,
    radius: Pixels,
    ink: &MenuInk,
    origin: gpui::Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let f = card.frame;
    let frame = Bounds::new(origin + point(px(f.x), px(f.y)), size(px(f.w), px(f.h)));
    window.paint_quad(quad(
        frame,
        radius,
        ink.surface,
        px(1.),
        ink.frame,
        BorderStyle::Solid,
    ));
    let inner = Bounds::new(
        frame.origin + point(px(1.), px(1.)),
        size(frame.size.width - px(2.), frame.size.height - px(2.)),
    );
    let view = Bounds::new(
        frame.origin + point(px(1.), px(PAD as f32)),
        size(inner.size.width, px(card.view_h as f32)),
    );
    window.with_content_mask(Some(ContentMask { bounds: view }), |window| {
        let mut y = 0;
        for (index, line) in card.built.lines.iter().enumerate() {
            let h = fit::height(&card.entries[index]);
            let top = y - card.content_y;
            y += h;
            if top + h <= 0 || top >= card.view_h {
                continue;
            }
            let row_top = view.origin.y + px(top as f32);
            paint_row(
                line,
                card.cursor == Some(index) || lit == Some(index),
                ink,
                point(inner.origin.x, row_top),
                inner.size.width,
                window,
                cx,
            );
        }
    });
    let (top, bottom) = fit::fades(&card.entries, card.content_y, card.view_h);
    let fade = |at: Pixels, angle: f32| {
        let bounds = Bounds::new(
            point(inner.origin.x, at),
            size(inner.size.width, px(FADE_H as f32)),
        );
        fill(
            bounds,
            linear_gradient(
                angle,
                linear_color_stop(ink.surface, 0.),
                linear_color_stop(ink.surface.opacity(0.), 1.),
            ),
        )
    };
    if top {
        window.paint_quad(fade(inner.origin.y, 180.));
    }
    if bottom {
        window.paint_quad(fade(
            inner.origin.y + inner.size.height - px(FADE_H as f32),
            0.,
        ));
    }
}

impl Element for MenuElement {
    type RequestLayoutState = ();
    type PrepaintState = MenuInk;

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
            position: Position::Absolute,
            size: size(gpui::relative(1.).into(), gpui::relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> MenuInk {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        // A failed provider and a destructive row read in the theme's error role.
        MenuInk {
            error: theme.error,
            ..MenuInk::from_palette(&Palette::inherited(&window.text_style(), theme))
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        ink: &mut MenuInk,
        window: &mut Window,
        cx: &mut App,
    ) {
        let menu = self.menu.borrow();
        let Some(card) = &menu.card else { return };
        let ink = *ink;
        window.paint_layer(bounds, |window| {
            let lit = menu.flyout.as_ref().and_then(|f| f.parent);
            paint_card(card, lit, self.radius, &ink, bounds.origin, window, cx);
            if let Some(flyout) = &menu.flyout {
                paint_card(
                    &flyout.card,
                    None,
                    self.radius,
                    &ink,
                    bounds.origin,
                    window,
                    cx,
                );
            }
        });
    }
}
