//! op1 presents the existing Trash review; pl3 alone owns its checks, request ids and destructive token.
use super::seams::Card;
use crate::{
    g2::op1::{host::Overlay, palette::CardPalette},
    palette::Theme,
    state::Shell,
};
use flea::ui_logic_op1::trash::{Review, Scope};
use gpui::{AppContext, Context, Window};

impl Shell {
    /// Review Empty Trash without replacing the current folder with the Trash pane.
    pub(crate) fn empty_trash_from_rail(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mount_trash(window, cx);
        let Some(host) = self.trash.as_mut() else {
            return;
        };
        let pane = host.pane.read(cx);
        let view = pane.view();
        if view.confirming || view.operation_active {
            return;
        }
        if !view.opened {
            host.side = side;
            // Cancel a running sweep on the same backend session that started it.
            if !pane.host.sweeping() {
                host.backend_side = side;
            }
        }
        host.sweep_timer = None;
        host.pane.update(cx, |pane, cx| {
            pane.watch(cx);
            pane.host.empty_from_rail();
            pane.pump(cx);
        });
        self.flush_trash(window, cx);
    }

    pub(super) fn trash_card(
        &mut self,
        card: Option<Card>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.trash.as_mut() else {
            return;
        };
        let Some(card) = card else {
            if host.pane.read(cx).view().confirming && host.pane.read(cx).view().busy() {
                if let Some(overlay) = &self.overlay {
                    overlay.update(cx, |overlay, cx| {
                        if let Some(crate::g2::op1::model::Dialog::Delete { busy, focus, .. }) =
                            &mut overlay.model.dialog
                        {
                            *busy = true;
                            *focus = flea::ui_logic_op1::trash::Button::Cancel;
                        }
                        cx.notify();
                    });
                }
                return;
            }
            host.card_token = None;
            if let Some(overlay) = &self.overlay {
                overlay.update(cx, |overlay, cx| {
                    if overlay.model.external_delete.take().is_some() {
                        overlay.model.dialog = None;
                    }
                    cx.notify();
                });
            }
            return;
        };
        let Some(snapshot) = host.pane.read(cx).view().confirmation.as_ref() else {
            return;
        };
        let review = Review {
            count: snapshot.count.and_then(|count| u64::try_from(count).ok()),
            bytes: snapshot.bytes,
            all: snapshot.all,
            token: snapshot.token.to_string(),
            stale: false,
        };
        host.card_token = Some(card.token);
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, self.corner);
        let catalog = self.locale.catalog.clone();
        let overlay = self
            .overlay
            .get_or_insert_with(|| cx.new(|_| Overlay::new(catalog, palette, "default")))
            .clone();
        overlay.update(cx, |overlay, cx| {
            overlay
                .model
                .present_external_delete(review, Scope::TrashItems, card.token);
            overlay.schedule(cx);
            cx.notify();
        });
        let watch = cx.observe_in(&overlay, window, |shell, overlay, window, cx| {
            let answers = overlay.update(cx, |overlay, _| {
                std::mem::take(&mut overlay.model.external_answers)
            });
            let Some(host) = &shell.trash else {
                return;
            };
            let pane = host.pane.clone();
            let mut answered = false;
            for (shown, token) in answers {
                if shell
                    .trash
                    .as_ref()
                    .is_some_and(|host| host.card_token == Some(shown))
                {
                    pane.update(cx, |pane, cx| {
                        if token.is_none() && pane.view().confirmation.is_none() {
                            pane.cancel_held_card(cx);
                        } else {
                            pane.card_answered(token, cx);
                        }
                    });
                    answered = true;
                }
            }
            if answered {
                let focus = if pane.read(cx).view().opened {
                    pane.read(cx).focus.clone()
                } else {
                    shell.focus.clone()
                };
                window.focus(&focus, cx);
                shell.flush_trash(window, cx);
            }
        });
        self.trash.as_mut().expect("mounted").overlay_watch = Some(watch);
        cx.notify();
    }
}
