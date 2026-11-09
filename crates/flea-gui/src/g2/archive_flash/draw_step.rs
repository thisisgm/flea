//! The FLASH step action: its frame role, its tap wiring and its press chrome.
use super::cards::*;
use super::draw::StandIn;
use super::draw_flash::FlashTaps;
use super::draw_press::PressButton;
use super::flash::{
    PressAnim, StepAction, StepRow, View, SEL_STEP_CANCEL, SEL_STEP_END, SEL_STEP_FLASH,
    SEL_STEP_FRAME, SEL_STEP_WASH_HOVER, SEL_STEP_WASH_PRESS,
};
use flea::i18n::I18n;
use gpui::{div, prelude::*, px, AnyElement, Hsla, IntoElement, TextRun, Window};
use std::time::Instant;

/// Buttons.js:19: the pressed chrome shrinks by two percent a side while the press shows.
const PRESS_SCALE: f32 = 0.96;
/// The focus ring's width and the resting hairline's, in pixels like border_2 and border_1.
const RING_W: f32 = 2.;
const HAIRLINE_W: f32 = 1.;
/// Buttons.js:14: the pointer's wash, laid in the control's own ink.
const WASH_HOVER: f32 = 0.08;
/// Buttons.js:15: the press wash, laid in the control's own ink.
pub(super) const WASH_PRESS: f32 = 0.14;
/// Buttons.js:17: a primary's resting wash, at the press wash's weight.
const WASH_PRIMARY: f32 = WASH_PRESS;
/// Buttons.js:12: a dimmed control keeps over half its ink, like a cut row.
pub(super) const DISABLED_OPACITY: f32 = 0.55;
/// The DialogButton frame role for the step action, shared by the draw code and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionFrame {
    Accent,
    Foreground,
    Muted,
}

#[cfg(test)]
thread_local! {
    /// The text ink the step action's label resolved to when it last painted, as gpui cascades it.
    pub(super) static LABEL_INK: std::cell::Cell<Option<Hsla>> = const { std::cell::Cell::new(None) };
}

/// The step action's per-frame inputs: the view, the press clock and the window it measures with.
pub(super) struct ActionIn<'a> {
    pub view: &'a View,
    pub i18n: &'a I18n,
    pub taps: &'a FlashTaps,
    pub press: &'a PressAnim,
    pub now: Instant,
    pub reduced: bool,
    pub window: &'a mut Window,
}

impl StandIn {
    /// The step action's availability, mirroring DialogButton.qml: only an available button activates.
    pub(super) fn action_available(step: &StepRow, action: StepAction) -> bool {
        match action {
            StepAction::Flash => step.live,
            StepAction::Cancel => step.cancel_live,
            StepAction::End => !step.end_primary || step.live,
            StepAction::Hidden => false,
        }
    }

    /// The dialog's own primary: Flash, and the Flash again end; Cancel and Eject stay secondary.
    pub(super) fn action_primary(step: &StepRow, action: StepAction) -> bool {
        match action {
            StepAction::Flash => true,
            StepAction::End => step.end_primary,
            StepAction::Cancel | StepAction::Hidden => false,
        }
    }

    /// The DialogButton frame: the focus frame while focused and available, else the accent frame while primary and available, else muted.
    pub(super) fn action_frame(step: &StepRow, action: StepAction, focused: bool) -> ActionFrame {
        if step.cursor && focused && Self::action_available(step, action) {
            return ActionFrame::Foreground;
        }
        match action {
            StepAction::Flash => {
                if step.live {
                    ActionFrame::Accent
                } else {
                    ActionFrame::Muted
                }
            }
            StepAction::Cancel => ActionFrame::Muted,
            StepAction::End => {
                if step.end_primary && step.live {
                    ActionFrame::Accent
                } else {
                    ActionFrame::Muted
                }
            }
            StepAction::Hidden => ActionFrame::Muted,
        }
    }

    /// DialogButton.qml:47: a live button labels in the foreground, an unavailable one in Theme.color.muted.
    pub(super) fn action_ink(t: &super::cards::Tokens, available: bool) -> Hsla {
        if available {
            t.fg
        } else {
            t.muted
        }
    }

    /// Buttons.js:45 with no set members: the ring where a hairline frame would match the rest.
    pub(super) fn frame_ring(primary: bool, accent_frame: Hsla, fg: Hsla) -> bool {
        primary && accent_frame == fg
    }

    /// The shown press progress: the end state at once under reduced motion, else the honest eased value.
    pub(super) fn shown_progress(press: &PressAnim, now: Instant, reduced: bool) -> f32 {
        if reduced {
            return press.to;
        }
        press.at(now)
    }

