//! Committed typed lines: peek, open, locate, say via RouteHost.
use crate::path::{kind_of_peek, refused, resolve, PeekKind, Said};
use flea::ui_logic_pk2::parent_of;

// A committed line asks: backend peek, refusal sentence, or close.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitAsk {
    Peek(String),
    Say(Said, bool),
    Close,
}

// A committed line resolves and peeks before listing; refused lists nothing.
pub fn commit_line(text: &str, base: &str, home: &str) -> CommitAsk {
    let target = resolve(text, base, home);
    if target.is_empty() {
        if refused(text) {
            return CommitAsk::Say(Said::RefusedUri, false);
        }
        return CommitAsk::Close;
    }
    CommitAsk::Peek(target)
}

// Peek answers: folders open; save/folder files refuse to marks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Peeked {
    OpenFolder(String),
    ProbeMark(String),
    LocateFile { parent: String, leaf: String },
    Say(Said, bool),
    Stale,
}

pub fn peeked(
    asked: &str,
    path: &str,
    read_failed: bool,
    mode: u32,
    saving: bool,
    folder_mode: bool,
) -> Peeked {
    if path != asked {
        return Peeked::Stale;
    }
    match kind_of_peek(read_failed, mode) {
        PeekKind::Missing => Peeked::Say(Said::Missing(path.to_owned()), true),
        PeekKind::Folder => Peeked::OpenFolder(path.to_owned()),
        _ => {
            if saving || folder_mode {
                Peeked::ProbeMark(path.to_owned())
            } else {
                Peeked::LocateFile {
                    parent: parent_of(path).to_owned(),
                    leaf: path.to_owned(),
                }
            }
        }
    }
}

// Locate answers: the row, the filtered sentence, or nothing stale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    Row(usize),
    Filtered(String),
    Stale,
}

pub fn located(leaf: &str, leaf_parent: &str, directory: &str, path: &str, index: i64) -> Located {
    if leaf.is_empty() || path != leaf || directory != leaf_parent {
        return Located::Stale;
    }
    if index < 0 {
        let name = leaf.rsplit('/').next().unwrap_or(leaf);
        return Located::Filtered(name.to_owned());
    }
    Located::Row(index as usize)
}

// A peeked file is taken once its window holds the row.
pub fn take_row(leaf: &str, leaf_index: i64, held: bool) -> Option<usize> {
    if leaf.is_empty() || leaf_index < 0 || !held {
        return None;
    }
    Some(leaf_index as usize)
}

// A failed listing answers a waiting typed file with the error.
pub fn leaf_on_listing_failed(leaf: &str, listing_failed: bool) -> &str {
    if listing_failed {
        ""
    } else {
        leaf
    }
}

// What pk1 does with the route's answers.
pub trait RouteHost {
    fn open_folder(&mut self, path: String);
    fn locate_file(&mut self, path: String);
    fn move_cursor(&mut self, index: usize);
    fn activate_row(&mut self, index: usize);
    fn say_route(&mut self, said: Said, loud: bool);
}
