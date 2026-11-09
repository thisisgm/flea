//! A neighbour menu edits the name where that column drew it, using the pinned menu source.
use super::{Edit, Shell};
use crate::{
    columns::{geometry::Slot, TrackRows},
    g2::context_menus::join::Captured,
    path::entry::Entry,
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn rename_menu_target(
        &mut self,
        capture: &Captured,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = &capture.target else { return };
        let Some(host) = self.panes[capture.side].as_ref() else {
            return;
        };
        let source = crate::navigation::history::join(&capture.path, &row.name);
        let mut entry = Entry::default();
        entry.begin(&row.name);
        let cut = row
            .name
            .rfind('.')
            .filter(|&at| at > 0)
            .unwrap_or(row.name.len());
        entry.move_to(0);
        entry.select_to(cut);
        self.rename.neighbour = Some((capture.path.clone(), row.name.clone()));
        self.rename.edit = Some(Edit {
            side: capture.side,
            index: 0,
            listing: host.pane.held.listing,
            source,
            name: row.name.clone(),
            menu: capture.id,
            entry,
            error: String::new(),
            pending: None,
            line: Default::default(),
        });
        self.focus_pane(capture.side, cx);
        window.focus(&self.focus, cx);
        self.rename_sync_gate(window, cx);
        cx.notify();
    }

    pub(crate) fn rename_neighbour_row(&self, side: usize) -> Option<(Slot, usize, bool)> {
        let (base, name) = self.rename.neighbour.as_ref()?;
        let edit = self.rename.edit.as_ref().filter(|edit| edit.side == side)?;
        let host = self.panes[side]
            .as_ref()
            .filter(|host| host.pane.held.listing == edit.listing)?;
        let track = host
            .columns
            .as_ref()?
            .area
            .tracks()
            .iter()
            .find(|track| track.path == *base && track.enabled)?;
        let TrackRows::Peek(rows) = &track.rows else {
            return None;
        };
        rows.iter()
            .enumerate()
            .find(|(_, row)| row.name == *name)
            .map(|(index, row)| (track.slot, index, row.dir))
    }
}

impl Shell {
    pub(super) fn rename_neighbour_landed(
        &mut self,
        side: usize,
        source: &str,
        destination: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let moved = destination.and_then(|destination| {
            let host = self.panes[side].as_ref()?;
            let suffix = std::path::Path::new(&host.path).strip_prefix(source).ok()?;
            let path = if suffix.as_os_str().is_empty() {
                std::path::PathBuf::from(destination)
            } else {
                std::path::Path::new(destination).join(suffix)
            }
            .to_string_lossy()
            .into_owned();
            let select = host
                .pane
                .held
                .row(host.pane.cursor)
                .map(|row| crate::navigation::history::join(&path, &row.name));
            Some((path, select))
        });
        self.rename_sync_gate(window, cx);
        if let Some(columns) = self.panes[side]
            .as_mut()
            .and_then(|host| host.columns.as_mut())
        {
            columns.area.peeks = Default::default();
        }
        // Renaming an ancestor moves the open directory itself; follow its new path and cursor name.
        if let Some((path, select)) = moved {
            if let Some(host) = self.panes[side].as_mut() {
                host.nav.pending_select = select;
            }
            let _ = self.step(
                side,
                crate::window::navigate::Step::OpenPlace(path),
                window,
                cx,
            );
        }
        self.sync_columns(window, cx);
        cx.notify();
    }
}
