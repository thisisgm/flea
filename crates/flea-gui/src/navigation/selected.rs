//! A selected file outside the retained window follows AnchorHold's tagged locate and landing.
use super::{history::join, Command, Navigator};
use crate::pane::Pane;

fn target_path(pane: &Pane, target: &str) -> String {
    if target.starts_with('/') {
        target.to_owned()
    } else {
        join(pane.path(), target)
    }
}

impl Navigator {
    pub(super) fn locate_select(&mut self, pane: &Pane, target: &str) {
        if self
            .select_locate
            .is_some_and(|(listing, _)| listing == pane.held.listing)
        {
            return;
        }
        let id = self.next_locate();
        self.select_locate = Some((pane.held.listing, id));
        self.emit(Command::LocateMany {
            paths: vec![target_path(pane, target)],
            id,
        });
    }

    pub(crate) fn take_selected(
        &mut self,
        pane: &mut Pane,
        directory: &str,
        id: u64,
        ok: bool,
        matches: &[(String, usize)],
        selected: &mut Vec<usize>,
    ) -> bool {
        if self.select_locate != Some((pane.held.listing, id)) || directory != pane.path() {
            return false;
        }
        let Some(target) = self.pending_select.as_ref() else {
            return false;
        };
        let path = target_path(pane, target);
        let found = matches
            .iter()
            .find(|(name, at)| ok && name == &path && *at < pane.held.total);
        let Some((_, at)) = found else {
            self.pending_select = None;
            self.select_locate = None;
            return true;
        };
        pane.cursor = *at;
        pane.reveal_cursor();
        if self.keep_screen_line {
            self.lines.restore(pane, super::position::leaf(&path));
        }
        selected.clear();
        selected.push(*at);
        self.landed = Some((pane.held.listing, *at));
        if let Some(request) = pane.anchor_window(*at) {
            self.emit(Command::Window {
                start: request.start,
                count: request.count,
            });
        }
        self.landing_select(pane, selected);
        true
    }
}
