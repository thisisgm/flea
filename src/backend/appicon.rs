// An AppImage row's own icon: 7z lists the image in the archive jail, the .DirIcon chain is resolved here, one member is extracted and published as the row's thumbnail.
use crate::backend::aliases::Aliases;
use crate::backend::archivework::{run_boxed_cancellable_capped, Work};
use crate::backend::child::{run_with_timeout, Ran};
use crate::backend::sandbox;
use crate::backend::thumbargv::argv;
use crate::backend::thumbcache::uri_for;
use crate::backend::thumbspec::{is_runnable, Spec};
use crate::backend::thumbs::{discard, publish, record_failure, records_marker, wrote_something, Job, Outcome, Tables, JOB_TIMEOUT, THUMB_SIZE};
use crate::backend::thumbwrite::{exclusive_temp, WORK_TAG};
use crate::error::FleaError;
use flea::bytename::ExactText as _;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// The 7zip optdepend; the jail binds /usr read-only, so this path resolves inside it.
pub(crate) const TOOL: &str = "/usr/bin/7z";
// exec[0] of the internal spec: never run, only read by is_route.
const ROUTE: &str = "appimage-icon";
pub(crate) const MIMES: [&str; 2] = ["application/vnd.appimage", "application/x-iso9660-appimage"];
// The first path component the icon is named by, at the image root.
const DIR_ICON: &str = ".DirIcon";
// A .DirIcon may link on to the real file; a fifth link, or a loop, is refused.
const LINK_LIMIT: usize = 4;
const MAX_ICON_BYTES: u64 = 4 << 20;
const MAX_LIST_BYTES: u64 = 16 << 20;
const MAX_LINK_BYTES: u64 = 4096;
// 7z's own words for a file that is not an archive, which is the one tool failure that judges the file.
const NOT_AN_ARCHIVE: [&str; 2] = ["Cannot open the file as", "Can not open the file as"];
// 7z's own words for an archive it opened and could not read to the end, the other tool failure that judges the file; sample last stderr line: "ERROR: Data Error : .DirIcon".
const DATA_ERROR: [&str; 4] = ["Data Error", "Headers Error", "Unexpected end of archive", "CRC Failed"];
const WATCH_STEP: Duration = Duration::from_millis(50);
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const SVG_SCAN_BYTES: usize = 4096;

pub(crate) fn available() -> bool {
    is_runnable(TOOL)
}

// Both names are canonicalised, because the mime database may answer either spelling of the type.
pub(crate) fn is_appimage(mime: &str, aliases: &Aliases) -> bool {
    MIMES.iter().any(|known| aliases.canonical(known) == aliases.canonical(mime))
}

pub(crate) fn route_spec() -> Spec {
    Spec { exec: vec![ROUTE.to_string()] }
}

pub(crate) fn is_route(spec: &Spec) -> bool {
    spec.exec.first().is_some_and(|program| program == ROUTE)
}

// Why no icon was made: the image said no, the machine failed, or the clock ran out; only the first is recorded against the file.
#[derive(Debug, PartialEq)]
pub(crate) enum Stop {
    Refused,
    Fault,
    TimedOut,
}

pub(crate) fn run(tables: &Tables, job: &Job) -> Outcome {
    run_with(&[TOOL.to_string()], within, jailed, tables, job)
}

// Runs one tool argv with the image readable and the work folder writable, under the deadline.
type Runner = fn(Vec<String>, &Path, &mut Work, Duration) -> Result<(), Stop>;

// Wraps one decoder argv for the jail, status fd included.
type Jail = fn(&[String], &Path, &Path) -> Vec<String>;

fn jailed(inner: &[String], input: &Path, out: &Path) -> Vec<String> {
    let mut full = sandbox::wrap(inner, input, out);
    sandbox::add_status(&mut full, inner.len());
    full
}

