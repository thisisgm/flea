//! The status bar's priority composition: what the three zones draw while producers own their sentences.
use flea::i18n::I18n;

/// A notice's display life in ms, StatusBar.qml's messageMs.
pub const NOTICE_MS: u64 = 4000;

/// Status.js noticeShown: the centre draws the notice only when no prompt, error, activity or search covers it.
pub fn notice_shown(slot: &Slot) -> bool {
    !slot.prompt_live
        && !slot.transient.is_empty()
        && !slot.transient_is_error
        && slot.sticky.is_empty()
        && !slot.searching
}

/// Whether the notice still stands at `now_ms` for the clear timer: undo stands until replaced, any other for 4000 ms from its stamp; the timer itself runs only while the centre shows it.
pub fn notice_live(now_ms: u64, shown_ms: u64, undo: bool) -> bool {
    undo || now_ms.wrapping_sub(shown_ms) < NOTICE_MS
}

/// The stamp `notice_live` counts from: a new notice resets it, as does the centre showing a covered notice again.
pub fn shown_since(
    now_ms: u64,
    was_shown: bool,
    now_shown: bool,
    shown_ms: u64,
    is_new: bool,
) -> u64 {
    if is_new || (now_shown && !was_shown) {
        now_ms
    } else {
        shown_ms
    }
}

/// The clear timer runs only while the centre shows a notice with no undo hint, so its click target stays.
pub fn needs_timer(slot: &Slot, undo: bool) -> bool {
    notice_shown(slot) && !undo
}

/// The listing states StatusBar.qml's itemText answers; Ready carries the held total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Empty,
    Error,
    Locked,
    Ready(usize),
}

/// The count the bar draws for a listing with no selection, filter or flash: the empty folder counts 0 items, never the word empty.
pub fn item_text(i18n: &I18n, listing: Listing) -> String {
    match listing {
        Listing::Loading => String::new(),
        Listing::Empty => i18n.trn("items.count", 0),
        Listing::Error | Listing::Locked => i18n.tr("pane.unavailable"),
        Listing::Ready(total) => i18n.trn("items.count", count_arg(total)),
    }
}

fn count(i18n: &I18n, value: usize) -> String {
    i18n.locale()
        .format_count(i64::try_from(value).unwrap_or(i64::MAX))
}

fn count_arg(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// The selection head, board filter-select-all: 10 of 80 selected, with the byte total while every selected row is held with a known size.
pub fn selection_head(
    i18n: &I18n,
    selected: usize,
    total: usize,
    bytes: i64,
    size: &str,
) -> String {
    let head = i18n.trn_args(
        "status.host.selected",
        count_arg(selected),
        &[&count(i18n, total)],
    );
    if bytes >= 0 {
        i18n.tr_args("status.host.selected_size", &[&head, size])
    } else {
        head
    }
}

/// The left zone: a not-ready listing answers first, then the flash count, selection, filter sentence, directory.
pub fn left(
    i18n: &I18n,
    listing: Listing,
    flash: &str,
    selection: Option<(usize, i64, String)>,
    filter_summary: Option<&str>,
) -> String {
    let Listing::Ready(total) = listing else {
        return item_text(i18n, listing);
    };
    if !flash.is_empty() {
        return flash.to_owned();
    }
    if let Some((selected, bytes, size)) = &selection {
        if *selected > 0 {
            return selection_head(i18n, *selected, total, *bytes, size);
        }
    }
    if let Some(summary) = filter_summary.filter(|summary| !summary.is_empty()) {
        return summary.to_owned();
    }
    item_text(i18n, listing)
}

/// The centre's inputs; every sentence arrives translated from its producer, except the reload notice this host formats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    pub transient: String,
    pub transient_is_error: bool,
    pub prompt_live: bool,
    pub sticky: String,
    pub searching: bool,
    pub search_line: String,
}

/// The centre zone is what just happened, and nothing else: a prompt or error stands over activity and search.
pub fn centre(slot: &Slot) -> &str {
    if slot.transient_is_error && !slot.transient.is_empty() || slot.prompt_live {
        return &slot.transient;
    }
    if !slot.sticky.is_empty() {
        return &slot.sticky;
    }
    if slot.searching {
        return &slot.search_line;
    }
    &slot.transient
}

/// The primary lane: the centre text with its hint split off for the secondary lane, as StatusBar.qml's centreText does.
pub fn primary<'a>(centre_text: &'a str, hint: &str) -> &'a str {
    if hint.is_empty() {
        centre_text
    } else {
        centre_text.strip_suffix(hint).unwrap_or(centre_text)
    }
}

/// The centre's ink: errors take the error role, everything else foreground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CentreRole {
    Error,
    Foreground,
}

pub fn centre_role(slot: &Slot) -> CentreRole {
    if slot.transient_is_error && !slot.transient.is_empty() {
        CentreRole::Error
    } else {
        CentreRole::Foreground
    }
}

/// The reload notice, board reload-notice: Reloaded with the muted changed count the watch reported.
pub fn reloaded(i18n: &I18n, changed: usize) -> String {
    i18n.trn_args(
        "status.host.reloaded",
        count_arg(changed),
        &[&count(i18n, changed)],
    )
}

/// The trailing disk facts, StatusBar.qml fsText: a known name draws even while the next answer is out, alone when its room is unknown.
pub fn disk(i18n: &I18n, name: &str, free_size: Option<&str>, pending: bool) -> String {
    if !name.is_empty() {
        return match free_size {
            Some(free) => i18n.tr_args("status.host.free", &[name, free]),
            None => name.to_owned(),
        };
    }
    if pending {
        String::new()
    } else {
        i18n.tr("status.host.unknown")
    }
}

/// The disk zone's name: where the room is unknown, as on a gvfs share that reads fuse, a named place reads as its kind in the menu's class words; no name stays empty.
pub fn place_name(i18n: &I18n, name: &str, class: &str, room_known: bool) -> String {
    match class {
        _ if room_known || name.is_empty() => name.to_owned(),
        "network" => i18n.tr("menu.class.network"),
        "phone" => i18n.tr("menu.class.phone"),
        "usb" => i18n.tr("menu.class.usb"),
        _ => name.to_owned(),
    }
}

/// The selection's byte total, or -1: every selected row held with a known size, dirs only through a finished walk.
pub fn selection_bytes(
    count: usize,
    held: usize,
    sizes: &[Option<u64>],
    dirs_walked: &[bool],
    is_dir: &[bool],
) -> i64 {
    if count == 0
        || count > held
        || sizes.len() < count
        || dirs_walked.len() < count
        || is_dir.len() < count
    {
        return -1;
    }
    let mut bytes: i64 = 0;
    for at in 0..count {
        if is_dir[at] && !dirs_walked[at] {
            return -1;
        }
        let Some(size) = sizes[at] else { return -1 };
        bytes = bytes.saturating_add(i64::try_from(size).unwrap_or(i64::MAX));
    }
    bytes
}
