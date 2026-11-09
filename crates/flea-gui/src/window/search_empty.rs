//! The approved no-match mark and headline, search's and the filter's, using the current pane's typeface and palette.
use crate::{
    g2::status_feedback::caption::{tracking_for, Tracked},
    list::{Palette, Typeface},
    state::Shell,
};
use flea::i18n::I18n;
use gpui::{
    canvas, div, prelude::*, px, AnyElement, Context, IntoElement, SharedString,
    TransformationMatrix, Window,
};

const SEARCH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M16.7 16.7 21 21 M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z"/></svg>"#;

/// Row padding either side of the headline at body 14, Theme.spacing.rowPaddingX.
const PAD_X: f32 = 14.;

/// A headline's template: the words around the query, in the caller's language.
type Headline = fn(&I18n, &str) -> String;

impl Shell {
    /// The no-match headline a pane draws: search's when it found nothing, else a standing filter's that shows nothing.
    pub(crate) fn no_match_headline(&self, side: usize) -> Option<String> {
        let (query, headline) = self.no_match_query(side)?;
        Some(headline(&self.locale.catalog, &query))
    }

    /// The query the headline names and the template around it, so a long query can elide alone.
    fn no_match_query(&self, side: usize) -> Option<(String, Headline)> {
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
                    (
                        host.pane.state.query.clone(),
                        flea::ui_logic_file_search::nothing_matches as Headline,
                    )
                });
        }
        if host.search.owner.active() || host.pane.state.kind != crate::pane::states::Kind::Ready {
            return None;
        }
        crate::g2::listing_filter::present::no_match_query(
            &host.filter.owner,
            host.pane.held.listing,
        )
        .map(|query| {
            (
                query.to_owned(),
                crate::g2::listing_filter::present::nothing_matches as Headline,
            )
        })
    }

    pub(crate) fn search_empty(
        &self,
        side: usize,
        room: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        self.no_match_hero(side, room, window, cx)
            .map(|(hero, _)| hero)
    }

    /// The approved no-match mark over its headline, centred in its box; a query too long for `room` elides to fit.
    pub(crate) fn no_match_hero(
        &self,
        side: usize,
        room: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<(AnyElement, f32)> {
        let (query, template) = self.no_match_query(side)?;
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
        let measure = |text: &str| {
            crate::g2::status_feedback::caption::measure(
                SharedString::from(text.to_owned()),
                face.font.clone().bold(),
                face.caption,
                tracking,
                window,
            )
        };
        let catalog = &self.locale.catalog;
        let headline = fitted(
            &query,
            |query| template(catalog, query),
            room - 2. * PAD_X * scale,
            measure,
        );
        let width = measure(&headline).max(mark);
        let headline = SharedString::from(headline);
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

/// The headline with the longest head of `query` that fits `room` before an ellipsis, else the whole line cut to fit.
pub(crate) fn fitted(
    query: &str,
    headline: impl Fn(&str) -> String,
    room: f32,
    measure: impl Fn(&str) -> f32,
) -> String {
    let whole = headline(query);
    if measure(&whole) <= room {
        return whole;
    }
    let cut = |text: &str, chars: usize| {
        let end = text
            .char_indices()
            .nth(chars)
            .map_or(text.len(), |(at, _)| at);
        format!("{}\u{2026}", &text[..end])
    };
    let longest = |text: &str, fits: &dyn Fn(usize) -> bool| {
        let (mut low, mut high) = (0, text.chars().count().saturating_sub(1));
        while low < high {
            let mid = (low + high).div_ceil(2);
            if fits(mid) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        low
    };
    let query_fits = |chars: usize| measure(&headline(&cut(query, chars))) <= room;
    if query_fits(0) {
        return headline(&cut(query, longest(query, &query_fits)));
    }
    let line_fits = |chars: usize| measure(&cut(&whole, chars)) <= room;
    cut(&whole, longest(&whole, &line_fits))
}

#[cfg(test)]
#[path = "search_empty_tests.rs"]
mod tests;
