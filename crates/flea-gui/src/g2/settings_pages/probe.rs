//! IPC geometry comes from the controls' laid-out boxes in the latest frame.
use gpui::{canvas, div, prelude::*, px, Bounds, Div, Pixels};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[path = "probe_pointer.rs"]
mod pointer;

#[derive(Clone, Default)]
pub struct Probe(
    Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    pointer::Pointer,
);

impl Probe {
    pub(crate) fn painted(&self, id: String, bounds: Bounds<Pixels>, window: &mut gpui::Window) {
        self.record(&id, bounds);
        pointer::listen(
            self.1.clone(),
            id,
            bounds.intersect(&window.content_mask().bounds),
            window,
        );
    }

    pub(crate) fn record(&self, id: &str, bounds: Bounds<Pixels>) {
        self.0.borrow_mut().insert(id.into(), bounds);
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn pointer(&self, id: &str) -> (bool, bool) {
        self.live_pointer(id).unwrap_or_default()
    }

    pub(crate) fn live_pointer(&self, id: &str) -> Option<(bool, bool)> {
        self.bounds(id)?;
        self.1.borrow().get(id).copied()
    }

    pub fn attach<E: ParentElement>(&self, div: E, id: impl Into<String>) -> E {
        div.child(self.mark(id))
    }

    /// The box its positioned parent was laid out in, recorded under `id`.
    pub fn mark(&self, id: impl Into<String>) -> impl IntoElement {
        let probe = self.clone();
        let id = id.into();
        let paint = id.clone();
        let pointer = self.1.clone();
        canvas(
            move |bounds, window, _| {
                let clip = window.content_mask().bounds;
                let mut boxes = probe.0.borrow_mut();
                boxes.insert(format!("{id}:mask"), clip);
                boxes.insert(id, bounds);
                bounds.intersect(&clip)
            },
            move |_, clipped, window, _| {
                pointer::listen(pointer.clone(), paint.clone(), clipped, window)
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .size_full()
    }

    /// As attach, and also the content mask the box is painted under, as `<id>:mask`.
    pub fn masked(&self, div: Div, id: &str) -> Div {
        let probe = self.clone();
        let id = id.to_owned();
        div.child(
            canvas(
                move |bounds, window, _| {
                    let mut boxes = probe.0.borrow_mut();
                    boxes.insert(format!("{id}:mask"), window.content_mask().bounds);
                    boxes.insert(id, bounds);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .left_0()
            .top_0()
            .size_full(),
        )
    }

    pub fn first_child(
        &self,
        id: &'static str,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut gpui::Window, &mut gpui::App) {
        let probe = self.clone();
        move |bounds, _, _| {
            if let Some(bounds) = bounds.first() {
                probe.0.borrow_mut().insert(id.into(), *bounds);
            }
        }
    }

    /// The box an element's children span, so a child drawn past its parent shows.
    pub fn children(
        &self,
        id: String,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut gpui::Window, &mut gpui::App) {
        let probe = self.clone();
        move |bounds, _, _| {
            if let Some(first) = bounds.first() {
                let span = bounds.iter().fold(*first, |span, child| span.union(child));
                probe.0.borrow_mut().insert(id.clone(), span);
            }
        }
    }

    /// The box an id was laid out in on the latest frame.
    pub fn bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.0.borrow().get(id).copied()
    }

    pub fn rect(&self, id: &str) -> String {
        self.0
            .borrow()
            .get(id)
            .map(|b| {
                crate::ipc::rows::rect_of(
                    b.origin.x.into(),
                    b.origin.y.into(),
                    b.size.width.into(),
                    b.size.height.into(),
                )
            })
            .unwrap_or_default()
    }

    pub fn centre(&self, id: &str, clip: &str) -> String {
        let boxes = self.0.borrow();
        let Some(b) = boxes.get(id) else {
            return String::new();
        };
        let Some(clip) = boxes.get(clip) else {
            return String::new();
        };
        let b = b.intersect(clip);
        if b.size.width <= px(0.) || b.size.height <= px(0.) {
            return String::new();
        }
        crate::ipc::rows::centre_of(
            b.origin.x.into(),
            b.origin.y.into(),
            b.size.width.into(),
            b.size.height.into(),
        )
    }
}

pub(super) fn row_probe(view: &super::SettingsView, id: &str) -> Div {
    view.probe.attach(div(), format!("row:{id}"))
}
