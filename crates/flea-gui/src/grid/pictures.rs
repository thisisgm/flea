//! A pane's production pictures and the Grid's marks: Thumbs.js's viewport asks from the Grid, the List or the active Columns track, the thumbed answers decoded off the UI thread, and LangMarks.js's slugs.
#[path = "pictures/surface.rs"]
mod surface;
#[path = "pictures/wire.rs"]
mod wire;
#[path = "pictures/readers.rs"]
mod readers;

use super::{Decor, Identity, Thumbnail};
use crate::list::pane::Held;
use crate::row::marks::{Language, Mark};
use flea::{jsondoc::Json, protocol::RowFacts};
use gpui::{Bounds, Corners, ObjectFit, Pixels, RenderImage, SharedString, Task, Window};
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
// Names whose language slug is remembered; a folder change forgets them.
const NAMES: usize = 1024;

/// ViewState.thumbnailMode: preview.thumbnails, absent reads media.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Images,
    Media,
}

/// The ui.json facts Grid pictures and marks follow: the thumbnail mode, ExtThumbs.js's class switches and ViewState.langMarks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub mode: Mode,
    pub usb: bool,
    pub network: bool,
    pub phone: bool,
    pub languages: bool,
}

impl Policy {
    /// Absent keys read as v0.3.12 reads them: media, USB on, network and phone off, code icons on.
    pub fn read(data: &Json) -> Self {
        let preview = data.get("preview");
        let flag = |key| preview.and_then(|p| p.get(key)).and_then(Json::as_bool);
        Self {
            mode: match preview.and_then(|p| p.get("thumbnails")).and_then(Json::as_str) {
                Some("off") => Mode::Off,
                Some("images") => Mode::Images,
                _ => Mode::Media,
            },
            usb: flag("thumbUsb") != Some(false),
            network: flag("thumbNetwork") == Some(true),
            phone: flag("thumbPhone") == Some(true),
            languages: data.get("langMarks").and_then(Json::as_bool) != Some(false),
        }
    }

    /// ExtThumbs.cacheOnly: a class whose switch is off asks the shared cache only, so pictures already made still draw.
    pub fn cache_only(&self, class: &str) -> bool {
        let off = match class {
            "usb" => !self.usb,
            "network" => !self.network,
            "phone" => !self.phone,
            _ => false,
        };
        off && self.mode != Mode::Off
    }

    /// Thumbs.allowed; RowFacts carries no t flag, so a file the backend cannot picture is asked and answers empty.
    pub fn allowed(&self, row: &RowFacts) -> bool {
        !row.dir
            && match self.mode {
                Mode::Off => false,
                Mode::Images => is_image(row),
                Mode::Media => true,
            }
    }
}

fn is_image(row: &RowFacts) -> bool {
    row.icon == "image-x-generic"
}

/// What the pictures did, so tests and the native run can read the policy back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub asked: usize,
    pub cancelled: usize,
    pub answered: usize,
    pub decoded: usize,
    pub refused: usize,
    pub retired: usize,
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
    Refused,
    // A cache-only ask the shared cache could not answer.
    Missed,
}

impl Slot {
    fn identity(&self) -> Option<&Identity> {
        match self {
            Self::Asked { identity, .. } | Self::Decoding { identity, .. } | Self::Failed { identity, .. } => Some(identity),
            Self::Ready { picture, .. } => Some(&picture.identity),
            Self::Refused | Self::Missed => None,
        }
    }
}

// The asks one settle covers: a change to any of these starts the settle again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    listing: usize,
    span: (usize, usize),
    // A List filter's drawn view positions and the identity of its shown rows, Filter.cut; None asks the whole span.
    only: Option<(usize, usize, usize)>,
    mode: Mode,
    // None until fsinfo names the folder's class.
    cache: Option<bool>,
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
    // A cut row's faded copy of each picture, keyed by the source image and kept at one level.
    faded: HashMap<usize, (u32, Arc<Thumbnail>)>,
}

impl Map {
    fn class_for(&self, path: &str) -> Option<&str> {
        self.class
            .as_ref()
            .filter(|(answered, _)| answered == path)
            .map(|(_, class)| class.as_str())
    }

    fn forget(&mut self, row: usize) {
        if let Some(Slot::Ready { picture, .. }) = self.slots.remove(&row) {
            self.retire(&picture);
        }
        self.order.retain(|kept| *kept != row);
    }

    // A picture leaves the listing: its texture and its faded copy go to the atlas drop.
    fn retire(&mut self, picture: &Thumbnail) {
        self.retired.push(Arc::clone(&picture.image));
        self.counters.retired += 1;
        if let Some((_, faded)) = self.faded.remove(&image_key(&picture.image)) {
            self.retired.push(Arc::clone(&faded.image));
        }
    }

