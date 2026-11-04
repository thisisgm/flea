//! Clipboard and Carry share one ordered backend reply pipe.
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    /// The clipboard's claim on one backend event, ahead of every other reader: clip lines, its own paths answers and their refusals; an ended backend drops its watch.
    pub(crate) fn clipboard_receive(
        &mut self,
        side: usize,
        event: &crate::backend::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::backend::Event;
        match event {
            Event::Failed(_) | Event::Drained => {
                self.clipboard_backend_ended(side);
                self.carry_source_ended(side, window, cx);
                false
            }
            Event::Producer(line) => self.clipboard_line(side, line, window, cx),
            Event::Paths(paths) => {
                self.drag_paths(side, paths, window, cx)
                    || self.carry_paths(side, paths, window, cx)
                    || self.clipboard_paths(side, paths, window, cx)
            }
            Event::Refused { operation, .. } if operation == "paths" => {
                self.drag_paths_failed(side, window, cx)
                    || self.carry_paths_failed(side, window, cx)
                    || self.clipboard_paths_failed(side, window, cx)
            }
            _ => false,
        }
    }

    /// A backend clip line; true when the Clipboard took it.
    pub(crate) fn clipboard_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !line.contains(r#""t":"clip""#) {
            return false;
        }
        let line = line.to_owned();
        self.clipboard_run(
            side,
            Box::new(
                move |board, i18n, backend, take, landing, wire, sink, owner| {
                    board.line(i18n, backend, &line, take, landing, wire, sink, owner)
                },
            ),
            window,
            cx,
        )
    }

    /// A paths answer, claimed only while this side's take or Copy As holds the shared slot.
    pub(crate) fn clipboard_paths(
        &mut self,
        side: usize,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.clipboard.board.paths_pending(side as u64) {
            return false;
        }
        let list = paths.to_vec();
        self.clipboard_run(
            side,
            Box::new(move |board, i18n, backend, take, _, wire, sink, owner| {
                (
                    true,
                    board.paths_resolved(i18n, backend, list, take, wire, sink, owner),
                )
            }),
            window,
            cx,
        )
    }

    /// A refused paths request ends the claim, so the next copy is never blocked.
    pub(crate) fn clipboard_paths_failed(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.clipboard.board.paths_pending(side as u64) {
            return false;
        }
        self.clipboard_run(
            side,
            Box::new(move |board, i18n, backend, take, _, wire, sink, owner| {
                (
                    true,
                    board.paths_failed(i18n, backend, take, wire, sink, owner),
                )
            }),
            window,
            cx,
        )
    }

    /// A drained or failed backend loses its watch, its queue and its captured paste; the next backend watches again at first use.
    pub(crate) fn clipboard_backend_ended(&mut self, side: usize) {
        self.clipboard.board.unwatch(side as u64);
        self.clipboard.captured[side] = None;
        if let Some(host) = self.panes[side].as_mut() {
            host.nav.gate.clip_pending = false;
        }
    }
}
