//! The configured remote menu uses the captured selection and the existing operation field.
use super::{seams::Wire, RcloneSend};
use crate::{g2::mn1::join::Captured, state::Shell};
use gpui::{Context, Window};

struct Pipe<'a>(Option<&'a crate::backend::Backend>);
impl Wire for Pipe<'_> {
    fn send(&self, line: String) -> bool {
        self.0.is_some_and(|backend| backend.line(line).is_ok())
    }
}

impl Shell {
    pub(crate) fn rclone_menu_opened(&mut self, side: usize) {
        let backend = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref());
        self.menu.rclone[side]
            .borrow_mut()
            .menu_opened(&Pipe(backend));
    }

    pub(crate) fn rclone_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let sender = self.menu.rclone[side].clone();
        if !sender.borrow_mut().on_line(line) {
            return false;
        }
        if let Ok(Some(flea::ui_logic_sd2::Event::Started { request, .. })) =
            flea::ui_logic_sd2::wire::parse(line)
        {
            let backend = self.panes[side]
                .as_ref()
                .and_then(|host| host.backend.as_ref());
            sender
                .borrow_mut()
                .cancel_closed_card(request, &Pipe(backend));
        }
        if self
            .menu
            .captured
            .as_ref()
            .is_some_and(|capture| capture.side == side)
        {
            self.provider_refresh(window, cx);
        }
        if let Some(view) = self.operation_view(side) {
            view.update(cx, |overlay, cx| {
                overlay.model.sync_rclone(&overlay.catalog);
                cx.notify();
            });
            self.flush_operations(side, cx);
        }
        let receipt = sender.borrow().receipt(&self.locale.catalog);
        if let Some(receipt) = receipt {
            self.owner_notice(
                side,
                &format!("{}{}", receipt.text, receipt.hint),
                false,
                flea::ui_logic_fb1::status::Hint::None,
                cx,
            );
            sender.borrow_mut().receipt_shown();
        }
        flea::ui_logic_sd2::wire::parse(line)
            .ok()
            .flatten()
            .is_some_and(|event| {
                matches!(
                    event,
                    flea::ui_logic_sd2::Event::Remotes { .. }
                        | flea::ui_logic_sd2::Event::Started { .. }
                )
            })
    }

    pub(crate) fn rclone_choose(
        &mut self,
        capture: &Captured,
        remote: &str,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = capture.side;
        let sender = self.menu.rclone[side].clone();
        let view = self.operation_overlay(side, window, cx);
        self.retire_menu(window, cx);
        // The sender opens its card only once there is a field to show it in.
        let Some(view) = view else {
            return;
        };
        if !sender.borrow_mut().choose(remote, paths) {
            return;
        }
        view.update(cx, |overlay, cx| {
            overlay
                .model
                .open_rclone(capture.id, sender, &overlay.catalog);
            cx.notify();
        });
        self.show_operation(&view, cx);
    }
}

pub(crate) type Target = std::rc::Rc<std::cell::RefCell<RcloneSend>>;
