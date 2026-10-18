//! Count a visible pane only when its element paints; the probe schedules no frame of its own.
use crate::pane::{states::Kind, swap::Frames, Pane};
use gpui::{
    relative, size, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Position, Style, Window,
};
use std::rc::Rc;

pub(super) struct Frame {
    counters: Rc<Frames>,
    busy: bool,
    loading: bool,
    fell_back: bool,
}

impl Frame {
    pub fn new(pane: &Pane) -> Self {
        Self {
            counters: pane.swap.frames.clone(),
            busy: pane.busy(),
            loading: pane.state.kind == Kind::Loading,
            fell_back: pane.swap.fell_back,
        }
    }
}

impl IntoElement for Frame {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Frame {
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
            position: Position::Absolute,
            size: size(relative(1.).into(), relative(1.).into()),
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
        _: &mut Window,
        _: &mut App,
    ) {
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
        self.counters
            .record(self.busy, self.loading, self.fell_back);
    }
}
