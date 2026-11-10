// gio trash is the freedesktop trash this box already shares with every GTK app; nothing about the spec is written here, only an argv and a result.
use crate::error::FleaError;
use flea::bytename::ExactText as _;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};

// gio trash --list prints one entry per line as "<uri>\t<original path>".
const LIST_SEP: char = '\t';
// A hung mount must not hold the single operation slot; trashbrowse.rs bounds gio the same way.
pub(crate) const TRASH_TIMEOUT: &str = "10s";
pub(crate) const TRASH_KILL_AFTER: &str = "1s";

// The bound every gio call runs under, shared with trashbrowse so the two never drift.
pub(crate) fn gio_prefix() -> [String; 4] {
    ["timeout".to_string(), format!("--kill-after={TRASH_KILL_AFTER}"), TRASH_TIMEOUT.to_string(), "gio".to_string()]
}

// What a successful trash recorded, which is the only thing that can reverse it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub original: PathBuf,
    pub uri: String,
    // URIs the settle list already held, so a step trashed while the settle list was unreadable resolves at undo.
    pub before: Vec<String>,
    // When the chunk holding a step the settle lists never attributed began, in .trashinfo DeletionDate form, else empty.
    pub trashed_at: String,
}

impl Entry {
    // A settle the list never answered keeps the path and the time, so undo resolves it then.
    pub fn unresolved(original: PathBuf, before: Vec<String>, trashed_at: String) -> Entry {
        Entry { original, uri: String::new(), before, trashed_at }
    }
}

// A test stands in for gio on its own thread, because a build container has no trash:// to list or restore.
#[cfg(test)]
type StandInFn = std::rc::Rc<dyn Fn(&[&str]) -> Option<std::process::Output>>;
#[cfg(test)]
thread_local! {
    pub(crate) static STAND_IN: std::cell::RefCell<Option<StandInFn>> =
        const { std::cell::RefCell::new(None) };
}

// corner: gio on an unresponsive network mount can hang, which stalls this operation's own thread and nothing else.
pub(crate) fn gio(args: &[&str]) -> Option<std::process::Output> {
    #[cfg(test)]
    if let Some(stand_in) = STAND_IN.with(|slot| slot.borrow().clone()) {
        return stand_in(args);
    }
    let argv = gio_argv(args);
    // Writes forward gio's diagnostic to the user; only listing and metadata output are parsed.
    let mut command = if args.first() == Some(&"info") || args == ["trash", "--list"] {
        crate::backend::locale::command(&argv[0])
    } else {
        std::process::Command::new(&argv[0])
    };
    command.args(flea::bytename::args(&argv[1..])).output().ok()
}

// The argv gio runs under, so the suite pins the bound without spawning a subprocess.
fn gio_argv(args: &[&str]) -> Vec<String> {
    let mut out = gio_prefix().to_vec();
    out.extend(args.iter().map(|s| s.to_string()));
    out
}

// One chunk's worth of paths per gio call, so the 10 s bound measures a stall and not the batch size.
const TRASH_CHUNK: usize = 64;

// The batch's own reason, so a hung mount or a denial reads as what happened rather than silence; `moved` is whether any item already went, in an earlier chunk or by rename.
pub(crate) fn failure_reason(output: &Option<std::process::Output>, moved: bool) -> String {
    match output {
        None if moved => "the Trash could not be reached, so the rest were not moved".to_string(),
        None => "the Trash could not be reached, so nothing was moved".to_string(),
        // timeout kills its own group on the kill-after, so a gio that outlived the first signal ends the wait as signal 9, with no exit code.
        Some(o) if matches!(o.status.code(), Some(124 | 137)) || o.status.signal() == Some(9) => "The Trash took too long to answer".to_string(),
        Some(o) => {
            let line = String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("").trim().to_string();
            if !line.is_empty() { line } else if moved { "the rest could not be moved to the Trash".to_string() } else { "the items could not be moved to the Trash".to_string() }
        }
    }
}

