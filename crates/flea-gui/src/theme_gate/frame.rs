//! Opt-in test-only receipt after the deferred cards, recording inks used by actual paints.
use crate::g2::file_ops::palette::CardPalette;
use crate::{list::Palette, palette::Theme, state::Shell};
use gpui::{canvas, deferred, prelude::*, App, Entity, Hsla};
use std::cell::{Cell, RefCell};

#[derive(Default)]
pub(crate) struct Frames {
    pub frames: RefCell<Vec<Frame>>,
    pub card: Cell<Option<CardPalette>>,
}
impl gpui::Global for Frames {}

pub(crate) struct Frame {
    pub theme: Theme,
    pub panes: Vec<Palette>,
    pub rail: Option<Palette>,
    pub footer: Option<Hsla>,
    pub card: Option<CardPalette>,
}

pub(crate) fn probe(shell: Entity<Shell>, cx: &App) -> Option<gpui::AnyElement> {
    cx.try_global::<Frames>()?;
    Some(
        deferred(
            canvas(
                |_, _, _| (),
                move |_, _, _, cx| {
                    let shell = shell.read(cx);
                    let frame = Frame {
                        theme: *cx.global::<Theme>(),
                        panes: shell
                            .panes
                            .iter()
                            .flatten()
                            .filter_map(|host| host.list.borrow().drawn.palette)
                            .collect(),
                        rail: shell
                            .rail
                            .as_ref()
                            .and_then(|join| join.places.read(cx).laid.borrow().palette),
                        footer: shell.surfaces[0]
                            .footer
                            .as_ref()
                            .and_then(|footer| footer.read(cx).painted.get("counts"))
                            .map(|text| text.color),
                        card: cx.global::<Frames>().card.get(),
                    };
                    cx.global::<Frames>().frames.borrow_mut().push(frame);
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .with_priority(usize::MAX)
        .into_any_element(),
    )
}

pub(crate) fn card_probe(palette: CardPalette, cx: &App) -> Option<gpui::AnyElement> {
    cx.try_global::<Frames>()?;
    Some(
        canvas(
            |_, _, _| (),
            move |_, _, _, cx| {
                cx.global::<Frames>().card.set(Some(palette));
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element(),
    )
}
