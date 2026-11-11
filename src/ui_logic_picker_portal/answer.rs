// Which answer a press asks for, decided from plain state so every chooser front end refuses alike.
use super::{file_uri, folder_mode, is_recent, saving, valid_name, Mode, Request};

// A checked identity: its path, its reviewed size and the URI it was first encoded with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub path: String,
    pub bytes: u64,
    pub uri: String,
}

impl Mark {
    // Reviewed marks keep the order the backend returned and the URI an earlier mark already carried.
    pub fn reviewed(previous: &[Mark], reviewed: &[(String, u64)]) -> Vec<Mark> {
        reviewed
            .iter()
            .map(|(path, bytes)| Mark {
                path: path.clone(),
                bytes: *bytes,
                uri: previous
                    .iter()
                    .find(|mark| &mark.path == path && !mark.uri.is_empty())
                    .map_or_else(|| file_uri(path), |mark| mark.uri.clone()),
            })
            .collect()
    }
}

// The backend's answer to a save probe, tied to the folder and name it was asked about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveReview {
    pub folder: String,
    pub name: String,
    pub collision: bool,
    pub review: u64,
    pub path: String,
}

// The window's answer state at the moment of a press; the listing facts come from the listing's owner.
#[derive(Clone, Copy, Debug)]
pub struct AcceptState<'a> {
    pub req: &'a Request,
    pub backend_unavailable: bool,
    pub accept_marks: bool,
    pub mark_request: u64,
    pub review_request: u64,
    pub marks: usize,
    pub location: &'a str,
    pub listing_loading: bool,
    // The cursor row's name when the listing is ready, unfailed and the cursor stands on a file.
    pub cursor_file: Option<&'a str>,
    pub save_name: &'a str,
    pub review: Option<&'a SaveReview>,
    pub review_asked: bool,
}

impl AcceptState<'_> {
    pub fn submitting(&self) -> bool {
        self.accept_marks || self.review_request > 0
    }
    fn busy(&self) -> bool {
        self.backend_unavailable || self.submitting() || self.mark_request > 0
    }
    // The probe answers only the folder and name it was asked about.
    pub fn save_ready(&self) -> bool {
        self.review
            .is_some_and(|review| review.folder == self.location && review.name == self.save_name)
    }
    pub fn save_collision(&self) -> bool {
        self.save_ready() && self.review.is_some_and(|review| review.collision)
    }
    // Replace, its error line and its role read this alone.
    pub fn collision_review(&self) -> bool {
        self.review_asked && self.save_collision()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceptStep {
    // A check, a submission or a lost backend already owns the answer, so the press does nothing.
    Busy,
    SaveNameEmpty,
    SaveNameRefused,
    ProbeSave,
    // The name is taken: the review starts and the field selects the stem; Replace answers it.
    AskReview,
    // The review already stands; only Replace overwrites.
    Reviewing,
    SubmitSave,
    // Recent is not a directory, so a folder request standing there has nothing to hand back unasked.
    RecentFolder,
    MarkFolder(String),
    MarkCursor(String),
    // Nothing checked and the cursor is not on a file.
    Nothing,
    ValidateMarks,
}

pub fn accept_step(state: &AcceptState) -> AcceptStep {
    if state.busy() {
        return AcceptStep::Busy;
    }
    if saving(state.req) {
        return if state.save_name.is_empty() {
            AcceptStep::SaveNameEmpty
        } else if !valid_name(state.save_name) {
            AcceptStep::SaveNameRefused
        } else if !state.save_ready() {
            AcceptStep::ProbeSave
        } else if state.save_collision() {
            if state.review_asked {
                AcceptStep::Reviewing
            } else {
                AcceptStep::AskReview
            }
        } else {
            AcceptStep::SubmitSave
        };
    }
    if state.marks > 0 {
        return AcceptStep::ValidateMarks;
    }
    if folder_mode(state.req) {
        return if is_recent(state.location) {
            AcceptStep::RecentFolder
        } else {
            AcceptStep::MarkFolder(state.location.to_owned())
        };
    }
    match state.cursor_file {
        Some(name) => AcceptStep::MarkCursor(super::row_path(state.location, name)),
        None => AcceptStep::Nothing,
    }
}

// Replace is the only answer that overwrites, and only while the asked review stands.
pub fn replace_allowed(state: &AcceptState) -> bool {
    !state.busy() && state.collision_review()
}

pub fn can_accept(state: &AcceptState) -> bool {
    if state.busy() {
        return false;
    }
    if saving(state.req) {
        return state.save_ready();
    }
    let folder = folder_mode(state.req);
    state.marks > 0
        || (!folder && state.cursor_file.is_some())
        || (folder && !is_recent(state.location) && !state.listing_loading)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoubleAction {
    Open,
    Mark,
    Accept,
    MarkAccept,
    None,
}

// A second tap counts only on the same path, so a list rebuilt between taps never marks or sends another row.
pub fn same_tap(first: &str, row: &str) -> bool {
    !row.is_empty() && row == first
}

// `row_directory` is None when no row stands at the index.
pub fn double_action(
    req: &Request,
    row_directory: Option<bool>,
    row_path: &str,
    first_path: &str,
    marked: bool,
) -> DoubleAction {
    let Some(is_directory) = row_directory else {
        return DoubleAction::None;
    };
    if is_directory {
        return DoubleAction::Open;
    }
    if req.mode != Mode::Open || req.directory || !same_tap(first_path, row_path) {
        return DoubleAction::None;
    }
    if req.multiple {
        DoubleAction::Mark
    } else if marked {
        DoubleAction::Accept
    } else {
        DoubleAction::MarkAccept
    }
}