// Sample line: trash:///a.txt\t/home/gm/a.txt
fn parse_list(stdout: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(2, LIST_SEP);
        let uri = match parts.next() {
            Some(u) if !u.is_empty() => u,
            _ => continue,
        };
        if let Some(original) = parts.next() {
            if !original.is_empty() {
                out.push(Entry { original: PathBuf::from(original), uri: uri.to_string(), before: Vec::new(), trashed_at: String::new() });
            }
        }
    }
    out
}

// A test counts the expensive call per item, never wall time: every list() here is one gio spawn.
#[cfg(test)]
thread_local! {
    static LIST_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn test_list_calls() -> usize {
    LIST_CALLS.with(|v| v.get())
}
#[cfg(test)]
pub(crate) fn test_reset_list_calls() {
    LIST_CALLS.with(|v| v.set(0));
}

pub fn list() -> Option<Vec<Entry>> {
    #[cfg(test)]
    LIST_CALLS.with(|v| v.set(v.get() + 1));
    // None unless gio exited 0, so a list cut short by the wait journals no URI at all.
    match gio(&["trash", "--list"]) {
        Some(o) if o.status.success() => Some(parse_list(&String::from_utf8_lossy(&o.stdout))),
        _ => None,
    }
}

// The URI is captured here, not looked up later, because two files trashed from one path both list that same original.
pub fn trash(paths: &[PathBuf]) -> (Vec<Entry>, usize, String) {
    match trash_checked(paths, None) {
        Ok(result) => result,
        Err(error) => { eprintln!("flea: {}", error); (Vec::new(), paths.len(), error) }
    }
}

pub(crate) fn trash_checked(paths: &[PathBuf], selection: Option<&[super::menu_actions::Selected]>) -> Result<(Vec<Entry>, usize, String), String> {
    trash_announcing(paths, selection, &mut |_, _, _| {})
}

// announce(moved, failed, reason) fires once gio has returned and the disk shows the outcome, before the settle list that only names the journal's URIs.
pub(crate) fn trash_announcing(paths: &[PathBuf], selection: Option<&[super::menu_actions::Selected]>, announce: &mut dyn FnMut(&[PathBuf], usize, &str)) -> Result<(Vec<Entry>, usize, String), String> {
    // A leading empty path is a drag that left this window. The receiver may already have moved
    // the file and still accepted a move; a path that is gone is then nothing left to delete.
    let (quiet_gone, paths) = match paths.first() {
        Some(path) if path.as_os_str().is_empty() => (true, &paths[1..]),
        _ => (false, paths),
    };
    super::menu_actions::validate_sources(selection, paths)?;
    if paths.is_empty() {
        return Ok((Vec::new(), 0, String::new()));
    }
    // Issue 88, nixfred: a path already gone before the call read as trashed afterwards and was
    // journaled as a step with no trash entry, which is where undo stopped, so it is a failure here.
    let (present, missing): (Vec<&PathBuf>, Vec<&PathBuf>) =
        paths.iter().partition(|p| p.symlink_metadata().is_ok());
    if present.is_empty() {
        let reason = if quiet_gone || missing.is_empty() { String::new() } else { "those items are already gone".to_string() };
        return Ok((Vec::new(), if quiet_gone { 0 } else { missing.len() }, reason));
    }
    // Home renames win; subvolume and bind mounts gio refuses use their own topdir Trash.
    let mut topdir = super::topdirtrash::Batch::read();
    let (direct, mut rest): (Vec<usize>, Vec<usize>) = (0..present.len()).partition(|&i| {
        super::hometrash::renames_to_home(present[i])
            || super::topdirtrash::renames_to_topdir(&mut topdir, present[i])
    });
    let mut before = if rest.is_empty() { None } else { Some(list()) };
    super::menu_actions::validate_sources(selection, paths)?;
    let mut entries: Vec<Option<Entry>> = vec![None; present.len()];
    let mut failed = if quiet_gone { 0 } else { missing.len() };
    let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
    for i in direct {
        // Equal st_dev does not make a rename cross a bind mount, so EXDEV from home still tries topdir.
        let moved = super::hometrash::rename_to_home(present[i], &mut unsynced, &mut durable)
            .or_else(|| {
                super::topdirtrash::rename_to_topdir(&mut topdir, present[i], &mut unsynced)
            });
        match moved {
            Some(Ok(entry)) => entries[i] = Some(entry),
            // Gone before its move, the Issue 88 refusal; the rename left no info file behind.
            Some(Err(_)) => failed += 1,
            // EXDEV, EBUSY or a taken name: nothing moved, so gio gets this one as it would have.
            None => rest.push(i),
        }
    }
    super::hometrash::sync_folders(&mut unsynced);
    let mut output: Option<std::process::Output> = None;
    let mut still_here = 0;
    let gio_ran = !rest.is_empty();
    if gio_ran {
        rest.sort_unstable();
        let before = before.get_or_insert_with(list);
        // The exit status covers one chunk, so which paths actually went is read off the filesystem instead.
        // The local time each chunk's trash began, so a path no list attributes still names when its chunk went.
        let mut stamps: Vec<String> = Vec::new();
        for chunk in rest.chunks(TRASH_CHUNK) {
            let mut argv: Vec<String> = vec!["trash".to_string(), "--".to_string()];
            for &i in chunk {
                argv.push(present[i].exact_text().to_string());
            }
            let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
            stamps.push(super::hometrash::deletion_date_local());
            let out = gio(&refs);
            let stop = !matches!(&out, Some(o) if o.status.success());
            output = out;
            if stop {
                break;
            }
        }
        // The disk answers whether each path went, so the outcome is known before the settle list is read.
        let gone: Vec<bool> = rest.iter().map(|&i| present[i].symlink_metadata().is_err()).collect();
        still_here = gone.iter().filter(|g| !**g).count();
        failed += still_here;
        let gio_failed = !matches!(&output, Some(o) if o.status.success());
        let moved: Vec<PathBuf> = entries.iter().enumerate().filter(|(_, entry)| entry.is_some()).map(|(i, _)| present[i].to_path_buf()).chain(rest.iter().zip(&gone).filter(|(_, gone)| **gone).map(|(&i, _)| present[i].to_path_buf())).collect();
        let reason = trash_reason(failed, gio_failed, still_here, &output, !moved.is_empty());
        announce(&moved, failed, &reason);
        let after = list();
        for (pos, i) in rest.into_iter().enumerate() {
            if !gone[pos] {
                continue;
            }
            let p = present[i];
            entries[i] = Some(match before.as_ref().zip(after.as_ref()).and_then(|(b, a)| newest_entry_for(b, a, p)) {
                Some(e) => e,
                // A list that failed, or listed nothing new, still leaves the path, what the Trash held before and the time, so undo resolves the entry from a fresh list.
                None => {
                    let at = stamps.get(pos / TRASH_CHUNK).cloned().unwrap_or_default();
                    Entry::unresolved(p.clone(), uris_at(before.as_deref().unwrap_or_default(), p), at)
                }
            });
        }
    }
    let ok: Vec<Entry> = entries.into_iter().flatten().collect();
    // A batch renamed whole ran no gio, so its silence is not a gio failure.
    let gio_failed = gio_ran && !matches!(&output, Some(o) if o.status.success());
    let moved = !ok.is_empty();
    Ok((ok, failed, trash_reason(failed, gio_failed, still_here, &output, moved)))
}

// The one sentence a batch's failures share, so the early announce and the final result never word it differently.
fn trash_reason(failed: usize, gio_failed: bool, still_here: usize, output: &Option<std::process::Output>, moved: bool) -> String {
    if failed == 0 {
        String::new()
    } else if gio_failed {
        failure_reason(output, moved)
    } else if still_here > 0 {
        "those items are still there".to_string()
    } else {
        "those items are already gone".to_string()
    }
}

// The URIs a list held for one original path, which is what tells a later list's new entry from the old ones.
pub(crate) fn uris_at(list: &[Entry], path: &Path) -> Vec<String> {
    list.iter().filter(|e| e.original == path).map(|e| e.uri.clone()).collect()
}

// Only an entry that was not already in the trash before this call can be one this call put there.
pub(crate) fn newest_entry_for(before: &[Entry], after: &[Entry], path: &Path) -> Option<Entry> {
    after
        .iter()
        .find(|e| e.original == path && !before.iter().any(|b| b.uri == e.uri))
        .cloned()
}

pub fn restore(entry: &Entry) -> Result<(), FleaError> {
    if !entry.uri.is_empty() {
        // A home Trash entry whose own record names this path goes back in-process, so undo spawns no gio for it.
        if let Some(done) = super::homerestore::restore_from_home(entry) {
            return done.map_err(|msg| err(&msg));
        }
        // A topdir Trash entry on a mount gio refuses to name restores in-process the same way, on its own record's proof.
        if let Some(done) = super::topdirtrash::restore_from_topdir(entry) {
            return done.map_err(|msg| err(&msg));
        }
        return restore_uri(entry);
    }
    // A step with no time (a record from before the time was kept) cannot be told from another item of the same path.
    if entry.trashed_at.is_empty() {
        return Err(err("this item was trashed without a Trash entry, so it cannot be restored"));
    }
    // A list that fails says so, never that the item is gone from a Trash nobody could read.
    let Some(after) = list() else {
        return Err(err("the Trash could not be listed, so that item cannot be restored yet"));
    };
    let fresh: Vec<&Entry> = after.iter().filter(|e| e.original == entry.original && !entry.before.contains(&e.uri)).collect();
    // Two new entries for one path cannot be told apart, so undo refuses rather than guess.
    if fresh.len() > 1 {
        return Err(err(&format!(
            "that item was trashed at {} but the Trash holds more than one new item from that place, so it cannot be restored",
            shown(&entry.trashed_at)
        )));
    }
    let [one] = fresh.as_slice() else {
        return Err(err(&format!("that item was trashed at {} but is no longer listed, so it cannot be restored", shown(&entry.trashed_at))));
    };
    // Only the item trashed at this step's own time is this step's; a later item from the same place is refused.
    if !matches_time(&entry.trashed_at, one) {
        return Err(err(&format!("that item was trashed at {} but the Trash holds no item from that time, so it cannot be restored", shown(&entry.trashed_at))));
    }
    restore_uri(one)
}

// The app's one date form, YYYY-MM-DD HH:MM, cut from a DeletionDate stamp; a stamp of another shape prints as it is.
fn shown(stamp: &str) -> String {
    match stamp_secs(stamp) {
        Some(_) => format!("{} {}", &stamp[..10], &stamp[11..16]),
        None => stamp.to_string(),
    }
}

// Both stamps keep whole seconds from the same trash event, so a few seconds cover truncation and scheduling.
const DELETION_SKEW_SECS: i64 = 5;
// A chunk's stamp is taken as its gio call begins, and that call ends within TRASH_TIMEOUT plus TRASH_KILL_AFTER.
const CHUNK_SECS: i64 = 11;

// True when the candidate's own deletion date falls in this step's chunk; an unreadable date is not evidence of another item.
fn matches_time(trashed_at: &str, candidate: &Entry) -> bool {
    let Some(date) = deletion_date(&candidate.uri) else { return true; };
    let (Some(was), Some(now)) = (stamp_secs(trashed_at), stamp_secs(&date)) else { return true; };
    (-DELETION_SKEW_SECS..=CHUNK_SECS + DELETION_SKEW_SECS).contains(&(now - was))
}

// The Trash's own deletion date for one listed URI, in DeletionDate form.
fn deletion_date(uri: &str) -> Option<String> {
    let out = gio(&["info", "--attributes=trash::deletion-date", uri])?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).lines()
        .find_map(|line| line.trim_start().strip_prefix("trash::deletion-date: "))
        .map(str::to_string)
}

