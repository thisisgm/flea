//! The approved search no-match mark and headline, using the current pane's typeface and palette.
use crate::{
    g2::fb1::caption::{tracking_for, Tracked},
    list::{Palette, Typeface},
    state::Shell,
};
use gpui::{
    canvas, div, prelude::*, px, AnyElement, Context, IntoElement, SharedString,
    TransformationMatrix, Window,
};

const SEARCH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M16.7 16.7 21 21 M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z"/></svg>"#;

impl Shell {
    pub(crate) fn search_empty(
        &self,
        side: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let host = self.panes[side].as_ref()?;
        let session = &host.search.owner.session;
        if !session.live()
            || session.running
            || session.leaving
            || host.pane.held.total != 0
            || !host.pane.state.search
            || host.pane.busy()
        {
            return None;
        }
        let headline =
            flea::ui_logic_sr1::nothing_matches(&self.locale.catalog, &host.pane.state.query);
        let face: Typeface = host.list.borrow().typeface.clone();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = Palette::inherited(&window.text_style(), theme).metadata;
        let scale = f32::from(face.body) / 14.;
        let mark = 40. * scale;
        let glyph = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let _ = window.paint_svg(
                    bounds,
                    SharedString::new_static("flea-search-empty"),
                    Some(SEARCH),
                    TransformationMatrix::unit(),
                    ink,
                    cx,
                );
            },
        )
        .w(px(mark))
        .h(px(mark));
        Some(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(9. * scale))
                        .child(glyph)
                        .child(Tracked {
                            text: headline.into(),
                            font: face.font.bold(),
                            size: face.caption,
                            line: px(16. * scale),
                            tracking: px(tracking_for(self.locale.catalog.language()) * scale),
                            color: ink,
                        }),
                )
                .into_any_element(),
        )
    }
}
