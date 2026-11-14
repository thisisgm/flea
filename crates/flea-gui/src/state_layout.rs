use super::Shell;
use crate::window::{composition::Geometry, startup::View};
use gpui::Window;

const FALLBACK_HEADER_HEIGHT: f32 = 27.;

impl Shell {
    pub(crate) fn has_tabs(&self) -> bool {
        // TabBar.qml open is tabCount > 1 in every view; in Dual the strip spans both panes for the focused one.
        self.tab_strip_visible()
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
        // Settings > View "Window bar": mixed keeps the bar across the window and lets a cover such as Trash take the
        // full height beside the places; list draws every folder that way, window keeps the places below the bar's
        // band under a cover too.
        let bar =
            flea::ui_logic_settings_pages::pages::word(&self.launch.settings.data, &["windowBar"], "mixed");
        if view != View::Dual && (self.slots.covers[0].is_some() || self.flash_covers(0)) {
            // Only Trash reads the inset that keeps its body beside the places; the flasher takes its whole rectangle.
            if bar == "window" && !self.flash_covers(0) {
                geometry.cover_below_bar();
            } else {
                geometry.cover_single();
            }
        } else if bar == "list" {
            geometry.bar_over_list();
        }
        geometry
    }
}
