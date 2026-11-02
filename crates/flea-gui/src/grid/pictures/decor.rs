//! The production Decor: the Grid's language marks, the pictures one pane lends, and their faded copies.
use super::{image_key, Faded, Mode, Pictures, Slot};
use crate::grid::element::{Decor, Thumbnail};
use crate::row::marks::{Language, Mark};
use flea::protocol::RowFacts;
use gpui::{Bounds, Corners, ObjectFit, Pixels, RenderImage, SharedString, Window};
use std::{rc::Rc, sync::Arc};

// Names whose language slug is remembered; a folder change forgets them.
const NAMES: usize = 1024;

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

/// Paints a lent picture whole inside a row's mark slot: a picture larger than the slot fits it, a smaller one keeps its own pixels.
pub fn paint_in_slot(picture: &Thumbnail, slot: Bounds<Pixels>, window: &mut Window) {
    let fit = slot_rect(slot, picture.decoded(), window.scale_factor());
    let _ = window.paint_image(
        slot,
        fit,
        Corners::default(),
        Arc::clone(&picture.image),
        0,
        false,
    );
}

/// A raster that fits keeps one texel per device pixel on the device grid, never enlarged (GM 2026-10-31); a larger one fits whole.
pub(crate) fn slot_rect(slot: Bounds<Pixels>, pixels: (u32, u32), scale: f32) -> Bounds<Pixels> {
    let (w, h) = crate::preview_quality::own_size(pixels, scale.into());
    if w > f32::from(slot.size.width) || h > f32::from(slot.size.height) {
        return ObjectFit::Contain.get_bounds(
            slot,
            gpui::size(
                gpui::DevicePixels(pixels.0 as i32),
                gpui::DevicePixels(pixels.1 as i32),
            ),
        );
    }
    let at = |start: Pixels, room: Pixels, own: f32| {
        let device = ((f32::from(start) + (f32::from(room) - own) / 2.) * scale).round();
        gpui::px(device / scale)
    };
    Bounds::new(
        gpui::point(
            at(slot.origin.x, slot.size.width, w),
            at(slot.origin.y, slot.size.height, h),
        ),
        gpui::size(gpui::px(w), gpui::px(h)),
    )
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

// The GPU samples a texture linearly with no mipmaps, so a picture several times its slot steps; a Catmull-Rom reduction draws it smooth, as v0.3.12 does.
fn reduce(image: &RenderImage, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let bytes = image.as_bytes(0)?.to_vec();
    let buffer = image::RgbaImage::from_raw(size.width.0 as u32, size.height.0 as u32, bytes)?;
    let reduced = image::imageops::resize(
        &buffer,
        width,
        height,
        image::imageops::FilterType::CatmullRom,
    );
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(reduced)])))
}

impl Pictures {
    pub(crate) fn record_paint(
        &self,
        row: usize,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        painted: bool,
    ) {
        if let Some((trace, full)) = self.trace(row) {
            trace.stage(window, painted, full);
        }
        if !painted || !crate::bench_thumbs::enabled() {
            return;
        }
        let map = self.map.borrow();
        if let Some(Slot::Ready { picture, .. }) = map.slots.get(&row) {
            crate::bench_thumbs::painted(
                "thumbnail",
                &crate::navigation::history::join(&map.path, &picture.identity.name),
                picture.decoded(),
                bounds,
                window,
            );
        }
    }

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
    /// A row's picture as its mark slot draws it: filtered down to the slot's device side and, for a cut row, at its dimmed share; made once and retired with its source.
    pub fn slotted(&self, picture: Arc<Thumbnail>, side: u32, level: f32) -> Arc<Thumbnail> {
        let (w, h) = picture.decoded();
        let fit =
            (f64::from(side) / f64::from(w.max(1))).min(f64::from(side) / f64::from(h.max(1)));
        if fit >= 1. && level >= 1. {
            return picture;
        }
        let mut map = self.map.borrow_mut();
        let key = image_key(&picture.image);
        if let Some(faded) = map.faded.get(&key).filter(|faded| {
            faded.level == level.to_bits()
                && faded.side == side
                && Arc::ptr_eq(&faded.source, &picture.image)
        }) {
            return Arc::clone(&faded.copy);
        }
        let sized = if fit < 1. {
            let to = |edge: u32| ((f64::from(edge) * fit).round() as u32).max(1);
            reduce(&picture.image, to(w), to(h))
        } else {
            Some(Arc::clone(&picture.image))
        };
        let Some(image) = sized.and_then(|image| {
            if level < 1. {
                fade(&image, level)
            } else {
                Some(image)
            }
        }) else {
            return picture;
        };
        let faded = Arc::new(Thumbnail {
            identity: picture.identity.clone(),
            kind: picture.kind,
            image,
        });
        let kept = Faded {
            level: level.to_bits(),
            side,
            source: Arc::clone(&picture.image),
            copy: Arc::clone(&faded),
        };
        if let Some(old) = map.faded.insert(key, kept) {
            map.retired.push(Arc::clone(&old.copy.image));
        }
        faded
    }

    /// The picture a List row or the active Columns track draws, only while the row's facts are the ones it was made from.
    pub fn lend(&self, listing: usize, row: &RowFacts) -> Option<Arc<Thumbnail>> {
        self.thumbnail(listing)
            .filter(|picture| picture.identity.is(row))
    }
}

impl Decor for Pictures {
    fn thumbnail(&self, listing: usize) -> Option<Arc<Thumbnail>> {
        let map = self.map.borrow();
        let mode = map.policy.map_or(Mode::Off, |policy| policy.mode);
        match map.slots.get(&listing)? {
            Slot::Ready { picture, image, .. }
                if mode == Mode::Media || (mode == Mode::Images && *image) =>
            {
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

    fn clip_row(
        &self,
        listing: usize,
        row: Option<&RowFacts>,
    ) -> Option<crate::grid::element::Clip> {
        self.clip.borrow().as_ref()?.clip_row(listing, row)
    }

    fn pictures(&self) -> Option<&Pictures> {
        Some(self)
    }
}
