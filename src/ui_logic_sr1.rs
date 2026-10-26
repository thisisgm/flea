//! Search's front-end logic shared by the window and the TUI: the live session, its wire and its words.
mod matching;
mod contents;
mod session;
mod text;
mod types;
mod wire;

pub use matching::{base, clean_paste, is_printable, location, run, tilde};
pub use session::Session;
pub use types::{Effect, Key, Kind, ListingState, Mode, Pane, Scope, Stamps};
pub use text::{frame_kind, nothing_matches, note, status_line, scope_label, way_out, Frame};
pub use wire::{parse_line, Line, Request, Wire};
pub use contents::{contents_hex, ContentsMatch, ContentsMatches};

#[cfg(test)]
mod matching_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod session_more_tests;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod wire_tests;
#[cfg(test)]
mod contents_tests;