// The civil-date arithmetic counts days in 400-year eras from 0000-03-01, then shifts to 1970-01-01.
const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_YEAR: i64 = 365;
const DAYS_PER_ERA: i64 = 146097;
// From March, every five months span 153 days, so a month's first day is (153 * month + 2) / 5.
const DAYS_PER_FIVE_MONTHS: i64 = 153;
const EPOCH_SHIFT_DAYS: i64 = 719468;
const SECS_PER_DAY: i64 = 86400;
const SECS_PER_HOUR: i64 = 3600;
const SECS_PER_MINUTE: i64 = 60;

// Days since the Unix epoch for a civil date, so a DeletionDate becomes comparable seconds.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted = if month <= 2 { year - 1 } else { year };
    let month_of_year = if month <= 2 { month + 9 } else { month - 3 };
    let era = shifted.div_euclid(YEARS_PER_ERA);
    let year_of_era = shifted - era * YEARS_PER_ERA;
    let day_of_year = (DAYS_PER_FIVE_MONTHS * month_of_year + 2) / 5 + day - 1;
    era * DAYS_PER_ERA + (year_of_era * DAYS_PER_YEAR + year_of_era / 4 - year_of_era / 100 + day_of_year) - EPOCH_SHIFT_DAYS
}

// Seconds since the epoch for a "YYYY-MM-DDTHH:MM:SS" stamp, in the local time both writers keep.
fn stamp_secs(stamp: &str) -> Option<i64> {
    let bytes = stamp.as_bytes();
    if bytes.len() != 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let num = |from: usize, to: usize| stamp[from..to].parse::<i64>().ok();
    let date = days_from_civil(num(0, 4)?, num(5, 7)?, num(8, 10)?);
    Some(date * SECS_PER_DAY + num(11, 13)? * SECS_PER_HOUR + num(14, 16)? * SECS_PER_MINUTE + num(17, 19)?)
}

