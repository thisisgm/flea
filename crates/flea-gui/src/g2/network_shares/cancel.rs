//! Retiring a pane cancels only its own open, including a bridge detached from the current flight.
use super::{
    mount::{Effect, Mounts},
    service::Service,
};
use gpui::Context;

impl Mounts {
    pub fn retire_location(&mut self, uri: &str) -> Vec<Effect> {
        let mut origins: Vec<_> = self
            .browsers
            .iter()
            .filter(|(_, browser)| {
                flea::ui_logic_network_shares::browser::same_server(&browser.uri, uri)
            })
            .map(|(origin, _)| *origin)
            .collect();
        if let Some(f) = &self.flight {
            if flea::ui_logic_network_shares::mounts::normalize(&f.request.uri)
                == flea::ui_logic_network_shares::mounts::normalize(uri)
            {
                origins.push(f.request.origin);
            }
        }
        origins.sort_unstable();
        origins.dedup();
        origins
            .into_iter()
            .flat_map(|origin| self.cancel_origin(origin))
            .collect()
    }
    pub fn cancel_origin(&mut self, origin: u64) -> Vec<Effect> {
        self.browsers.remove(&origin);
        let mut generations = Vec::new();
        if self
            .flight
            .as_ref()
            .is_some_and(|f| f.request.origin == origin)
        {
            generations.push(self.generation);
            self.flight = None;
            self.generation += 1;
        }
        self.bridges.retain(|b| {
            if b.request.origin == origin {
                generations.push(b.generation);
                false
            } else {
                true
            }
        });
        generations.sort_unstable();
        generations.dedup();
        generations.into_iter().map(Effect::Kill).collect()
    }
}
impl Service {
    pub fn cancel_origin(&mut self, origin: u64, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let effects = self.mounts.cancel_origin(origin);
        self.apply(effects, now, cx);
    }
}
