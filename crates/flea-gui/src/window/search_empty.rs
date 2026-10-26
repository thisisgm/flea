//! The approved no-match mark and headline, search's and the filter's, using the current pane's typeface and palette.
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
    /// The no-match headline a pane draws: search's when it found nothing, else a standing filter's that shows nothing.
    pub(crate) fn no_match_headline(&self, side: usize) -> Option<String> {
        let host = self.panes[side].as_ref()?;
        if host.pane.busy() {
            return None;
        }
        let session = &host.search.owner.session;
        if host.pane.state.search {
            return (session.live()
                && !session.running
                && !session.leaving
                && host.pane.held.total == 0)
                .then(|| {
                    flea::ui_logic_sr1::nothing_matches(
                        &self.locale.catalog,
                        &host.pane.state.query,
                    )
                });
        }
        if host.search.owner.active() || host.pane.state.kind != crate::pane::states::Kind::Ready {
            return None;
        }
        crate::g2::sr2::present::no_match(
            &host.filter.owner,
            host.pane.held.listing,
            &self.locale.catalog,
        )
    }

    pub(crate) fn search_empty(
        &self,
        side: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        self.no_match_hero(side, window, cx).map(|(hero, _)| hero)
    }

    /// The approved no-match mark over its headline, centred in its box, with the width it needs.
    pub(crate) fn no_match_hero(
        &self,
        side: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<(AnyElement, f32)> {
        let headline = self.no_match_headline(side)?;
        let host = self.panes[side].as_ref()?;
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
        let tracking = px(tracking_for(self.locale.catalog.language()) * scale);
        let headline = SharedString::from(headline);
        let width = crate::g2::fb1::caption::measure(
            headline.clone(),
            face.font.clone().bold(),
            face.caption,
            tracking,
            window,
        )
        .max(mark);
        let hero = div()
            .debug_selector(|| "no-match-hero".into())
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
                        text: headline,
                        font: face.font.bold(),
                        size: face.caption,
                        line: px(16. * scale),
                        tracking,
                        color: ink,
                    }),
            )
            .into_any_element();
        Some((hero, width))
    }
}
