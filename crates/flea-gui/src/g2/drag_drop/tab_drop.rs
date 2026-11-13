//! A tab drop names the painted tab's identity and folder while disk facts are read off thread.
use super::*;
use crate::tabs::bar::Hit;

pub(super) struct Destination {
    pub(super) side: usize,
    pub(super) identity: u64,
    pub(super) path: String,
}

impl Shell {
    pub(super) fn file_tab_destination(&self, point: Point<gpui::Pixels>) -> Option<Destination> {
        if !self.strip.bounds.get()?.contains(&point) {
            return None;
        }
        let side = self.strip.snapshot.as_ref()?.side;
        if side != self.focus_side {
            return None;
        }
        let index = match self.tab_hit(point) {
            Hit::Tab(index) | Hit::Close(index) => index,
            _ => return None,
        };
        let tabs = &self.strip.tabs[side];
        let identity = tabs.tabs.as_ref()?.items.get(index)?.identity;
        let path = tabs.path_at(index, &self.panes[side].as_ref()?.path);
        Some(Destination {
            side,
            identity,
            path,
        })
    }

    pub(super) fn land_file_tab(
        &mut self,
        tab: Destination,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.file_tab_ready(&tab) {
            if self.panes[tab.side]
                .as_ref()
                .is_some_and(|host| host.pane.busy())
            {
                self.refuse_drop_loading(tab.side, cx);
            }
            return;
        }
        let sources = owned.sources();
        let dest = tab.path.clone();
        let facts = cx
            .background_executor()
            .spawn(async move { facts::read(&sources, &dest) });
        cx.spawn_in(window, async move |this, cx| {
            let facts = facts.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell.file_tab_ready(&tab) {
                    shell.finish_drop(
                        tab.side,
                        &shell.panes[tab.side].as_ref().unwrap().path.clone(),
                        owned,
                        &tab.path,
                        facts,
                        window,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn file_tab_ready(&self, tab: &Destination) -> bool {
        let Some(index) = self.strip.tabs[tab.side].resolve(tab.identity) else {
            return false;
        };
        let Some(host) = self.panes[tab.side].as_ref() else {
            return false;
        };
        let current = index == self.strip.tabs[tab.side].current();
        !host.stopped
            && !(current && (host.pane.busy() || host.search.owner.active()))
            && !tab.path.is_empty()
            && self.strip.tabs[tab.side].path_at(index, &host.path) == tab.path
    }
}

#[cfg(test)]
#[path = "tab_drop_guard_tests.rs"]
mod tests;
