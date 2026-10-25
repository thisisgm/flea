//! The menu facts read at draw time: the rows Settings hides, the backend's archive answer and the Update Flea hint.
use super::join::flag;
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
        inventory::effective_hidden(
            stored.as_deref(),
            self.launch
                .settings
                .data
                .get("shelf")
                .is_some_and(|s| flag(s, "enabled")),
        )
    }

    /// The facts a formats reply or an update check may change while the menu is open.
    pub(super) fn menu_live_facts(&self, side: usize, context: &mut MenuContext) {
        context.archive_formats = self.output.archive[side].clone().unwrap_or_default();
        context.can_extract = flea::ui_logic_mn1::archive::can_extract(
            &context.archive_name,
            self.output.extract[side],
        );
        context.update_version = self.menu_update_version();
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
