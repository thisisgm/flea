use super::{Band, Frame};
mod delta;
use crate::{list::pane::Pane, state::Shell};
use gpui::{Context, Window};
use std::time::{Duration, Instant};

pub const EDGE_RATE: f32 = 3. * 24. * 4.;
const ROW_DRAG_BAND: f32 = 37.;
const ROW_DRAG_STEP: Duration = Duration::from_millis(100);

/// Visits entering and leaving cells only, returning their count for both callers and work tests.
pub fn update_band(
    frame: Frame,
    band: &mut Band,
    selected: &mut Vec<usize>,
    shown: Option<&[usize]>,
) -> usize {
    let mut touched = 0;
    let (x, y) = frame.local(band.pointer);
    band.direction = if y < 0. {
        -1.
    } else if y > frame.viewport() {
        1.
    } else {
        0.
    };
    band.end = (
        x.clamp(0., f32::from(frame.bounds.size.width)),
        y.clamp(0., frame.viewport()) + frame.scroll,
    );
    if !band.active {
        return 0;
    }
    let basis = (frame.columns.max(1), frame.total);
    if band.basis != Some(basis) {
        // The first update, or a renumbered view: the selection restarts from the marks the band began over.
        band.before.sort_unstable();
        band.before.dedup();
        selected.clear();
        if band.additive {
            selected.extend_from_slice(&band.before);
        }
        band.cells = None;
        band.basis = Some(basis);
    }
    let (left, right) = (band.anchor.0.min(band.end.0), band.anchor.0.max(band.end.0));
    let (top, bottom) = (band.anchor.1.min(band.end.1), band.anchor.1.max(band.end.1));
    let mut cells = None;
    if left < right
        && left < f32::from(frame.bounds.size.width) - frame.lane
        && right > 0.
        && top < bottom
    {
        let first = frame.grow.position_at(top - frame.cell_origin.1, frame.row);
        let end = frame.grow.end_at(bottom - frame.cell_origin.1, frame.row);
        let col_first = ((left - frame.cell_origin.0) / frame.cell_width.max(1.))
            .floor()
            .max(0.) as usize;
        let col_end = ((right - frame.cell_origin.0) / frame.cell_width.max(1.))
            .ceil()
            .max(0.) as usize;
        let cols = frame.columns.max(1);
        let col_first = col_first.min(cols);
        let col_end = col_end.min(cols);
        let row_end = end.min(frame.total.div_ceil(cols));
        if first < row_end && col_first < col_end {
            cells = Some((col_first, col_end, first, row_end));
        }
    }
    touched += delta::apply(frame, band, selected, shown, cells);
    band.cells = cells;
    touched
}

impl super::PaneInput {
    /// SelectionBand.qml:28-30 and :122-128 step the edge scroll per presented frame by that frame's own time.
    pub fn advance(&mut self, now: Instant, pane: &mut Pane, selected: &mut Vec<usize>) {
        let last = self.stepped_at.take();
        let Some(mut frame) = self.frame else {
            return;
        };
        let Some(band) = self
            .band
            .as_mut()
            .filter(|band| band.active && band.direction != 0.)
        else {
            return;
        };
        self.stepped_at = Some(now);
        let seconds = last.map_or(0., |at| {
            now.saturating_duration_since(at)
                .min(Duration::from_millis(100))
                .as_secs_f32()
        });
        pane.scroll_to(
            (pane.scroll_y + band.direction * EDGE_RATE * seconds).clamp(0., frame.max_scroll()),
        );
        frame.scroll = pane.scroll_y;
        let before = selected.len();
        let touched = update_band(frame, band, selected, self.shown.as_deref());
        if touched > 0 || before != selected.len() {
            self.selection.changed(selected);
        }
        self.moved_at = Some(now);
    }
    /// True while an edge scroll or the scrollbar fade needs the next presented frame.
    pub fn animating(&self, now: Instant, reduced: bool) -> bool {
        let edge = self
            .band
            .as_ref()
            .is_some_and(|band| band.active && band.direction != 0.);
        // The hold deadline starts the fade while its first opacity is still one.
        let fading = !reduced && !self.shown(now) && self.opacity(now, reduced) > 0.;
        edge || fading || self.moving()
    }
}

impl Shell {
    /// FileDrag.qml and HoverTimers.qml scroll a held row lift by one row per 100 ms at an edge.
    pub(crate) fn advance_row_drag(
        &mut self,
        side: usize,
        frame: Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = cx.has_active_drag() && !self.input_blocked(cx);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let last = host.input.row_drag_at.take();
        if !active
            || host.pane.busy()
            || host.input.band.is_some()
            || !host
                .input
                .press
                .as_ref()
                .is_some_and(|press| press.listing == frame.listing)
        {
            return;
        }
        let (x, y) = frame.local(window.mouse_position());
        if x < 0.
            || x >= f32::from(frame.bounds.size.width) - frame.lane
            || y < -frame.row
            || y > frame.viewport() + frame.row
            || frame.viewport() < 2. * ROW_DRAG_BAND
        {
            return;
        }
        let direction = if y < ROW_DRAG_BAND {
            -1.
        } else if y > frame.viewport() - ROW_DRAG_BAND {
            1.
        } else {
            return;
        };
        let now = cx.background_executor().now();
        host.input.row_drag_at = Some(now);
        let seconds = last.map_or(0., |at| {
            now.saturating_duration_since(at)
                .min(ROW_DRAG_STEP)
                .as_secs_f32()
        });
        host.pane.scroll_to(
            (host.pane.scroll_y + direction * frame.row * seconds / ROW_DRAG_STEP.as_secs_f32())
                .clamp(0., frame.max_scroll()),
        );
        if host.pane.scroll_y != frame.scroll {
            host.input
                .scrolled(host.pane.scroll_y, &mut host.pane.cursor);
            host.input.moved(now);
            cx.notify();
        }
        if (direction < 0. && host.pane.scroll_y > 0.)
            || (direction > 0. && host.pane.scroll_y < frame.max_scroll())
        {
            window.request_animation_frame();
        }
    }

    /// One deadline wake for the bar's hold and fade end; presented frames drive the motion between.
    pub(crate) fn arm_input_tick(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let Some(at) = host.input.moved_at else {
            host.input.timer = None;
            return;
        };
        if host.input.timer.is_some() {
            return;
        }
        let end: u64 = if cx.reduce_motion() { 1000 } else { 1300 };
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(at)
            .as_millis() as u64;
        let delay = Duration::from_millis(if elapsed < 1000 {
            1000 - elapsed
        } else {
            end.saturating_sub(elapsed).max(1)
        });
        host.input.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| {
                    let Some(host) = &mut shell.panes[side] else {
                        return;
                    };
                    host.input.timer = None;
                    let end = if cx.reduce_motion() { 1000 } else { 1300 };
                    if host.input.moved_at.is_some_and(|at| {
                        cx.background_executor().now().saturating_duration_since(at)
                            >= Duration::from_millis(end)
                    }) {
                        host.input.moved_at = None;
                    }
                    cx.notify();
                    shell.arm_input_tick(side, window, cx);
                })
            });
        }));
    }
}
