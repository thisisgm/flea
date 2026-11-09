// The Properties card's walk lines and photo and media pairs, and the storage-class thumbnail wording.
use super::ask::{class_of, class_on, Class, ClassSwitches};
use super::photo::{self, Photo, Ratio};
use crate::i18n::I18n;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    pub bytes: u64,
    pub items: u64,
    pub partial: bool,
    pub done: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    pub cancelled: bool,
    pub bytes: u64,
    pub items: u64,
    pub partial: bool,
    pub done: bool,
}

// The walk's first floor is the folder's own entry.
pub fn begin(bytes: u64) -> Walk { Walk { bytes, ..Walk::default() } }

// A cancelled or refused walk ends as the floor already counted, so the card never counts forever.
pub fn next(walk: Walk, reply: &Reply) -> Walk {
    if reply.cancelled {
        return if walk.done { walk } else { Walk { partial: true, done: true, ..walk } };
    }
    if !reply.ok {
        return Walk { partial: true, done: true, ..walk };
    }
    Walk { bytes: reply.bytes, items: reply.items, partial: reply.partial, done: reply.done }
}

// A value and its muted tail, drawn as two runs on one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub value: String,
    pub tail: String,
}

fn tail(walk: &Walk, i18n: &I18n) -> String { if walk.done { String::new() } else { i18n.tr("preview.property.counting") } }

pub fn size(bytes: u64, walk: Option<&Walk>, i18n: &I18n) -> Line {
    let shown = walk.map_or(bytes, |w| w.bytes);
    let value = i18n.tr_args("preview.property.size", &[&i18n.locale().format_size(shown), &shown.to_string()]);
    match walk {
        Some(w) if !w.done || w.partial => Line { value: i18n.tr_args("preview.floor", &[&value]), tail: tail(w, i18n) },
        _ => Line { value, tail: String::new() },
    }
}

pub fn contains(walk: Option<&Walk>, i18n: &I18n) -> Option<Line> {
    let w = walk?;
    let items = i18n.trn("items.count", w.items as i64);
    let value = if !w.done || w.partial { i18n.tr_args("preview.floor", &[&items]) } else { items };
    Some(Line { value, tail: tail(w, i18n) })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub ok: bool,
    pub vcodec: String,
    pub acodec: String,
    pub bps: f64,
    pub make: String,
    pub model: String,
    pub lens: String,
    pub shutter: Ratio,
    pub fnumber: Ratio,
    pub iso: u32,
    pub taken: String,
}

fn first(a: &str, b: &str) -> String { if a.is_empty() { b.into() } else { a.into() } }

// One merged photo, formatted once: present deferred EXIF fields win a disagreement, absent ones keep the immediate.
pub fn extra_pairs(reply: Option<&Facts>, immediate: Option<&Photo>, i18n: &I18n) -> Vec<(String, String)> {
    let Some(r) = reply.filter(|r| r.ok) else { return Vec::new() };
    let p = immediate.cloned().unwrap_or_default();
    let shutter = if photo::ratio(r.shutter) > 0.0 { r.shutter } else { p.exp };
    let fnumber = if photo::ratio(r.fnumber) > 0.0 { r.fnumber } else { p.fnum };
    let iso = if r.iso > 0 { r.iso } else { p.iso };
    let (make, model, lens, taken) = (first(&r.make, &p.make), first(&r.model, &p.model), first(&r.lens, &p.lens), first(&r.taken, &p.taken));
    let (camera, exposure) = if immediate.is_some() {
        let merged = Photo { make: make.clone(), model: model.clone(), lens: lens.clone(), exp: shutter, fnum: fnumber, iso, taken: taken.clone(), ..p };
        let shown = photo::photo_facts(Some(&merged), i18n);
        (shown.camera, shown.exposure)
    } else {
        (photo::camera(&make, &model), photo::exposure(shutter, fnumber, iso, i18n))
    };
    [
        ("preview.fact.codecs", photo::codecs(&r.vcodec, &r.acodec)),
        ("preview.fact.bit_rate", photo::bit_rate(r.bps, i18n)),
        ("preview.fact.camera", camera),
        ("preview.fact.lens", lens),
        ("preview.fact.exposure", exposure),
        ("preview.fact.taken", photo::taken(&taken)),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_empty())
    .map(|(k, v)| (i18n.tr(k), v))
    .collect()
}

fn noun(class: Class, title: bool, i18n: &I18n) -> String {
    let key = match (class, title) {
        (Class::Network, false) => "preview.class.network",
        (Class::Phone, false) => "preview.class.phone",
        (Class::Usb, false) => "preview.class.usb",
        (Class::Network, true) => "preview.class.network_title",
        (Class::Phone, true) => "preview.class.phone_title",
        (Class::Usb, true) => "preview.class.usb_title",
    };
    i18n.tr(key)
}

// The menu row's label for a class: "Hide USB thumbnails" when on, "Show network thumbnails" when off; none locally.
pub fn thumbs_label(storage_class: &str, switches: &ClassSwitches, i18n: &I18n) -> Option<String> {
    let class = class_of(storage_class)?;
    let key = if class_on(storage_class, switches) { "preview.thumbs.hide" } else { "preview.thumbs.show" };
    Some(i18n.tr_args(key, &[&noun(class, false, i18n)]))
}

// The status line's verdict for the toggle: "Network thumbnails are on".
pub fn thumbs_status(storage_class: &str, on: bool, i18n: &I18n) -> Option<String> {
    let class = class_of(storage_class)?;
    Some(i18n.tr_args(if on { "preview.thumbs.on" } else { "preview.thumbs.off" }, &[&noun(class, true, i18n)]))
}
