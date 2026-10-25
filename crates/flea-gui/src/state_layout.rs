use super::{Shell, FALLBACK_HEADER_HEIGHT};
use crate::window::{composition::Geometry, startup::View};
use gpui::Window;

impl Shell {
    pub(crate) fn has_tabs(&self) -> bool {
        self.slots.tabs.is_some()
            || self
                .launch
                .settings
                .data
                .get("lastTabs")
                .and_then(|tabs| tabs.get("paths"))
                .and_then(flea::jsondoc::Json::as_array)
                .is_some_and(|paths| paths.len() > 1)
    }

    pub(crate) fn geometry(&self, window: &Window) -> Geometry {
        self.geometry_for(self.view, window)
    }

    pub(crate) fn geometry_for(&self, view: View, window: &Window) -> Geometry {
        let viewport = self.seam.viewport().unwrap_or(window.viewport_size());
        let mut geometry = Geometry::with_header_min(
            f32::from(viewport.width),
            f32::from(viewport.height),
            &self.launch.settings,
            view,
            self.has_tabs(),
            self.font_key.as_ref().map_or(FALLBACK_HEADER_HEIGHT, |_| {
                crate::list::Metrics::for_typeface(&self.active_typeface()).header
            }),
            self.dual_min(),
        );
        let detail = self.surfaces[0].detail_height;
        geometry.status.y = (geometry.status.y - detail).max(0.);
        geometry.status.height += detail;
        geometry.rail.height = (geometry.rail.height - detail).max(0.);
        for pane in &mut geometry.panes {
            pane.height = (pane.height - detail).max(0.);
        }
        if view != View::Dual && self.slots.covers[0].is_some() {
            geometry.cover_single();
        }
        geometry
    }
}
