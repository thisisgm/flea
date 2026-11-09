//! Optional thumbnail request identity, captured once and carried by the existing job.
use super::{thumbed_line, State};
use crate::{
    json::escape,
    jsondoc::{self, Json},
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

#[derive(Debug)]
pub struct Invalid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub token: String,
    pub path: String,
    pub listing: u64,
}

impl Tag {
    // Sample input: {"c":"thumb","rows":[0],"thumbToken":"7","thumbPath":"/files/photo.png","thumbListing":1}.
    pub fn parse(line: &str) -> Result<Option<Self>, Invalid> {
        let document = jsondoc::parse(line).map_err(|_| Invalid)?;
        let keys = ["thumbToken", "thumbPath", "thumbListing"];
        if keys.iter().all(|key| document.get(key).is_none()) {
            return Ok(None);
        }
        let token = document
            .get("thumbToken")
            .and_then(Json::as_str)
            .filter(|s| !s.is_empty())
            .ok_or(Invalid)?;
        let path = document
            .get("thumbPath")
            .and_then(Json::as_str)
            .filter(|s| s.starts_with('/'))
            .ok_or(Invalid)?;
        let Some(Json::Num(listing)) = document.get("thumbListing") else {
            return Err(Invalid);
        };
        let listing = listing.parse().map_err(|_| Invalid)?;
        Ok(Some(Self {
            token: token.into(),
            path: path.into(),
            listing,
        }))
    }

    pub fn line(&self, row: usize, file: &str, ms: f64) -> String {
        let mut line = thumbed_line(row, file, ms);
        line.pop();
        line.push_str(&format!(
            r#","thumbToken":"{}","thumbPath":"{}","thumbListing":{}}}"#,
            escape(&self.token),
            escape(&self.path),
            self.listing
        ));
        line
    }
}

pub struct Options {
    pub cache_only: bool,
    pub identity: Option<Tag>,
}
impl From<bool> for Options {
    fn from(cache_only: bool) -> Self {
        Self {
            cache_only,
            identity: None,
        }
    }
}

// The path and row are request-time facts, never recovered from a later row mapping.
#[derive(Clone)]
pub struct Reply {
    tag: Tag,
    row: usize,
}
impl Reply {
    pub fn capture(tag: &Tag, row: usize, path: &Path, listing: u64) -> Option<Self> {
        (&*flea::bytename::view(&tag.path) == path && tag.listing == listing).then(|| Self {
            tag: tag.clone(),
            row,
        })
    }
    pub fn line(&self, file: &str, ms: f64) -> String {
        self.tag.line(self.row, file, ms)
    }
}

pub struct Replies {
    path: PathBuf,
    mtime: i64,
    listing: u64,
    replies: Vec<Reply>,
    refused: Vec<Reply>,
    finished: bool,
}
pub type Shared = Arc<Mutex<Replies>>;
pub type Registry = Vec<std::sync::Weak<Mutex<Replies>>>;

// The lists hold plain values with no invariant a panic can break mid-update, so a poisoned lock still yields them.
fn lock(shared: &Mutex<Replies>) -> MutexGuard<'_, Replies> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn attach(st: &State, path: &Path, mtime: i64, reply: &Reply) -> bool {
    // Subscription eligibility uses the original work's version, never a later row mapping.
    let mut refusal = None;
    for shared in st
        .thumb_replies
        .iter()
        .rev()
        .filter_map(std::sync::Weak::upgrade)
    {
        let mut replies = lock(&shared);
        if replies.finished || replies.path != path {
            continue;
        }
        // Already answered refusals are terminal, so a retry cannot publish or subscribe again.
        if replies
            .replies
            .iter()
            .chain(&replies.refused)
            .any(|old| old.tag == reply.tag && old.row == reply.row)
        {
            return true;
        }
        if replies.mtime == mtime && replies.listing == reply.tag.listing {
            replies.replies.push(reply.clone());
            return true;
        }
        refusal.get_or_insert_with(|| shared.clone());
    }
    if let Some(shared) = refusal {
        lock(&shared).refused.push(reply.clone());
    }
    false
}

pub fn start(st: &mut State, path: PathBuf, mtime: i64, reply: Option<Reply>) -> Shared {
    st.thumb_replies.retain(|weak| weak.strong_count() > 0);
    let shared = Arc::new(Mutex::new(Replies {
        path,
        mtime,
        listing: st.generation,
        replies: reply.into_iter().collect(),
        refused: Vec::new(),
        finished: false,
    }));
    st.thumb_replies.push(Arc::downgrade(&shared));
    shared
}

pub fn retire_row(st: &State, row: usize) {
    for shared in st.thumb_replies.iter().filter_map(std::sync::Weak::upgrade) {
        lock(&shared).replies.retain(|reply| reply.row != row);
    }
}

pub fn retire_all(st: &mut State) {
    st.thumb_replies.retain(|weak| {
        if let Some(shared) = weak.upgrade() {
            lock(&shared).replies.clear();
            true
        } else {
            false
        }
    });
}

pub fn complete(
    out: &mut impl Write,
    st: &mut State,
    shared: &Option<Shared>,
    file: &str,
    ms: f64,
) -> bool {
    let Some(shared) = shared else { return false };
    let replies = {
        let mut context = lock(shared);
        context.finished = true;
        std::mem::take(&mut context.replies)
    };
    st.thumb_replies.retain(|weak| {
        weak.upgrade()
            .is_some_and(|live| !Arc::ptr_eq(&live, shared))
    });
    let observed = !replies.is_empty();
    for reply in replies {
        writeln!(out, "{}", reply.line(file, ms)).ok();
    }
    observed
}
