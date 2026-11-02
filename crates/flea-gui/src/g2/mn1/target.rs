//! A menu may inspect a peeked path without changing the pane's listing or selection.
use super::join::{identity, Captured};
use crate::{state::Shell, window::pane::PaneHost};
use flea::{json::escape, protocol::RowFacts, ui_logic_mn1::build::Context as MenuContext};

pub(crate) struct PathMenu {
    pub base: String,
    pub row: Option<RowFacts>,
}

impl Captured {
    pub(crate) fn external(&self, host: &PaneHost) -> bool {
        self.target.is_some() || self.path != host.path
    }

    pub(crate) fn row<'a>(&'a self, host: &'a PaneHost, index: usize) -> Option<&'a RowFacts> {
        if self.external(host) {
            (index == 0).then_some(self.target.as_ref()).flatten()
        } else {
            host.pane.held.row(index)
        }
    }

    pub(crate) fn cursor_path(&self, host: &PaneHost) -> String {
        self.row(host, self.cursor)
            .map(|row| crate::navigation::history::join(&self.path, &row.name))
            .unwrap_or_else(|| self.path.clone())
    }

    pub(crate) fn snapshot_request(&self) -> String {
        if let Some(row) = &self.target {
            let path = escape(&crate::navigation::history::join(&self.path, &row.name));
            format!(
                r#"{{"c":"menuaction","op":"snapshot","id":{},"listing":{},"paths":["{path}"],"cursorPath":"{path}"}}"#,
                self.id, self.listing
            )
        } else {
            let rows = self
                .rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"c":"menuaction","op":"snapshot","id":{},"listing":{},"rows":[{rows}],"cursor":{}}}"#,
                self.id, self.listing, self.cursor
            )
        }
    }
}

impl Shell {
    pub(crate) fn captured_menu_context(&self, capture: &Captured) -> MenuContext {
        let side = capture.side;
        let mut context =
            self.listing_menu_context(side, &capture.rows, capture.cursor, capture.background);
        let Some(host) = self.panes[side].as_ref() else {
            return context;
        };
        if !capture.external(host) {
            return context;
        }
        context.selection_count = usize::from(capture.target.is_some());
        context.tag_submenu = None;
        // The zip probe follows the listing's own target, never a peeked path.
        context.flash_zip_member.clear();
        context.dir_writable = host
            .columns
            .as_ref()
            .and_then(|columns| columns.area.peeks.source(&capture.path))
            .map_or_else(
                || {
                    host.backend
                        .as_ref()
                        .is_some_and(|backend| backend.dir_writable(&capture.path))
                },
                |source| source.writable,
            );
        context.cursor_is_target = capture.target.is_some();
        if let Some(row) = &capture.target {
            let image = crate::window::output::kind(row) == flea::ui_logic_op4::output::Kind::Image;
            context.row_name = row.name.clone();
            context.archive_name = row.name.clone();
            context.row_mode = row.mode;
            context.row_is_file = !row.dir;
            context.row_is_symlink = row.mode & 0o170000 == 0o120000;
            context.row_is_archive =
                flea::ui_logic_mn1::archive::row_is_archive(&row.name, row.dir);
            context.row_is_image = image;
            context.row_is_document = crate::window::output::kind(row).paged();
            context.selection_images = usize::from(image);
            context.selection_modes = vec![row.mode];
            context.printable =
                flea::ui_logic_op4::output::can_print(&[crate::window::output::kind(row)]);
        }
        context
    }
}

impl Shell {
    pub(super) fn neighbour_open_with(
        &mut self,
        capture: &Captured,
        leaf: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if leaf == flea::ui_logic_mn1::rows::OPEN_WITH_OTHER {
            self.retire_menu(window, cx);
            self.open_with_captured(capture, cx);
            return;
        }
        self.menu.captured = None;
        self.menu.menu.borrow_mut().close();
        self.menu.presentation.cancel();
        self.menu.activating = false;
        self.menu.reading = false;
        self.menu.tags.retire();
        self.providers_menu_closed();
        let side = capture.side;
        if let Some(host) = self.panes[side].as_ref() {
            let gate = crate::navigation::watch::Gate {
                menu_open: false,
                menu_pending: false,
                ..host.nav.gate
            };
            self.set_gate(side, gate, window, cx);
        }
        self.keyboard.context = flea::keys::Context::Listing;
        self.open_with_application(side, capture.id, leaf, cx);
    }
}

