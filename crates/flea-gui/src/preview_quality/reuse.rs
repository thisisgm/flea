//! A first decode that already drew sharp device pixels stands as its refinement, the last refined pictures are kept
//! for going back to a file, and source decodes are counted.
use gpui::RenderImage;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

type Offer = (
    PathBuf,
    (u32, u32),
    crate::g2::pv1::photo_source::Identity,
    Weak<RenderImage>,
);
type Kept = (
    PathBuf,
    (u64, i64),
    (u32, u32),
    Arc<RenderImage>,
    Option<crate::g2::pv1::photo_source::Identity>,
);
static OFFERS: Mutex<Vec<Offer>> = Mutex::new(Vec::new());
static KEPT: Mutex<VecDeque<Kept>> = Mutex::new(VecDeque::new());
const KEPT_PICTURES: usize = 8;
const KEPT_BYTES: usize = 16 << 20;
static DECODES: Mutex<VecDeque<PathBuf>> = Mutex::new(VecDeque::new());
const OPEN_OFFERS: usize = 8;
const LEDGER: usize = 64;

pub(crate) fn file_identity(path: &Path) -> Option<crate::g2::pv1::photo_source::Identity> {
    path.metadata()
        .ok()
        .map(|meta| crate::g2::pv1::photo_source::identity(&meta))
}

/// The sharp picture one decode made for `path` at `size`; held weakly, so an unclaimed one frees with its card.
pub(crate) fn offer(
    path: &Path,
    size: (u32, u32),
    identity: crate::g2::pv1::photo_source::Identity,
    image: &Arc<RenderImage>,
) {
    if let Ok(mut offers) = OFFERS.lock() {
        offers.retain(|(at, want, _, held)| {
            held.strong_count() > 0 && !(at == path && *want == size)
        });
        if offers.len() == OPEN_OFFERS {
            offers.remove(0);
        }
        offers.push((path.to_path_buf(), size, identity, Arc::downgrade(image)));
    }
}

/// Claims the offered picture once, only for the same path and device size.
pub(crate) fn take(path: &Path, size: (u32, u32)) -> Option<Arc<RenderImage>> {
    let identity = crate::g2::pv1::photo_source::identity(&path.metadata().ok()?);
    let mut offers = OFFERS.lock().ok()?;
    let at = offers
        .iter()
        .position(|(at, want, held, _)| at == path && *want == size && *held == identity)?;
    offers.swap_remove(at).3.upgrade()
}

fn bytes(image: &RenderImage) -> usize {
    (0..)
        .map_while(|frame| image.as_bytes(frame))
        .fold(0usize, |total, pixels| total.saturating_add(pixels.len()))
}

/// Keeps the newest refined picture of a file version, the last eight within 16 MiB, so going back draws it again.
pub(crate) fn keep(
    path: &Path,
    version: (u64, i64),
    size: (u32, u32),
    image: &Arc<RenderImage>,
    identity: Option<crate::g2::pv1::photo_source::Identity>,
) {
    if bytes(image) > KEPT_BYTES / 2 || file_identity(path) != identity {
        return;
    }
    if let Ok(mut kept) = KEPT.lock() {
        kept.retain(|(at, ..)| at != path);
        kept.push_back((path.to_path_buf(), version, size, image.clone(), identity));
        while kept.len() > KEPT_PICTURES
            || kept.iter().map(|k| bytes(&k.3)).sum::<usize>() > KEPT_BYTES
        {
            kept.pop_front();
        }
    }
}

/// The kept picture of this file version and the device size it was drawn for.
pub(crate) fn kept(path: &Path, version: (u64, i64)) -> Option<((u32, u32), Arc<RenderImage>)> {
    let identity = file_identity(path);
    let kept = KEPT.lock().ok()?;
    kept.iter()
        .find(|(at, held, _, _, stamp)| at == path && *held == version && *stamp == identity)
        .map(|(_, _, size, image, _)| (*size, image.clone()))
}

/// One read of a source file's pixels, for the decodes-per-open reader.
pub(crate) fn decoded(path: &Path) {
    if let Ok(mut ledger) = DECODES.lock() {
        if ledger.len() == LEDGER {
            ledger.pop_front();
        }
        ledger.push_back(path.to_path_buf());
    }
}

/// How many of the last decodes read `path`.
pub(crate) fn decodes(path: &Path) -> usize {
    DECODES
        .lock()
        .map_or(0, |ledger| ledger.iter().filter(|at| *at == path).count())
}
