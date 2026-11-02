//! The menu card as one custom Element: the frame, each row's wash, mark, label and trailing parts, and the edge fades.
pub(crate) mod icons;

use super::glyphs;
use super::lines::{paint_text, Ink, Lead, Line};
use super::menu::{Card, Menu};
use crate::list::Palette;
use crate::palette::Theme;
use flea::ui_logic_mn1::fit::Metrics;
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
const TAG: f32 = 14.;
const CHECK: f32 = 16.;

#[cfg(test)]
#[path = "tag_tests.rs"]
mod tag_tests;

#[cfg(test)]
#[path = "fade_tests.rs"]
mod fade_tests;

#[cfg(test)]
thread_local! {
    // Test-only receipt of each card frame a MenuElement painted in a real draw.
    pub(crate) static PAINTED: RefCell<Vec<Bounds<Pixels>>> = const { RefCell::new(Vec::new()) };
    pub(crate) static MARKED: RefCell<Vec<Bounds<Pixels>>> = const { RefCell::new(Vec::new()) };
}

mod ink;
pub use ink::MenuInk;

pub struct MenuElement {
    menu: Rc<RefCell<Menu>>,
    // The compositor's corner radius, which the window module reads from Hyprland.
    radius: Pixels,
    faded: bool,
}

impl MenuElement {
    pub fn new(menu: Rc<RefCell<Menu>>, radius: Pixels) -> Self {
        Self {
            menu,
            radius,
            faded: false,
        }
    }
    pub fn fading(menu: Rc<RefCell<Menu>>, radius: Pixels) -> Self {
        Self {
            menu,
            radius,
            faded: true,
        }
    }
}

impl IntoElement for MenuElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn paint_mark(
    key: gpui::SharedString,
    data: &'static [u8],
    bounds: Bounds<Pixels>,
    color: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(test)]
    MARKED.with(|marks| marks.borrow_mut().push(bounds));
    let _ = window.paint_svg(
        bounds,
        key,
        Some(data),
        TransformationMatrix::unit(),
        color,
        cx,
    );
}

fn slot(at: gpui::Point<Pixels>, w: f32, h: f32, metrics: Metrics) -> Bounds<Pixels> {
    let x = at.x + px(metrics.pad_x + (metrics.slot - w) / 2.);
    Bounds::new(
        point(x, at.y + px((metrics.row as f32 - h) / 2.)),
        size(px(w), px(h)),
    )
}

