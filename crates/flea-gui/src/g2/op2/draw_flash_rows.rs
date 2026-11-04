//! The flash pane's strip, section titles and file rows, laid out by design-hig flash.png's numbers.
use super::cards::*;
use super::draw::StandIn;
use super::draw_flash::FlashTaps;
use super::draw_step::DISABLED_OPACITY;
use super::flash::{
    DriveRow, ImageRow, SEL_CURSOR_EDGE, SEL_CURSOR_EDGE_LIVE, SEL_DRIVE_ROW, SEL_IMAGE_HINT,
    SEL_IMAGE_ROW,
};
use crate::g2::fb1::caption::{caseless, heading_for, Tracked};
use crate::g2::op3::shell::glyph;
use crate::g2::pl3::board::{Glyph as StripGlyph, CBTN_GLYPH, CBTN_W, HAIR, STRIP_H};
use crate::g2::pl3::glyph::GlyphBox;
use gpui::{div, prelude::*, px, AnyElement, Div, Hsla};

/// `.row` and `.rh`: the pane's 14 inset on both sides.
pub(super) const INSET: f32 = 14.;
/// `.rh`: a 30 tall title, 3 above its first row; the first title sits 9 under the strip.
pub(super) const TITLE_H: f32 = 30.;
pub(super) const TITLE_GAP: f32 = 3.;
pub(super) const BODY_TOP: f32 = 9.;
/// A later title opens 11 under the rows above it, as the rail's `.rgap` does.
pub(super) const GROUP_GAP: f32 = 11.;
/// The refusal line: caption 12 on a 15 line, 9 under its row and 7 above what follows.
pub(super) const HINT_ABOVE: f32 = 9.;
pub(super) const HINT_LINE: f32 = 15.;
pub(super) const HINT_BELOW: f32 = 7.;
/// The drive's model column, 160 wide and 9 before the size; the size column, 102 wide.
pub(super) const DETAIL_W: f32 = 160.;
pub(super) const SIZE_W: f32 = 102.;
/// Section titles keep Omarchy's caption capitals: regular weight, 1 px tracking.
pub(super) const TRACKING: f32 = 1.;

/// The inks the rows take beyond the card tokens: the caption ink, the board's metadata ink, the strip's chrome and the selection plate.
#[derive(Clone, Copy)]
pub(super) struct RowInk {
    pub caption: Hsla,
    pub meta: Hsla,
    pub chrome: Hsla,
    pub marked: Hsla,
}

impl StandIn {
    /// The 2 px cursor edge, flush on the pane's left, accent while the pane holds the keys.
    pub(super) fn cursor_edge(&self, draw: &Draw, focused: bool) -> Div {
        div()
            .debug_selector(|| SEL_CURSOR_EDGE.into())
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(draw.px(CURSOR_EDGE))
            .bg(if focused {
                draw.t.accent_frame
            } else {
                draw.t.muted_frame
            })
            .child(
                div()
                    .size_full()
                    .debug_selector(|| SEL_CURSOR_EDGE_LIVE.into()),
            )
    }

    /// One `.row`: the cursor rung under a 19 mark and the name.
    pub(super) fn plate(&self, draw: &Draw, cursor: bool, focused: bool) -> Div {
        self.dimmed_plate(draw, cursor, focused, 1.)
    }

    /// A row whose cursor plate and edge fade with it, as a QML row's opacity takes its children.
    pub(super) fn dimmed_plate(
        &self,
        draw: &Draw,
        cursor: bool,
        focused: bool,
        opacity: f32,
    ) -> Div {
        let t = draw.t;
        div()
            .relative()
            .h(draw.px(ROW_HEIGHT))
            .flex()
            .items_center()
            .pl(draw.px(INSET))
            .pr(draw.px(INSET))
            .text_color(t.fg)
            .when(cursor && focused, |d| {
                d.bg(t.accent.opacity(ROW_WASH * opacity))
            })
            .when(cursor, |d| {
                d.child(self.cursor_edge(draw, focused).opacity(opacity))
            })
    }

    pub(super) fn name(&self, draw: &Draw, mark: Glyph, ink: Hsla, name: &str) -> [Div; 2] {
        [
            div()
                .mr(draw.px(ROW_GAP))
                .flex_none()
                .child(glyph(mark, MARK * draw.s, ink)),
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(name.to_owned()),
        ]
    }

    pub(super) fn detail(&self, draw: &Draw, ink: Hsla, text: &str) -> Div {
        draw.text(CAPTION)
            .w(draw.px(DETAIL_W))
            .mr(draw.px(ROW_GAP))
            .flex_none()
            .overflow_hidden()
            .text_ellipsis()
            .text_color(ink)
            .child(text.to_owned())
    }

    pub(super) fn size(&self, draw: &Draw, ink: Hsla, text: &str) -> Div {
        draw.text(CAPTION)
            .w(draw.px(SIZE_W))
            .flex_none()
            .flex()
            .justify_end()
            .text_color(ink)
            .child(text.to_owned())
    }

    /// The refusal under a row, at the name's column in the caption ink.
    pub(super) fn hint(&self, draw: &Draw, ink: Hsla, text: &str) -> Div {
        draw.text(CAPTION)
            .pl(draw.px(MARK_INDENT))
            .pr(draw.px(INSET))
            .pt(draw.px(HINT_ABOVE))
            .pb(draw.px(HINT_BELOW))
            .line_height(draw.px(HINT_LINE))
            .text_color(ink)
            .child(text.to_owned())
    }

