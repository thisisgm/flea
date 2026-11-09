//! A JPEG's full decode serves every device size asked for while it is still wanted, so a resize never decodes twice.
use image::{metadata::Orientation, DynamicImage};
use std::{
    fs::File,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, Weak},
    time::Duration,
};

pub(crate) type Decoded = (DynamicImage, Orientation);
pub(crate) type Identity = (u64, u64, u64, i64, i64, i64, i64);
pub(crate) fn identity(meta: &std::fs::Metadata) -> Identity {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}
/// None while the first decode runs; then its pixels, or None when it failed.
type State = Option<Option<Arc<Decoded>>>;

pub(crate) struct Source {
    path: PathBuf,
    identity: Identity,
    state: Mutex<State>,
    ready: Condvar,
}
static SOURCES: Mutex<Vec<Weak<Source>>> = Mutex::new(Vec::new());

/// Keeps a decode of `path` that is running, or done and still held, alive for a request queued behind it.
pub(crate) fn hold(path: &Path) -> Option<Arc<Source>> {
    let identity = identity(&path.metadata().ok()?);
    let sources = SOURCES.lock().ok()?;
    sources
        .iter()
        .filter_map(Weak::upgrade)
        .find(|source| source.path == path && source.identity == identity)
}

/// The pixels of `file`: joins a decode of the same file already under way, else decodes and shares them.
/// The flag says another request still holds the decode, so it will draw these pixels itself.
pub(super) fn shared(
    path: &Path,
    file: File,
    current: &dyn Fn() -> bool,
    decode: impl FnOnce(File) -> Result<Decoded, ()>,
) -> Result<(Arc<Decoded>, bool, Arc<Source>), ()> {
    let meta = file.metadata().map_err(|_| ())?;
    let identity = identity(&meta);
    let (source, owner) = {
        let mut sources = SOURCES.lock().map_err(|_| ())?;
        sources.retain(|source| source.strong_count() > 0);
        let found = sources
            .iter()
            .filter_map(Weak::upgrade)
            .find(|source| source.path == path && source.identity == identity);
        match found {
            Some(source) => (source, false),
            None => {
                let source = Arc::new(Source {
                    path: path.to_path_buf(),
                    identity,
                    state: Mutex::new(None),
                    ready: Condvar::new(),
                });
                sources.push(Arc::downgrade(&source));
                (source, true)
            }
        }
    };
    if owner {
        let publish = Publish(&source);
        let decoded = decode(file).ok().map(Arc::new);
        if let Ok(mut state) = source.state.lock() {
            *state = Some(decoded.clone());
        }
        drop(publish);
        return Ok((decoded.ok_or(())?, Arc::strong_count(&source) > 1, source));
    }
    let mut state = source.state.lock().map_err(|_| ())?;
    while state.is_none() {
        if !current() {
            return Err(());
        }
        state = source
            .ready
            .wait_timeout(state, Duration::from_millis(10))
            .map_err(|_| ())?
            .0;
    }
    let decoded = state.clone().flatten().ok_or(())?;
    drop(state);
    Ok((decoded, Arc::strong_count(&source) > 1, source))
}

/// Wakes every waiter even when the decode unwinds, which then reads as a failed decode.
struct Publish<'a>(&'a Source);
impl Drop for Publish<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.get_or_insert(None);
        }
        self.0.ready.notify_all();
    }
}
