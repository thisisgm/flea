//! Publish a complete Trash opening once; a slow first reply uses the shared held-snapshot and loading-minimum clocks.
use super::presentation::Stage;
use crate::state::Shell;
use gpui::{Context, Window};
use std::time::Duration;

impl Shell {
    pub(super) fn begin_trash_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.trash.as_mut() else {
            return;
        };
        let now = host.presentation.now(cx.background_executor().now());
        let held = self.panes[host.side]
            .as_ref()
            .is_some_and(|pane| pane.pane.state.kind != crate::pane::Kind::Loading);
        host.pane.update(cx, |pane, _| {
            pane.paint.borrow_mut().invalidate_surface();
        });
        host.presentation.begin(held, now);
        if host.presentation.feedback.is_none() {
            // Wayland holds its callback borrow through draw; a foreground task runs after that dispatch returns.
            cx.spawn_in(window, async move |shell, cx| {
                let _ = shell.update_in(cx, |shell, window, cx| {
                    shell.observe_trash_presentation(window, cx);
                });
            })
            .detach();
        }
    }

    fn observe_trash_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.trash.as_mut() else {
            return;
        };
        if host.presentation.feedback.is_some() {
            return;
        }
        #[cfg(test)]
        super::trashview_tests::record_registration(window, cx);
        let shell = cx.entity().downgrade();
        host.presentation.feedback = Some(window.observe_frame_presented(cx, move |window, cx| {
            let Some(frames) = window.presented_frames() else {
                return;
            };
            let _ = shell.update(cx, |shell, _| {
                if let Some(host) = &mut shell.trash {
                    host.presentation.presented(frames.presented);
                }
            });
        }));
        // Feedback may have advanced before the registration task ran.
        if let Some(frames) = window.presented_frames() {
            host.presentation.presented(frames.presented);
        }
    }

    pub(crate) fn trash_frame(&mut self, window: &Window, cx: &Context<Self>) {
        let Some(frames) = window.presented_frames() else {
            return;
        };
        let Some(host) = &mut self.trash else {
            return;
        };
        let pane = host.pane.read(cx);
        let paint = pane.paint.borrow();
        let row = paint.board.row_h;
        let top = (paint.scroll_y / row).floor() as usize;
        let end = ((paint.scroll_y + pane.view().height) / row).ceil() as usize;
        let blank = (top..end.min(paint.total)).any(|index| paint.line(index).is_none());
        host.presentation
            .frame(frames.committed + 1, blank, paint.source_request);
    }

    pub(crate) fn sync_trash_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &mut self.trash else {
            return;
        };
        let pane = host.pane.clone();
        let now = host.presentation.now(cx.background_executor().now());
        let view = pane.read(cx).view();
        if !view.opened {
            host.presentation.close(now);
            self.slots.covers[host.side] = None;
            pane.update(cx, |pane, _| {
                pane.paint.borrow_mut().invalidate_surface();
                pane.presentation_loading = false;
                pane.loading_at = None;
            });
            self.sync_trash_message(cx);
            return;
        }
        if host.presentation.stage == Stage::Closed {
            host.presentation.ready(now);
        }
        // A window read keeps the drawn rows, so only a listing opens the hold that can fall back to Reading Trash…
        if host.presentation.stage == Stage::Ready
            && matches!(view.pending_op.as_str(), "list" | "summary")
        {
            host.presentation.begin(true, now);
            host.presentation.held_trash = true;
        }
        // A window read over held rows draws whole already, so it never keeps a hold from publishing.
        let ready = !view.busy() || (view.pending_op == "window" && view.covers_viewport());
        let loading_at = pane.read(cx).loading_at;
        if let Some(at) = loading_at {
            host.presentation
                .swap
                .mark_shown(true, host.presentation.now(at));
        }
        let mut wait = None;
        match host.presentation.stage {
            Stage::Holding if ready => host.presentation.ready(now),
            Stage::Holding => {
                let hold = if host.presentation.swap.holding {
                    crate::pane::swap::CAP_MS
                } else {
                    crate::columns::preview_host::HOLD_MS
                };
                let left = (host.presentation.swap.since + hold).saturating_sub(now);
                if left > 0 {
                    wait = Some(left);
                } else {
                    host.presentation.swap.expire(now);
                    host.presentation.stage = Stage::Loading;
                }
            }
            Stage::Loading if ready => {
                let left = host.presentation.swap.show_left(now);
                if left > 0 {
                    wait = Some(left);
                } else {
                    host.presentation.ready(now);
                }
            }
            _ => {}
        }
        let loading = host.presentation.stage == Stage::Loading;
        let covered = (host.side == 0 || self.view == crate::window::startup::View::Dual)
            && (matches!(host.presentation.stage, Stage::Loading | Stage::Ready)
                || (host.presentation.stage == Stage::Holding && host.presentation.held_trash));
        pane.update(cx, |pane, _| {
            if loading || !covered {
                pane.paint.borrow_mut().invalidate_surface();
            }
            if !loading {
                pane.loading_at = None;
            }
            pane.presentation_loading = loading;
        });
        self.slots.covers[host.side] = covered.then(|| pane.into());
        host.presentation.timer = wait.map(|wait| {
            cx.spawn_in(window, async move |shell, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(wait))
                    .await;
                let _ = cx.update(|window, cx| {
                    shell.update(cx, |shell, cx| {
                        shell.flush_trash(window, cx);
                        cx.notify();
                    })
                });
            })
        });
        self.sync_trash_message(cx);
    }
}