    /// `.rh.title`: caption capitals in the caption ink, regular, tracked 1, at the pane's inset.
    pub(super) fn title(&self, draw: &Draw, ink: Hsla, text: &str, index: usize) -> Div {
        // The shared tracked caption the rail headings use: one shaped line, so the tracking stays 1 px.
        let letters = Tracked {
            text: heading_for(text, draw.language).into(),
            font: draw.typeface.font.clone(),
            size: draw.px(CAPTION),
            line: draw.px(HINT_LINE),
            tracking: draw.px(if caseless(draw.language) {
                0.
            } else {
                TRACKING
            }),
            color: ink,
        };
        draw.text(CAPTION)
            .debug_selector(move || format!("op2-flash-title-{index}"))
            .h(draw.px(TITLE_H))
            .mt(draw.px(if index == 0 { BODY_TOP } else { GROUP_GAP }))
            .mb(draw.px(TITLE_GAP))
            .px(draw.px(INSET))
            .flex()
            .items_center()
            .text_color(ink)
            .child(letters)
    }

    /// The IMAGE row: the image's mark and name in foreground, its size in the metadata ink until the cursor lifts it.
    pub(super) fn image_row(
        &self,
        draw: &Draw,
        ink: RowInk,
        row: &ImageRow,
        focused: bool,
        taps: &FlashTaps,
    ) -> AnyElement {
        let lift = if row.cursor { draw.t.fg } else { ink.meta };
        let line = self
            .plate(draw, row.cursor, focused)
            .when(row.refused, |d| d.opacity(DISABLED_OPACITY))
            .children(self.name(draw, row.mark(), draw.t.fg, &row.name))
            .when(!row.detail.is_empty(), |d| {
                d.child(self.detail(draw, lift, &row.detail))
            })
            .child(self.size(draw, lift, &row.size));
        let line = match &taps.image {
            Some(tap) => {
                let tap = tap.clone();
                line.id(SEL_IMAGE_ROW)
                    .debug_selector(|| SEL_IMAGE_ROW.into())
                    .on_click(move |ev, window, cx| tap(ev, window, cx))
                    .into_any_element()
            }
            None => line.into_any_element(),
        };
        if row.hint.is_empty() {
            return line;
        }
        div()
            .flex()
            .flex_col()
            .child(line)
            .child(
                self.hint(draw, ink.caption, &row.hint)
                    .debug_selector(|| SEL_IMAGE_HINT.into()),
            )
            .into_any_element()
    }

    /// A DRIVES row: the chosen or cursor drive lifts its model to foreground; a refused drive dims with its refusal under it.
    pub(super) fn drive_row(
        &self,
        draw: &Draw,
        ink: RowInk,
        at: usize,
        row: &DriveRow,
        focused: bool,
        taps: &FlashTaps,
    ) -> AnyElement {
        let t = draw.t;
        let mark = if row.dimmed { t.muted_frame } else { t.fg };
        let model = if row.cursor || row.selected {
            t.fg
        } else {
            ink.caption
        };
        let line = self
            .plate(draw, row.cursor, focused)
            // FlashDriveRows.qml: off the cursor, the chosen drive keeps Omarchy's selection plate.
            .when(!row.cursor && row.selected, |d| d.bg(ink.marked))
            .when(row.dimmed, |d| d.opacity(DISABLED_OPACITY))
            .children(self.name(draw, Glyph::Drive, mark, &row.name))
            .child(self.detail(draw, model, &row.detail))
            .child(self.size(draw, t.fg, &row.size));
        let line = match taps.drives.get(at) {
            Some(tap) => {
                let tap = tap.clone();
                line.id((SEL_DRIVE_ROW, at))
                    .debug_selector(move || format!("{SEL_DRIVE_ROW}-{at}"))
                    .on_click(move |ev, window, cx| tap(ev, window, cx))
                    .into_any_element()
            }
            None => line.into_any_element(),
        };
        if row.hint.is_empty() {
            return line;
        }
        div()
            .flex()
            .flex_col()
            .child(line)
            .child(
                self.hint(draw, ink.caption, &row.hint)
                    .debug_selector(move || format!("op2-flash-drive-hint-{at}")),
            )
            .into_any_element()
    }

    /// The pane's own strip: Back, an unavailable Up and the title, as the Trash strip draws them.
    pub(super) fn strip(
        &self,
        draw: &Draw,
        ink: RowInk,
        title: &str,
        taps: &FlashTaps,
    ) -> AnyElement {
        let t = draw.t;
        let button = |mark: StripGlyph, enabled: bool, id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .w(draw.px(CBTN_W))
                .h(draw.px(STRIP_H))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .opacity(if enabled { 1. } else { DISABLED_OPACITY })
                .child(GlyphBox {
                    glyph: mark,
                    side: CBTN_GLYPH * draw.s,
                    ink: if enabled { t.fg } else { t.muted_frame },
                })
        };
        let mut back = button(StripGlyph::Back, true, "op2-flash-back");
        if let Some(tap) = &taps.back {
            let tap = tap.clone();
            back = back.on_click(move |ev, window, cx| tap(ev, window, cx));
        }
        draw.text(CAPTION)
            .debug_selector(|| "op2-flash-strip".into())
            .h(draw.px(STRIP_H))
            .flex_none()
            .flex()
            .items_center()
            .px(draw.px(INSET))
            .gap(draw.px(ROW_GAP))
            .bg(ink.chrome)
            .border_b(px(1.))
            .border_color(t.fg.opacity(HAIR))
            .text_color(t.fg)
            .child(back)
            .child(button(StripGlyph::Up, false, "op2-flash-up"))
            .child(div().flex_none().child(title.to_owned()))
            .into_any_element()
    }
}
