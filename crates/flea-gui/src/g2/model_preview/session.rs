//! One slot's request lifecycle: each open takes a token no other request in the process has had, a close cancels it, and only the live token's reply applies.
use flea::ui_logic_model_preview::wire::{self, Reply};
use std::sync::atomic::{AtomicU64, Ordering};

// Tokens come from one counter for the whole process, so a view mounted later never reuses a token an earlier view's late reply carries.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    slot: &'static str,
    token: u64,
    live: bool,
}

impl Session {
    pub fn new(slot: &'static str) -> Self {
        Self {
            slot,
            token: 0,
            live: false,
        }
    }

    pub fn slot(&self) -> &'static str {
        self.slot
    }

    pub fn token(&self) -> u64 {
        self.token
    }

    /// The request line for `path`; any earlier request on the slot is superseded by the new token.
    pub fn open(&mut self, path: &str, plate: Option<usize>) -> String {
        self.token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        self.live = true;
        wire::request(self.slot, self.token, path, plate)
    }

    /// The cancel line for the live request, none when nothing is in flight.
    pub fn close(&mut self) -> Option<String> {
        std::mem::take(&mut self.live).then(|| wire::cancel(self.slot, self.token))
    }

    /// Whether `reply` answers the live request; a later reply for the same token is refused.
    pub fn accept(&mut self, reply: &Reply) -> bool {
        let ours = self.live && reply.slot == self.slot && reply.token == self.token;
        if ours {
            self.live = false;
        }
        ours
    }

    pub fn in_flight(&self) -> bool {
        self.live
    }
}
