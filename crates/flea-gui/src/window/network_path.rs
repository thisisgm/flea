//! Typed network locations enter the window's existing single-flight share opener.
use super::*;
use crate::g2::pl2::routing::{address, AddressRoute};

impl Shell {
    /// A successful connect retires its own earlier failures, leaving other producers' errors alone.
    pub(super) fn network_connected(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(network) = self.network() else {
            return;
        };
        let errors = std::mem::take(&mut network.connect_errors[side]);
        let resolved: Vec<_> = errors
            .iter()
            .filter(|text| {
                !network
                    .connect_errors
                    .iter()
                    .flatten()
                    .any(|other| other == *text)
            })
            .cloned()
            .collect();
        if let Some(host) = self.panes[side].as_mut() {
            if errors.iter().any(|text| host.status.as_ref() == text) {
                host.status = Default::default();
            }
        }
        self.status_messages(cx, |messages, now| {
            for text in resolved {
                messages.forget(&text, now);
            }
        });
    }

    pub(super) fn network_connect_error(&mut self, side: usize, text: &str) {
        if let Some(network) = self.network() {
            if !network.connect_errors[side].iter().any(|old| old == text) {
                network.connect_errors[side].push(text.to_owned());
            }
        }
    }

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