fn paint_lead(
    lead: &Lead,
    mark: Hsla,
    ink: &MenuInk,
    at: gpui::Point<Pixels>,
    metrics: Metrics,
    window: &mut Window,
    cx: &mut App,
) {
    match lead {
        Lead::None | Lead::Check(false) => {}
        Lead::Cut(name) => {
            let (key, data) = glyphs::mark(name);
            paint_mark(
                key,
                data,
                slot(at, metrics.slot, metrics.slot, metrics),
                mark,
                window,
                cx,
            );
        }
        Lead::Check(true) => {
            let (key, data) = glyphs::mark("checkmark");
            paint_mark(
                key,
                data,
                slot(at, metrics.of(CHECK), metrics.of(CHECK), metrics),
                mark,
                window,
                cx,
            );
        }
        Lead::Brand(brand) => {
            let (key, data, ratio) = glyphs::brand(*brand);
            paint_mark(
                key,
                data,
                slot(at, (metrics.slot * ratio).round(), metrics.slot, metrics),
                mark,
                window,
                cx,
            );
        }
        Lead::Tag(tag) => {
            let color = tag
                .checked_sub(1)
                .and_then(|at| ink.tags.get(usize::from(at)))
                .copied()
                .unwrap_or(ink.label);
            window.paint_quad(fill(
                slot(at, metrics.of(TAG), metrics.of(TAG), metrics),
                color,
            ));
        }
        Lead::Icon(name) => {
            let side = metrics.of(16.);
            let scale = window.scale_factor().ceil().max(1.) as u32;
            if let Some(image) = icons::image(name, side as u32, scale, window, cx) {
                let bounds = slot(at, side, side, metrics);
                let fit = gpui::ObjectFit::Contain.get_bounds(bounds, image.size(0));
                let _ = window.paint_image(bounds, fit, gpui::Corners::default(), image, 0, false);
                return;
            }
            let (key, data) = glyphs::mark("app-window");
            paint_mark(
                key,
                data,
                slot(at, metrics.slot, metrics.slot, metrics),
                mark,
                window,
                cx,
            );
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
    let metrics = line.metrics;
    let (left, top, right) = (at.x, at.y, at.x + width);
    if line.separator {
        let y = top + px(((metrics.sep - 1) / 2) as f32);
        window.paint_quad(fill(
            Bounds::new(point(left, y), size(width, px(1.))),
            ink.rule,
        ));
        return;
    }
    if lit {
        window.paint_quad(fill(
            Bounds::new(point(left, top), size(width, px(metrics.row as f32))),
            ink.wash,
        ));
    }
    let dim = if line.disabled { DIM } else { 1. };
    let frame = px(1. - line.frame_inset());
    let (left, right) = (left - frame, right + frame);
    let red = matches!(line.ink, Ink::Errored | Ink::Danger);
    let label_ink = if red { ink.error } else { ink.label }.opacity(dim);
    let mark_ink = if red { ink.error } else { ink.mark }.opacity(dim);
    paint_lead(
        &line.lead,
        mark_ink,
        ink,
        point(left, top),
        metrics,
        window,
        cx,
    );
    #[cfg(test)]
    crate::presentation::frame_gate::mark(&line.id, mark_ink, cx);
    // Trailing parts end on one edge: the chevron's column, which a state word keeps, then the key, then the word.
    let mut edge = right - px(metrics.pad_x);
    if line.has_trailing_column() {
        if line.chevron {
            let (key, data) = if window.layout_mirrored() {
                (
                    gpui::SharedString::new_static(glyphs::CHEVRON_LEFT.0),
                    glyphs::CHEVRON_LEFT.1,
                )
            } else {
                glyphs::mark("chevron-right")
            };
            let bounds = Bounds::new(
                point(
                    edge - px(metrics.chevron),
                    top + px((metrics.row as f32 - metrics.chevron) / 2.),
                ),
                size(px(metrics.chevron), px(metrics.chevron)),
            );
            paint_mark(key, data, bounds, ink.frame.opacity(dim), window, cx);
        }
        edge -= px(metrics.chevron + metrics.gap);
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
            paint_text(
                part,
                point(x, top),
                ink.muted.opacity(dim),
                metrics.row,
                window,
            );
        }
        edge = x - px(metrics.gap);
    }
    if let Some(state) = line.tag_state {
        super::tag_checks::paint(state, edge, top, label_ink, metrics, window, cx);
    }
    let Some(label) = line.painted_label() else {
        return;
    };
    let start = left + px(metrics.pad_x + metrics.slot + metrics.gap);
    let column = Bounds::new(
        point(start, top),
        size(px(line.label_room(width.into())), px(metrics.row as f32)),
    );
    window.with_content_mask(Some(ContentMask { bounds: column }), |window| {
        paint_text(label, point(start, top), label_ink, metrics.row, window)
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
    let metrics = card.built.metrics;
    let f = card.frame;
    let frame = Bounds::new(origin + point(px(f.x), px(f.y)), size(px(f.w), px(f.h)));
    #[cfg(test)]
    PAINTED.with(|painted| painted.borrow_mut().push(frame));
    let inner = Bounds::new(
        frame.origin + point(px(1.), px(1.)),
        size(frame.size.width - px(2.), frame.size.height - px(2.)),
    );
    let view = Bounds::new(
        frame.origin + point(px(1.), px(metrics.pad as f32)),
        size(inner.size.width, px(card.view_h as f32)),
    );
    window.paint_layer(frame, |window| {
        window.paint_quad(quad(
            frame,
            radius,
            ink.surface,
            px(1.),
            ink.frame,
            BorderStyle::Solid,
        ));
        window.with_content_mask(Some(ContentMask { bounds: view }), |window| {
            let mut y = 0;
            for (index, line) in card.built.lines.iter().enumerate() {
                let h = metrics.height(&card.entries[index]);
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
    });
    let (top, bottom) = metrics.fades(&card.entries, card.content_y, card.view_h);
    // Fades cross the padding and the cursor clearance, inside the one-pixel frame.
    let fade_h = metrics.pad + metrics.clear - 1;
    let fade = |at: Pixels, angle: f32| {
        let bounds = Bounds::new(
            point(inner.origin.x, at),
            size(inner.size.width, px(fade_h as f32)),
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
    // Equal-order quads precede glyph sprites in GPUI; fades need their own later order.
    window.paint_layer(inner, |window| {
        if top {
            window.paint_quad(fade(inner.origin.y, 180.));
        }
        if bottom {
            window.paint_quad(fade(
                inner.origin.y + inner.size.height - px(fade_h as f32),
                0.,
            ));
        }
    });
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
            tags: theme.tag_menu_inks,
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
        let card = if self.faded {
            menu.faded.as_ref().map(|faded| &faded.flyout.card)
        } else {
            menu.card.as_ref()
        };
        let Some(card) = card else { return };
        let ink = *ink;
        let lit = (!self.faded)
            .then(|| menu.flyout.as_ref().and_then(|f| f.parent))
            .flatten();
        paint_card(card, lit, self.radius, &ink, bounds.origin, window, cx);
        if let Some(flyout) = menu.flyout.as_ref().filter(|_| !self.faded) {
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
    }
}
