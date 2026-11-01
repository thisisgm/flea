//! What a committed listing owes: the Back landing, the deep target, the anchor and the held sort.
use super::{history::join, Command, Navigator};
use crate::pane::{Kind, Pane};

impl Navigator {
    // Called after every reply the pane took; a new commit count means a listing landed.
    pub fn after_reply(&mut self, pane: &mut Pane, before: &str, selected: &mut Vec<usize>) {
        self.sync_orders(pane);
        let commits = pane.swap.counters.commits;
        if commits != self.seen {
            self.seen = commits;
            if pane.path() != before {
                self.watch.clear();
                self.deletion_anchor = None;
                self.grid_landing = None;
            }
            if self
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.path != pane.path())
            {
                self.anchor = None;
            }
            self.land_restore(pane);
            self.landing_select(pane, selected);
            self.land_reload(pane);
            self.land_anchor(pane, selected);
            if let Some(at) = self.menus.landed(pane) {
                self.background_at = Some(at);
            }
            if let Some(preference) = self.preference.take() {
                if preference.path == pane.path() {
                    preference.apply(pane, selected);
                }
            }
            if self.relist_owed {
                let path = pane.path().to_owned();
                let _ = self.open(pane, &path);
            }
        } else {
            self.locate_parked(pane);
        }
        let now = pane.now();
        self.settle(pane, now);
    }

    // Name, then clamped index, then the top; a walk or an empty listing keeps the commit's own cursor.
    fn land_restore(&mut self, pane: &mut Pane) {
        let Some(restore) = self.restore.take() else {
            return;
        };
        let total = pane.held.total;
        if restore.entry.path != pane.path() || pane.state.search || total == 0 {
            return;
        }
        let name = restore.entry.name.as_str();
        pane.cursor = super::anchor::index_of(pane, name)
            .unwrap_or_else(|| restore.entry.index.min(total - 1));
        pane.reveal_cursor();
        pane.requests.initial_first = Some((pane.scroll_y / pane.row_height()).floor() as usize);
    }

    fn land_anchor(&mut self, pane: &mut Pane, selected: &mut Vec<usize>) {
        let Some(mut anchor) = self.anchor.take() else {
            return;
        };
        if anchor.need_paths.is_some() {
            self.anchor = Some(anchor);
            return;
        }
        let paths = anchor.apply(pane, selected);
        if anchor.landed {
            self.grid_landing = anchor
                .grid_offset
                .map(|offset| (pane.held.listing, pane.cursor, offset));
        }
        if let Some(paths) = paths {
            let id = self.next_locate();
            anchor.locate = Some(id);
            self.anchor = Some(anchor);
            self.emit(Command::LocateMany { paths, id });
        }
    }

    // A Back whose first window lacks the owed name asks where it is; the rows wait for its window.
    fn locate_parked(&mut self, pane: &Pane) {
        let Some(pending) = pane.swap.pending.as_ref() else {
            return;
        };
        let (Some(ready), Some(_)) = (pending.ready.as_ref(), pending.parked_until) else {
            return;
        };
        let Some(restore) = self.restore.as_mut() else {
            return;
        };
        if restore.locate.is_some() || restore.entry.path != pending.path {
            return;
        }
        let path = join(&restore.entry.path, &restore.entry.name);
        restore.locate = Some(path.clone());
        restore.total = ready.total;
        self.emit(Command::Locate { path });
    }

    // The locate reply for a waiting landing fetches the window holding the row, or the clamped index on a miss.
    pub fn take_located(
        &mut self,
        pane: &mut Pane,
        directory: &str,
        path: &str,
        index: i64,
    ) -> bool {
        let Some(restore) = self.restore.as_mut() else {
            return false;
        };
        if restore.fetched
            || restore.locate.as_deref() != Some(path)
            || restore.entry.path != directory
            || restore.total == 0
        {
            return false;
        }
        let total = restore.total;
        let index = usize::try_from(index)
            .ok()
            .filter(|index| *index < total)
            .unwrap_or_else(|| restore.entry.index.min(total - 1));
        restore.fetched = true;
        if let Some(request) = pane.anchor_window(index) {
            self.emit(Command::Window {
                start: request.start,
                count: request.count,
            });
        }
        true
    }

    // Only the anchor's own batch reply resolves it; a refused one lands what is already kept.
    pub fn take_located_many(
        &mut self,
        pane: &Pane,
        directory: &str,
        id: u64,
        ok: bool,
        matches: &[(String, usize)],
        selected: &mut Vec<usize>,
    ) -> bool {
        let Some(anchor) = self.anchor.as_mut() else {
            return false;
        };
        if anchor.locate != Some(id) || anchor.locate_done || directory != pane.path() {
            return false;
        }
        anchor.located(if ok { matches } else { &[] }, selected);
        self.anchor = None;
        true
    }

    // A listing that ended in a refusal owes no landing, and its anchor must not hold the watch forever.
    pub fn after_failure(&mut self, pane: &mut Pane, operation: &str) {
        if operation == "sort" {
            if let Some(at) = self.orders.requests.iter().position(|request| request.sort) {
                self.orders.requests.remove(at);
            }
            self.order = self
                .orders
                .requests
                .back()
                .map(|request| request.order)
                .unwrap_or(self.drawn_order());
        }
        if matches!(operation, "paths" | "stale") && self.gate.paths_pending {
            self.take_paths(pane, &[]);
            return;
        }
        let ended = !pane.busy() && matches!(pane.state.kind, Kind::Error | Kind::Locked);
        self.menus.failed(operation, ended);
        if ended {
            self.orders.requests.clear();
            self.order = self.drawn_order();
            self.orders.path = pane.path().into();
            self.restore = None;
            // A failed reload owes no notice, so a later landing of the same folder must not spend it.
            self.reload_from = None;
            self.reload_changed = None;
            // Only a failed Esc relist drops the cursor file; any other ended listing keeps it armed (ui/js/Swap.js failListing).
            if std::mem::take(&mut self.relisting) {
                self.pending_select = None;
            }
            self.relist_owed = false;
            if self
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.need_paths.is_none())
            {
                self.anchor = None;
            }
        }
        // The names on disk changed in a way no reply listed, so re-read the listing and select nothing.
        if matches!(operation, "rename-kept" | "rename-stranded") {
            let _ = self.refresh(pane, None);
        }
    }
}

