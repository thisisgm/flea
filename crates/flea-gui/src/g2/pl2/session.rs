//! The session's passwords by normalized address, so a reopen mounts without asking again, as v0.3.12 did.
use super::mount::{Mounts, Phase, Request};
use crate::g2::pl1::secret::Secret;

impl Mounts {
    /// A request with no password of its own takes the one this session last used for its address.
    pub(super) fn recall(&self, request: &mut Request) {
        if request.mounted || !request.password.is_empty() {
            return;
        }
        if let Some(password) = self.passwords.get(&request.uri) {
            request.password = Secret::from_bytes(password.expose());
        }
    }
    /// Only a flight leaving Auth for Info has had its password taken by the server.
    pub(super) fn keep_flight(&mut self) {
        if let Some(f) = self
            .flight
            .as_ref()
            .filter(|f| f.phase == Phase::Auth && !f.request.password.is_empty())
        {
            let password = Secret::from_bytes(f.request.password.expose());
            self.passwords.insert(f.request.uri.clone(), password);
        }
    }
    /// A refused password is dropped, since replaying it would count against the account.
    pub(super) fn forget_refused(&mut self, request: &Request, key: &str) {
        if key == "network.password_refused" {
            self.passwords.remove(&request.uri);
        }
    }
}
