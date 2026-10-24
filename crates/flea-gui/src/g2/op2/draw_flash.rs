//! The FLASH card's own draw pass: rows for the image, the drives and the running bar, all over the op1 dialog shell seam.
use super::cards::*;
use super::draw::StandIn;
use super::flash::{
    DriveRow, ImageRow, PressAnim, StepMark, View, SEL_CURSOR_EDGE, SEL_CURSOR_EDGE_LIVE, SEL_DRIVE_ROW,
    SEL_EMPTY_MARK, SEL_IMAGE_HINT, SEL_IMAGE_ROW, SEL_STEP_BAR, SEL_STEP_BYTE,
};
use flea::i18n::I18n;
use gpui::{
    div, prelude::*, px, relative, AnyElement, App, ClickEvent, Div, MouseButton, MouseDownEvent, MouseUpEvent,
    Window,
};
use std::rc::Rc;
use std::time::Instant;
use crate::g2::op3::shell::glyph;
use crate::list::{Palette, Typeface};

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

/// The pointer entries op2 installs itself: the image tap, one tap per drive row, and the step action's own hover, press and tap. Empty until the G1 pane host passes its listeners at join.
#[derive(Clone, Default)]
pub struct FlashTaps {
    pub image: Option<ClickTap>,
    pub drives: Vec<ClickTap>,
    pub step: Option<StepTap>,
    pub hover: Option<HoverTap>,
    pub down: Option<DownTap>,
    pub up: Option<UpTap>,
    pub cancel: Option<CancelTap>,
}

/// The flash view's per-frame inputs: the model view, the press clock and the window it measures with.
pub struct FlashFrame<'a> {
    pub view: &'a View,
    pub i18n: &'a I18n,
    pub palette: &'a Palette,
    pub typeface: &'a Typeface,
    pub radius: f32,
    pub taps: &'a FlashTaps,
    pub press: &'a PressAnim,
    pub now: Instant,
    pub reduced: bool,
    pub window: &'a mut Window,
}

impl StandIn {
    fn cursor_edge(&self, draw: &Draw, cursor: bool, focused: bool) -> Div {
        div()
            .w(px(2.))
            .h_full()
            .flex_none()
            .when(cursor, |d| d.debug_selector(|| SEL_CURSOR_EDGE.into()))
            .bg(if cursor && focused { draw.t.accent } else if cursor { draw.t.muted_frame } else { draw.t.surface })
            .when(!cursor, |d| d.invisible())
            .when(cursor, |d| {
                d.child(div().w_full().h_full().debug_selector(|| SEL_CURSOR_EDGE_LIVE.into()))
            })
    }

