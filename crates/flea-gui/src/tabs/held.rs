//! A drag holds its tab by identity, and the strip counts its own held and swapped frames.
use crate::state::Shell;
use std::cell::Cell;

// Caps the submitted frames that still wait for their terminal feedback.
const PENDING_FRAMES: usize = 64;

impl Shell {
    /// Resolve the held tab's identity to its current slot, so a close or a receive during the drag never moves another tab.
    pub(crate) fn resolve_held_tab(&mut self) {
        self.with_tabs(|bar, panes| {
            if bar.drag_from.is_some() || bar.out_active {
                bar.resolve_drag(panes);
            }
        });
    }
}

/// Completed strip metadata is joined to compositor terminal feedback by submission identity.
#[derive(Clone, Copy, Default)]
pub(crate) struct Painted {
    pub draw: u64,
    pub version: u64,
    pub holding: bool,
    pub complete: bool,
}

#[derive(Default)]
pub(crate) struct StripFrames {
    pub observer: Option<gpui::Subscription>,
    pub staged: Cell<Option<Painted>>,
    pending: std::collections::VecDeque<(u64, Painted)>,
    retained: Option<Painted>,
    pub version: u64,
    pub holding: bool,
    pub visible: bool,
    holds: usize,
    frames: usize,
    blank: usize,
    held: usize,
    swapped: usize,
    blink_backs: usize,
    last_id: u64,
    last_version: u64,
    held_presented: bool,
    lost: usize,
}

impl StripFrames {
    pub fn follow(&mut self, holding: bool) {
        self.holds += usize::from(holding && !self.holding);
        self.holding = holding;
    }

