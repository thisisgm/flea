//! The metadata cells and match run as ui/Row.qml prints them, with numbers, sizes and dates from core Locale.
use super::is_symlink;
use crate::navigation::sort::Size;
use flea::{
    i18n::{I18n, Locale},
    protocol::RowFacts,
};
use std::ffi::{c_char, c_long};

const PERMISSION_BITS: u32 = 9;
const TRIAD: [char; 3] = ['r', 'w', 'x'];
const NO_DATE: &str = "--";
const NO_SIZE_YET: &str = "\u{b7}";
const PARTIAL: &str = ">";
const SECONDS_PER_DAY: i64 = 86_400;

/// The words a row prints, resolved from one catalog snapshot outside paint so a language change reshapes once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Words {
    pub locale: Locale,
    pub link: String,
    pub name: String,
    pub mode: String,
    pub size: String,
    pub unknown_size: String,
    pub modified: String,
    pub location: String,
    pub used: String,
    pub kind: String,
    /// Set once the words come from a catalog snapshot, whose thread already warmed the zone.
    pub ready: bool,
}

impl Words {
    pub fn from_catalog(catalog: &I18n) -> Self {
        Self {
            locale: catalog.locale().clone(),
            link: catalog.tr("list.size_link"),
            name: catalog.tr("window.column_name"),
            mode: catalog.tr("list.column_mode"),
            size: catalog.tr("window.column_size"),
            unknown_size: catalog.tr("dialogs.file.properties.unknown"),
            modified: catalog.tr("window.column_modified"),
            location: catalog.tr("window.column_location"),
            used: catalog.tr("window.column_used"),
            kind: catalog.tr("list.column_kind"),
            ready: true,
        }
    }
}

/// Row.qml sizeText(): a link says so, a file prints its bytes, a folder its walk once answered, a dot before that.
pub fn size_text(row: &RowFacts, walked: Option<Size>, words: &Words) -> String {
    if is_symlink(row.mode) {
        return words.link.clone();
    }
    if !row.dir {
        return words.locale.format_size(row.size);
    }
    match walked {
        None => NO_SIZE_YET.into(),
        Some(size) if size.partial && size.bytes == 0 => words.unknown_size.clone(),
        Some(size) => {
            let bytes = words.locale.format_size(size.bytes);
            if size.partial {
                format!("{PARTIAL}{bytes}")
            } else {
                bytes
            }
        }
    }
}

/// Row.qml dateText(): the one fixed local form, or two dashes when the clock cannot place the stamp.
pub fn date_text(mtime: i64, words: &Words) -> String {
    // Words without a catalog snapshot would make this thread the first to read the zone, so dates wait for the catalog's.
    if !words.ready {
        return String::new();
    }
    words
        .locale
        .format_date(mtime)
        .unwrap_or_else(|| NO_DATE.into())
}

/// Format.permissions(): the nine owner, group and other bits as rwx triads.
pub fn permissions(mode: u32) -> String {
    (0..PERMISSION_BITS)
        .map(|at| {
            let bit = 1 << (PERMISSION_BITS - 1 - at);
            if mode & bit != 0 {
                TRIAD[at as usize % TRIAD.len()]
            } else {
                '-'
            }
        })
        .collect()
}

/// Match.run(): where a case-insensitive needle starts in a name, as a byte range safe to slice; a close match spans its first to its last matched letter.
pub fn match_run(hay: &str, needle: &str) -> Option<(usize, usize)> {
    let letters = match_letters(hay, needle);
    Some((letters.first()?.0, letters.last()?.1))
}

/// The matched letters of a name as byte ranges: one for a case-insensitive substring, each stretch the close matcher kept otherwise.
pub fn match_letters(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let lower = needle.to_lowercase();
    let mut folded = String::with_capacity(hay.len());
    let mut starts = Vec::with_capacity(hay.len());
    for (at, ch) in hay.char_indices() {
        for low in ch.to_lowercase() {
            starts.push(at);
            folded.push(low);
        }
    }
    let Some(at) = folded.find(&lower) else {
        let Some(close) = flea::ui_logic_listing_filter::matcher::Matcher::new(needle)
            .and_then(|mut matcher| matcher.close(hay))
        else {
            return Vec::new();
        };
        let byte = |at| {
            hay.char_indices()
                .nth(at)
                .map_or(hay.len(), |(byte, _)| byte)
        };
        return close
            .runs
            .iter()
            .map(|run| (byte(run.start), byte(run.start + run.len)))
            .collect();
    };
    let chars_before = folded[..at].chars().count();
    let chars_in = lower.chars().count();
    let Some(&start) = starts.get(chars_before) else {
        return Vec::new();
    };
    let end = starts
        .get(chars_before + chars_in)
        .copied()
        .unwrap_or(hay.len());
    vec![(start, end.max(start))]
}

