//! A field's text menu card as a child element, for field mounts that build without their host's context.
use super::{Answer, TextMenu};
use gpui::{
    prelude::*, px, AnyElement, App, Bounds, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Position, Style, Window,
};

/// A field's menu card as a child that reads its host while it lays out, so a mount without a context can still draw it.
pub(crate) struct Slot<T: 'static> {
    view: Entity<T>,
    menu: fn(&T) -> Option<&TextMenu>,
    answer: Answer<T>,
}

impl<T: 'static> Slot<T> {
    pub fn new(view: Entity<T>, menu: fn(&T) -> Option<&TextMenu>, answer: Answer<T>) -> Self {
        Self { view, menu, answer }
    }
}

impl<T: 'static> IntoElement for Slot<T> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl<T: 'static> Element for Slot<T> {
    type RequestLayoutState = Option<AnyElement>;
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
    ) -> (LayoutId, Option<AnyElement>) {
        let radius = px(crate::compositor::corner(cx));
        let mut overlay = (self.menu)(self.view.read(cx))
            .and_then(|menu| menu.overlay(&self.view, radius, self.answer));
        let child = overlay
            .as_mut()
            .map(|overlay| overlay.request_layout(window, cx));
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, child, cx), overlay)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        overlay: &mut Option<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(overlay) = overlay {
            overlay.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        overlay: &mut Option<AnyElement>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(overlay) = overlay {
            overlay.paint(window, cx);
        }
    }
}