fn restore_uri(entry: &Entry) -> Result<(), FleaError> {
    match gio(&["trash", "--restore", &entry.uri]) {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => {
            let msg = String::from_utf8_lossy(&o.stderr);
            Err(err(msg.lines().last().unwrap_or("the item could not be restored from the Trash")))
        }
        None => Err(err("gio is not installed, so nothing can be restored from the Trash")),
    }
}

fn err(msg: &str) -> FleaError {
    FleaError { where_: "undo".to_string(), path: String::new(), msg: msg.to_string() }
}

// The journal's "t" record: an ordinary entry encodes as before, an unattributed settle adds its resolution data.
pub(crate) fn encode_step(entry: &Entry) -> crate::jsondoc::Json {
    use super::undocodec::{arr, obj, s};
    let mut fields = vec![("k", s("t")), ("path", s(&entry.original.exact_text())), ("uri", s(&entry.uri))];
    if !entry.before.is_empty() {
        fields.push(("before", arr(entry.before.iter().map(|uri| s(uri)).collect())));
    }
    if !entry.trashed_at.is_empty() {
        fields.push(("time", s(&entry.trashed_at)));
    }
    obj(fields)
}

// A record from before settle-time resolution data was kept has neither "before" nor "time", and decodes empty.
pub(crate) fn decode_step(pairs: &[(String, crate::jsondoc::Json)]) -> Option<super::undo::Step> {
    use super::undocodec::{checked_path, checked_text, checked_uri, get};
    let before = match get(pairs, "before") {
        Some(crate::jsondoc::Json::Arr(items)) => items.iter().filter_map(checked_text).collect(),
        _ => Vec::new(),
    };
    Some(super::undo::Step::Trashed(Entry {
        original: checked_path(get(pairs, "path")?)?,
        uri: checked_uri(get(pairs, "uri")?)?,
        before,
        trashed_at: get(pairs, "time").and_then(checked_text).unwrap_or_default(),
    }))
}

#[cfg(test)]
#[path = "trash_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "trashundo_tests.rs"]
mod undo_tests;
#[cfg(test)]
#[path = "trashfast_tests.rs"]
mod fast;
#[cfg(test)]
#[path = "trashwording_tests.rs"]
mod wording;
