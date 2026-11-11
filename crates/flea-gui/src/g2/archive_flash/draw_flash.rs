//! The flash pane's own draw pass: the strip, the IMAGE, DRIVES and FLASH sections and the running bar.
use super::cards::*;
use super::draw::StandIn;
use super::draw_flash_rows::{RowInk, INSET};
use super::draw_step::DISABLED_OPACITY;
use super::flash::{PressAnim, StepMark, View, SEL_EMPTY_MARK, SEL_STEP_BAR, SEL_STEP_BYTE};
use crate::g2::open_with_properties::shell::glyph;
use crate::list::{Palette, Typeface};
use flea::i18n::I18n;
use gpui::{
    div, prelude::*, px, relative, AnyElement, App, ClickEvent, Hsla, MouseButton, MouseDownEvent,
    MouseUpEvent, Window,
};
use std::rc::Rc;
use std::time::Instant;

/// A left-release tap: the entity-free closure the host builds with `cx.listener`.
pub type ClickTap = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// The step action's hover: the entity-free closure the host builds with `cx.listener`.
pub type HoverTap = Rc<dyn Fn(&bool, &mut Window, &mut App)>;
/// The step action's press: the entity-free closure the host builds with `cx.listener`.
pub type DownTap = Rc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;
/// The step action's release and fire: the release clears, then fires where the press began on the action.
pub type UpTap = Rc<dyn Fn(&MouseUpEvent, &mut Window, &mut App)>;
/// A release inside the action fires the step where a click used to, so GPUI keeps no pending down.
pub type StepTap = Rc<dyn Fn(&MouseUpEvent, &mut Window, &mut App)>;
/// Cancel a pointer gesture without turning cancellation into an activating release.
pub type CancelTap = Rc<dyn Fn(&mut Window, &mut App)>;
/// Which step action last painted and where, for flashControlCentre.
pub type ActionProbe =
    Rc<std::cell::Cell<Option<(super::flash::StepAction, gpui::Bounds<gpui::Pixels>)>>>;

/// The pointer entries op2 installs itself: Back, the image tap, one tap per drive row, and the step action's own hover, press and tap.
#[derive(Clone, Default)]
pub struct FlashTaps {
    pub back: Option<ClickTap>,
    pub image: Option<ClickTap>,
    pub drives: Vec<ClickTap>,
    pub step: Option<StepTap>,
    pub hover: Option<HoverTap>,
    pub down: Option<DownTap>,
    pub up: Option<UpTap>,
    pub cancel: Option<CancelTap>,
    pub action_probe: Option<ActionProbe>,
}

/// The flash view's per-frame inputs: the model view, the press clock and the window it measures with.
pub struct FlashFrame<'a> {
    pub view: &'a View,
    pub i18n: &'a I18n,
    pub palette: &'a Palette,
    /// The pane strip's ground, Theme::chrome as the path bar draws it.
    pub chrome: Hsla,
    pub typeface: &'a Typeface,
    pub radius: f32,
    pub taps: &'a FlashTaps,
    pub press: &'a PressAnim,
    pub now: Instant,
    pub reduced: bool,
    pub window: &'a mut Window,
}