// The tool is a prefix argv, the runner and the decoder's jail are parameters, so a test can stand a script in for 7z and run on a box whose bwrap cannot start.
pub(crate) fn run_with(tool: &[String], runner: Runner, jail: Jail, tables: &Tables, job: &Job) -> Outcome {
    // corner: a missing package or an input that will not canonicalise is the machine's, so nothing is recorded; see AGENTS.md "Thumbnail sandbox".
    if !usable(tool, sandbox::available()) {
        return Outcome::Failed;
    }
    let deadline = Instant::now() + JOB_TIMEOUT;
    let image = match std::fs::canonicalize(&job.path) {
        Ok(p) => p,
        Err(_) => return Outcome::Failed,
    };
    let key_uri = uri_for(&job.path);
    let final_path = tables.cache.large_path(&key_uri);
    let dir = tables.cache.large_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return Outcome::Failed;
    }
    let Some((mut work, temp)) = begin(&dir, Work::new) else { return Outcome::Failed };
    let ran = match icon(tool, runner, &image, &mut work, deadline).and_then(|member| make(tables, &member, &temp, left(deadline), jail)) {
        Ok(ran) => ran,
        Err(Stop::TimedOut) => Ran::TimedOut,
        Err(Stop::Refused) => Ran::Failed,
        // corner: a jail that would not start, a full disk and a crashed tool are the machine's, so the file stays unrecorded; see AGENTS.md "Thumbnail pool".
        Err(Stop::Fault) => Ran::NotStarted,
    };
    if matches!(ran, Ran::Succeeded) && wrote_something(&temp) {
        return publish(&temp, &key_uri, job.mtime, final_path);
    }
    if records_marker(&ran, super::extclass::classify(&job.path)) {
        record_failure(&tables.cache, &key_uri, job.mtime, tables.specs.fingerprint());
    }
    discard(&temp)
}

fn usable(tool: &[String], sandbox_ready: bool) -> bool {
    sandbox_ready && tool.first().is_some_and(|program| is_runnable(program))
}

// The exclusive temp and the work folder, or neither: a temp whose folder could not be made is removed here.
fn begin(dir: &Path, new_work: fn(&Path, &str) -> Result<Work, FleaError>) -> Option<(Work, PathBuf)> {
    let temp = exclusive_temp(dir)?;
    match new_work(dir, WORK_TAG) {
        Ok(work) => Some((work, temp)),
        Err(_) => {
            discard(&temp);
            None
        }
    }
}

fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

// Writes the thumbnail into temp: a PNG that fits as it is, anything else through the spec an image of that type takes, in the same jail.
fn make(tables: &Tables, member: &Path, temp: &Path, limit: Duration, jail: Jail) -> Result<Ran, Stop> {
    let bytes = std::fs::read(member).map_err(|_| Stop::Fault)?;
    let mime = if is_png(&bytes) {
        if fits(&bytes) {
            return Ok(if std::fs::write(temp, &bytes).is_ok() { Ran::Succeeded } else { Ran::NotStarted });
        }
        "image/png"
    } else if is_svg(&bytes) {
        "image/svg+xml"
    } else {
        return Ok(Ran::Failed);
    };
    // A box with no thumbnailer for the type cannot judge the file, so nothing is recorded.
    let Some(spec) = tables.specs.for_mime(mime, &tables.aliases) else { return Ok(Ran::NotStarted) };
    let Some((abs, inner)) = argv(spec, member, temp, THUMB_SIZE) else { return Ok(Ran::NotStarted) };
    if limit.is_zero() {
        return Err(Stop::TimedOut);
    }
    Ok(run_with_timeout(&jail(&inner, &abs, temp), limit))
}

// The stamp needs an IHDR right after the signature, so a PNG without one is refused here and recorded.
fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&PNG_SIGNATURE) && bytes.get(12..16) == Some(b"IHDR")
}

