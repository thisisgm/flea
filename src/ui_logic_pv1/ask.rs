// The cursor row's one thumbnail ask and the storage-class gate it shares with the listing.
use super::settle::{self, Plan};

// The preview settings the class gate reads: three per-class switches and the global thumbnail mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassSwitches {
    pub network: Option<bool>,
    pub phone: Option<bool>,
    pub usb: Option<bool>,
    pub thumbnails_off: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Network,
    Phone,
    Usb,
}

// src/backend/extclass.rs's word for a storage class; anything else is local and always gets thumbnails.
pub fn class_of(storage_class: &str) -> Option<Class> {
    match storage_class {
        "network" => Some(Class::Network),
        "phone" => Some(Class::Phone),
        "usb" => Some(Class::Usb),
        _ => None,
    }
}

pub fn present(storage_class: &str) -> bool { class_of(storage_class).is_some() }

// Network and phone ship off, USB ships on, local is always on.
pub fn class_on(storage_class: &str, switches: &ClassSwitches) -> bool {
    match class_of(storage_class) {
        None => true,
        Some(Class::Usb) => switches.usb != Some(false),
        Some(Class::Network) => switches.network == Some(true),
        Some(Class::Phone) => switches.phone == Some(true),
    }
}

pub fn class_off(storage_class: &str, switches: &ClassSwitches) -> bool {
    present(storage_class) && !class_on(storage_class, switches)
}

// Off is cache-only, not blank: a visible row still asks and the backend answers from the shared cache.
pub fn cache_only(storage_class: &str, switches: &ClassSwitches) -> bool {
    class_off(storage_class, switches) && !switches.thumbnails_off
}

// In Columns the preview is manual on a class that is off: one condition, two surfaces.
pub fn manual_hold(storage_class: &str, switches: &ClassSwitches) -> bool { cache_only(storage_class, switches) }

// Quick Look reads at most the first 256 KiB of text on network and phone storage, else 1 MiB.
pub const TEXT_PREVIEW_MAX: u64 = 1_048_576;
pub const REMOTE_TEXT_PREVIEW_MAX: u64 = 262_144;

pub fn text_limit(storage_class: &str) -> u64 {
    match class_of(storage_class) {
        Some(Class::Network) | Some(Class::Phone) => REMOTE_TEXT_PREVIEW_MAX,
        _ => TEXT_PREVIEW_MAX,
    }
}

// Sample input: ("network", default) is (true, "media"); zoom and other classes change nothing.
pub fn verdict(storage_class: &str, switches: &ClassSwitches, mode: &str) -> (bool, String) {
    let mode = if mode.is_empty() { "media" } else { mode };
    (cache_only(storage_class, switches), mode.into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Work {
    pub ask: Vec<usize>,
    pub drop: Vec<usize>,
    pub cache_only: bool,
}

#[derive(Clone, Debug)]
pub struct Ask<'a> {
    pub can_read: bool,
    pub storage_known: bool,
    pub selection_count: usize,
    pub index: Option<usize>,
    pub thumbnailable: bool,
    pub asked: bool,
    pub storage_class: &'a str,
    pub switches: &'a ClassSwitches,
}

// Sample input: readable, storage known, one selection, index 3, thumbnailable and unasked asks [3].
pub fn work_for(o: &Ask) -> Option<Work> {
    if !o.can_read || !o.storage_known || o.selection_count > 1 || !o.thumbnailable || o.asked {
        return None;
    }
    let index = o.index?;
    Some(Work { ask: vec![index], drop: Vec::new(), cache_only: cache_only(o.storage_class, o.switches) })
}

#[derive(Clone, Copy, Debug)]
pub struct Clock<'a> {
    pub now: i64,
    pub last_move_at: i64,
    pub interval: i64,
    pub key: &'a str,
    pub last_move_key: &'a str,
    pub armed: bool,
}

// The early ask's idle half is the settle's own rule: a walk step trails and asks nothing early.
pub fn early_work_for(o: &Ask, clock: &Clock) -> Option<Work> {
    if settle::plan(clock.now, clock.last_move_at, clock.interval, clock.key, clock.last_move_key, clock.armed) != Plan::Now {
        return None;
    }
    if manual_hold(o.storage_class, o.switches) {
        return None;
    }
    work_for(o)
}