#[cfg(test)]
mod window_tests {
    use crate::navigation::folder_state::Order;
    use crate::{backend::Backend, state::tests::shell_with, window::navigate::Step};
    use flea::protocol::{Reply, RowFacts};
    use gpui::{AppContext, Render, TestAppContext};

    fn deliver(
        shell: &mut crate::state::Shell,
        path: &str,
        listing: usize,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::state::Shell>,
    ) {
        for reply in [
            Reply::Listed {
                path: path.into(),
                total: 3,
            },
            Reply::Rows {
                start: 0,
                listing,
                rows: vec![RowFacts::default(); 3],
            },
        ] {
            shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
        }
        let _ = shell.render(window, cx);
    }

    #[gpui::test]
    fn m9_g1rows_05_shell_restored_header_waits_for_matching_rows(cx: &mut TestAppContext) {
        let (backend, _requests) = Backend::test_channel();
        let cx = cx.add_empty_window();
        let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                let data = flea::uistate::from_file(
                    r#"{"folderSorts":{"/B":{"key":"size","reverse":true}}}"#,
                );
                *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
                    super::super::folder_state::Folders::from_settings(&data, Vec::new());
                assert_eq!(shell.step(0, Step::Open("/B".into()), window, cx), Ok(true));
                let _ = shell.render(window, cx);
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.pane.path(), "/files", "old rows remain held");
                assert_eq!(
                    host.list.borrow().view.order,
                    Order::default(),
                    "the header must retain the old order while the old rows are drawn"
                );
                deliver(shell, "/B", 3, window, cx);
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.pane.path(), "/B");
                assert_eq!(host.pane.held.listing, 3);
                assert_eq!(
                    host.list.borrow().view.order,
                    Order {
                        key: "size",
                        desc: true
                    }
                );
            })
        });
    }

    #[gpui::test]
    fn m9_g1rows_05_shell_headers_follow_each_sort_pair_and_a_refused_sort(
        cx: &mut TestAppContext,
    ) {
        let (backend, _requests) = Backend::test_channel();
        let cx = cx.add_empty_window();
        let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                assert_eq!(
                    shell.step(0, Step::SortColumn("size".into()), window, cx),
                    Ok(true)
                );
                assert_eq!(
                    shell.step(0, Step::SortColumn("mtime".into()), window, cx),
                    Ok(true)
                );
                let _ = shell.render(window, cx);
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().list.borrow().view.order,
                    Order::default()
                );
                deliver(shell, "/files", 3, window, cx);
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(
                    host.nav.order.key, "mtime",
                    "the second sort remains requested"
                );
                assert_eq!(
                    host.list.borrow().view.order,
                    Order {
                        key: "size",
                        desc: false
                    }
                );
                deliver(shell, "/files", 4, window, cx);
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .list
                        .borrow()
                        .view
                        .order
                        .key,
                    "mtime"
                );
                assert_eq!(
                    shell.step(0, Step::SortColumn("kind".into()), window, cx),
                    Ok(true)
                );
                shell.receive(
                    0,
                    crate::backend::Event::Refused {
                        operation: "sort".into(),
                        path: "/files".into(),
                        message: String::new(),
                        mode: 0,
                    },
                    window,
                    cx,
                );
                let _ = shell.render(window, cx);
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.list.borrow().view.order.key, "mtime");
                assert_eq!(
                    host.nav.sort_snapshot(&host.pane).unwrap().order.key,
                    "mtime"
                );
            })
        });
    }

    #[gpui::test]
    fn ey476_shell_external_folder_map_updates_memory_not_explicit_order(cx: &mut TestAppContext) {
        let (backend, _requests) = Backend::test_channel();
        let cx = cx.add_empty_window();
        let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
                    super::super::folder_state::Folders::from_settings(
                        &flea::uistate::from_file("{}"),
                        Vec::new(),
                    );
                assert_eq!(
                    shell.step(0, Step::SortColumn("size".into()), window, cx),
                    Ok(true)
                );
                deliver(shell, "/files", 3, window, cx);
                let data = flea::uistate::from_file(
                    r#"{"folderSorts":{"/files":{"key":"kind","reverse":true}}}"#,
                );
                shell.apply_settings(data, window, cx);
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(
                    host.nav.order,
                    Order {
                        key: "size",
                        desc: false
                    }
                );
                assert_eq!(
                    host.nav.folders.borrow().get("/files"),
                    Some(Order {
                        key: "kind",
                        desc: true
                    })
                );
                deliver(shell, "/files", 4, window, cx);
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .list
                        .borrow()
                        .view
                        .order
                        .key,
                    "size"
                );
            })
        });
    }
}
