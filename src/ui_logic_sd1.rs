//! LocalSend and Taildrop/provider send picker (Send): the pure send logic every front end shares.
//! The backend already drives localsend-cli, the jobs list and the providers facts; mn1 already
//! draws every provider row and brand mark. This module owns what neither owns: Taildrop peer
//! eligibility and status, Dropbox account and status, LocalSend wording and discovery policy,
//! the ShareBrowser cursor and URI join, the ShareLink copy handshake, provider enable gating,
//! the localsend reply parser and one send's phase. Ported from ui/js/{LocalSend,Taildrop,Dropbox}.js,
//! ui/js/Ops.js send words, ui/{LocalSend,Taildrop,ShareBrowser,ShareLink}.qml and ui/js/Menu.js rows.
pub mod dropbox;
pub mod localsend;
pub mod providers;
pub mod send;
pub mod share;
pub mod taildrop;
pub mod wire;

pub use dropbox::{Account, Status as DropboxStatus};
pub use providers::{Provider, Role};
pub use send::{Phase, Send};
pub use share::{ShareBrowser, ShareLink};
pub use taildrop::{Peer, Status as TaildropStatus};
pub use wire::Event;
