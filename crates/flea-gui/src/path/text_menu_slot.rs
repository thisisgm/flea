//! A field's text menu card as a child element, for field mounts that build without their host's context.
use super::{Answer, TextMenu};
use crate::leaf::{Leaf, Painted};
use gpui::{
    prelude::*, px, AnyElement, App, Bounds, Entity, LayoutId, Pixels, Position, Style, Window,
};

/// A field's menu card as a child that reads its host while it lays out, so a mount without a context can still draw it.
pub(crate) struct Slot<T: 'static> {
    view: Entity<T>,
    menu: fn(&T) -> Option<&TextMenu>,
    answer: Answer<T>,
    overlay: Option<AnyElement>,
}

impl<T: 'static> Slot<T> {
    pub fn new(view: Entity<T>, menu: fn(&T) -> Option<&TextMenu>, answer: Answer<T>) -> Self {
        Self {
            view,
            menu,
            answer,
            overlay: None,
        }
    }
}

impl<T: 'static> IntoElement for Slot<T> {
    type Element = Painted<Self>;
    fn into_element(self) -> Self::Element {
        Painted(self)
    }
}

impl<T: 'static> Leaf for Slot<T> {
    type Prepaint = ();

    fn layout(&mut self, window: &mut Window, cx: &mut App) -> LayoutId {
        let radius = px(crate::compositor::corner(cx));
        self.overlay = (self.menu)(self.view.read(cx))
            .and_then(|menu| menu.overlay(&self.view, radius, self.answer));
        let child = self
            .overlay
            .as_mut()
            .map(|overlay| overlay.request_layout(window, cx));
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        window.request_layout(style, child, cx)
    }

    fn prepaint(&mut self, _: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        if let Some(overlay) = &mut self.overlay {
            overlay.prepaint(window, cx);
        }
    }

    fn paint(&mut self, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        if let Some(overlay) = &mut self.overlay {
            overlay.paint(window, cx);
        }
    }
}
