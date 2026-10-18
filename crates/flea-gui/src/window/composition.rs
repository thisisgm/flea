//! Host geometry and typed producer slots; surfaces retain their own state and input handlers.
use super::startup::{Settings, View};
use gpui::{div, prelude::*, px, AnyElement, AnyView, Div, SharedString};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    fn at(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: width.max(0.),
            height: height.max(0.),
        }
    }
    fn element(self) -> Div {
        div()
            .absolute()
            .left(px(self.x))
            .top(px(self.y))
            .w(px(self.width))
            .h(px(self.height))
            .overflow_hidden()
    }
}

#[derive(Clone, Debug)]
pub struct Geometry {
    pub chrome: Rect,
    pub tabs: Rect,
    pub rail: Rect,
    pub panes: [Rect; 2],
    pub status: Rect,
}

impl Geometry {
    pub fn measure(width: f32, height: f32, settings: &Settings, view: View, tabs: bool) -> Self {
        let width = width.max(0.);
        let height = height.max(0.);
        let top = 27. + if tabs && view != View::Dual { 27. } else { 0. };
        let bottom = (height - 27.).max(top);
        let rail = settings.rail_width.clamp(0., width);
        let room = (width - rail).max(0.);
        let share = if room < 480. {
            0.5
        } else {
            settings.split.clamp(240. / room, 1. - 240. / room)
        };
        let left = if view == View::Dual {
            (room - 1.).max(0.) * share
        } else {
            room
        };
        Self {
            chrome: Rect::at(0., 0., width, 27.),
            tabs: Rect::at(0., 27., width, top - 27.),
            rail: Rect::at(0., top, rail, bottom - top),
            panes: [
                Rect::at(rail, top, left, bottom - top),
                Rect::at(
                    rail + left + 1.,
                    top,
                    (room - left - 1.).max(0.),
                    bottom - top,
                ),
            ],
            status: Rect::at(0., (height - 27.).max(0.), width, 27.),
        }
    }

    pub fn rows_height(&self, dual: bool, strip: f32) -> f32 {
        (self.panes[0].height - 27. - if dual { 27. } else { 0. } - strip).max(0.)
    }
}

#[derive(Default)]
pub struct Slots {
    pub chrome: Option<AnyView>,
    pub rail: Option<AnyView>,
    pub tabs: Option<AnyView>,
    pub headers: [Option<AnyView>; 2],
    pub strips: [Option<AnyView>; 2],
    pub states: [Option<AnyView>; 2],
    pub strip_height: f32,
    pub status: Option<AnyView>,
}

#[derive(Clone)]
pub struct Labels {
    pub name: SharedString,
    pub size: SharedString,
    pub modified: SharedString,
}

pub(crate) struct Content<'a> {
    pub paths: [SharedString; 2],
    pub status: SharedString,
    pub labels: &'a Labels,
}

pub(crate) fn render(
    geometry: &Geometry,
    slots: &Slots,
    panes: [Option<AnyElement>; 2],
    content: Content<'_>,
    dual: bool,
    probe: Option<crate::visual::Probe>,
) -> Div {
    use crate::visual::geometry::{label, surface};
    let Content {
        paths,
        status,
        labels,
    } = content;
    let text = |div: Div, id: String, parent: &str, value: &SharedString, inset: f32, right: bool| {
        label(div, id, parent.into(), value.clone(), inset, right, probe.clone())
    };
    let chrome = surface(geometry.chrome.element(), "chrome", probe.clone())
        .px(px(14.))
        .text_size(px(12.));
    let mut root = div().size_full().child(match &slots.chrome {
        Some(view) => chrome.child(view.clone()),
        None => text(chrome, "chrome-path".into(), "chrome", &paths[0], 14., false)
            .child(paths[0].clone()),
    });
    if let Some(view) = &slots.tabs {
        root = root.child(geometry.tabs.element().child(view.clone()));
    }
    if let Some(view) = &slots.rail {
        root = root.child(geometry.rail.element().child(view.clone()));
    }
    for (side, content) in panes.into_iter().enumerate() {
        let Some(content) = content else { continue };
        let mut pane = geometry.panes[side].element().flex().flex_col();
        if dual {
            let id = format!("path-{side}");
            let row = surface(div(), id.clone(), probe.clone())
                .relative()
                .h(px(27.))
                .flex_none()
                .px(px(14.))
                .text_size(px(12.));
            pane = pane.child(
                text(row, format!("{id}-text"), &id, &paths[side], 14., false)
                    .child(paths[side].clone()),
            );
        }
        let id = format!("header-{side}");
        let header = surface(div(), id.clone(), probe.clone())
            .relative()
            .h(px(27.))
            .flex_none();
        let cell = |div: Div, key: &str, value: &SharedString, right: bool| {
            text(div.relative(), format!("{id}-{key}"), &id, value, 0., right).child(value.clone())
        };
        pane = pane.child(match &slots.headers[side] {
            Some(view) => header.child(view.clone()),
            None => header
                .flex()
                .items_center()
                .pl(px(14.))
                .pr(px(28.))
                .text_size(px(12.))
                .child(cell(div().flex_1(), "name", &labels.name, false))
                .child(cell(div().w(px(70.)).text_right(), "size", &labels.size, true))
                .child(cell(
                    div().w(px(134.)).text_right(),
                    "modified",
                    &labels.modified,
                    true,
                )),
        });
        if let Some(view) = &slots.strips[side] {
            pane = pane.child(
                div()
                    .h(px(slots.strip_height))
                    .flex_none()
                    .child(view.clone()),
            );
        }
        root = root.child(pane.child(div().flex_1().min_h_0().overflow_hidden().child(content)));
    }
    let bar = surface(geometry.status.element(), "status", probe.clone())
        .px(px(14.))
        .text_size(px(12.));
    root.child(match &slots.status {
        Some(view) => bar.child(view.clone()),
        None => text(bar, "status-text".into(), "status", &status, 14., false).child(status),
    })
}
