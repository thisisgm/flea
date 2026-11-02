//! Initial publication for the captured Open With card shares the file dialogs' hold policy.
use crate::{
    g2::op3::{initial, openwith::WIDTH, shell::Card},
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
}
