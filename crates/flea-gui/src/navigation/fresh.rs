//! An operation result takes ownership of its folder's unfinished reread.
use super::{Navigator, Refusal};
use crate::pane::{swap::Options, Pane};

impl Navigator {
    pub fn fresh(&mut self, pane: &mut Pane) -> Result<bool, Refusal> {
        self.pending_select = None;
        self.select_locate = None;
        if self
            .anchor
            .take()
            .is_some_and(|anchor| anchor.need_paths.is_some())
        {
            self.gate.paths_pending = false;
        }
        let path = pane.path().to_owned();
        if let Some(pending) = pane
            .swap
            .pending
            .as_mut()
            .filter(|pending| pending.path == path)
        {
            pending.options.in_place = false;
            pending.options.restore_name = None;
            pending.retained_start = None;
            pending.retained_cursor = None;
        }
        let options = Options {
            kept_query: pane.state.query.clone(),
            supersede: true,
            ..Options::default()
        };
        self.open_without_history(pane, &path, options, None, false)
    }
}
