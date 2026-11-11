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
    pub rail_overlay: bool,
    pub panes: [Rect; 2],
    pub status: Rect,
}

impl Geometry {
    pub fn cover_single(&mut self) {
        self.chrome.height = 0.;
        self.tabs.height = 0.;
        self.rail.y = 0.;
        self.rail.height = self.status.y;
        self.panes[0].y = 0.;
        self.panes[0].height = self.status.y;
    }

    pub fn measure(width: f32, height: f32, settings: &Settings, view: View, tabs: bool) -> Self {
        Self::with_header(width, height, settings, view, tabs, 27.)
    }

    pub fn with_header(
        width: f32,
        height: f32,
        settings: &Settings,
        view: View,
        tabs: bool,
        header: f32,
    ) -> Self {
        Self::with_header_min(
            width,
            height,
            settings,
            view,
            tabs,
            header,
            crate::dual::ROOM_MIN,
        )
    }

    pub(crate) fn with_header_min(
        width: f32,
        height: f32,
        settings: &Settings,
        view: View,
        tabs: bool,
        header: f32,
        min: f32,
    ) -> Self {
        let width = width.max(0.);
        let height = height.max(0.);
        let top = header + if tabs { header } else { 0. };
        let bottom = (height - header).max(top);
        let places = settings
            .data
            .get("places")
            .unwrap_or(&flea::jsondoc::Json::Null);
        let mode = crate::rail::Mode::from_places(places);
        let rail = if mode == crate::rail::Mode::Docked {
            settings.rail_width.clamp(0., width)
        } else {
            0.
        };
        let room = (width - rail).max(0.);
        // DualRoom in WindowBody.qml: the room right of the rail less the hairline divider.
        let dual_room = (room - 1.).max(0.);
        let share = if view != View::Dual || dual_room < 2. * min {
            0.5
        } else {
            let low = min / dual_room;
            settings.split.clamp(low, 1. - low)
        };
        let left = if view == View::Dual {
            dual_room * share
        } else {
            room
        };
        Self {
            chrome: Rect::at(0., 0., width, header),
            tabs: Rect::at(0., header, width, top - header),
            rail: Rect::at(
                0.,
                top,
                if mode == crate::rail::Mode::Overlay {
                    width
                } else {
                    rail
                },
                bottom - top,
            ),
            rail_overlay: mode == crate::rail::Mode::Overlay,
            panes: [
                Rect::at(rail, top, left, bottom - top),
                Rect::at(
                    rail + left + 1.,
                    top,
                    (room - left - 1.).max(0.),
                    bottom - top,
                ),
            ],
            status: Rect::at(0., (height - header).max(0.), width, header),
        }
    }

    /// Where a pane's rows draw: below its path row, header and this pane's mounted strip.
    pub fn rows(&self, side: usize, dual: bool, strip: f32) -> Rect {
        let pane = self.panes[side];
        let top = self.chrome.height + if dual { self.chrome.height } else { 0. } + strip;
        Rect::at(pane.x, pane.y + top, pane.width, pane.height - top)
    }

    pub fn rows_height(&self, dual: bool, strip: f32) -> f32 {
        (self.panes[0].height
            - self.chrome.height
            - if dual { self.chrome.height } else { 0. }
            - strip)
            .max(0.)
    }

    pub fn content_height(&self, view: View, strip: f32) -> f32 {
        self.content_height_for(0, view, strip)
    }

    pub fn content_height_for(&self, side: usize, view: View, strip: f32) -> f32 {
        if matches!(view, View::Columns | View::Grid) {
            (self.panes[side].height - strip).max(0.)
        } else {
            self.rows(side, view == View::Dual, strip).height
        }
    }
}

#[derive(Default)]
pub struct Slots {
    pub chrome: Option<AnyView>,
    pub rail: Option<AnyView>,
    pub tabs: Option<AnyView>,
    pub headers: [Option<AnyView>; 2],
    pub paths: [Option<AnyView>; 2],
    pub strips: [Option<AnyView>; 2],
    pub states: [Option<AnyView>; 2],
    pub covers: [Option<AnyView>; 2],
    pub strip_height: f32,
    pub status: Option<AnyView>,
}

impl Slots {
    pub(crate) fn strip_height_for(&self, side: usize) -> f32 {
        if self.strips[side].is_some() {
            self.strip_height
        } else {
            0.
        }
    }
}

#[derive(Clone)]
pub struct Labels {
    pub name: SharedString,
    pub size: SharedString,
    pub modified: SharedString,
}

pub(crate) struct Content<'a> {
    pub glass: super::glass::Grounds,
    pub paths: [SharedString; 2],
    pub headers: [Option<(AnyElement, f32)>; 2],
    pub status: SharedString,
    pub labels: &'a Labels,
    pub typeface: crate::list::Typeface,
    pub focus: &'a crate::dual::focus::Focus,
    pub pane_edge: Option<Div>,
}

/// ChromeBar.qml, the tab strip and the docked rail take the keyboard only from Tab, never from a press.
fn keep_the_keys(_: &gpui::MouseDownEvent, window: &mut gpui::Window, _: &mut gpui::App) {
    window.prevent_default();
}

