//! A pane's production pictures and the Grid's marks: Thumbs.js's viewport asks from the Grid, the List or the active Columns track, the thumbed answers decoded off the UI thread, and LangMarks.js's slugs.
#[path = "pictures/decor.rs"]
mod decor;
#[path = "pictures/demand.rs"]
pub(crate) mod demand;
#[cfg(test)]
#[path = "pictures/faded_tests.rs"]
mod faded_tests;
#[path = "pictures/policy.rs"]
pub(crate) mod policy;
#[cfg(test)]
#[path = "pictures/policy_readers_tests.rs"]
mod policy_readers_tests;
#[path = "pictures/quality.rs"]
mod quality;
#[cfg(test)]
#[path = "pictures/quality_tests.rs"]
mod quality_tests;
#[path = "pictures/readers.rs"]
mod readers;
#[cfg(test)]
#[path = "pictures/slot_tests.rs"]
mod slot_tests;
#[path = "pictures/surface.rs"]
pub(crate) mod surface;
#[path = "pictures/wire.rs"]
mod wire;

pub use decor::{paint_in_slot, Counters};
pub use policy::{Mode, Policy};
pub(crate) use quality::picture_bounds;

use super::{Decor, Identity, Thumbnail};
use crate::{list::pane::Held, row::marks::Language};
use flea::protocol::RowFacts;
use gpui::{RenderImage, Task};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
    sync::Arc,
};

// Pane.qml settleMs, firstSettleMs and thumbCap; only viewport rows are recorded, so the cap bounds a policy bug.
const SETTLE_MS: u64 = 120;
const FIRST_SETTLE_MS: u64 = 70;
const CAP: usize = 240;

fn is_image(row: &RowFacts) -> bool {
    row.icon == "image-x-generic"
}

// One row's thumbnail as this listing has dealt with it; image records whether Images mode may draw it.
enum Slot {
    Asked {
        identity: Identity,
        image: bool,
        cache: bool,
    },
    // The decode runs on the background executor; dropping the slot drops its task and cancels it.
    Decoding {
        identity: Identity,
        image: bool,
        file: String,
        _task: Task<()>,
    },
    Ready {
        file: String,
        picture: Arc<Thumbnail>,
        image: bool,
    },
    Failed {
        identity: Identity,
        file: String,
    },
    // Answered with nothing, or with a file that would not decode: the mark stays.
    Refused {
        observed: Identity,
    },
    // A cache-only ask the shared cache could not answer.
    Missed {
        observed: Identity,
    },
}

impl Slot {
    fn identity(&self) -> Option<&Identity> {
        match self {
            Self::Asked { identity, .. }
            | Self::Decoding { identity, .. }
            | Self::Failed { identity, .. } => Some(identity),
            Self::Ready { picture, .. } => Some(&picture.identity),
            Self::Refused { .. } | Self::Missed { .. } => None,
        }
    }
}

// The asks one settle covers: a change to any of these starts the settle again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    listing: usize,
    span: (usize, usize),
    // A List filter's drawn view positions and the identity of its shown rows, Filter.cut; None asks the whole span.
    only: Option<(usize, usize, usize)>,
    mode: Mode,
    // None until fsinfo names the folder's class.
    cache: Option<bool>,
}

struct Faded {
    level: u32,
    side: u32,
    source: Arc<RenderImage>,
    copy: Arc<Thumbnail>,
}

#[derive(Default)]
struct Map {
    listing: usize,
    path: String,
    slots: HashMap<usize, Slot>,
    order: VecDeque<usize>,
    key: Option<Key>,
    settle: Option<Task<()>>,
    settled: bool,
    class: Option<(String, String)>,
    asked_fs: usize,
    // The held window last checked, so a window replaced in place is compared once.
    seen: Weak<Held>,
    policy: Option<Policy>,
    names: HashMap<Box<str>, Option<Language>>,
    retired: Vec<Arc<RenderImage>>,
    counters: Counters,
    refinements: HashMap<usize, crate::preview_quality::Refinement>,
    traces: HashMap<usize, crate::preview_trace::Trace>,
    bases: HashMap<usize, crate::preview_quality::Basis>,
    // A cut row's faded copy of each picture at one level, keyed by the source image it holds, so the key is never reused while kept.
    faded: HashMap<usize, Faded>,
}

impl Map {
    fn class_for(&self, path: &str) -> Option<&str> {
        self.class
            .as_ref()
            .filter(|(answered, _)| answered == path)
            .map(|(_, class)| class.as_str())
    }

    fn forget(&mut self, row: usize) {
        self.traces.remove(&row);
        self.bases.remove(&row);
        if let Some(mut refinement) = self.refinements.remove(&row) {
            refinement.cancel();
        }
        if let Some(Slot::Ready { picture, .. }) = self.slots.remove(&row) {
            self.retire(&picture);
        }
        self.order.retain(|kept| *kept != row);
    }