    pub fn feedback(&mut self, event: gpui::FrameFeedbackEvent) {
        match event {
            gpui::FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                let painted = self.staged.take().or(self.retained);
                let Some(mut painted) = painted else {
                    self.lost += usize::from(self.visible);
                    return;
                };
                if painted.draw != draw_id {
                    if painted.draw > draw_id || !self.visible || painted.version != self.version {
                        self.lost += usize::from(self.visible);
                        return;
                    }
                    // The cached strip keeps its painted version while another root layer redraws.
                    painted.draw = draw_id;
                    painted.holding = self.holding;
                }
                self.retained = Some(painted);
                if self.pending.len() == PENDING_FRAMES {
                    self.pending.pop_front();
                    self.lost += 1;
                }
                self.pending.push_back((frame_id, painted));
            }
            gpui::FrameFeedbackEvent::Terminal(terminal) => {
                let Some(at) = self
                    .pending
                    .iter()
                    .position(|(id, _)| *id == terminal.frame_id)
                else {
                    return;
                };
                let (id, painted) = self.pending.remove(at).unwrap();
                if !matches!(terminal.outcome, gpui::FrameOutcome::Presented(_)) {
                    return;
                }
                self.frames += 1;
                self.blank += usize::from(!painted.complete);
                self.held += usize::from(painted.holding);
                if id <= self.last_id {
                    self.lost += 1;
                    return;
                }
                self.blink_backs += usize::from(painted.version < self.last_version);
                self.swapped +=
                    usize::from(self.held_presented && !painted.holding && painted.complete);
                self.held_presented = painted.holding;
                self.last_id = id;
                self.last_version = painted.version;
            }
        }
    }

    pub fn close(&mut self) {
        self.observer = None;
        self.staged.take();
        self.pending.clear();
        self.retained = None;
    }

    pub fn json(&self) -> String {
        format!(
            "{{\"holds\":{},\"fallbacks\":0,\"blankFrames\":{},\"loadingFrames\":0,\"blinkBacks\":{},\"heldFrames\":{},\"swappedFrames\":{},\"presentedFrames\":{},\"lostMetadata\":{},\"available\":{}}}",
            self.holds, self.blank, self.blink_backs, self.held, self.swapped, self.frames, self.lost, self.observer.is_some() && self.lost == 0
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn u25_missing_strip_draw_metadata_marks_counters_unavailable() {
        let mut frames = super::StripFrames {
            visible: true,
            ..Default::default()
        };
        frames.feedback(gpui::FrameFeedbackEvent::Submitted {
            draw_id: 7,
            frame_id: 8,
        });
        let counters = frames.json();
        assert!(counters.contains("\"lostMetadata\":1"), "{counters}");
        assert!(counters.contains("\"available\":false"), "{counters}");
        assert!(counters.contains("\"presentedFrames\":0"), "{counters}");
    }

    #[gpui::test]
    fn u25_cached_strip_draws_join_actual_root_submissions_and_discard(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::{
            backend::Backend,
            state::{tests::shell_with, Shell},
            tabs::state::{Tab, Tabs},
            window::root::Root,
        };
        use gpui::{
            AppContext, FrameOutcome, FrameTerminal, PresentationTime, RequestFrameOptions,
        };
        let (backend, _requests) = Backend::test_channel();
        let (root, cx) = cx.add_window_view(|_, cx| Root {
            shell: cx.new(|cx| {
                let mut shell = shell_with(backend, cx);
                shell.strip.tabs[0].tabs = Some(Tabs {
                    items: vec![
                        Tab {
                            identity: 1,
                            ..Tab::at("/files")
                        },
                        Tab {
                            identity: 2,
                            ..Tab::at("/other")
                        },
                    ],
                    index: 0,
                    pending: Default::default(),
                });
                shell
            }),
        });
        let shell = root.read_with(cx, |root, _| root.shell.clone());
        cx.enable_frame_feedback();
        cx.update(|_, cx| {
            Shell::observe_empty_omi_shell_renders(cx);
            shell.update(cx, |_, cx| cx.notify());
        });
        cx.run_until_parked();
        let submit = |cx: &mut gpui::VisualTestContext| {
            cx.simulate_frame_request(RequestFrameOptions {
                require_presentation: true,
                ..Default::default()
            });
            cx.run_until_parked();
            cx.update(|window, _| window.presented_frames().unwrap().committed)
        };
        let presented = || {
            FrameOutcome::Presented(PresentationTime {
                timestamp: None,
                clock_id: None,
            })
        };
        let first = submit(cx);
        let draw = shell.read_with(cx, |shell, _| {
            let frames = shell.strip.frames.borrow();
            assert!(frames.observer.is_some());
            assert_eq!(
                frames.frames, 0,
                "submission must not count as presentation"
            );
            frames
                .pending
                .iter()
                .find(|(id, _)| *id == first)
                .unwrap()
                .1
                .draw
        });
        cx.simulate_frame_terminal(FrameTerminal {
            frame_id: first,
            outcome: presented(),
        });
        cx.run_until_parked();
        let renders = cx.update(|_, cx| Shell::empty_omi_shell_renders(cx));
        root.update(cx, |_, cx| cx.notify());
        let second = submit(cx);
        shell.read_with(cx, |shell, _| {
            let frames = shell.strip.frames.borrow();
            let next = frames
                .pending
                .iter()
                .find(|(id, _)| *id == second)
                .unwrap()
                .1
                .draw;
            assert_ne!(next, draw, "the root submitted a new draw");
            assert_eq!(frames.frames, 1);
            assert_eq!(
                frames.lost, 0,
                "the unchanged painted strip survives a cached-shell draw"
            );
        });
        assert_eq!(
            cx.update(|_, cx| Shell::empty_omi_shell_renders(cx)),
            renders,
            "Shell stayed cached"
        );
        cx.simulate_frame_terminal(FrameTerminal {
            frame_id: second,
            outcome: FrameOutcome::Discarded,
        });
        cx.run_until_parked();
        let third = submit(cx);
        cx.simulate_frame_terminal(FrameTerminal {
            frame_id: third,
            outcome: presented(),
        });
        cx.run_until_parked();
        shell.read_with(cx, |shell, _| {
            let frames = shell.strip.frames.borrow();
            assert_eq!(frames.frames, 2, "only the two presented receipts count");
            assert_eq!(frames.lost, 0);
            assert_eq!(frames.blank, 0);
            assert!(frames.json().contains("\"available\":true"));
        });
    }
}
