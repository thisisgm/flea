//! Initial publication for the captured Open With card shares the file dialogs' hold policy.
use crate::{
    g2::open_with_properties::{initial, openwith::WIDTH, shell::Card},
    state::Shell,
};
use gpui::Context;

impl Shell {
    pub(super) fn initial_open_with_card(&mut self, cx: &mut Context<Self>) -> Option<Card> {
        let open = self.open_with.as_mut()?;
        open.frame.clear();
        open.initial.present(
            open.card.ready(),
            open.card.card(&self.locale.catalog),
            initial::loading(
                &self.locale.catalog,
                "dialogs.file.openwith.title_plain",
                WIDTH,
            ),
            cx,
        )
    }

    pub(super) fn load_open_with_icons(
        &self,
        card: &Card,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let icons = card
            .body
            .iter()
            .filter_map(|block| match block {
                crate::g2::open_with_properties::shell::Block::Apps(rows, _) => {
                    Some(rows.iter().filter_map(|row| match row {
                        crate::g2::open_with_properties::shell::AppRow::App { icon, .. } => {
                            Some(icon.clone())
                        }
                        _ => None,
                    }))
                }
                _ => None,
            })
            .flatten();
        crate::g2::context_menus::app_icons::load(
            icons,
            crate::g2::open_with_properties::shell::MARK * f32::from(self.active_typeface().body)
                / crate::g2::open_with_properties::shell::BOARD_BODY,
            window,
            cx,
        );
    }
}
