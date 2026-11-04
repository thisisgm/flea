//! Platform quit: every pane backend is told to end before the window lets go of it.
use crate::state::Shell;

impl Shell {
    // Platform quit closes each backend's stdin and lets go of it, so its EOF cleanup outlives this process.
    pub(crate) fn stop_for_app_quit(&mut self) {
        self.plugins.input = None;
        self.lifecycle.begin_close();
        self.stop_tabs();
        self.stop_settings();
        self.presentation.close();
        self._ipc_reader = None;
        self._locale_reader = None;
        self.locale_service = None;
        self._presentation_reader = None;
        self.presentation_service = None;
        self.deadlines = [None, None];
        for host in self.panes.iter_mut().flatten() {
            if let Some(columns) = &mut host.columns {
                columns.timer = None;
            }
            host.transition = None;
            host.pane.swap.mark_shown(false, host.pane.now());
            host.reader = None;
            host.stopped = true;
            if let Some(backend) = host.backend.take() {
                backend.close_stdin();
            }
        }
    }
}
