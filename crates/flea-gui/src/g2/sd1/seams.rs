//! Join seams: the plain data and the one trait this unit needs from surfaces other units own.

// The backend pipe for the localsend requests (the backend's own producer, through G1.02's line).
pub trait Wire {
    // Queues one request line; false when the backend is gone.
    fn send(&self, line: String) -> bool;
}

// One provider's enable state as the st2 listing reports it; the host maps st2 Plugin to this.
// None (no listing yet) means the st2 join has not landed and gating stays off, as BASE draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderState {
    pub plugin: String,
    pub enabled: bool,
    pub failed: bool,
}

// A host-spawned process the unit cannot run itself: the tailscale status check and the two
// ShareLink steps run captured so the host reports their outcome; a Taildrop send runs detached,
// its own desktop notification the only result, as ui/Taildrop.qml runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunKind {
    // One status ask carries its generation, so a result is only ever read by the ask that made it.
    TaildropStatus(u64),
    DropboxStatus(u64),
    ShareLink,
    Clipboard,
}

// What one sd1 call asks of its host, which owns the transport, the processes and the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    // Run captured; the host reports back through the matching done() call.
    Run(RunKind, Vec<String>),
    // Run detached; nothing reports back.
    Spawn(Vec<String>),
    // Say on the status bar, error-styled when true.
    Say(String, bool),
}

// The facts snapshot a menu open hands in, parsed once from the providers facts line.
pub use flea::ui_logic_sd1::providers::Facts as FactsSnapshot;