    // A picture leaves the listing: its texture and its faded copy go to the atlas drop.
    fn retire(&mut self, picture: &Thumbnail) {
        self.retired.push(Arc::clone(&picture.image));
        self.counters.retired += 1;
        if let Some(faded) = self.faded.remove(&image_key(&picture.image)) {
            self.retired.push(Arc::clone(&faded.copy.image));
        }
    }

    // Faded copies whose source no Ready slot holds, made by a paint the source had already left.
    fn sweep_faded(&mut self) {
        let held: Vec<usize> = self
            .slots
            .values()
            .filter_map(|slot| match slot {
                Slot::Ready { picture, .. } => Some(image_key(&picture.image)),
                _ => None,
            })
            .collect();
        let gone: Vec<usize> = self
            .faded
            .keys()
            .copied()
            .filter(|key| !held.contains(key))
            .collect();
        for key in gone {
            if let Some(faded) = self.faded.remove(&key) {
                self.retired.push(Arc::clone(&faded.copy.image));
            }
        }
    }

    // Thumbs.remember: the oldest record goes once the cap is passed.
    fn remember(&mut self, row: usize, slot: Slot) {
        match self.slots.insert(row, slot) {
            None => self.order.push_back(row),
            Some(Slot::Ready { picture, .. }) => {
                // The basis and refinement belonged to the replaced picture.
                self.bases.remove(&row);
                if let Some(mut refinement) = self.refinements.remove(&row) {
                    refinement.cancel();
                }
                self.retire(&picture);
            }
            Some(_) => {}
        }
        while self.order.len() > CAP {
            let oldest = self.order[0];
            self.forget(oldest);
        }
    }

    // A recorded row whose held facts are no longer the ones it was asked for.
    fn stale(&self, held: &Held, row: usize) -> bool {
        let identity = self.slots.get(&row).and_then(Slot::identity);
        identity.is_some_and(|identity| held.row(row).is_some_and(|facts| !identity.is(facts)))
    }

    // The rows still waiting on the backend, the only ones a thumbcancel names.
    fn pending(&self) -> impl Iterator<Item = usize> + '_ {
        self.order
            .iter()
            .copied()
            .filter(|row| matches!(self.slots.get(row), Some(Slot::Asked { .. })))
    }

    // A new listing: the backend forgot every ask, and on the same folder each finished picture follows its row by its source facts, Thumbs.carried.
    fn follow(&mut self, held: &Held, path: &str) {
        let mut carried = Vec::new();
        if path == self.path {
            let at: HashMap<&str, usize> = held
                .rows
                .iter()
                .enumerate()
                .map(|(at, row)| (row.name.as_str(), held.start + at))
                .collect();
            for row in std::mem::take(&mut self.order) {
                if let Some(Slot::Ready {
                    picture,
                    image,
                    file,
                }) = self.slots.remove(&row)
                {
                    let moved = at
                        .get(picture.identity.name.as_str())
                        .copied()
                        .filter(|&to| held.row(to).is_some_and(|facts| picture.identity.is(facts)));
                    match moved {
                        Some(to) => carried.push((
                            to,
                            Slot::Ready {
                                picture,
                                image,
                                file,
                            },
                        )),
                        None => self.retire(&picture),
                    }
                }
            }
        } else {
            self.names.clear();
        }
        self.clear();
        for (row, slot) in carried {
            self.remember(row, slot);
        }
        self.sweep_faded();
        self.listing = held.listing;
        self.path = path.to_owned();
    }

    // Forgets every record and every faded copy, and returns the rows the backend still holds jobs for.
    fn release(&mut self) -> Vec<usize> {
        let pending = self.clear();
        self.sweep_faded();
        pending
    }

    fn clear(&mut self) -> Vec<usize> {
        self.traces.clear();
        self.bases.clear();
        for (_, mut refinement) in self.refinements.drain() {
            refinement.cancel();
        }
        let pending = self.pending().collect();
        for row in std::mem::take(&mut self.order) {
            self.forget(row);
        }
        self.slots.clear();
        self.key = None;
        self.settle = None;
        self.settled = false;
        pending
    }
}

fn image_key(image: &Arc<RenderImage>) -> usize {
    Arc::as_ptr(image) as usize
}

/// The production Decor: the thumbnails one pane lends its Grid tiles, List rows and active Columns track, and the Grid's language marks.
#[derive(Default)]
pub struct Pictures {
    map: RefCell<Map>,
    // The clipboard marks the pane's tiles wear, which Pictures draws through; it never paints marks of its own.
    clip: RefCell<Option<Rc<dyn Decor>>>,
}
