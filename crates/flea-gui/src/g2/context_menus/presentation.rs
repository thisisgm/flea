//! Menu preparation keeps the complete listing visible until one fully shaped card can replace it.
use super::{element::MenuInk, menu::Face};
use crate::{feedback::spinner::Crawl, list::Palette, palette::Theme, state::Shell};
use flea::ui_logic_context_menus::fit::Rect;
use gpui::{Context, Subscription, Task, Window};
use std::{
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

const HOLD_MS: u64 = 150;

#[derive(Default)]
pub struct Presentation {
    pub at: Option<(f32, f32)>,
    pub loading: bool,
    pub mark: Rc<Crawl>,
    since: Option<Instant>,
    timer: Option<Task<()>>,
    observer: Option<Subscription>,
    frames: VecDeque<Frame>,
    last: Option<Frame>,
    reported: u64,
    pub counts: Counts,
}

#[derive(Default)]
pub struct Counts {
    pub available: bool,
    pub holds: u64,
    pub fallbacks: u64,
    pub blank: u64,
    pub loading: u64,
    pub blink_backs: u64,
    pub clock_id: Option<u32>,
    last_sequence: u64,
    last_loading: bool,
    last_complete: u64,
}

#[derive(Clone)]
struct Frame {
    committed: u64,
    sequence: u64,
    active: bool,
    holding: bool,
    loading: bool,
    blank: bool,
}

impl Presentation {
    pub fn cancel(&mut self) {
        self.at = None;
        self.loading = false;
        self.since = None;
        self.timer = None;
        self.mark.show(false, Default::default());
    }
}

impl Shell {
    pub(crate) fn observe_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A host that builds its card before this call, as the rail's does, calls keycaps::watch itself first.
        super::keycaps::watch(cx);
        if self.menu.presentation.observer.is_none() {
            let weak = cx.entity().downgrade();
            self.menu.presentation.observer =
                Some(window.observe_frame_presented(cx, move |window, cx| {
                    let _ = weak.update(cx, |shell, cx| shell.menu_presented(window, cx));
                }));
        }
    }

    pub(crate) fn arm_menu_loading(&mut self, window: &Window, cx: &mut Context<Self>) {
        let id = self.menu.sequence;
        self.menu.presentation.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(HOLD_MS))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell.menu.sequence != id || shell.menu.presentation.at.is_none() {
                    return;
                }
                let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
                let palette = Palette::inherited(&window.text_style(), theme);
                shell.menu.presentation.loading = true;
                shell
                    .menu
                    .presentation
                    .mark
                    .show(true, MenuInk::from_palette(&palette).muted);
                cx.notify();
            });
        }));
    }

    pub(crate) fn refresh_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = &self.menu.captured else {
            return;
        };
        if capture.identity != self.menu_identity()
            || self.panes[capture.side]
                .as_ref()
                .is_none_or(|host| host.pane.busy() || host.stopped)
        {
            self.menu_refuse("menu.refusal.moved", cx);
            self.retire_menu(window, cx);
            return;
        }
        if (!capture.background && !capture.ready)
            || self.menu.reading
            || !self.menu.context.open_with_loaded
            || self.menu.tags_reading
        {
            return;
        }
        let entries = self.menu_entries();
        if entries.is_empty() {
            self.menu_refuse("menu.refusal.unavailable", cx);
            self.retire_menu(window, cx);
            return;
        }
        let Some(at) = self.menu.presentation.at else {
            return;
        };
        if self.menu.presentation.loading {
            let Some(since) = self.menu.presentation.since else {
                return;
            };
            let left = Duration::from_millis(crate::pane::swap::MIN_SHOW_MS).saturating_sub(
                cx.background_executor()
                    .now()
                    .saturating_duration_since(since),
            );
            if !left.is_zero() {
                let id = capture.id;
                self.menu.presentation.timer = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(left).await;
                    let _ = this.update_in(cx, |shell, window, cx| {
                        if shell.menu.sequence == id {
                            shell.refresh_menu(window, cx);
                        }
                    });
                }));
                return;
            }
        }
        let list = self.panes[capture.side]
            .as_ref()
            .expect("the menu's pane")
            .list
            .borrow();
        let env = self.menu_env(&list.typeface);
        let viewport = window.viewport_size();
        self.menu.menu.borrow_mut().open(
            if self.menu.header {
                Face::Header
            } else if capture.background && self.locked_folder(capture.side).is_some() {
                Face::Locked
            } else if capture.background {
                Face::Background
            } else {
                Face::File
            },
            entries,
            at,
            Rect {
                x: 0.,
                y: 0.,
                w: viewport.width.into(),
                h: viewport.height.into(),
            },
            &capture.identity,
            &env,
            window,
        );
        drop(list);
        self.menu.presentation.cancel();
        self.flyout_pending(window, cx);
        cx.notify();
    }

    // Each queued sample names the submitted buffer; discarded commits never count as presented.
    pub(crate) fn menu_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.presentation.observer.is_none() {
            return;
        }
        let active = self.menu.captured.is_some() || self.menu_opened();
        let sequence = self.menu.sequence;
        let preparing = self.menu.presentation.at.is_some();
        let loading = self.menu.presentation.loading;
        let menu = self.menu.menu.borrow();
        let blank = menu
            .card
            .as_ref()
            .is_some_and(|card| card.built.lines.len() != card.entries.len());
        drop(menu);
        if let Some(frames) = window.presented_frames() {
            let samples = &mut self.menu.presentation.frames;
            let committed = frames.committed + 1;
            if samples
                .back()
                .is_some_and(|sample| sample.committed == committed)
            {
                samples.pop_back();
            }
            if samples.len() == 64 {
                samples.pop_front();
            }
            samples.push_back(Frame {
                committed,
                sequence,
                active,
                holding: preparing && !loading,
                loading,
                blank,
            });
            return;
        }
        if !loading || self.menu.presentation.since.is_some() {
            return;
        }
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |shell, cx| {
                if loading
                    && shell.menu.sequence == sequence
                    && shell.menu.presentation.since.is_none()
                {
                    shell.menu.presentation.since = Some(cx.background_executor().now());
                    shell.refresh_menu(window, cx);
                }
            });
        });
    }

    fn menu_presented(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(frames) = window.presented_frames() else {
            return;
        };
        let report = &mut self.menu.presentation;
        while report
            .frames
            .front()
            .is_some_and(|sample| sample.committed <= frames.presented)
        {
            report.last = report.frames.pop_front();
        }
        let Some(sample) = report.last.as_ref() else {
            return;
        };
        if frames.presented <= report.reported {
            return;
        }
        report.reported = frames.presented;
        if !sample.active {
            report.counts.last_loading = false;
            return;
        }
        report.counts.available = true;
        report.counts.clock_id = Some(frames.clock_id);
        report.counts.holds += u64::from(sample.holding);
        report.counts.fallbacks += u64::from(sample.loading && !report.counts.last_loading);
        report.counts.loading += u64::from(sample.loading);
        report.counts.blank += u64::from(sample.blank);
        report.counts.blink_backs += u64::from(
            sample.sequence < report.counts.last_sequence
                || ((sample.holding || sample.loading)
                    && sample.sequence == report.counts.last_complete),
        );
        report.counts.last_sequence = sample.sequence;
        report.counts.last_loading = sample.loading;
        if !sample.holding && !sample.loading {
            report.counts.last_complete = sample.sequence;
        }
        let first_loading =
            sample.loading && self.menu.sequence == sample.sequence && report.since.is_none();
        if first_loading {
            report.since = Some(cx.background_executor().now());
            self.refresh_menu(window, cx);
        }
    }
}