// The IHDR width and height, big-endian at bytes 16 and 20; is_png has already checked they exist.
fn fits(bytes: &[u8]) -> bool {
    let side = |at: usize| bytes.get(at..at + 4).map_or(u32::MAX, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    side(16) <= THUMB_SIZE && side(20) <= THUMB_SIZE
}

fn is_svg(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(&bytes[..bytes.len().min(SVG_SCAN_BYTES)]).contains("<svg")
}

// The extracted member's path: list once, resolve the chain, extract that one member.
fn icon(tool: &[String], runner: Runner, image: &Path, work: &mut Work, deadline: Instant) -> Result<PathBuf, Stop> {
    let list = work.dir.join("list.txt");
    // 7z lists on stdout and the jail discards it, so the shell redirects it into the work folder.
    let mut inner: Vec<String> = ["/usr/bin/sh", "-c", r#"out=$1; shift; exec "$@" >"$out""#, "sh"].map(String::from).to_vec();
    inner.push(list.exact_text().to_string());
    inner.extend_from_slice(tool);
    inner.extend(["l", "-slt", "--"].map(String::from));
    inner.push(image.exact_text().to_string());
    step(runner, inner, image, work, deadline)?;
    let mut hops = 0;
    let member = resolve(&read_capped(&list, MAX_LIST_BYTES)?, &mut |link| {
        hops += 1;
        let out = work.dir.join(format!("link-{}", hops));
        step(runner, extract(tool, image, &out, link), image, work, deadline)?;
        only_link(&out)
    })?;
    let out = work.dir.join("icon");
    step(runner, extract(tool, image, &out, &member), image, work, deadline)?;
    only_file(&out)
}

// 7z e flattens paths, so one member lands as out/<base name>; a symlink member lands as a symlink.
fn extract(tool: &[String], image: &Path, out: &Path, member: &str) -> Vec<String> {
    let mut inner: Vec<String> = tool.to_vec();
    inner.extend(["e", "-y", "-bd"].map(String::from));
    inner.push(format!("-o{}", out.exact_text()));
    inner.push("--".to_string());
    inner.push(image.exact_text().to_string());
    inner.push(member.to_string());
    inner
}

fn step(runner: Runner, inner: Vec<String>, image: &Path, work: &mut Work, deadline: Instant) -> Result<(), Stop> {
    match left(deadline) {
        limit if limit.is_zero() => Err(Stop::TimedOut),
        limit => runner(inner, image, work, limit),
    }
}

// One job in the archive jail under a deadline; a cancel the clock set reads as a timeout.
fn within(inner: Vec<String>, image: &Path, work: &mut Work, limit: Duration) -> Result<(), Stop> {
    let cancel = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            let end = Instant::now() + limit;
            while !done.load(Ordering::Relaxed) && Instant::now() < end {
                std::thread::sleep(WATCH_STEP);
            }
            if !done.load(Ordering::Relaxed) {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        let result = run_boxed_cancellable_capped("appimage icon", inner, image, work, &cancel);
        done.store(true, Ordering::Relaxed);
        result
    });
    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(stop_for(&e, cancel.load(Ordering::Relaxed))),
    }
}

// Only 7z saying the file is not an archive, or that its data is damaged, judges the file; a jail that would not start, a signal and a bad exit are the machine's.
fn stop_for(err: &FleaError, timed_out: bool) -> Stop {
    if timed_out {
        Stop::TimedOut
    } else if NOT_AN_ARCHIVE.iter().chain(&DATA_ERROR).any(|words| err.msg.contains(words)) {
        Stop::Refused
    } else {
        Stop::Fault
    }
}

// A missing file is an extract that made nothing, which is the image's answer; any other IO error is the work folder's.
fn io_stop(err: &std::io::Error) -> Stop {
    if err.kind() == std::io::ErrorKind::NotFound { Stop::Refused } else { Stop::Fault }
}

