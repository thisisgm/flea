//! Presented scene receipts and on-demand viewport geometry for native capture clients.
pub(crate) mod contract;
#[cfg(test)]
mod corner_tests;
pub(crate) mod geometry;
mod ipc;
#[cfg(test)]
mod tests;

use crate::{ipc::server::Answer, state::Shell, window::startup::View};
use flea::{ipc::wire::Reply, jsondoc::Json};
pub(crate) use geometry::Probe;
use gpui::{App, Context, PresentedFrames, Subscription, Task, Window};
use std::sync::mpsc::SyncSender;

pub(crate) const FRAME_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);

pub(crate) struct Pending {
    pub spec: contract::Spec,
    pub answer: SyncSender<Answer>,
}

#[derive(Default)]
pub(crate) struct Visual {
    /// Explicit scene rounding stays authoritative for this window until another scene replaces it.
    pub scene_corner: Option<f32>,
    pub revision: u64,
    pub receipt: Option<Json>,
    pub pending: Option<Pending>,
    pub probe: Probe,
    pub deadline: Option<Task<()>>,
    pub awaiting: Option<Awaiting>,
    pub presented: Option<Subscription>,
    /// Stands in for the compositor's counts in tests, because the GPUI test window reports none.
    #[cfg(test)]
    pub compositor: Option<PresentedFrames>,
}

/// An eligible frame whose present was submitted, waiting for the compositor to report it presented.
pub(crate) struct Awaiting {
    pub committed: u64,
    pub revision: u64,
    pub live: contract::Live,
    pub probe: Probe,
}

pub(crate) fn view_name(view: View) -> &'static str {
    match view {
        View::List => "list",
        View::Columns => "columns",
        View::Grid => "grid",
        View::Dual => "dual",
    }
}

