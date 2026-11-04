//! Native drops resolve chrome, rail and pane destinations through the same file operation owner.
use super::*;

impl Shell {
    pub(super) fn land_arrived(
        &mut self,
        arrived: Arrived,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_cue(window, cx);
        self.clear_file_rest(window, cx);
        let point = gpui::point(gpui::px(arrived.position.0), gpui::px(arrived.position.1));
        if let Some((side, dest)) = self.file_rail_folder(point, cx) {
            if !self.input_blocked(cx) {
                self.land_crumb(side, dest, Owned::Native(arrived), window, cx);
            }
            return;
        }
        if let Some((side, dest)) = self.file_crumb_destination(point, cx) {
            if !self.input_blocked(cx) {
                self.land_crumb(side, dest, Owned::Native(arrived), window, cx);
            }
            return;
        }
        if self.file_tab_destination(point).is_some() {
            self.land_at(self.focus_side, point, Owned::Native(arrived), window, cx);
            return;
        }
        let side = (0..self.panes.len()).find(|side| {
            self.panes[*side].as_ref().is_some_and(|host| {
                if host.drawn_view == Some(crate::window::startup::View::List)
                    && host.pane.state.kind == crate::pane::states::Kind::Empty
                {
                    // An empty List paints its floor but mounts no row input surface.
                    let drawn = host.list.borrow().drawn;
                    return drawn.listing == host.pane.held.listing
                        && drawn.bounds.contains(&point);
                }
                host.input
                    .frame
                    .is_some_and(|frame| frame.bounds.contains(&point))
            })
        });
        if let Some(side) = side {
            self.land_at(side, point, Owned::Native(arrived), window, cx);
        }
    }
}