impl StandIn {
    /// The running bar with its byte line under the name column: half a caption thick on a quarter-ink track.
    fn bar(&self, draw: &Draw, view: &View) -> AnyElement {
        let t = draw.t;
        let step = &view.step;
        let share = step.share.clamp(0.0, 1.0) as f32;
        let mut bytes = div().flex().items_center();
        for (text, figure) in &step.byte {
            bytes = bytes.child(
                draw.text(CAPTION)
                    .text_color(if *figure { t.fg } else { t.muted })
                    .child(text.clone()),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(draw.px(BAR_GAP))
            .pl(draw.px(MARK_INDENT))
            .pr(draw.px(INSET))
            .child(
                div()
                    .debug_selector(|| SEL_STEP_BAR.into())
                    .w_full()
                    .h(draw.px(BAR_HEIGHT))
                    .bg(t.muted_frame.opacity(TRACK_WASH))
                    .child(div().w(relative(share)).h_full().bg(t.accent_frame)),
            )
            .child(bytes.debug_selector(|| SEL_STEP_BYTE.into()))
            .into_any_element()
    }

    /// The flash view as the pane that covers its slot: its strip over the IMAGE, DRIVES and FLASH sections.
    pub fn present_flash(&self, p: FlashFrame) -> AnyElement {
        let FlashFrame {
            view,
            i18n,
            palette,
            chrome,
            typeface,
            radius,
            taps,
            press,
            now,
            reduced,
            window,
        } = p;
        let s = f32::from(typeface.body) / BOARD_BODY;
        let draw = Draw {
            language: i18n.language(),
            t: Tokens::of(palette),
            s,
            typeface,
            r: radius,
            press: None,
        };
        let t = draw.t;
        let ink = RowInk {
            caption: palette.muted,
            meta: palette.metadata,
            chrome,
            marked: palette.marked,
        };
        let step = &view.step;
        let mark = match step.mark {
            StepMark::Alert => glyph(Glyph::Alert, MARK * s, t.error).into_any_element(),
            StepMark::Spinner => div().size(px(MARK * s)).flex_none().into_any_element(),
            StepMark::Check => glyph(Glyph::Check, MARK * s, t.fg).into_any_element(),
            StepMark::Cross => glyph(Glyph::Cross, MARK * s, t.muted).into_any_element(),
        };
        let mut column = div()
            .debug_selector(|| "op2-flash".into())
            .size_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .font_family(draw.typeface.font.family.clone())
            .text_size(draw.px(BOARD_BODY))
            .line_height(relative(1.2))
            .child(self.strip(&draw, ink, &view.title, taps))
            .child(self.title(&draw, ink.caption, &i18n.tr("archive.flash.image"), 0))
            .child(self.image_row(&draw, ink, &view.image, view.focused, taps))
            .child(self.title(&draw, ink.caption, &i18n.tr("archive.flash.drives"), 1));
        for (at, row) in view.drives.iter().enumerate() {
            column = column.child(self.drive_row(&draw, ink, at, row, view.focused, taps));
        }
        if view.drives_ready && view.drives.is_empty() {
            column = column.child(
                self.plate(&draw, false, view.focused)
                    .debug_selector(|| SEL_EMPTY_MARK.into())
                    .text_color(ink.caption)
                    .children(self.name(
                        &draw,
                        Glyph::Drive,
                        t.muted_frame,
                        &i18n.tr("archive.flash.no_drives"),
                    )),
            );
        }
        let action = self.action(
            &draw,
            super::draw_step::ActionIn {
                view,
                i18n,
                taps,
                press,
                now,
                reduced,
                window,
            },
        );
        let line_ink = if step.mark == StepMark::Alert && !step.running {
            t.error
        } else {
            t.fg
        };
        let mut step_row = self
            // FlashStepRow.qml:46: the greyed row dims its cursor, its line and the button inside it, over the button's own dim.
            .dimmed_plate(
                &draw,
                step.cursor,
                view.focused,
                if step.resting { DISABLED_OPACITY } else { 1. },
            )
            .debug_selector(|| "op2-flash-step".into())
            .h(draw.px(STEP_ROW))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .text_color(line_ink)
                    .when(step.resting, |d| d.opacity(DISABLED_OPACITY))
                    .child(div().mr(draw.px(ROW_GAP)).flex_none().child(mark))
                    .child(
                        div()
                            .debug_selector(|| "op2-flash-step-line".into())
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(step.line.clone()),
                    ),
            );
        if let Some(action) = action {
            step_row = step_row.child(
                div()
                    .debug_selector(|| "op2-flash-step-action".into())
                    .ml(draw.px(ROW_GAP))
                    .flex_none()
                    .when(step.resting, |d| d.opacity(DISABLED_OPACITY))
                    .child(action),
            );
        }
        column = column
            .child(self.title(&draw, ink.caption, &i18n.tr("archive.flash.step"), 2))
            .child(step_row);
        if step.show_bar() {
            column = column.child(self.bar(&draw, view));
        }
        if !step.hint.is_empty() {
            column = column.child(self.hint(&draw, ink.caption, &step.hint));
        }
        let mut root = div()
            .debug_selector(|| format!("op2-radius-{}", draw.r))
            .size_full();
        if let Some(up) = &taps.up {
            let up = up.clone();
            root =
                root.on_mouse_up_out(MouseButton::Left, move |ev, window, cx| up(ev, window, cx));
        }
        root.child(column).into_any_element()
    }
}
