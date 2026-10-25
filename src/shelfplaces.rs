// Where a shelf action can send files, which is Flea's own places list and not a second picker:
// the destinations this shelf used last, then Home, the XDG user directories and the bookmarks the
// rail already draws. Actions: recent destinations first, because the same folder is usually the
// answer twice running.
use crate::error::io_message;
use crate::jsondoc::{self, Json};
use crate::uistore;
use std::fs;
use std::path::{Path, PathBuf};

const DIR: &str = "omarchy/flea-shelf";
const RECENTS: &str = "dests.json";
// Five, the same number of piles the card's own menu keeps, and the same reason: a list, not a log.
const KEPT: usize = 5;

// A reply created for this invocation is transient, including one the consumer refuses.
struct ChooserReply<'a>(&'a Path);

impl Drop for ChooserReply<'_> {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(self.0) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("flea: {} could not be cleaned up: {}", self.0.display(), io_message(&e));
            }
        }
    }
}

fn file() -> Result<PathBuf, String> {
    Ok(uistore::state_home()?.join(DIR).join(RECENTS))
}

fn discard_reply(path: &Path) -> Result<(), String> {
    crate::persistdoc::read(path, crate::persistdoc::reply)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{} could not be consumed: {}", path.display(), io_message(&e))),
    }
}

fn consume_reply(path: &Path, answer: &str) -> Result<(), String> {
    crate::persistdoc::parse(answer, crate::persistdoc::reply)
        .map_err(|e| format!("{}: the chooser reply was rejected ({})", path.display(), e))?;
    fs::remove_file(path)
        .map_err(|e| format!("{} could not be consumed: {}", path.display(), io_message(&e)))
}

pub fn recents() -> Vec<String> {
    let Ok(path) = file() else { return Vec::new() };
    let doc = match crate::persistdoc::read(&path, crate::persistdoc::dests) {
        Ok(doc) => doc,
        Err(_) => return Vec::new(),
    };
    doc.get("dests")
        .and_then(Json::as_array)
        .map(|list| list.iter().filter_map(Json::as_str).map(String::from).collect())
        .unwrap_or_default()
}

// A destination used again moves to the front rather than appearing twice.
fn ordered(held: &[String], dest: &str) -> Vec<String> {
    let mut kept: Vec<String> = held.iter().filter(|one| one.as_str() != dest).cloned().collect();
    kept.insert(0, dest.to_string());
    kept.truncate(KEPT);
    kept
}

pub fn remember(dest: &str) -> Result<(), String> {
    let path = file()?;
    uistore::make_dir(path.parent().ok_or("the shelf has no folder to write in")?)?;
    let _lock = uistore::take_lock(&path.with_extension("json.lock"))?;
    crate::persistdoc::read(&path, crate::persistdoc::dests)?;
    let kept = ordered(&recents(), dest);
    let doc = Json::Obj(vec![(
        "dests".to_string(),
        Json::Arr(kept.into_iter().map(Json::Str).collect()),
    )]);
    uistore::replace(&path, &jsondoc::render(&doc))
}

// flea shelf places: one path per line, in the order the card offers them.
pub fn command() -> i32 {
    for place in places() {
        println!("{}", place);
    }
    0
}

pub fn places() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dest in recents() {
        if Path::new(&dest).is_dir() && seen.insert(dest.clone()) {
            out.push(dest);
        }
    }
    if !home.is_empty() && seen.insert(home.clone()) {
        out.push(home.clone());
    }
    let config = crate::userfile::config_home().unwrap_or_else(|_| PathBuf::from(&home).join(".config"));
    for dir in user_dirs(&fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default(), &home) {
        if dir != home && Path::new(&dir).is_dir() && seen.insert(dir.clone()) {
            out.push(dir);
        }
    }
    for mark in bookmarks(&fs::read_to_string(config.join("gtk-3.0/bookmarks")).unwrap_or_default()) {
        if Path::new(&mark).is_dir() && seen.insert(mark.clone()) {
            out.push(mark);
        }
    }
    out
}

