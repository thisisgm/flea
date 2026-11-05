//! Optional thumbnail request identity, captured once and carried by the existing job.
use super::{thumbed_line, State};
use crate::{
    json::escape,
    jsondoc::{self, Json},
};
use std::{
    io::Write,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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
        (Path::new(&tag.path) == path && tag.listing == listing).then(|| Self {
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
    video: VideoControl,
}
pub type Shared = Arc<Mutex<Replies>>;
pub type Registry = Vec<std::sync::Weak<Mutex<Replies>>>;

enum VideoControl {
    Disabled,
    Pending,
    Armed(OwnedFd),
    Cancelled,
}

/// Attach a subscriber to a matching job that can still return a result.
pub fn attach(st: &State, path: &Path, mtime: i64, reply: &Reply) -> bool {
    // Subscription eligibility uses the original work's version, never a later row mapping.
    let mut refusal = None;
    for shared in st
        .thumb_replies
        .iter()
        .rev()
        .filter_map(std::sync::Weak::upgrade)
    {
        let mut replies = shared.lock().unwrap();
        if replies.finished || matches!(replies.video, VideoControl::Cancelled) || replies.path != path {
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
        shared.lock().unwrap().refused.push(reply.clone());
    }
    false
}

/// Register the initial subscriber and the job's listing identity.
pub fn start(st: &mut State, path: PathBuf, mtime: i64, reply: Option<Reply>) -> Shared {
    st.thumb_replies.retain(|weak| weak.strong_count() > 0);
    let shared = Arc::new(Mutex::new(Replies {
        path,
        mtime,
        listing: st.generation,
        replies: reply.into_iter().collect(),
        refused: Vec::new(),
        finished: false,
        video: VideoControl::Disabled,
    }));
    st.thumb_replies.push(Arc::downgrade(&shared));
    shared
}

/// Mark a job eligible before it leaves the pool queue.
pub(in crate::backend) fn begin_video(shared: &Shared) {
    shared.lock().unwrap().video = VideoControl::Pending;
}

/// Tell the worker whether a pending cancellation already retired this job.
pub(in crate::backend) fn video_cancelled(shared: &Shared) -> bool {
    matches!(shared.lock().unwrap().video, VideoControl::Cancelled)
}

/// Install the reply socket unless the request was cancelled before dispatch.
pub(in crate::backend) fn arm_video(shared: &Shared, reply: &OwnedFd) -> bool {
    let mut context = shared.lock().unwrap();
    if matches!(context.video, VideoControl::Cancelled) {
        return false;
    }
    context.video = reply.try_clone().map_or(VideoControl::Disabled, VideoControl::Armed);
    true
}

/// Retire the control socket and keep a recorded cancellation terminal.
pub(in crate::backend) fn finish_video(shared: &Shared) -> bool {
    let mut context = shared.lock().unwrap();
    if matches!(context.video, VideoControl::Cancelled) {
        return true;
    }
    context.video = VideoControl::Disabled;
    false
}

/// Cancel an eligible video after its last subscriber has left.
pub(in crate::backend) fn cancel_video(st: &State, path: &Path) -> bool {
    let Some(mtime) = st.thumb_mtimes.get(path) else { return false };
    for shared in st.thumb_replies.iter().rev().filter_map(std::sync::Weak::upgrade) {
        let mut context = shared.lock().unwrap();
        if context.path != path || context.listing != st.generation || context.mtime != *mtime
            || context.finished || !context.replies.is_empty() || matches!(context.video, VideoControl::Cancelled)
        {
            continue;
        }
        let accepted = match &context.video {
            VideoControl::Pending => true,
            VideoControl::Armed(reply) => crate::backend::fdpass::send_byte(reply, crate::backend::thumbworker::CANCEL_VIDEO).is_ok(),
            VideoControl::Disabled | VideoControl::Cancelled => false,
        };
        if !accepted {
            return false;
        }
        context.video = VideoControl::Cancelled;
        return true;
    }
    false
}

pub fn retire_row(st: &State, row: usize) {
    for shared in st.thumb_replies.iter().filter_map(std::sync::Weak::upgrade) {
        shared
            .lock()
            .unwrap()
            .replies
            .retain(|reply| reply.row != row);
    }
}

pub fn retire_all(st: &mut State) {
    st.thumb_replies.retain(|weak| {
        if let Some(shared) = weak.upgrade() {
            shared.lock().unwrap().replies.clear();
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
        let mut context = shared.lock().unwrap();
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