pub(crate) fn render(
    geometry: &Geometry,
    slots: &Slots,
    panes: [Option<AnyElement>; 2],
    content: Content<'_>,
    view: View,
    probe: Option<crate::visual::Probe>,
) -> Div {
    let dual = view == View::Dual;
    use crate::visual::geometry::{label, surface};
    let Content {
        paths,
        glass,
        mut headers,
        status,
        labels,
        typeface,
        focus,
        pane_edge,
    } = content;
    let metrics = crate::list::Metrics::for_typeface(&typeface);
    let text =
        |div: Div, id: String, parent: &str, value: &SharedString, inset: f32, right: bool| {
            label(
                div,
                id,
                parent.into(),
                value.clone(),
                inset,
                right,
                probe.clone(),
            )
        };
    let chrome = surface(geometry.chrome.element(), "chrome", probe.clone())
        .when(glass.glassy && slots.chrome.is_none(), |div| {
            div.child(glass.chrome())
        })
        .track_focus(&focus.chrome)
        .on_any_mouse_down(keep_the_keys)
        .text_size(typeface.caption);
    let mut root = div().size_full().child(match &slots.chrome {
        Some(view) => chrome.child(view.clone()),
        None => text(
            chrome.px(px(metrics.pad)),
            "chrome-path".into(),
            "chrome",
            &paths[0],
            metrics.pad,
            false,
        )
        .child(paths[0].clone()),
    }
    // The toolbar's focus holder is a node too, named by the folder its path shows.
    .id("toolbar")
    .role(gpui::Role::Toolbar)
    .aria_label(paths[0].clone()));
    if let Some(view) = &slots.tabs {
        root = root.child(
            geometry
                .tabs
                .element()
                .track_focus(&focus.strip)
                .on_any_mouse_down(keep_the_keys)
                .child(view.clone()),
        );
    }
    if let Some(view) = slots.rail.as_ref().filter(|_| !geometry.rail_overlay) {
        root = root.child(
            geometry
                .rail
                .element()
                .track_focus(&focus.rail)
                .on_any_mouse_down(keep_the_keys)
                .child(view.clone())
                // the rail's focus holder is a node, so the rail's cursor row reports as the focused item
                .id("rail")
                .role(gpui::Role::Group),
        );
    }
    for (side, content) in panes.into_iter().enumerate() {
        if side == 1 && !dual {
            continue;
        }
        if let Some(cover) = &slots.covers[side] {
            root = root.child(geometry.panes[side].element().child(cover.clone()));
            continue;
        }
        let Some(content) = content else { continue };
        let mut pane = geometry.panes[side].element().flex().flex_col();
        if dual {
            let id = format!("path-{side}");
            let row = surface(div(), id.clone(), probe.clone())
                .children(glass.path_fallback(side, slots.paths[side].is_some()))
                .relative()
                .h(px(metrics.header))
                .flex_none()
                .text_size(typeface.caption);
            pane = pane.child(match &slots.paths[side] {
                Some(view) => row.child(view.clone()),
                None => text(
                    row.px(px(metrics.pad)),
                    format!("{id}-text"),
                    &id,
                    &paths[side],
                    metrics.pad,
                    false,
                )
                .child(paths[side].clone()),
            });
        }
        let id = format!("header-{side}");
        let header = surface(div(), id.clone(), probe.clone())
            .when(glass.glassy, |div| div.child(glass.header()))
            .relative()
            .h(px(headers[side]
                .as_ref()
                .map_or(metrics.header, |(_, height)| *height)))
            .flex_none();
        let cell = |div: Div, key: &str, value: &SharedString, right: bool| {
            text(div.relative(), format!("{id}-{key}"), &id, value, 0., right).child(value.clone())
        };
        if matches!(view, View::List | View::Dual) {
            pane = pane.child(match &slots.headers[side] {
                Some(view) => header.child(view.clone()),
                None if headers[side].is_some() => header.child(headers[side].take().unwrap().0),
                None => header
                    .flex()
                    .items_center()
                    .pl(px(metrics.pad))
                    .pr(px(2. * metrics.pad))
                    .text_size(typeface.caption)
                    .child(cell(div().flex_1(), "name", &labels.name, false))
                    .child(cell(
                        div()
                            .w(px(70. * f32::from(typeface.body_small) / 13.))
                            .text_right(),
                        "size",
                        &labels.size,
                        true,
                    ))
                    .child(cell(
                        div()
                            .w(px(134. * f32::from(typeface.body_small) / 13.))
                            .text_right(),
                        "modified",
                        &labels.modified,
                        true,
                    )),
            });
        } else if let Some((element, _)) = headers[side].take() {
            // HIG cols4-hero-search: Columns has no list header, so only a search strip arrives here, above the columns.
            pane = pane.child(header.child(element));
        }
        if let Some(view) = &slots.strips[side] {
            pane = pane.child(
                div()
                    .when(glass.glassy, |div| div.child(glass.chrome()))
                    .h(px(slots.strip_height))
                    .flex_none()
                    .child(view.clone()),
            );
        }
        root = root.child(pane.child(div().flex_1().min_h_0().overflow_hidden().child(content)));
    }
    root = root.children(pane_edge);
    if let Some(view) = slots.rail.as_ref().filter(|_| geometry.rail_overlay) {
        // PaneRail.qml: the overlay owns only its edge and body (the host hears those presses), so its stop hangs on a zero-size node.
        root = root.child(
            geometry
                .rail
                .element()
                .child(div().absolute().size_0().track_focus(&focus.rail).id("rail").role(gpui::Role::Group))
                .child(view.clone()),
        );
    }
    let bar = surface(geometry.status.element(), "status", probe.clone())
        .when(glass.glassy && slots.status.is_none(), |div| {
            div.child(glass.status_backdrop())
        })
        .px(px(metrics.pad))
        .text_size(typeface.caption);
    root.child(match &slots.status {
        Some(view) => bar.px(px(0.)).child(view.clone()),
        None => text(
            bar,
            "status-text".into(),
            "status",
            &status,
            metrics.pad,
            false,
        )
        .child(status),
    })
}
