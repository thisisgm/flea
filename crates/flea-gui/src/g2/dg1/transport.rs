//! The clipboard family uses the pane backend's existing ordered pipe.
use super::seams::Wire;
use crate::backend::Backend;

// The pane backend's ordered pipe; a stopped backend takes nothing.
pub(crate) struct PaneWire<'a>(pub(crate) Option<&'a Backend>);

impl Wire for PaneWire<'_> {
    fn send(&self, line: String) -> bool {
        self.0.is_some_and(|backend| backend.line(line).is_ok())
    }
}
