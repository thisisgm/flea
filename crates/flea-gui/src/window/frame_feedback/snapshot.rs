//! Reuse immutable pane facts while the cached List has not drawn again.
use super::{PaneFrame, Presentation};
use std::rc::Rc;

pub(super) struct Cached {
    drawn: u64,
    pane: Rc<PaneFrame>,
}

impl Presentation {
    pub(super) fn snapshot(&self, side: usize, frame: &Rc<PaneFrame>) -> Rc<PaneFrame> {
        let list = frame.list.borrow();
        let mut snapshots = self.snapshots.borrow_mut();
        let cached = &mut snapshots[side];
        if let Some(cached) = cached
            .as_ref()
            .filter(|cached| cached.drawn == list.drawn.frames)
        {
            return cached.pane.clone();
        }
        #[cfg(test)]
        self.snapshot_builds.set(self.snapshot_builds.get() + 1);
        let pane = Rc::new(PaneFrame {
            content: Rc::new(frame.content.painted(&list, frame.ready)),
            clock: frame.clock.clone(),
            observed: frame.observed.clone(),
            list: frame.list.clone(),
            ready: frame.ready,
            loading: frame.loading,
            blank: frame.blank,
        });
        *cached = Some(Cached {
            drawn: list.drawn.frames,
            pane: pane.clone(),
        });
        pane
    }
}