// glibc's struct tm, which localtime_r fills whole and mktime reads back; Rust itself reads only mday.
#[repr(C)]
struct Tm {
    _sec: i32,
    _min: i32,
    _hour: i32,
    mday: i32,
    _mon: i32,
    _year: i32,
    _wday: i32,
    _yday: i32,
    _isdst: i32,
    _gmtoff: c_long,
    _zone: *const c_char,
}

extern "C" {
    fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    fn mktime(tm: *mut Tm) -> i64;
}

// Local midnight of the day holding this stamp, days ahead, built from components so a daylight-saving night still lands on midnight.
fn local_midnight(now: i64, days_ahead: i32) -> Option<i64> {
    let mut tm = std::mem::MaybeUninit::<Tm>::uninit();
    // SAFETY: both pointers are valid for the call, and a non-null return means tm was written whole.
    let mut tm = unsafe {
        if localtime_r(&now, tm.as_mut_ptr()).is_null() {
            return None;
        }
        tm.assume_init()
    };
    tm._sec = 0;
    tm._min = 0;
    tm._hour = 0;
    tm.mday += days_ahead;
    tm._isdst = -1;
    // SAFETY: tm is a whole struct tm, which mktime normalizes in place.
    let midnight = unsafe { mktime(&mut tm) };
    (midnight != -1).then_some(midnight)
}

/// Format.dayStart(): today's local midnight, in seconds.
pub fn day_start(now: i64) -> i64 {
    local_midnight(now, 0).unwrap_or(now - now.rem_euclid(SECONDS_PER_DAY))
}

/// Format.msUntilMidnight(): seconds to the next local midnight, so a recent-date highlight wakes once a day.
pub fn seconds_until_midnight(now: i64) -> i64 {
    local_midnight(now, 1).map_or(SECONDS_PER_DAY - now.rem_euclid(SECONDS_PER_DAY), |next| {
        next - now
    })
}

/// Format.isRecent(): a stamp on or after today's local midnight.
pub fn is_recent(mtime: i64, today_start: i64) -> bool {
    mtime >= today_start
}

#[cfg(test)]
mod tests {
    #[test]
    fn vf_more_close_match_bolds_the_matched_prefix() {
        assert_eq!(super::match_run("notes.txt", "ntoes"), Some((0, 5)));
        assert_eq!(super::match_run("Notes.TXT", "notes"), Some((0, 5)));
        assert_eq!(super::match_run("notes.txt", "zzzzz"), None);
        let (start, end) = super::match_run("\u{e9}notes.txt", "\u{e9}ntoes").unwrap();
        assert_eq!(&"\u{e9}notes.txt"[start..end], "\u{e9}notes");
    }

    /// S10V-10: a close match washes its matched letters only, never the letters between them (filter-close, CHANGES line 372).
    #[test]
    fn s10v10_a_close_match_washes_only_its_matched_letters() {
        let name = "a-very-long-file-name-that-goes-on-past-the-name-column.txt";
        let letters = super::match_letters(name, "note");
        let washed: String = letters.iter().map(|&(a, b)| &name[a..b]).collect();
        assert_eq!(washed, "note", "{letters:?}");
        let (start, end) = super::match_run(name, "note").unwrap();
        assert_eq!((letters[0].0, letters.last().unwrap().1), (start, end));
        assert!(
            end - start > 4,
            "the span still reaches from the first to the last letter, for elision"
        );
        assert_eq!(super::match_letters("todo.md", "DO"), vec![(2, 4)]);
        assert_eq!(super::match_letters("a.txt", "zz"), Vec::new());
        let letters = super::match_letters("\u{e9}notes.txt", "\u{e9}nts");
        let washed: String = letters
            .iter()
            .map(|&(a, b)| &"\u{e9}notes.txt"[a..b])
            .collect();
        assert_eq!(washed, "\u{e9}nts");
    }

    use super::*;

    #[test]
    fn permissions_and_match_runs_follow_format_and_match_js() {
        assert_eq!(permissions(0o100755), "rwxr-xr-x");
        assert_eq!(permissions(0o040700), "rwx------");
        assert_eq!(permissions(0o100644), "rw-r--r--");
        assert_eq!(match_run("Notes.TXT", "notes"), Some((0, 5)));
        assert_eq!(match_run("todo.md", "DO"), Some((2, 4)));
        assert_eq!(match_run("a.txt", ""), None);
        assert_eq!(match_run("a.txt", "zz"), None);
        let (start, end) = match_run("\u{e9}t\u{e9}.txt", "T\u{c9}").unwrap();
        assert_eq!(&"\u{e9}t\u{e9}.txt"[start..end], "t\u{e9}");
    }
}
