//! A share chosen from a bare server inherits that browser's successful session credentials.
use super::{
    mount::{Effect, Mounts, Request},
    service::Service,
};
use crate::g2::pl1::secret::Secret;
use flea::ui_logic_pl2::browser;
use gpui::Context;

#[derive(Debug)]
pub(super) struct BrowserSession {
    pub(super) uri: String,
    pub(super) names: Vec<String>,
    pub(super) password: Secret,
}

impl Mounts {
    pub fn close_browser(&mut self, origin: u64) {
        self.browsers.remove(&origin);
    }
    pub(super) fn remember_browser(&mut self, request: &mut Request, names: &[String]) {
        self.browsers.insert(
            request.origin,
            BrowserSession {
                uri: request.uri.clone(),
                names: names.to_vec(),
                password: request.password.take(),
            },
        );
    }
    pub fn has_credentials(&self, origin: u64) -> bool {
        self.browsers
            .get(&origin)
            .is_some_and(|b| !b.password.is_empty())
            || self
                .flight
                .as_ref()
                .is_some_and(|f| f.request.origin == origin && !f.request.password.is_empty())
            || self
                .bridges
                .iter()
                .any(|b| b.request.origin == origin && !b.request.password.is_empty())
    }
    pub fn open_child_share(
        &mut self,
        uri: String,
        label: String,
        origin: u64,
        now: u64,
    ) -> Vec<Effect> {
        let valid = self.browsers.get(&origin).is_some_and(|b| {
            browser::child_name(&b.uri, &uri).is_some_and(|name| b.names.contains(&name))
        });
        if !valid {
            return vec![Effect::Say {
                note: flea::ui_logic_pl1::Note::new("network.invalid_address", &[]),
                origin,
            }];
        }
        let password = self.browsers.remove(&origin).unwrap().password;
        let authenticated = !password.is_empty();
        self.open_share(
            Request {
                id: 0,
                uri,
                label,
                origin,
                mounted: false,
                authenticated,
                password,
            },
            now,
        )
    }
}
impl Service {
    pub fn close_browser(&mut self, origin: u64) {
        self.mounts.close_browser(origin);
    }
    pub fn open_child_share(
        &mut self,
        uri: String,
        label: String,
        origin: u64,
        cx: &mut Context<Self>,
    ) {
        let now = self.now(cx);
        let effects = self.mounts.open_child_share(uri, label, origin, now);
        self.apply(effects, now, cx);
    }
}
