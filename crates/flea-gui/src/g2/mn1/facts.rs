//! The menu facts: the cursor rows' context, the rows Settings hides, the backend's archive answer and the Update Flea hint.
use crate::{state::Shell, window::pane::PaneHost};
use flea::{
    jsondoc::Json,
    ui_logic_mn1::{build::Context as MenuContext, inventory},
};

impl Shell {
    /// The rows Settings hides, which the menu and the provider gating read alike.
    pub(crate) fn menu_hidden(&self) -> Vec<String> {
        let stored = self
            .launch
            .settings
            .data
            .get("menu")
            .and_then(|m| m.get("hidden"))
            .and_then(Json::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
        // Add to Shelf follows any shelf plugin switched on and ready, the built-in Shelf (shelf.enabled) or one added.
        inventory::effective_hidden(
            stored.as_deref(),
            self.declared_menu_plugins("shelf").next().is_some(),
        )
    }

    /// The menu's context for these rows of a side before any producer answers, as the menu and the keyboard sheet read it.
    pub(crate) fn listing_menu_context(
        &self,
        side: usize,
        rows: &[usize],
        cursor: usize,
        background: bool,
    ) -> MenuContext {
        let (clipboard_available, clipboard_watch_failed, can_link) =
            self.clipboard_menu_facts(side);
        let Some(host) = &self.panes[side] else {
            return MenuContext::default();
        };
        let facts = (!background).then(|| host.pane.held.row(cursor)).flatten();
        // Print, Convert and Combine Images read the marked rows' output kinds.
        let kinds = rows
            .iter()
            .filter_map(|&row| host.pane.held.row(row).map(crate::window::output::kind))
            .collect::<Vec<_>>();
        let image = flea::ui_logic_op4::output::Kind::Image;
        MenuContext {
            tag_submenu: (!background)
                .then(|| self.tag_entries_for(side, rows))
                .flatten(),
            has_row: !background,
            selection_count: if background {
                background_count(host)
            } else {
                rows.len()
            },
            has_shebang: false,
            cursor_is_target: rows == [cursor],
            row_mode: facts.map_or(0, |r| r.mode),
            row_is_file: facts.is_some_and(|r| !r.dir),
            row_is_symlink: facts.is_some_and(|r| r.link.is_some()),
            row_name: facts.map_or_else(String::new, |r| r.name.clone()),
            flash_zip_member: facts.map_or_else(String::new, |r| {
                let base = host.search.owner.session.base_of(&host.path);
                self.menu.zip[side]
                    .member_for(&crate::navigation::history::join(base, &r.name))
                    .to_owned()
            }),
            row_is_archive: facts
                .is_some_and(|r| flea::ui_logic_mn1::archive::row_is_archive(&r.name, r.dir)),
            archive_name: facts.map_or_else(String::new, |r| r.name.clone()),
            row_is_image: facts.is_some_and(|r| crate::window::output::kind(r) == image),
            row_is_document: facts.is_some_and(|r| crate::window::output::kind(r).paged()),
            selection_images: kinds.iter().filter(|&&kind| kind == image).count(),
            printable: kinds.len() == rows.len() && flea::ui_logic_op4::output::can_print(&kinds),
            can_convert: self.output.convert[side] != Some(false),
            selection_modes: rows
                .iter()
                .filter_map(|&row| host.pane.held.row(row).map(|r| r.mode))
                .collect(),
            lang_marks: self
                .launch
                .settings
                .data
                .get("langMarks")
                .and_then(Json::as_bool)
                != Some(false),
            open_with_loaded: background,
            clipboard_available,
            clipboard_image_available: self.clipboard.board.image_available(),
            clipboard_watch_failed,
            can_link,
            can_trash: flea::ui_logic_pl1::mounts::trashable(&host.path, None),
            // Menu.js greys New Folder, New File, Paste and every writing row where the backend cannot write the folder.
            dir_writable: host
                .backend
                .as_ref()
                .is_none_or(|backend| backend.dir_writable(host.pane.path())),
            recent: host.recent.on || flea::ui_logic_pk1::is_recent(host.pane.place()),
            searching: host.search.owner.active(),
            ..MenuContext::default()
        }
    }

    /// The facts a formats reply or an update check may change while the menu is open.
    pub(super) fn menu_live_facts(&self, side: usize, context: &mut MenuContext) {
        context.archive_formats = self.output.archive[side].clone().unwrap_or_default();
        context.can_extract = flea::ui_logic_mn1::archive::can_extract(
            &context.archive_name,
            self.output.extract[side],
        );
        context.update_version = self.menu_update_version();
        if let Some(host) = &self.panes[side] {
            context.storage_class = host.sizes.class().unwrap_or_default().into();
        }
        let preview = self.launch.settings.data.get("preview");
        let flag = |key| {
            preview
                .and_then(|preview| preview.get(key))
                .and_then(Json::as_bool)
        };
        context.thumbs = flea::ui_logic_mn1::build::ThumbPrefs {
            network: flag("thumbNetwork") == Some(true),
            phone: flag("thumbPhone") == Some(true),
            usb: flag("thumbUsb") != Some(false),
        };
    }
}

/// Ops.targetIndices on a background menu: the marked rows, else the cursor row of a listing that has one.
pub(super) fn background_count(host: &PaneHost) -> usize {
    if host.selected.is_empty() {
        usize::from(host.pane.held.total > 0)
    } else {
        host.selected.len()
    }
}