    fn imagerow(&self, draw: &Draw, row: &ImageRow, focused: bool, taps: &FlashTaps) -> AnyElement {
        let t = draw.t;
        let line = div()
            .h(draw.px(31.))
            .flex()
            .items_center()
            .gap(draw.px(ROW_GAP))
            .px(draw.px(PAD))
            .text_color(t.fg)
            .when(row.cursor && focused, |d| d.bg(t.accent.opacity(0.18)))
            .when(row.refused, |d| d.opacity(0.55))
            .child(self.cursor_edge(draw, row.cursor, focused))
            .child(glyph(row.mark(), 19. * draw.s, if row.cursor { t.fg } else { t.muted_frame }))
            .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().child(row.name.clone()))
            .child(draw.text(CAPTION).text_color(if row.cursor { t.fg } else { t.muted }).child(row.detail.clone()))
            .child(draw.text(CAPTION).text_color(if row.cursor { t.fg } else { t.muted }).child(row.size.clone()));
        let line = match &taps.image {
            Some(tap) => {
                let tap = tap.clone();
                line
                    .id(SEL_IMAGE_ROW)
                    .debug_selector(|| SEL_IMAGE_ROW.into())
                    .on_click(move |ev, window, cx| tap(ev, window, cx))
                    .into_any_element()
            }
            None => line.into_any_element(),
        };
        if row.hint.is_empty() {
            line
        } else {
            div()
                .flex()
                .flex_col()
                .child(line)
                .child(
                    draw.text(CAPTION)
                        .debug_selector(|| SEL_IMAGE_HINT.into())
                        .pl(draw.px(42.))
                        .text_color(t.muted)
                        .child(row.hint.clone()),
                )
                .into_any_element()
        }
    }

    fn driverow(&self, draw: &Draw, at: usize, row: &DriveRow, focused: bool, taps: &FlashTaps) -> AnyElement {
        let t = draw.t;
        let line = div()
            .h(draw.px(31.))
            .flex()
            .items_center()
            .gap(draw.px(ROW_GAP))
            .px(draw.px(PAD))
            .text_color(t.fg)
            .when(row.cursor && focused, |d| d.bg(t.accent.opacity(0.18)))
            .when(!row.cursor && row.selected, |d| d.bg(t.fg.opacity(0.18)))
            .when(row.dimmed, |d| d.opacity(0.55))
            .child(self.cursor_edge(draw, row.cursor, focused))
            .child(glyph(Glyph::Drive, 19. * draw.s, t.fg))
            .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().child(row.name.clone()))
            .child(draw.text(CAPTION).text_color(if row.cursor || row.selected { t.fg } else { t.muted }).child(row.detail.clone()))
            .child(draw.text(CAPTION).text_color(if row.cursor || row.selected { t.fg } else { t.muted }).child(row.size.clone()));
        let line = match taps.drives.get(at) {
            Some(tap) => {
                let tap = tap.clone();
                line
                    .id((SEL_DRIVE_ROW, at))
                    .debug_selector(move || format!("{}-{}", SEL_DRIVE_ROW, at))
                    .on_click(move |ev, window, cx| tap(ev, window, cx))
                    .into_any_element()
            }
            None => line.into_any_element(),
        };
        if row.hint.is_empty() {
            line
        } else {
            div().flex().flex_col().child(line).child(draw.text(CAPTION).pl(draw.px(42.)).text_color(t.muted).child(row.hint.clone())).into_any_element()
        }
    }

    /// The running bar with its byte line: half a caption thick on a quarter-ink track, figures in foreground.
    fn bar(&self, draw: &Draw, view: &View) -> AnyElement {
        let t = draw.t;
        let step = &view.step;
        let share = step.share.clamp(0.0, 1.0) as f32;
        let mut bytes = div().flex().items_center();
        for (text, figure) in &step.byte {
            bytes = bytes.child(
                draw.text(CAPTION).text_color(if *figure { t.fg } else { t.muted }).child(text.clone()),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(draw.px(6.))
            .pl(draw.px(42.))
            .pr(draw.px(PAD))
            .child(
                div()
                    .debug_selector(|| SEL_STEP_BAR.into())
                    .w_full()
                    .h(draw.px(6.))
                    .bg(t.muted_frame.opacity(0.25))
                    .child(div().w(relative(share)).h_full().bg(t.accent_frame)),
            )
            .child(bytes.debug_selector(|| SEL_STEP_BYTE.into()))
            .into_any_element()
    }

    /// The flash view as plain rows under its strip, until the G1 pane host covers the slot.
    pub fn present_flash(&self, p: FlashFrame) -> AnyElement {
        let FlashFrame { view, i18n, palette, typeface, radius, taps, press, now, reduced, window } = p;
        let s = f32::from(typeface.body) / BOARD_BODY;
        let draw = Draw { t: Tokens::of(palette), s, typeface, r: radius };
        let t = draw.t;
        let step = &view.step;
        let mark = match step.mark {
            StepMark::Alert => glyph(Glyph::Alert, 19. * s, t.error).into_any_element(),
            StepMark::Spinner => div().size(px(19. * s)).flex_none().into_any_element(),
            StepMark::Check => glyph(Glyph::Check, 19. * s, t.fg).into_any_element(),
            StepMark::Cross => glyph(Glyph::Cross, 19. * s, t.muted).into_any_element(),
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
            .child(draw.text(CAPTION).px(draw.px(PAD)).text_color(t.fg).child(view.title.clone()))
            .child(self.section(&draw, &i18n.tr("archive.flash.image")))
            .child(self.imagerow(&draw, &view.image, view.focused, taps))
            .child(self.section(&draw, &i18n.tr("archive.flash.drives")));
        for (at, row) in view.drives.iter().enumerate() {
            column = column.child(self.driverow(&draw, at, row, view.focused, taps));
        }
        if view.drives_ready && view.drives.is_empty() {
            column = column.child(
                div()
                    .debug_selector(|| SEL_EMPTY_MARK.into())
                    .h(draw.px(31.))
                    .flex()
                    .items_center()
                    .gap(draw.px(ROW_GAP))
                    .px(draw.px(PAD))
                    .text_color(t.muted)
                    .child(glyph(Glyph::Drive, 19. * draw.s, t.muted_frame))
                    .child(draw.text(BOARD_BODY).child(i18n.tr("archive.flash.no_drives"))),
            );
        }
        let action = self.action(&draw, super::draw_step::ActionIn { view, i18n, taps, press, now, reduced, window });
        let mut step_row = div()
            .h(draw.px(37.))
            .flex()
            .items_center()
            .gap(draw.px(ROW_GAP))
            .px(draw.px(PAD))
            .text_color(if step.mark == StepMark::Alert && !step.running { t.error } else { t.fg })
            .when(step.cursor && view.focused, |d| d.bg(t.accent.opacity(0.18)))
            .child(self.cursor_edge(&draw, step.cursor, view.focused))
            .child(mark)
            .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().child(step.line.clone()));
        if let Some(action) = action {
            step_row = step_row.child(action);
        }
        column = column.child(self.section(&draw, &i18n.tr("archive.flash.step"))).child(step_row);
        if step.show_bar() {
            column = column.child(self.bar(&draw, view));
        }
        if !step.hint.is_empty() {
            column = column.child(draw.text(CAPTION).pl(draw.px(42.)).text_color(t.muted).child(step.hint.clone()));
        }
        let mut root = div().debug_selector(|| format!("op2-radius-{}", draw.r)).size_full();
        if let Some(up) = &taps.up {
            let up = up.clone();
            root = root.on_mouse_up_out(MouseButton::Left, move |ev, window, cx| up(ev, window, cx));
        }
        root.child(column).into_any_element()
    }
}