// The globs the pane accepts, one per suffix of the probe's own table.
fn image_globs() -> Vec<String> {
    crate::backend::flash::IMAGE_SUFFIXES.iter().map(|(suffix, _)| format!("*{suffix}")).collect()
}

// flea flash <verb>: choose [start] is the flasher's one verb, and anything else is a usage error.
pub fn flash_command(args: &[String]) -> i32 {
    if args.get(2).map(String::as_str) == Some("choose") {
        return flash_choose(&args[3..]);
    }
    eprintln!("flea: flash takes choose [start]");
    2
}

// flea flash choose [start]: the "Choose an image…" row, Flea's own picker with disk image filters first, in the download directory.
fn flash_choose(rest: &[String]) -> i32 {
    let start = rest.first().map(String::as_str).unwrap_or("");
    let dir = match uistore::state_home() {
        Ok(state) => state.join(DIR),
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let reply = dir.join("reply.json");
    if let Err(e) = uistore::make_dir(&dir) {
        eprintln!("flea: {}", e);
        return 2;
    }
    if let Err(e) = discard_reply(&reply) {
        eprintln!("flea: {}", e);
        return 2;
    }
    let _reply_cleanup = ChooserReply(&reply);
    let globs = image_globs().into_iter().map(Json::Str).collect();
    let request = Json::Obj(vec![
        ("mode".to_string(), Json::Str("open".to_string())),
        ("directory".to_string(), Json::Bool(false)),
        ("multiple".to_string(), Json::Bool(false)),
        ("title".to_string(), Json::Str("Choose an image".to_string())),
        ("folder".to_string(), Json::Str(if start.is_empty() { download_dir() } else { start.to_string() })),
        ("filters".to_string(), Json::Arr(vec![
            Json::Obj(vec![("label".to_string(), Json::Str("Disk images".to_string())), ("globs".to_string(), Json::Arr(globs))]),
            Json::Obj(vec![("label".to_string(), Json::Str("All files".to_string())), ("globs".to_string(), Json::Arr(vec![Json::Str("*".to_string())]))]),
        ])),
    ]);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("flea"));
    let finished = std::process::Command::new(exe)
        .arg("--pick")
        .arg(&reply)
        .env("FLEA_PICKER", jsondoc::render(&request))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
    if finished.map(|s| !s.success()).unwrap_or(true) {
        eprintln!("flea: the chooser could not be opened");
        return 2;
    }
    let answer = match fs::read_to_string(&reply) {
        Ok(answer) => answer,
        Err(e) => {
            eprintln!("flea: {} was not written, so the chooser answered nothing: {}", reply.display(), io_message(&e));
            return 2;
        }
    };
    if let Err(e) = consume_reply(&reply, &answer) {
        eprintln!("flea: {}", e);
        return 2;
    }
    if let Some(path) = chosen_file(&answer) {
        println!("{}", path);
    }
    0
}

// Sample input, the picker's reply file: {"response":0,"uris":["file:///home/gm/Downloads/debian-13.iso"]}
pub fn chosen_file(answer: &str) -> Option<String> {
    chosen_dir(answer)
}

// The XDG download directory, or the home directory when user-dirs names none.
fn download_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let config = crate::userfile::config_home().unwrap_or_else(|_| PathBuf::from(&home).join(".config"));
    let text = fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
    download_dir_with(&home, &text)
}