fn read_capped(path: &Path, cap: u64) -> Result<String, Stop> {
    let mut bytes = Vec::new();
    let file = std::fs::File::open(path).map_err(|e| io_stop(&e))?;
    file.take(cap + 1).read_to_end(&mut bytes).map_err(|e| io_stop(&e))?;
    if bytes.len() as u64 > cap {
        return Err(Stop::Refused);
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

// The one entry an extract left in out, with its metadata read without following a link.
fn only_entry(out: &Path) -> Result<(PathBuf, std::fs::Metadata), Stop> {
    let mut found = std::fs::read_dir(out).map_err(|e| io_stop(&e))?.flatten();
    let (Some(entry), None) = (found.next(), found.next()) else { return Err(Stop::Refused) };
    let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| io_stop(&e))?;
    Ok((entry.path(), meta))
}

// The extract must leave exactly one regular file, never a link, within the size cap.
fn only_file(out: &Path) -> Result<PathBuf, Stop> {
    let (path, meta) = only_entry(out)?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_ICON_BYTES {
        return Err(Stop::Refused);
    }
    Ok(path)
}

// 7z lists a squashfs link without its target, so the link is extracted as a link and only its text is read, never followed.
fn only_link(out: &Path) -> Result<String, Stop> {
    let (path, meta) = only_entry(out)?;
    if !meta.file_type().is_symlink() {
        return Err(Stop::Refused);
    }
    let target = std::fs::read_link(&path).map_err(|e| io_stop(&e))?;
    let target = target.to_string_lossy().to_string();
    if target.is_empty() || target.len() as u64 > MAX_LINK_BYTES {
        return Err(Stop::Refused);
    }
    Ok(target)
}

struct Entry {
    link: Option<String>,
    is_link: bool,
    folder: bool,
    size: u64,
}

// Sample input, `7z l -slt` after the "----------" line: "Path = .DirIcon", "Mode = lrwxrwxrwx" for a squashfs link (no target is printed), then a blank line per record.
fn entries(listing: &str) -> HashMap<String, Entry> {
    let mut found = HashMap::new();
    let Some((_, records)) = listing.split_once("\n----------\n") else { return found };
    for record in records.split("\n\n") {
        let (mut path, mut link, mut is_link, mut folder, mut size) = (None, None, false, false, 0);
        for line in record.lines() {
            match line.split_once(" = ") {
                Some(("Path", v)) => path = clean(v),
                Some(("Symbolic Link", v)) if !v.is_empty() => link = Some(v.to_string()),
                Some(("Mode", v)) => is_link = v.starts_with('l'),
                Some(("Folder", v)) => folder = v == "+",
                Some(("Size", v)) => size = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        if let Some(path) = path {
            let is_link = is_link || link.is_some();
            found.insert(path, Entry { link, is_link, folder, size });
        }
    }
    found
}

// A path inside the image with `.` and `..` folded away; None for an absolute path or one that climbs past the root.
fn clean(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

// The member holding the icon: .DirIcon, or where its link chain ends, never outside the image; read_link answers the target of a link the listing does not name.
pub(crate) fn resolve(listing: &str, read_link: &mut dyn FnMut(&str) -> Result<String, Stop>) -> Result<String, Stop> {
    let all = entries(listing);
    let mut at = DIR_ICON.to_string();
    for _ in 0..=LINK_LIMIT {
        let entry = all.get(&at).ok_or(Stop::Refused)?;
        if !entry.is_link {
            if entry.folder || entry.size == 0 || entry.size > MAX_ICON_BYTES {
                return Err(Stop::Refused);
            }
            return Ok(at);
        }
        let target = match &entry.link {
            Some(target) => target.clone(),
            None => read_link(&at)?,
        };
        if target.starts_with('/') {
            return Err(Stop::Refused);
        }
        let joined = match at.rsplit_once('/') {
            Some((dir, _)) => format!("{}/{}", dir, target),
            None => target,
        };
        at = clean(&joined).ok_or(Stop::Refused)?;
    }
    Err(Stop::Refused)
}

#[cfg(test)]
#[path = "appicon_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "appicon_damage_tests.rs"]
mod damage_tests;
