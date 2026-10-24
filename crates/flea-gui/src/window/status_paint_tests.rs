//! Opt-in history of actual footer paints, including automatic effect-flush draws.
use super::Footer;
use gpui::{prelude::*, App, Div, SharedString};

// Test-only history retains every actual footer paint, including automatic effect-flush draws.
#[derive(Clone, Debug)]
pub(crate) struct FooterPaint {
    pub owner: gpui::EntityId,
    pub detail: SharedString,
    pub bounds: gpui::Bounds<gpui::Pixels>,
}

#[derive(Default)]
pub(crate) struct FooterPaints(pub std::cell::RefCell<Vec<FooterPaint>>);
impl gpui::Global for FooterPaints {}

pub(super) fn watch(footer: Div, host: &Footer, cx: &App) -> Div {
    footer.when(cx.try_global::<FooterPaints>().is_some(), |footer| {
        let line = host.message_line();
        let owner = line.entity_id();
        let detail = line.read(cx).drawn().detail.clone();
        footer.child(
            gpui::canvas(
                |bounds, _, _| bounds,
                move |_, bounds, _, cx| {
                    cx.global::<FooterPaints>()
                        .0
                        .borrow_mut()
                        .push(FooterPaint {
                            owner,
                            detail,
                            bounds,
                        });
                },
            )
            .absolute()
            .left_0()
            .top_0()
            .size_full(),
        )
    })
}
