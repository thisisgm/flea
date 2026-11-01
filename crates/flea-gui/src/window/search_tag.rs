//! Tag rail activation enters the existing search flow without a second query engine.
use super::*;
use crate::window::startup::View;

impl Shell {
    pub(crate) fn search_tag(
        &mut self,
        side: usize,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return;
        };
        if self.input_blocked(cx)
            || (side == 1 && self.view != View::Dual)
            || host.stopped
            || host.backend.is_none()
            || self.locked_folder(side).is_some()
            || !host.search.owner.session.takes_query()
        {
            return;
        }
        if !host.search.owner.active() {
            let host = self.panes[side].as_mut().expect("checked pane");
            host.search.origin = Some(self.view);
            host.search.scroll = host.pane.scroll_y;
        }
        self.focus_pane(side, cx);
        self.rail_focus(false, cx);
        self.filter_update(
            side,
            |filter, pane| {
                filter.close(pane);
                None
            },
            cx,
        );
        let query = format!("tag:{name}");
        self.search_update(
            side,
            None,
            |search, host| search.open_query(&query, host),
            window,
            cx,
        );
        window.focus(&self.focus, cx);
    }
}
