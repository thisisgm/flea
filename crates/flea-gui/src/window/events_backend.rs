use crate::{
    backend::{Event as BackendEvent, Failure},
    state::Shell,
};
use gpui::{Context, Window};
use std::io::Write;

impl Shell {
    pub(super) fn backend_line(
        &mut self,
        side: usize,
        event: BackendEvent,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        // One unreadable line is logged and refused as v0.3.12's "parse" failure: the footer says it, a listing out still lands.
        match event {
            BackendEvent::Garbled(error) => {
                let diagnostic = self.locale.catalog.tr_args(error.key, &[&error.detail]);
                let _ = writeln!(std::io::stderr().lock(), "flea-gui: {diagnostic}");
                if self.trash.as_ref().is_some_and(|host| {
                    host.backend_side == side && host.pane.read(cx).view().opened
                }) {
                    let text = flea::ui_logic_fb1::errors::sentence("parse", "", "")
                        .render(&self.locale.catalog);
                    self.trash_say(&text, true, cx);
                    return None;
                }
                Some(BackendEvent::Refused {
                    operation: "parse".into(),
                    path: String::new(),
                    message: "the backend sent a line this build cannot read".into(),
                    mode: 0,
                })
            }
            event => Some(event),
        }
    }

    pub(super) fn backend_failed(
        &mut self,
        side: usize,
        error: Failure,
        rename_out: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        // A backend that never listed leaves the path bar empty, as v0.3.12 writes the path only from an answer.
        let never_listed = host.pane.greatest_listing == 0;
        if host.pane.fail("backend", "", "", 0, host.pane.now()) {
            let refusal = crate::window::surfaces::Refusal {
                operation: "backend".into(),
                message: String::new(),
                mode: 0,
            };
            if rename_out {
                self.surfaces[side].refuse_saying(refusal, "jobs.rename.stopped");
            } else {
                self.surfaces[side].refuse(refusal);
            }
        }
        host.input.queued = None;
        host.changed(&self.locale);
        host.record_error(error, &self.locale);
        if never_listed {
            self.path_labels[side] = Default::default();
        }
        self.chrome.jumps[side] = 0;
        self.retire_status_owner(side, window, cx);
        if !rename_out {
            self.status_refused("backend", "", "", cx);
        }
    }
}
