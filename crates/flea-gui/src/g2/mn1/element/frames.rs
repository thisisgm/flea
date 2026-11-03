//! A menu draws one frame per card, each under its own Reveal.qml opacity; a frame at opacity 0 draws nothing.
use super::{paint_card, MenuElement, MenuInk};
use crate::{g2::mn1::menu::Menu, list::Palette, palette::Theme};
use gpui::{canvas, div, prelude::*, App, Window};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy)]
enum Part {
    Card,
    Flyout,
    Retired,
}

impl RenderOnce for MenuElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let parts = if self.faded {
            vec![(Part::Retired, 1.)]
        } else {
            let now = cx.background_executor().now();
            let mut menu = self.menu.borrow_mut();
            let moving = menu.sample_fades(now, cx.reduce_motion());
            let mut parts = Vec::new();
            if let Some((_, alpha)) = menu.revealed_card() {
                parts.push((Part::Card, alpha));
            }
            if menu.flyout.is_some() {
                parts.push((Part::Flyout, menu.fades.flyout_frame.opacity));
            }
            drop(menu);
            if moving {
                crate::feedback::spinner::request_frame(window, cx);
            }
            parts
        };
        let radius = self.radius;
        // GPUI splits a quad with no visible ink into four strips, so a frame nobody can see paints nothing at all.
        div().absolute().size_full().children(
            parts
                .into_iter()
                .filter(|(_, alpha)| *alpha > 0.)
                .map(|(part, alpha)| {
                    let menu = Rc::clone(&self.menu);
                    div().absolute().size_full().opacity(alpha).child(
                        canvas(
                            |_, window, cx| ink(window, cx),
                            move |bounds, ink, window, cx| {
                                paint(&menu, part, radius, &ink, bounds.origin, window, cx)
                            },
                        )
                        .size_full(),
                    )
                }),
        )
    }
}

pub(in crate::g2::mn1) fn ink(window: &Window, cx: &App) -> MenuInk {
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    // A failed provider and a destructive row read in the theme's error role.
    MenuInk {
        surface: theme.surface,
        error: theme.error,
        tags: theme.tag_menu_inks,
        ..MenuInk::from_palette(&Palette::inherited(&window.text_style(), theme))
    }
}

fn paint(
    menu: &Rc<RefCell<Menu>>,
    part: Part,
    radius: gpui::Pixels,
    ink: &MenuInk,
    origin: gpui::Point<gpui::Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let menu = menu.borrow();
    let (card, lit) = match part {
        Part::Card => (
            menu.revealed_card().map(|(card, _)| card),
            menu.flyout.as_ref().and_then(|flyout| flyout.parent),
        ),
        Part::Flyout => (menu.flyout.as_ref().map(|flyout| &flyout.card), None),
        Part::Retired => (menu.faded.as_ref().map(|faded| &faded.flyout.card), None),
    };
    if let Some(card) = card {
        paint_card(card, lit, radius, ink, origin, window, cx);
    }
}