// Split from download_dir so the fallback is pinned without touching the environment.
fn download_dir_with(home: &str, config: &str) -> String {
    for line in config.lines() {
        if let Some((name, value)) = crate::captures::user_dirs_line(line, home) {
            if name == "XDG_DOWNLOAD_DIR" && !value.is_empty() && value != home {
                return value.trim_end_matches('/').to_string();
            }
        }
    }
    if home.is_empty() {
        return String::new();
    }
    PathBuf::from(home).join("Downloads").to_string_lossy().to_string()
}
// flea shelf choose <title> [start]: the "Choose a folder" row of the destination flyout, which is
// Flea's own picker rather than a second one. It prints the directory that came back, and nothing at
// all when the operator pressed esc in it.
pub fn choose(rest: &[String]) -> i32 {
    let title = rest.first().map(String::as_str).unwrap_or("Choose a folder");
    let start = rest.get(1).map(String::as_str).unwrap_or("");
    let dir = match uistore::state_home() {
        Ok(state) => state.join(DIR),
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let reply = dir.join("reply.json");
    if let Err(e) = uistore::make_dir(&dir) {
        eprintln!("flea: {}", e);
        return 2;
    }
    if let Err(e) = discard_reply(&reply) {
        eprintln!("flea: {}", e);
        return 2;
    }
    let _reply_cleanup = ChooserReply(&reply);
    let request = Json::Obj(vec![
        ("mode".to_string(), Json::Str("open".to_string())),
        ("directory".to_string(), Json::Bool(true)),
        ("multiple".to_string(), Json::Bool(false)),
        ("title".to_string(), Json::Str(title.to_string())),
        ("folder".to_string(), Json::Str(start.to_string())),
    ]);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("flea"));
    let finished = std::process::Command::new(exe)
        .arg("--pick")
        .arg(&reply)
        .env("FLEA_PICKER", jsondoc::render(&request))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
    if finished.map(|s| !s.success()).unwrap_or(true) {
        eprintln!("flea: the chooser could not be opened");
        return 2;
    }
    // The picker writes its reply whichever way it ends, esc included, so a missing file is a
    // chooser that died rather than an operator who changed their mind.
    let answer = match fs::read_to_string(&reply) {
        Ok(answer) => answer,
        Err(e) => {
            eprintln!("flea: {} was not written, so the chooser answered nothing: {}", reply.display(), io_message(&e));
            return 2;
        }
    };
    if let Err(e) = consume_reply(&reply, &answer) {
        eprintln!("flea: {}", e);
        return 2;
    }
    if let Some(path) = chosen_dir(&answer) {
        println!("{}", path);
    }
    0
}

// Sample input, what the picker writes into its reply file:
// {"response":0,"uris":["file:///home/gm/Work"]}
pub fn chosen_dir(answer: &str) -> Option<String> {
    let doc = jsondoc::parse(answer).ok()?;
    if doc.get("response").and_then(Json::as_f64)? as i64 != 0 {
        return None;
    }
    let uri = doc.get("uris").and_then(Json::as_array)?.first().and_then(Json::as_str)?;
    let path = uri.strip_prefix("file://")?;
    Some(decode(path))
}

// Every XDG directory rather than one by name, which is the half captures::user_dirs_entry cannot
// answer; the line itself is read by that module's own parser so the two cannot drift apart.
pub fn user_dirs(text: &str, home: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((name, value)) = crate::captures::user_dirs_line(line, home) else { continue };
        if !name.starts_with("XDG_") {
            continue;
        }
        let path = value.trim_end_matches('/').to_string();
        // corner: this box points TEMPLATES, PUBLICSHARE and DESKTOP at $HOME, which is not a place.
        if path.is_empty() || path == home {
            continue;
        }
        out.push(path);
    }
    out
}

// Sample input, one line of ~/.config/gtk-3.0/bookmarks: file:///home/gm/Work Work
pub fn bookmarks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // corner: a bookmark may be smb:// or sftp://, which is a location and not a folder here.
        if !line.starts_with("file://") {
            continue;
        }
        let uri = line.split_whitespace().next().unwrap_or_default();
        out.push(decode(&uri["file://".len()..]));
    }
    out
}

// Percent escapes, which a bookmarks file carries for every space in a path.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Read as bytes, never as a string slice: a percent followed by one hex digit and a
        // multi-byte character would put a slice boundary inside that character and panic.
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "shelfplaces_tests.rs"]
mod tests;
