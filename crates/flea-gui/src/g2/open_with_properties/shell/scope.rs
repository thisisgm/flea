//! Permissions' folder scope box, as v0.3.12 PermissionsDialog.qml:551-568 draws it.
use super::*;
use gpui::{div, prelude::*, px, Div};

/// The mark is bodySmall square, 13 at the 14 body.
const SCOPE_MARK: f32 = 13.;

impl Draw<'_> {
    /// Theme.spacing.gap (ROW_GAP) insets the box and follows the mark; the accent check and the sentence in foreground caption, wrapped, inside a hairline box on the window ground.
    pub(super) fn scope(&self, text: &str) -> Div {
        let t = self.t;
        div()
            .debug_selector(|| "op3-scope".into())
            .flex()
            .items_start()
            .gap(self.px(ROW_GAP))
            .p(self.px(ROW_GAP))
            .bg(t.ground)
            .border(px(1.))
            .border_color(t.muted_frame)
            .child(glyph(Glyph::Check, SCOPE_MARK * self.s, t.accent))
            .child(
                self.text(CAPTION)
                    .debug_selector(|| "op3-scope-text".into())
                    .whitespace_normal()
                    .flex_1()
                    .min_w_0()
                    .text_color(t.fg)
                    .child(text.to_owned()),
            )
    }
}
