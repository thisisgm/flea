//! The 3D preview's text parts the host mounts: the bar's plate words, the facts table and the G-code layer strip.
use super::live::Live;
use crate::list::Palette as Roles;
use gpui::{
    div, fill, point, prelude::*, px, relative, size, App, Bounds, DispatchPhase, Div, Element,
    ElementId, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, LayoutId,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Style, Window,
};
use std::{cell::RefCell, rc::Rc};

/// Board: caption words 12 px, 14 px apart, the chosen plate in foreground and the others muted, no frames.
pub const CAPTION: f32 = 12.;
pub const WORD_GAP: f32 = 14.;
/// Board facts: label column as wide as "Layer height" at 7.2 px a cell plus 15, rows 21 px, groups 9 px apart.
pub const LABEL_COLUMN: f32 = 101.;
pub const FACT_ROW: f32 = 21.;
pub const GROUP_GAP: f32 = 9.;
/// Board strip: 27 px tall, 9 px padding and gap, a 2 px track and a 6 px square handle.
pub const STRIP: f32 = 27.;
pub const STRIP_PAD: f32 = 9.;
pub const TRACK: f32 = 2.;
pub const HANDLE: f32 = 6.;
const HAIR: f32 = 0.12;

pub fn plate_words(words: &[String], chosen: usize, roles: &Roles) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(WORD_GAP))
        .text_size(px(CAPTION))
        .whitespace_nowrap()
        .children(words.iter().enumerate().map(|(i, word)| {
            div()
                .text_color(if i == chosen {
                    roles.foreground
                } else {
                    roles.metadata
                })
                .child(SharedString::from(word.clone()))
        }))
}

pub fn facts_table(groups: &[Vec<(String, String)>], roles: &Roles) -> Div {
    div()
        .flex()
        .flex_col()
        .children(groups.iter().enumerate().map(|(g, rows)| {
            div()
                .flex()
                .flex_col()
                .mt(px(if g == 0 { 0. } else { GROUP_GAP }))
                .text_size(px(CAPTION))
                .whitespace_nowrap()
                .children(rows.iter().map(|(label, value)| {
                    div()
                        .flex()
                        .items_center()
                        .h(px(FACT_ROW))
                        .child(
                            div()
                                .w(px(LABEL_COLUMN))
                                .flex_none()
                                .text_color(roles.metadata)
                                .child(SharedString::from(label.clone())),
                        )
                        .child(
                            div()
                                .text_color(roles.foreground)
                                .child(SharedString::from(value.clone())),
                        )
                }))
        }))
}

/// The media strip across the card's foot: the hairline track, its played run and handle, and the tabular layer label.
pub fn layer_strip(live: Rc<RefCell<Live>>, label: String, roles: &Roles, ground: Hsla) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(STRIP))
        .gap(px(STRIP_PAD))
        .px(px(STRIP_PAD))
        .bg(ground)
        .border_t_1()
        .border_color(roles.foreground.opacity(HAIR))
        .text_size(px(CAPTION))
        .child(div().flex_1().h_full().child(Track {
            live,
            roles: *roles,
        }))
        .child(
            div()
                .flex_none()
                .text_color(roles.metadata)
                .child(SharedString::from(label)),
        )
}

struct Track {
    live: Rc<RefCell<Live>>,
    roles: Roles,
}

impl IntoElement for Track {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Track {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;
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
        _: &mut App,
    ) -> Hitbox {
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        _: &mut App,
    ) {
        let share = self
            .live
            .borrow()
            .viewer
            .as_ref()
            .and_then(|v| v.layer_share())
            .unwrap_or(0.);
        let width = f32::from(bounds.size.width);
        let line = bounds.origin.y + (bounds.size.height - px(TRACK)) / 2.;
        let at = width * share;
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x, line),
                size(bounds.size.width, px(TRACK)),
            ),
            self.roles.metadata_frame,
        ));
        window.paint_quad(fill(
            Bounds::new(point(bounds.origin.x, line), size(px(at), px(TRACK))),
            self.roles.accent_frame,
        ));
        let handle = point(
            bounds.origin.x + px(at - HANDLE / 2.),
            line - px((HANDLE - TRACK) / 2.),
        );
        window.paint_quad(fill(
            Bounds::new(handle, size(px(HANDLE), px(HANDLE))),
            self.roles.accent_frame,
        ));
        let seek = move |live: &Rc<RefCell<Live>>, x: Pixels, window: &mut Window| {
            let share = f32::from(x - bounds.origin.x) / width.max(1.);
            if live
                .borrow_mut()
                .viewer
                .as_mut()
                .is_some_and(|v| v.seek_layer(share))
            {
                window.refresh();
            }
        };
        let (live, hit) = (self.live.clone(), hitbox.clone());
        window.on_mouse_event(move |e: &MouseDownEvent, phase, window, _| {
            if phase == DispatchPhase::Bubble && hit.is_hovered(window) {
                live.borrow_mut().seeking = true;
                seek(&live, e.position.x, window);
            }
        });
        let live = self.live.clone();
        window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, _| {
            if phase == DispatchPhase::Bubble && live.borrow().seeking {
                seek(&live, e.position.x, window);
            }
        });
        let live = self.live.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, _, _, _| live.borrow_mut().seeking = false);
    }
}
