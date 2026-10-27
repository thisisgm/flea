//! Typed network locations enter the window's existing single-flight share opener.
use super::*;
use crate::g2::pl2::routing::{address, AddressRoute};

impl Shell {
    pub(super) fn network_address(
        &mut self,
        side: usize,
        uri: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_network(window, cx);
        let origin = self.network_origin(side);
        match address(uri, origin) {
            AddressRoute::Network(request) => {
                let Some(network) = self.network() else {
                    return;
                };
                let service = network.service.clone();
                service.update(cx, |service, cx| {
                    let now = service.now(cx);
                    service.open_share(request, now, cx);
                });
            }
            AddressRoute::Invalid(key) => {
                self.network_say(side, self.locale.catalog.tr(key), true, cx);
            }
            AddressRoute::Local => {}
        }
    }

    pub(super) fn network_say(
        &mut self,
        side: usize,
        text: String,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) {
            host.status = text.clone().into();
            if side == self.focus_side {
                self.status_messages(cx, |messages, now| {
                    messages.say(
                        &text,
                        error,
                        "",
                        flea::ui_logic_fb1::status::Hint::None,
                        now,
                    );
                });
            }
            cx.notify();
        }
    }
}