impl Shell {
    pub(super) fn neighbour_sort(
        &mut self,
        capture: &Captured,
        leaf: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[capture.side]
            .as_mut()
            .filter(|host| capture.external(host))
        else {
            return false;
        };
        let patch = {
            let mut folders = host.nav.folders.borrow_mut();
            if leaf == flea::ui_logic_mn1::rows::SORT_DEFAULT {
                folders.forget(&capture.path)
            } else {
                crate::navigation::sort::column_order(folders.order_for(&capture.path), leaf)
                    .and_then(|order| {
                        if folders.remember_sort {
                            folders.remember(&capture.path, order)
                        } else {
                            folders.global = order;
                            Some(format!(
                                r#"{{"sort":{{"key":"{}","reverse":{}}}}}"#,
                                crate::navigation::folder_state::stored_key(order.key),
                                order.desc
                            ))
                        }
                    })
            }
        };
        if let Some(patch) = patch {
            if let Some(data) = flea::jsondoc::parse(&patch)
                .ok()
                .and_then(|patch| flea::uistate::patched(&self.launch.settings.data, &patch).ok())
            {
                self.launch.settings.data = data;
            }
            host.nav
                .persist(patch, self.locale.catalog.tr("navigation.save_failed"));
        }
        self.retire_menu(window, cx);
        self.run_navigation(capture.side, window, cx);
        self.sync_columns(window, cx);
        true
    }
}

impl Shell {
    pub(super) fn menu_folder_facts(&self, host: &PaneHost, context: &mut MenuContext) {
        let base = self
            .menu
            .captured
            .as_ref()
            .map_or(host.path.as_str(), |capture| capture.path.as_str());
        context.sort_by = if base == host.path {
            host.nav.order.key
        } else {
            host.nav.folders.borrow().order_for(base).key
        }
        .into();
        context.has_folder_sort =
            !host.pane.swap.session.open && host.nav.folders.borrow().has(base);
    }
}

impl Shell {
    pub(crate) fn menu_identity(&self) -> String {
        if self.menu.rail.is_some() || self.strip.menu.is_some() {
            return String::new();
        }
        if let Some(place) = &self.menu.place {
            return place.key.clone();
        }
        self.menu
            .captured
            .as_ref()
            .and_then(|capture| {
                self.panes[capture.side]
                    .as_ref()
                    .map(|host| identity(host, capture.side, capture.background))
            })
            .unwrap_or_default()
    }
}

impl Shell {
    pub(super) fn menu_target_modes(
        &mut self,
        reply: &flea::jsondoc::Json,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self
            .menu
            .captured
            .as_mut()
            .and_then(|capture| capture.target.as_mut())
        else {
            return;
        };
        if let Some(mode) = reply
            .get("modes")
            .and_then(flea::jsondoc::Json::as_array)
            .and_then(|modes| modes.first())
            .and_then(flea::jsondoc::Json::as_f64)
        {
            target.mode = mode as u32;
        }
        target.dir = match target.mode & 0o170000 {
            0o040000 => true,
            0o100000 => false,
            _ => target.dir,
        };
        let capture = self.menu.captured.as_ref().unwrap().clone();
        self.menu.context = self.captured_menu_context(&capture);
        self.menu.context.compare_kinds = Some(capture.kinds.clone());
        self.archive_menu_context(capture.side);
        self.check_menu_shebang(window, cx);
    }
}

impl Shell {
    pub(crate) fn column_menu_focus(
        &self,
        side: usize,
    ) -> (
        Option<(crate::columns::geometry::Slot, usize)>,
        Option<crate::columns::geometry::Slot>,
    ) {
        let menu_target = self
            .menu
            .captured
            .as_ref()
            .filter(|capture| capture.side == side)
            .and_then(|capture| {
                let target = capture.target.as_ref()?;
                let tracks = self.panes[side].as_ref()?.columns.as_ref()?.area.tracks();
                let track = tracks
                    .iter()
                    .find(|track| track.path == capture.path && track.enabled)?;
                let crate::columns::TrackRows::Peek(rows) = &track.rows else {
                    return None;
                };
                rows.iter()
                    .position(|row| row.name == target.name)
                    .map(|index| (track.slot, index))
            });
        (
            menu_target,
            self.rename_neighbour_row(side).map(|(slot, _, _)| slot),
        )
    }
}
