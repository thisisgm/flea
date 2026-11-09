//! The clamped body reveals its target before any child prepaints, only on an explicit request.
use super::{
    controls::Control,
    model::{Card, Focus, Model},
};
use gpui::{
    div, prelude::*, px, AnyElement, App, Bounds, Div, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, ScrollHandle, Window,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Format(usize),
    Focus(Focus),
}

impl Target {
    fn matches(self, control: Control) -> bool {
        match (self, control) {
            (Self::Format(a), Control::Choice(Focus::Formats, b)) => a == b,
            (Self::Focus(a), Control::Field(b) | Control::Check(b) | Control::Choice(b, _)) => {
                a == b
            }
            _ => false,
        }
    }
}

#[cfg(test)]
type PaintedTarget = (Bounds<Pixels>, Bounds<Pixels>);

#[derive(Default)]
pub(super) struct BodyScroll {
    pub handle: ScrollHandle,
    last: Cell<Option<Target>>,
    requested: Cell<bool>,
    pending: Cell<Option<Target>>,
    layout: Rc<Cell<Option<LayoutId>>>,
    #[cfg(test)]
    pub painted: Rc<std::cell::RefCell<Vec<PaintedTarget>>>,
}

impl BodyScroll {
    pub fn reset(&self) {
        self.handle.set_offset(Default::default());
        self.request_reveal();
    }

    pub fn request_reveal(&self) {
        self.requested.set(true);
    }

    pub fn prepare(&self, model: &Model) {
        let target = match (model.focus, &model.card) {
            (Some(Focus::Formats), Some(Card::Convert(c))) => Some(Target::Format(c.cursor)),
            (None | Some(Focus::Cancel | Focus::Submit), _) => None,
            (Some(focus), _) => Some(Target::Focus(focus)),
        };
        self.layout.set(None);
        self.last.set(target);
        self.pending
            .set(if self.requested.take() { target } else { None });
    }

    pub fn body(&self, element: impl IntoElement) -> impl IntoElement {
        Reveal {
            inner: element.into_any_element(),
            layout: self.layout.clone(),
            scroll: Some(self.handle.clone()),
        }
    }

    pub fn observe(&self, control: Control, element: Div) -> Div {
        #[cfg(test)]
        let element = if self
            .last
            .get()
            .is_some_and(|target| target.matches(control))
        {
            let painted = self.painted.clone();
            let scroll = self.handle.clone();
            element.relative().child(
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, _, _| {
                        painted.borrow_mut().push((bounds, scroll.bounds()));
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .size_full(),
            )
        } else {
            element
        };
        if !self
            .pending
            .get()
            .is_some_and(|target| target.matches(control))
        {
            return element;
        }
        element.relative().child(Reveal {
            inner: div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .into_any_element(),
            layout: self.layout.clone(),
            scroll: None,
        })
    }
}

struct Reveal {
    inner: AnyElement,
    layout: Rc<Cell<Option<LayoutId>>>,
    scroll: Option<ScrollHandle>,
}

impl IntoElement for Reveal {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Reveal {
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
        let id = self.inner.request_layout(window, cx);
        if self.scroll.is_none() {
            self.layout.set(Some(id));
        }
        (id, ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(scroll) = &self.scroll {
            if let Some(id) = self.layout.take() {
                // Current layout bounds exclude scrolling; apply the final offset before the body prepaints.
                let target = window.layout_bounds(id).dilate(px(4.));
                let mut offset = scroll.offset();
                if target.size.height > bounds.size.height || target.top() + offset.y < bounds.top()
                {
                    offset.y = bounds.top() - target.top();
                } else if target.bottom() + offset.y > bounds.bottom() {
                    offset.y = bounds.bottom() - target.bottom();
                }
                // The body clamps against this layout's content extent in its own prepaint.
                offset.y = offset.y.min(px(0.));
                scroll.set_offset(offset);
            }
        }
        self.inner.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner.paint(window, cx);
    }
}