    // Thumbs.remember: the oldest record goes once the cap is passed.
    fn remember(&mut self, row: usize, slot: Slot) {
        if self.slots.insert(row, slot).is_none() {
            self.order.push_back(row);
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
                if let Some(Slot::Ready { picture, image, file }) = self.slots.remove(&row) {
                    let moved = at
                        .get(picture.identity.name.as_str())
                        .copied()
                        .filter(|&to| held.row(to).is_some_and(|facts| picture.identity.is(facts)));
                    match moved {
                        Some(to) => carried.push((to, Slot::Ready { picture, image, file })),
                        None => self.retire(&picture),
                    }
                }
            }
        } else {
            self.names.clear();
        }
        self.release();
        for (row, slot) in carried {
            self.remember(row, slot);
        }
        self.listing = held.listing;
        self.path = path.to_owned();
    }

    // Forgets every record and returns the rows the backend still holds jobs for.
    fn release(&mut self) -> Vec<usize> {
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

/// Paints a lent picture whole inside a row's mark slot, the way Row.qml and ColumnRow.qml fit it.
pub fn paint_in_slot(picture: &Thumbnail, slot: Bounds<Pixels>, window: &mut Window) {
    let fit = ObjectFit::Contain.get_bounds(slot, picture.image.size(0));
    let _ = window.paint_image(slot, fit, Corners::default(), Arc::clone(&picture.image), 0, false);
}

fn image_key(image: &Arc<RenderImage>) -> usize {
    Arc::as_ptr(image) as usize
}

// The decoded picture with every texel's straight alpha scaled, Row.qml's dimOpacity, since GPUI keeps element opacity crate-private.
fn fade(image: &RenderImage, level: f32) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let mut bytes = image.as_bytes(0)?.to_vec();
    for texel in bytes.as_chunks_mut::<4>().0 {
        texel[3] = (f32::from(texel[3]) * level).round() as u8;
    }
    let buffer = image::RgbaImage::from_raw(size.width.0 as u32, size.height.0 as u32, bytes)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

/// The production Decor: the thumbnails one pane lends its Grid tiles, List rows and active Columns track, and the Grid's language marks.
#[derive(Default)]
pub struct Pictures {
    map: RefCell<Map>,
    // The clipboard marks the pane's tiles wear, which Pictures draws through; it never paints marks of its own.
    clip: RefCell<Option<Rc<dyn Decor>>>,
}

impl Pictures {
    /// Hands the tiles their clipboard marks, which outlive every picture this decor lends.
    pub fn set_clip(&self, marks: Rc<dyn Decor>) {
        *self.clip.borrow_mut() = Some(marks);
    }

    pub fn counters(&self) -> Counters {
        self.map.borrow().counters
    }
    /// Rows holding a decoded picture.
    pub fn lent(&self) -> usize {
        let map = self.map.borrow();
        map.slots
            .values()
            .filter(|slot| matches!(slot, Slot::Ready { .. }))
            .count()
    }
    /// Rows asked and not yet answered.
    pub fn waiting(&self) -> Vec<usize> {
        self.map.borrow().pending().collect()
    }
    /// A cut row's picture at the row's dimmed share, made once per picture and level and retired with its source.
    pub fn faded(&self, picture: Arc<Thumbnail>, level: f32) -> Arc<Thumbnail> {
        let mut map = self.map.borrow_mut();
        let key = image_key(&picture.image);
        if let Some((_, faded)) = map.faded.get(&key).filter(|(at, _)| *at == level.to_bits()) {
            return Arc::clone(faded);
        }
        let Some(image) = fade(&picture.image, level) else {
            return picture;
        };
        let faded = Arc::new(Thumbnail {
            identity: picture.identity.clone(),
            kind: picture.kind,
            image,
        });
        if let Some((_, old)) = map.faded.insert(key, (level.to_bits(), Arc::clone(&faded))) {
            map.retired.push(Arc::clone(&old.image));
        }
        faded
    }

    /// The picture a List row or the active Columns track draws, only while the row's facts are the ones it was made from.
    pub fn lend(&self, listing: usize, row: &RowFacts) -> Option<Arc<Thumbnail>> {
        self.thumbnail(listing).filter(|picture| picture.identity.is(row))
    }
}

impl Decor for Pictures {
    fn thumbnail(&self, listing: usize) -> Option<Arc<Thumbnail>> {
        let map = self.map.borrow();
        let mode = map.policy.map_or(Mode::Off, |policy| policy.mode);
        match map.slots.get(&listing)? {
            Slot::Ready { picture, image, .. } if mode == Mode::Media || (mode == Mode::Images && *image) => {
                Some(Arc::clone(picture))
            }
            _ => None,
        }
    }

    // Mark::for_facts's gate, with each name's slug remembered so a still frame allocates nothing.
    fn language(&self, row: &RowFacts) -> Option<(SharedString, &'static [u8])> {
        let mut map = self.map.borrow_mut();
        if !map.policy.is_some_and(|policy| policy.languages)
            || row.dir
            || crate::row::is_symlink(row.mode)
        {
            return None;
        }
        let language = match map.names.get(row.name.as_str()) {
            Some(language) => *language,
            None => {
                if map.names.len() >= NAMES {
                    map.names.clear();
                }
                let language = Language::for_file(&row.name);
                map.names.insert(row.name.as_str().into(), language);
                language
            }
        };
        language.map(|language| Mark::Language(language).svg())
    }

    fn clip_row(&self, listing: usize, row: Option<&RowFacts>) -> Option<crate::grid::element::Clip> {
        self.clip.borrow().as_ref()?.clip_row(listing, row)
    }

    fn pictures(&self) -> Option<&Pictures> {
        Some(self)
    }
}
