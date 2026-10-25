//! Bind a tab's saved cursor to the listing it requested, including its initial held window.
use crate::{
    navigation::{history::join, Command},
    state::Shell,
};

impl super::host::Host {
    pub(crate) fn launch_listing(
        &self,
        host: &mut crate::window::pane::PaneHost,
        side: usize,
    ) -> Option<crate::navigation::Listing> {
        let snapshot = self
            .launch
            .sort
            .as_ref()
            .filter(|snapshot| side == 0 && snapshot.path == host.path)?;
        if host.nav.restore_sort_snapshot(&mut host.pane, snapshot) != Ok(true) {
            return None;
        }
        host.nav
            .take()
            .into_iter()
            .find_map(|command| match command {
                Command::List(listing) => Some(listing),
                _ => None,
            })
    }

    // A torn-off tab's cursor names the file its first listing selects, on the launch pane only.
    pub(crate) fn launch_select(&self, side: usize, path: &str) -> Option<String> {
        (side == 0 && !self.launch.cursor.is_empty()).then(|| join(path, &self.launch.cursor))
    }
}

impl Shell {
    pub(crate) fn bind_tab_landings(&mut self) {
        for side in 0..2 {
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let Some(tabs) = self.strip.tabs[side].tabs.as_mut() else {
                continue;
            };
            if tabs.pending.cursor.is_none()
                && tabs.pending.cursor_name.is_none()
                && tabs.pending.order.is_none()
            {
                continue;
            }
            let Some(Command::List(listing)) = host
                .nav
                .out
                .iter_mut()
                .rev()
                .find(|command| matches!(command, Command::List(_)))
            else {
                continue;
            };
            if listing.path != host.pane.place() {
                continue;
            }
            self.strip.landing_request[side] = Some(host.pane.list_requests);
            let saved = tabs
                .items
                .get(tabs.index)
                .filter(|tab| tab.path == listing.path)
                .map(|tab| tab.cursor_name.as_str())
                .unwrap_or("");
            let name = tabs
                .pending
                .cursor_name
                .as_deref()
                .filter(|name| !name.is_empty())
                .unwrap_or(saved);
            if !name.is_empty() {
                listing.select = Some(join(&listing.path, name));
                tabs.pending.cursor_name = Some(name.to_owned());
            }
        }
    }

    pub(crate) fn guard_tab_landing(&mut self, side: usize) {
        let Some(request) = self.strip.landing_request[side] else {
            return;
        };
        let Some(host) = &self.panes[side] else {
            return;
        };
        if request != host.pane.list_requests
            || !matches!(
                host.pane.state.kind,
                crate::pane::Kind::Ready | crate::pane::Kind::Empty
            )
        {
            if let Some(tabs) = self.strip.tabs[side].tabs.as_mut() {
                tabs.pending = Default::default();
            }
            self.strip.landing_request[side] = None;
        }
    }
}
