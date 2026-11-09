use super::Joined;
use crate::{
    g2::file_clipboard::marks::{GridMarks, ViewMarks},
    state::Shell,
};
use std::rc::Rc;

impl Joined {
    /// The marks Columns tracks read this frame; None while the clipboard is empty, so a track asks nothing.
    pub fn column_marks(&self) -> Option<Rc<dyn crate::columns::paint::ClipMarks>> {
        let (_, marks) = self.marks.as_ref()?;
        (!marks.is_empty()).then(|| Rc::clone(marks) as Rc<dyn crate::columns::paint::ClipMarks>)
    }
}

impl Shell {
    /// Once per frame: a new selection revision builds one mark set, and a side whose folder or Grid changed rescopes; a still frame compares and returns.
    pub(crate) fn sync_clip_marks(&mut self) {
        let revision = self.clipboard.board.revision();
        if self
            .clipboard
            .marks
            .as_ref()
            .is_none_or(|(seen, _)| *seen != revision)
        {
            let marks = ViewMarks::new(revision, self.clipboard.board.current());
            self.clipboard.marks = Some((revision, Rc::new(marks)));
        }
        let Some((_, marks)) = self.clipboard.marks.clone() else {
            return;
        };
        for side in 0..2 {
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let grid = host.grid.is_some();
            let drawn = &mut self.clipboard.drawn[side];
            if drawn.as_ref().is_some_and(|(seen, path, had)| {
                *seen == revision && *path == host.path && *had == grid
            }) {
                continue;
            }
            let fresh = drawn.as_ref().is_none_or(|(seen, _, _)| *seen != revision);
            {
                let mut list = host.list.borrow_mut();
                if fresh {
                    let current = self.clipboard.board.current();
                    list.view.clip.set(&current.paths, current.moving);
                }
                list.view.clip.scope(&host.path);
            }
            if let Some(grid) = host.grid.as_mut() {
                GridMarks::lend(grid, Rc::clone(&marks), host.path.clone());
            }
            *drawn = Some((revision, host.path.clone(), grid));
        }
    }
}
