//! Geometry fixtures inspect slow progress after its shared hold, with every size assertion intact.
use super::{host::Overlay, palette::CardPalette};
use gpui::{TestAppContext, VisualTestContext};
use std::{sync::Arc, time::Duration};

pub(super) fn settle_progress(cx: &VisualTestContext) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
}

pub(super) fn open_card(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
    open: impl FnOnce(&mut Overlay),
) -> (gpui::Entity<Overlay>, &mut VisualTestContext) {
    let (overlay, vcx) = cx.add_window_view(|window, cx| {
        // Board-size fixtures request the board's14px setting explicitly.
        window.set_rem_size(gpui::px(14.));
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = CardPalette::from_list(
            &crate::list::Palette::inherited(&window.text_style(), theme),
            8.,
        );
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            palette,
            "default",
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
    overlay.update(vcx, |o, cx| {
        open(o);
        cx.notify();
    });
    vcx.run_until_parked();
    if overlay.read_with(vcx, |o, _| o.model.jobs.unfinished() > 0) {
        settle_progress(vcx);
    }
    (overlay, vcx)
}
