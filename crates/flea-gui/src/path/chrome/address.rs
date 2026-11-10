//! The path's slot in the toolbar, and the copy mark that follows the path while the pointer is over the slot.
use super::{ChromeView, Sizes, COPY};
use crate::{list::Palette, path::view::PathView, window::startup::View};
use gpui::{
    div, point, prelude::*, px, size, AnyElement, App, AvailableSpace, Bounds, Context, Element,
    ElementId, Entity, GlobalElementId, InspectorElementId, LayoutId, Pixels, Position, Style,
    Window,
};

impl ChromeView {
    pub(super) fn address(
        &self,
        sizes: Sizes,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let present = self.inputs.view != View::Dual && !self.path.read(cx).editing();
        let mark = present.then(|| {
            let control = self.frame.borrow().controls[COPY];
            // Nothing draws at rest: the pointer over the slot, the keyboard or a held press shows the mark.
            let shown = (self.frame.borrow().address_hovered && self.inputs.live)
                || control.focused
                || control.pressed;
            let resting = if shown {
                palette.foreground
            } else {
                palette.foreground.opacity(0.)
            };
            let button = self
                .button(sizes, COPY, true, resting, palette.foreground, cx)
                .into_any_element();
            self.frame.borrow_mut().controls[COPY].visible = shown;
            Follow {
                path: self.path.clone(),
                button,
                sizes,
            }
        });
        div()
            .id("chrome-address")
            .h(px(sizes.inner()))
            .flex_1()
            .min_w_0()
            .relative()
            .hover_listener_mode(gpui::HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                let mut frame = view.frame.borrow_mut();
                if frame.address_hovered != *hovered {
                    frame.address_hovered = *hovered;
                    drop(frame);
                    cx.notify();
                }
            }))
            .child(self.path.clone())
            .children(mark)
    }
}

/// Places the copy mark one gap after the path's line, which the crumbs measured earlier in this same prepaint.
struct Follow {
    path: Entity<PathView>,
    button: AnyElement,
    sizes: Sizes,
}

impl IntoElement for Follow {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Follow {
    type RequestLayoutState = ();
    type PrepaintState = bool;

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
        let mut style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        style.inset.top = px(0.).into();
        style.inset.left = px(0.).into();
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
    ) -> bool {
        let Some(end) = self.path.read(cx).paint.borrow().chrome_end else {
            return false;
        };
        let origin = point(bounds.origin.x + px(end + self.sizes.gap), bounds.origin.y);
        let room = size(px(self.sizes.slot), px(self.sizes.band)).map(AvailableSpace::Definite);
        self.button.prepaint_as_root(origin, room, window, cx);
        true
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        laid: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *laid {
            self.button.paint(window, cx);
        }
    }
}
