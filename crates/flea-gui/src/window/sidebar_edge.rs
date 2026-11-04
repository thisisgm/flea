//! Columns' sidebar hairline drags the docked rail through its width stops; a double click restores the shipped 192.
use crate::{
    columns::{
        edge::{EdgeElement, EdgePaint},
        preview_host::{EDGE_MARK, EDGE_ZONE, START_DRAG_DISTANCE},
    },
    rail::geometry::{width_stop, DEFAULT_WIDTH},
    state::Shell,
    window::{composition::Geometry, startup::View},
};
use flea::jsondoc::Json;
use gpui::{
    div, prelude::*, px, Context, Div, HoverListenerMode, MouseButton, MouseDownEvent, Window,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SidebarEdge {
    pub hovered: bool,
    /// The press x and the rail width the held drag started from.
    held: Option<(f32, f32)>,
    pulled: bool,
}

impl SidebarEdge {
    pub fn dragging(&self) -> bool {
        self.held.is_some()
    }

    pub fn press(&mut self, x: f32, from: f32) {
        self.held = Some((x, from));
        self.pulled = false;
    }

    /// The width a held drag shows at pointer x: the nearest stop once the pointer passes the drag distance.
    pub fn width_at(&mut self, x: f32) -> Option<f32> {
        let (press, from) = self.held?;
        let shift = (x - press).round();
        if !self.pulled && shift.abs() > START_DRAG_DISTANCE {
            self.pulled = true;
        }
        self.pulled
            .then(|| width_stop(Some(f64::from(from + shift))))
    }

    /// Release ends the hold; true when a pulled drag has a width to store.
    pub fn release(&mut self) -> bool {
        let pulled = self.held.take().is_some() && self.pulled;
        self.pulled = false;
        pulled
    }
}

/// The docked rail's hairline zone in window pixels, centred on the rail's last pixel column: left, top, width, height.
pub fn zone(geometry: &Geometry) -> Option<(f32, f32, f32, f32)> {
    let rail = geometry.rail;
    (rail.width > 0. && !geometry.rail_overlay).then(|| {
        let line = rail.x + rail.width - 1.;
        (
            line - (EDGE_ZONE / 2.).floor(),
            rail.y,
            EDGE_ZONE,
            rail.height,
        )
    })
}

impl Shell {
    /// The sidebar hairline in Columns: the resize cursor on its zone, and while hovered or held the accent line and the mark.
    pub(crate) fn sidebar_edge(
        &self,
        geometry: &Geometry,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        if self.view != View::Columns {
            return None;
        }
        let (left, top, width, height) = zone(geometry)?;
        let lit = self.sidebar.hovered || self.sidebar.dragging();
        // The mark sits on the Columns area's middle row, where every Columns hairline draws it.
        let area_top = geometry.panes[0].y + self.slots.strip_height + self.columns_search_strip(0);
        let area_height = (geometry.panes[0].height - (area_top - geometry.panes[0].y)).max(0.);
        let line = (EDGE_ZONE / 2.).floor();
        let paint = EdgePaint {
            line: lit.then_some((line, 0., 1., height)),
            mark: lit.then_some((
                line + 0.5 - EDGE_MARK / 2.,
                area_top - top + ((area_height - EDGE_MARK) / 2.).round(),
                EDGE_MARK,
            )),
        };
        Some(
            div()
                .id("sidebar-edge")
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(width))
                .h(px(height))
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
                .on_hover(cx.listener(|shell, hovered: &bool, _, cx| {
                    if shell.sidebar.hovered != *hovered {
                        shell.sidebar.hovered = *hovered;
                        cx.notify();
                    }
                }))
                .child(EdgeElement::new(paint))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|shell, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if shell.input_blocked(cx) {
                            return;
                        }
                        if event.click_count == 2 {
                            shell.sidebar.release();
                            shell.store_sidebar(DEFAULT_WIDTH, window, cx);
                        } else {
                            let from = shell.launch.settings.rail_width;
                            shell.sidebar.press(f32::from(event.position.x), from);
                        }
                        cx.notify();
                    }),
                ),
        )
    }

    /// A held sidebar drag shows the stop under the pointer live; a move with the button up ends it.
    pub(crate) fn sidebar_move(
        &mut self,
        x: f32,
        pressed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.sidebar.dragging() {
            return;
        }
        if !pressed {
            return self.sidebar_end(window, cx);
        }
        let Some(width) = self.sidebar.width_at(x) else {
            return;
        };
        if width != self.launch.settings.rail_width {
            self.launch.settings.rail_width = width;
            if let Some(join) = &self.rail {
                join.host.update(cx, |host, cx| host.set_width(width, cx));
            }
            cx.notify();
        }
    }

    pub(crate) fn sidebar_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sidebar.dragging() {
            return;
        }
        if self.sidebar.release() {
            self.store_sidebar(self.launch.settings.rail_width, window, cx);
        }
        cx.notify();
    }

    fn store_sidebar(&mut self, width: f32, window: &mut Window, cx: &mut Context<Self>) {
        let places = Json::Obj(vec![("sidebarWidth".into(), Json::Num(width.to_string()))]);
        self.change_settings(Json::Obj(vec![("places".into(), places)]), window, cx);
    }
}
