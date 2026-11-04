//! EY-297: the greyed Flash button's frame and label, against v0.3.12's DialogButton.
use super::look::{at, latte, painted, quads_at};
use super::*;
use crate::g2::op2::draw_step::{DISABLED_OPACITY, LABEL_INK};

/// EY-297: the greyed Flash button takes the step row's dim over its own and labels in Theme.color.muted.
#[test]
fn fixchrome_ey297_the_greyed_flash_button_takes_the_row_dim_and_the_muted_label() {
    for theme in [Theme::default(), latte()] {
        let live = LiveFlash::with_targets(Vec::new(), false);
        LABEL_INK.with(|ink| ink.set(None));
        let (mut visual, quads, palette, scale) = painted(live, theme);
        assert_eq!(
            LABEL_INK.with(|ink| ink.get()),
            Some(palette.muted),
            "the greyed Flash label paints in Theme.color.muted"
        );
        let button = at(&mut visual, "op2-flash-action");
        let alphas: Vec<f32> = quads_at(&quads, button, scale)
            .iter()
            .filter(|q| q.border_widths.top.0 > 0.)
            .map(|q| q.border_color.a)
            .collect();
        let want = palette.muted_frame.a * DISABLED_OPACITY * DISABLED_OPACITY;
        assert!(
            !alphas.is_empty() && alphas.iter().all(|a| (a - want).abs() < 1e-3),
            "greyed Flash frame alphas {alphas:?}, want {want}"
        );
        let t = super::super::cards::Tokens::of(&palette);
        assert_eq!(StandIn::action_ink(&t, false), palette.muted);
        assert_eq!(StandIn::action_ink(&t, true), palette.foreground);
    }
}
