//! The status bar's retry-selection line: a transfer's failed sources, marked again in the folder they stayed in.
use crate::{backend::Located, state::Shell};
use gpui::{Context, Window};

// The line and the selection it was written for; the line goes once the path, marks or cursor move, as PaneWire.qml.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryLine {
    pub text: String,
    pub path: String,
    pub selected: Vec<usize>,
    pub cursor: usize,
}

impl super::pane::PaneHost {
    pub fn retry_text(&self) -> String {
        let mut line = self.retry_line.borrow_mut();
        if line.as_ref().is_some_and(|line| {
            line.path != self.path
                || line.selected != self.selected
                || line.cursor != self.pane.cursor
        }) {
            *line = None;
        }
        line.as_ref()
            .map_or_else(String::new, |line| line.text.clone())
    }
}

impl Shell {
    pub(crate) fn retry_failed(
        &mut self,
        side: usize,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        host.nav.retry_failed(&host.pane, paths);
        self.run_navigation(side, window, cx);
    }

    // PaneWire.qml onTransferDone: the folder the transfer finished in is read again, even when nothing else asked for it.
    pub(crate) fn transfer_refresh(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        let _ = host.nav.refresh_after_transfer(&mut host.pane);
        self.run_navigation(side, window, cx);
    }

    // True when the reply answered the retry ask, whether or not anything was still there to mark.
    pub(crate) fn retry_located(
        &mut self,
        side: usize,
        located: &Located,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        let Some(id) = located.id else {
            return false;
        };
        let matches = if located.ok {
            located.matches.as_slice()
        } else {
            &[]
        };
        let Some(paths) = host.nav.take_retry(
            &mut host.pane,
            &located.directory,
            id,
            matches,
            &mut host.selected,
        ) else {
            return false;
        };
        if !paths.is_empty() {
            host.input.selection.sync(&host.selected);
            let named: Vec<&str> = paths.iter().map(String::as_str).collect();
            *host.retry_line.borrow_mut() = Some(RetryLine {
                text: flea::ui_logic_op1::lines::retry_selection_line(&self.locale.catalog, &named),
                path: host.path.clone(),
                selected: host.selected.clone(),
                cursor: host.pane.cursor,
            });
            cx.notify();
        }
        self.run_navigation(side, window, cx);
        true
    }
}
