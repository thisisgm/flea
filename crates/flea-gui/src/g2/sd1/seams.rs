//! Join seams: the plain data and the one trait this unit needs from surfaces other units own.

// The backend pipe for the localsend requests (the backend's own producer, through G1.02's line).
pub trait Wire {
    // Queues one request line; false when the backend is gone.
    fn send(&self, line: String) -> bool;
}

// One provider's st2 enable state; None means the st2 listing has not landed and gating stays off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderState {
    pub plugin: String,
    pub enabled: bool,
    pub failed: bool,
}

// A host-spawned process: status and ShareLink runs are captured, a Taildrop send reports only its exit status.
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
    // Run on its own thread; a start failure or non-zero exit says the sentence it carries.
    Spawn(Vec<String>, String),
    // Say on the status bar, error-styled when true.
    Say(String, bool),
}

// The facts snapshot a menu open hands in, parsed once from the providers facts line.
pub use flea::ui_logic_sd1::providers::Facts as FactsSnapshot;