pub(crate) fn object(pairs: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::Obj(
        pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

pub(crate) fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}

impl Shell {
    /// The frames the compositor reported committed and presented, `None` where it reports none.
    pub(crate) fn presented_frames(&self, window: &Window) -> Option<PresentedFrames> {
        #[cfg(test)]
        if let Some(frames) = self.visual.compositor {
            return Some(frames);
        }
        window.presented_frames()
    }

    /// Work whose answer still draws a frame: the shown panes' requests and the path bar's asks and history reads.
    pub(crate) fn pending_requests(&self) -> usize {
        self.panes
            .iter()
            .enumerate()
            .filter(|(side, _)| *side == 0 || self.view == View::Dual)
            .filter_map(|(_, host)| host.as_ref())
            .map(|host| host.pane.pending() + usize::from(host.error.is_some() || host.stopped))
            .sum::<usize>()
            + self.chrome.waiting
            + self.chrome.jumps.iter().sum::<usize>()
    }

    pub(crate) fn visual_state(&self, window: &Window) -> Json {
        let host = self.panes[self.focus_side].as_ref();
        object([
            ("window", number(std::process::id())),
            ("view", Json::Str(view_name(self.view).into())),
            (
                "path",
                Json::Str(host.map_or("", |host| host.path.as_str()).into()),
            ),
            ("cursor", number(host.map_or(0, |host| host.pane.cursor))),
            ("pending", number(self.pending_requests())),
            ("preview", Json::Str("absent".into())),
            (
                "frame",
                self.presented_frames(window)
                    .map_or(Json::Null, |frames| number(frames.presented)),
            ),
            ("scale", number(window.scale_factor())),
            ("width", number(f32::from(window.viewport_size().width))),
            ("height", number(f32::from(window.viewport_size().height))),
            ("receipt", self.visual.receipt.clone().unwrap_or(Json::Null)),
            ("presentation", self.presentation.json()),
            (
                "activity",
                self.presentation
                    .activity
                    .json(window, self.presented_frames(window)),
            ),
        ])
    }

    /// The values a receipt reports, read from what the window and the focused pane's rows drew.
    pub(crate) fn visual_live(&self, window: &Window, cx: &App) -> contract::Live {
        let host = self.panes[self.focus_side].as_ref();
        contract::Live {
            scale: window.scale_factor(),
            text_size: host.map_or(0., |host| f32::from(host.list.borrow().typeface.body)),
            row_height: host.map_or(0., |host| host.list.borrow().drawn.metrics.row),
            reduced_motion: cx.reduce_motion(),
            cursor: host.map_or(0, |host| {
                host.list.borrow().drawn_cursor.unwrap_or(host.pane.cursor)
            }),
            selection: host.map_or_else(Vec::new, |host| host.selected.clone()),
        }
    }

    fn visual_answered(&self) -> bool {
        self.panes
            .iter()
            .enumerate()
            .filter(|(side, _)| *side == 0 || self.view == View::Dual)
            .all(|(_, host)| host.as_ref().is_some_and(|host| host.answered))
    }

    /// The focused pane's scene cursor and selection once every shown pane answered; render applies them before the build.
    pub(crate) fn visual_marks(&self) -> Option<(usize, Vec<usize>)> {
        let pending = self.visual.pending.as_ref()?;
        self.visual_answered()
            .then(|| (pending.spec.cursor, pending.spec.selection.clone()))
    }

    /// Scene work runs only while a scene is pending or its receipt is live; other frames return here.
    pub(crate) fn visual_frame(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.visual.pending.is_none() && self.visual.receipt.is_none() {
            return;
        }
        let revision = self.visual.revision;
        let answered = self.visual_answered();
        let live = self.visual_live(window, cx);
        let focused = self.panes[self.focus_side].as_ref();
        // The cursor and selection this render built the rows with, applied before the build by visual_marks.
        let built = focused.map(|host| (host.pane.cursor, &host.selected));
        let eligible = answered
            && self.visual.pending.as_ref().is_some_and(|pending| {
                let size = window.viewport_size();
                let spec = &pending.spec;
                self.pending_requests() == 0
                    && f32::from(size.width) == spec.width as f32
                    && f32::from(size.height) == spec.height as f32
                    && live.text_size == spec.text_size as f32
                    && live.reduced_motion == spec.reduced_motion
                    && built == Some((spec.cursor, &spec.selection))
            });
        let probe = Probe::default();
        if eligible {
            self.visual.probe = probe.clone();
        }
        for host in self.panes.iter().flatten() {
            host.list.borrow_mut().probe = eligible.then(|| probe.clone());
        }
        if !eligible {
            return;
        }
        // The frames committed before this one, so a present that fails is never awaited.
        let before = self.presented_frames(window).map(|frames| frames.committed);
        let weak = cx.entity().downgrade();
        // Native frame callbacks queued during render run after that frame's present submission.
        window.on_next_frame(move |window, cx| {
            if !window.is_visible() {
                return;
            }
            let _ = weak.update(cx, |shell, cx| {
                if revision != shell.visual.revision {
                    return;
                }
                let live = shell.visual_live(window, cx);
                let drawn = shell.panes[shell.focus_side]
                    .as_ref()
                    .and_then(|host| host.list.borrow().drawn_cursor);
                let Some(pending) = shell.visual.pending.as_ref() else {
                    return;
                };
                // A receipt names only a frame whose rows drew the scene's cursor.
                if drawn != Some(pending.spec.cursor) {
                    cx.notify();
                    return;
                }
                // `scene` refuses a window without presentation feedback, so a pending scene always has counts.
                let Some(frames) = shell.presented_frames(window) else {
                    return;
                };
                // A failed present numbers no frame, so the compositor never shows this one; draw again.
                if Some(frames.committed) == before {
                    cx.notify();
                    return;
                }
                shell.visual.awaiting = Some(Awaiting {
                    committed: frames.committed,
                    revision,
                    live,
                    probe,
                });
                shell.visual_presented(window);
            });
        });
    }

    /// Issues the receipt when the compositor reports the awaited frame itself presented, never a later one.
    pub(crate) fn visual_presented(&mut self, window: &Window) {
        let Some(frames) = self.presented_frames(window) else {
            return;
        };
        let ready = self.visual.awaiting.as_ref().is_some_and(|awaiting| {
            awaiting.revision == self.visual.revision && frames.presented == awaiting.committed
        });
        if !ready {
            return;
        }
        self.visual.presented = None;
        let (Some(awaiting), Some(pending)) =
            (self.visual.awaiting.take(), self.visual.pending.take())
        else {
            return;
        };
        let receipt = pending.spec.receipt(frames.presented, &awaiting.live);
        self.visual.probe = awaiting.probe;
        self.visual.receipt = Some(receipt.clone());
        self.visual.deadline = None;
        let _ = pending.answer.try_send(Answer {
            reply: Reply::value(flea::jsondoc::render(&receipt).trim_end()),
            quit: false,
        });
    }
}

/// Gives a pane the scene's cursor, when it holds that row, and its selection.
pub(crate) fn apply_marks(
    host: &mut crate::window::pane::PaneHost,
    cursor: usize,
    selection: &[usize],
) {
    if host.pane.cursor != cursor && cursor < host.pane.held.total {
        host.pane.cursor = cursor;
        host.pane.reveal_cursor();
    }
    if host.selected != selection {
        host.selected = selection.to_vec();
        host.input.selection.sync(&host.selected);
    }
}
