//! The chooser body: the path strip over the places rail and the listing or share browser, with the listing's wheel.
use super::{browse::Browse, Picker};
use crate::g2::picker_listing::view::ListingView;
use crate::g2::status_feedback::hero::EmptyHero;
use gpui::{div, prelude::*, AnyView, Context, Entity, IntoElement, Render, Window};

// The path strip's band above the listing, 27 px under the 34 px title on the picker-path board.
pub(super) const PATH_HEIGHT: f32 = 27.;

// The body slot: the path strip over the places rail and the listing, as picker-open draws them.
struct Body {
    picker: gpui::WeakEntity<Picker>,
    path: Entity<super::chrome_view::ChromeView>,
    rail: Entity<super::rail_view::RailView>,
    listing: Entity<ListingView>,
    // PickerWindow.qml's EmptyState: the browser's hero over the listing while the folder is empty.
    hero: Option<Entity<EmptyHero>>,
}

impl Render for Body {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picker = self.picker.clone();
        let shares = self
            .picker
            .upgrade()
            .and_then(|picker| {
                picker
                    .read(cx)
                    .rail
                    .network
                    .as_ref()
                    .map(|network| network.shares.clone())
            })
            .filter(|shares| shares.read(cx).visible());
        if let Some(shares) = &shares {
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let palette = crate::g2::file_ops::palette::CardPalette::from_theme(
                &window.text_style(),
                theme,
                0.,
            );
            if shares.read(cx).palette != palette {
                shares.update(cx, |shares, cx| shares.set_palette(palette, cx));
            }
        }
        let hero = self.sync_hero(shares.is_none(), cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(super::size::Size::of(window, cx).px(PATH_HEIGHT))
                    .child(self.path.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.rail.clone())
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_scroll_wheel(move |event, _, cx| {
                                let _ =
                                    picker.update(cx, |picker, cx| picker.listing_wheel(event, cx));
                            })
                            .child(self.listing.clone())
                            // Pinned to the top left: an unplaced absolute child would sit below the full-height listing, outside the window (EY-313).
                            .children(hero.map(|hero| {
                                let mark = hero.read(cx).omi.clone();
                                let painted = mark.clone();
                                div()
                                    .debug_selector(|| "picker-empty-hero".into())
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .child(hero)
                                    // This chooser has no browser-root overlay to sample and paint the hero's mark.
                                    .child(
                                        gpui::canvas(
                                            move |_, window, cx| {
                                                mark.prepare(!cx.reduce_motion(), None, window, cx);
                                            },
                                            move |_, _, window, _| painted.paint(window),
                                        )
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                    )
                            }))
                            .children(shares.map(|shares| {
                                div().absolute().top_0().left_0().size_full().child(shares)
                            })),
                    ),
            )
    }
}

impl Body {
    // Built on the first empty folder and kept; entering empty again replays its entrance as the browser's does.
    fn sync_hero(
        &mut self,
        listing_shown: bool,
        cx: &mut Context<Self>,
    ) -> Option<Entity<EmptyHero>> {
        let picker = self.picker.upgrade()?;
        let empty = listing_shown && picker.read(cx).listing.state == "empty";
        let shown = self
            .hero
            .as_ref()
            .is_some_and(|hero| hero.read(cx).rotation().visible());
        if !empty {
            if shown {
                if let Some(hero) = &self.hero {
                    hero.update(cx, |hero, cx| hero.hide(cx));
                }
            }
            return None;
        }
        let catalog = picker.read(cx).locale.catalog.clone();
        let hero = self
            .hero
            .get_or_insert_with(|| cx.new(|cx| EmptyHero::new(&catalog, cx)))
            .clone();
        if !shown {
            hero.update(cx, |hero, cx| hero.show(true, cx));
        }
        Some(hero)
    }
}

impl Browse {
    pub fn body(&self, cx: &mut Context<Picker>) -> AnyView {
        let (path, listing) = (self.path.clone(), self.listing.clone());
        let picker = cx.entity().downgrade();
        let rail = cx.new(|_| super::rail_view::RailView {
            picker: picker.clone(),
        });
        let path = cx.new(|_| super::chrome_view::ChromeView {
            picker: picker.clone(),
            path,
        });
        AnyView::from(cx.new(|_| Body {
            picker,
            path,
            rail,
            listing,
            hero: None,
        }))
    }
}
