//! The pure send logic every front end shares: Taildrop, Dropbox, LocalSend, ShareBrowser and ShareLink, ported from ui/js and ui/*.qml.
pub mod airdrop;
pub mod airdrop_ask;
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