    /// The press margin in board pixels: eased progress times the final rounded inset, rounded once more.
    pub(super) fn press_margin(board_w: f32, q: f32) -> f32 {
        ((board_w * (1. - PRESS_SCALE) / 2.).round() * q).round()
    }

    /// The step action probe: its registry key, its frame role and the exact labels of the live copy.
    pub(super) fn action(&self, draw: &Draw, input: ActionIn) -> Option<AnyElement> {
        let ActionIn {
            view,
            i18n,
            taps,
            press,
            now,
            reduced,
            window,
        } = input;
        let step = &view.step;
        let action = step.step_action();
        let probe = match action {
            StepAction::Flash => SEL_STEP_FLASH,
            StepAction::Cancel => SEL_STEP_CANCEL,
            StepAction::End => SEL_STEP_END,
            StepAction::Hidden => return None,
        };
        let label = match action {
            StepAction::Flash => step.action.clone(),
            StepAction::Cancel => i18n.tr("archive.cancel"),
            StepAction::End => step.end_label.clone(),
            StepAction::Hidden => return None,
        };
        let available = Self::action_available(step, action);
        let primary = Self::action_primary(step, action);
        let focus_shown = step.cursor && view.focused && available;
        let frame = match Self::action_frame(step, action, view.focused) {
            ActionFrame::Foreground => draw.t.fg,
            ActionFrame::Accent => draw.t.accent_frame,
            ActionFrame::Muted => draw.t.muted_frame,
        };
        let ring = focus_shown && Self::frame_ring(primary, draw.t.accent_frame, draw.t.fg);
        let fill = if primary && available {
            draw.t.accent.opacity(WASH_PRIMARY)
        } else {
            draw.t.accent.opacity(0.)
        };
        let ink = Self::action_ink(&draw.t, available);
        let wash = if step.action_pressed && step.action_hover && available {
            Some((ink.opacity(WASH_PRESS), SEL_STEP_WASH_PRESS))
        } else if !step.action_pressed && step.action_hover && available {
            Some((ink.opacity(WASH_HOVER), SEL_STEP_WASH_HOVER))
        } else {
            None
        };
        let run = TextRun {
            len: label.len(),
            font: draw.typeface.font.clone(),
            color: ink,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let label_w = window
            .text_system()
            .shape_line(label.clone().into(), draw.px(BOARD_BODY), &[run], None)
            .width();
        let pad = draw.px(BUTTON_PAD);
        let board_w = f32::from(label_w + pad + pad) / draw.s;
        let q = Self::shown_progress(press, now, reduced);
        let inset_x = px(Self::press_margin(board_w, q) * draw.s);
        let inset_y = px(Self::press_margin(BUTTON_HEIGHT, q) * draw.s);
        let frame_w = px(if ring { RING_W } else { HAIRLINE_W });
        let mut button = div()
            .debug_selector(|| probe.into())
            .id(probe)
            .relative()
            .h(draw.px(BUTTON_HEIGHT))
            .px(pad)
            .flex()
            .items_center()
            .text_color(ink)
            .opacity(if available { 1. } else { DISABLED_OPACITY })
            .whitespace_nowrap()
            .child({
                let text = div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(label);
                #[cfg(test)]
                let text = text.child(
                    gpui::canvas(
                        |_, window, _| {
                            LABEL_INK.with(|ink| ink.set(Some(window.text_style().color)))
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_0(),
                );
                text
            });
        if let Some(hover) = &taps.hover {
            let hover = hover.clone();
            button = button.on_hover(move |over, window, cx| hover(over, window, cx));
        }
        let frame_div = div()
            .debug_selector(|| SEL_STEP_FRAME.into())
            .absolute()
            .top(inset_y)
            .left(inset_x)
            .right(inset_x)
            .bottom(inset_y)
            .rounded(draw.px(draw.r))
            .bg(fill)
            .border_color(frame);
        let frame_div = if ring {
            frame_div.border_2()
        } else {
            frame_div.border_1()
        };
        button = button.child(frame_div);
        if let Some(probe) = taps.action_probe.clone() {
            button = button.child(
                gpui::canvas(
                    |_, _, _| {},
                    move |painted, _, _, _| probe.set(Some((action, painted))),
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        if let Some((color, probe)) = wash {
            button = button.child(
                div()
                    .debug_selector(|| probe.into())
                    .absolute()
                    .top(inset_y + frame_w)
                    .left(inset_x + frame_w)
                    .right(inset_x + frame_w)
                    .bottom(inset_y + frame_w)
                    .rounded((draw.px(draw.r) - frame_w).max(px(0.)))
                    .bg(color),
            );
        }
        Some(
            PressButton::new(
                button,
                taps,
                available,
                step.action_pressed,
                !reduced && q != press.to,
            )
            .into_any_element(),
        )
    }
}
