// Trash joins the listing's thumbnail queue only after its backing identity reads back.
use flea::bytename::ExactText as _;
use super::state::{State, Tables};
use super::thumbcache::{Cache, Hit};
use super::thumbs::{Job, Pool};
use super::trashdelete::Reviewed;
use crate::json::{escape, field_bool, field_str, field_usize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) struct Ask {
    id: usize,
    path: PathBuf,
    mtime: i64,
    identity: String,
    reviewed: Reviewed,
    cancelled: bool,
}

fn reply(out: &mut impl Write, id: usize, file: &str) {
    writeln!(
        out,
        r#"{{"t":"trashthumbed","id":{},"file":"{}"}}"#,
        id,
        escape(file)
    )
    .ok();
}

// Sample input: {"c":"trashthumb","id":41,"original":"/home/user/Pictures/photo.jpg","path":"/home/user/.local/share/Trash/files/photo.jpg","provider":"l2049:123","identity":"Identity { dev: 2049, ino: 123, size: 4096, mtime: 1760000000, nanos: 0, mode: 33188 }|Identity { dev: 2049, ino: 124, size: 64, mtime: 1760000000, nanos: 0, mode: 33188 }","mtime":1760000000,"cacheOnly":false}
fn inspect(line: &str, tb: &Tables) -> Option<(Reviewed, PathBuf, i64, String)> {
    let path = flea::bytename::exact(field_str(line, "path")?);
    let original = field_str(line, "original")?;
    let mime = tb
        .mime
        .lookup(flea::bytename::display(&original).rsplit('/').next()?)?;
    if !mime.starts_with("image/") || tb.thumbs.for_mime(mime, &tb.aliases).is_none() {
        return None;
    }
    let provider = field_str(line, "provider")?;
    let expected = field_str(line, "identity")?;
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let owned = path.clone();
    let (reviewed, path, mtime) = super::iomount::call(&path, &body, "trashthumb", move || {
        let reviewed = Reviewed::inspect(owned, &provider).ok()?;
        if reviewed.selection_identity() != expected {
            return None;
        }
        let (path, mtime) = reviewed.thumbnail()?;
        Some((reviewed, path, mtime))
    })
    .ok()??;
    Some((reviewed, path, mtime, mime.into()))
}

pub(crate) fn request(
    out: &mut impl Write,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    cache: &Cache,
    line: &str,
) {
    let Some(id) = field_usize(line, "id") else {
        return;
    };
    if st
        .trash_asked
        .iter()
        .any(|ask| ask.id == id && !ask.cancelled)
    {
        return;
    }
    let cancelled = st
        .trash_asked
        .iter()
        .position(|ask| ask.id == id)
        .map(|at| st.trash_asked.remove(at));
    let Some((reviewed, path, mtime, mime)) = inspect(line, tb) else {
        reply(out, id, "");
        return;
    };
    let identity = reviewed.selection_identity();
    match cache.lookup_for(&path, mtime, tb.thumbs.fingerprint(), Some(&identity)) {
        Hit::Ready(file) => {
            reply(out, id, &file.exact_text());
            return;
        }
        Hit::Failed => {
            reply(out, id, "");
            return;
        }
        Hit::Miss if field_bool(line, "cacheOnly") => {
            reply(out, id, "");
            return;
        }
        Hit::Miss => {}
    }
    super::thumbdefer::pump(out, st, pool);
    if st.trash_asked.len() >= super::thumbdefer::MAX_DEFERRED || st.deferred.full() {
        reply(out, id, "");
        return;
    }
    let shared = st
        .trash_asked
        .iter()
        .chain(cancelled.iter())
        .any(|ask| ask.path == path && ask.mtime == mtime && ask.identity == identity);
    st.trash_asked.push(Ask {
        id,
        path: path.clone(),
        mtime,
        identity: identity.clone(),
        reviewed,
        cancelled: false,
    });
    if !shared {
        st.outstanding += 1;
        st.deferred.enqueue(Job {
            replies: None,
            path,
            mtime,
            source_size: None,
            trash_identity: Some(identity),
            mime,
            trace: None,
        });
        super::thumbdefer::pump(out, st, pool);
    }
}

pub(crate) fn cancel(out: &mut impl Write, st: &mut State, pool: &Pool, ids: &[usize]) {
    let mut keys = Vec::new();
    for ask in &mut st.trash_asked {
        if ids.contains(&ask.id) {
            ask.cancelled = true;
            let key = (ask.path.clone(), ask.mtime, ask.identity.clone());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    for (path, mtime, identity) in keys {
        if st.trash_asked.iter().any(|ask| {
            ask.path == path && ask.mtime == mtime && ask.identity == identity && !ask.cancelled
        }) {
            continue;
        }
        let dropped = st.deferred.take(&path, Some(&identity)).len()
            + pool.cancel(&path, Some(&identity)).len();
        st.outstanding = st.outstanding.saturating_sub(dropped);
        if dropped > 0 {
            st.trash_asked
                .retain(|ask| ask.path != path || ask.mtime != mtime || ask.identity != identity);
        }
    }
    super::thumbdefer::pump(out, st, pool);
}

pub(crate) fn answer(
    out: &mut impl Write,
    st: &mut State,
    path: &Path,
    mtime: i64,
    identity: Option<&str>,
    file: &str,
) {
    let Some(identity) = identity else {
        return;
    };
    if st.trash_asked.is_empty() {
        return;
    }
    let (done, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut st.trash_asked)
        .into_iter()
        .partition(|ask| ask.path == path && ask.mtime == mtime && ask.identity == identity);
    st.trash_asked = kept;
    if done.is_empty() {
        return;
    }
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    for ask in done {
        if ask.cancelled {
            continue;
        }
        let reviewed = ask.reviewed;
        let current = super::iomount::call(path, &body, "trashthumb", move || {
            reviewed
                .thumbnail()
                .is_some_and(|(_, stamp)| stamp == mtime)
        })
        .unwrap_or(false);
        reply(out, ask.id, if current { file } else { "" });
    }
}

pub(crate) fn drain(out: &mut impl Write, st: &mut State) {
    for ask in std::mem::take(&mut st.trash_asked) {
        if !ask.cancelled {
            reply(out, ask.id, "");
        }
    }
}

#[cfg(test)]
#[path = "trashthumb_tests.rs"]
mod tests;
