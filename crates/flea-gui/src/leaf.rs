//! One Element implementation for leaf elements that keep no element id and need only layout, prepaint and paint.
use gpui::{
    relative, size, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Style, Window,
};

pub trait Leaf: 'static {
    type Prepaint: 'static;
    fn layout(&mut self, window: &mut Window, cx: &mut App) -> LayoutId;
    fn prepaint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::Prepaint;
    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        prepaint: &mut Self::Prepaint,
        window: &mut Window,
        cx: &mut App,
    );
}

/// The leaf as a GPUI element.
pub struct Painted<T>(pub T);

/// A layout that fills the parent on both axes.
pub fn fill_parent(window: &mut Window, cx: &mut App) -> LayoutId {
    let style = Style {
        size: size(relative(1.).into(), relative(1.).into()),
        ..Style::default()
    };
    window.request_layout(style, None, cx)
}

impl<T: Leaf> IntoElement for Painted<T> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<T: Leaf> Element for Painted<T> {
    type RequestLayoutState = ();
    type PrepaintState = T::Prepaint;

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
        (self.0.layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> T::Prepaint {
        self.0.prepaint(bounds, window, cx)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut T::Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.paint(bounds, prepaint, window, cx);
    }
}
