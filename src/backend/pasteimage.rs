// Clipboard image paste as a named file: the backend runs wl-paste itself, so the bytes never cross JSON.
use crate::backend::opsreq::{op_err, usable_dest, OpMsg};
use crate::backend::proto::{error_line, error_line_with_code};
use crate::backend::undo::{Entry, Step};
use crate::json::field_str;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

// The code an empty clipboard answers with; the UI shows Menu.EMPTY_CLIPBOARD for it, so the sentence has one copy.
const EMPTY_CODE: &str = "emptyclipboard";
// What a client that ignores the code reads; the UI never shows it.
const EMPTY_MSG: &str = "the clipboard holds no image";
// A clipboard image bigger than this is refused rather than buffered, so it is headroom and not a limit anyone reaches.
const IMAGE_MAX_BYTES: u64 = 256 * 1024 * 1024;
// A clipboard owner that never answers must not hold the backend, and wl-paste is local, so this is a hang.
const PASTE_TIMEOUT: Duration = Duration::from_secs(30);
// A rename that loses a same-name race takes the next free name and tries again, up to this many landings.
const PASTE_LAND_TRIES: u32 = 1000;
// One pipe read, so a huge image crosses the cap in bounded steps rather than one allocation.
const PASTE_READ_CHUNK: usize = 65536;
// The suffixes unique() tries past the leaf: stem (2) through stem (999), so a second paste in the same second lands beside the first.
const PASTE_UNIQUE_END: u32 = 1000;

// The folder that takes the image and the name it takes.
pub struct Ask {
    dest: String,
    name: String,
}

impl Ask {
    // Sample input: {"c":"pasteimage","dest":"/home/gm/Pictures","name":"Pasted image 2026-09-23 at 10.14.32.png"}
    pub fn parse(line: &str) -> Ask {
        Ask { dest: field_str(line, "dest").unwrap_or_default(), name: field_str(line, "name").unwrap_or_default() }
    }
}

pub fn start(out: &mut impl Write, ops: &mut super::opsdispatch::Ops, ask: &Ask) {
    let (dest, name) = (ask.dest.as_str(), ask.name.as_str());
    let dest_path = match usable_dest(dest) {
        Ok(d) => d,
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            return;
        }
    };
    // The loop checks the name's shape only; unique() stats the destination, so it runs on the paste thread.
    if !quick_name_ok(name) {
        let e = op_err("pasteimage", name, "a name cannot be empty, . or .., or contain /.");
        writeln!(out, "{}", error_line(&e)).ok();
        out.flush().ok();
        return;
    };
    let tx = ops.tx.clone();
    let dest_owned = dest_path;
    let raw = name.to_string();
    std::thread::spawn(move || {
        let (line, entry) = execute(&dest_owned, &raw, &["wl-paste".to_string(), "--type".to_string(), "image/png".to_string()]);
        let _ = tx.send(OpMsg::Pasted { line, entry });
    });
}

// The reply lands on the loop: the entry journals beside the made line, so z undoes the file.
pub fn land(out: &mut impl Write, ops: &mut super::opsdispatch::Ops, line: String, entry: Option<Entry>) {
    if let Some(entry) = entry {
        ops.journal.push(entry);
    }
    writeln!(out, "{}", line).ok();
}

// The shape check with no path call, so the loop never waits on a destination.
fn quick_name_ok(name: &str) -> bool {
    !(name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0'))
}

// The name the file takes: as given, FAT-sanitized on a FAT destination, made unique by a " (n)" suffix; None is a name no file can take.
fn valid_name(name: &str, dest: &Path) -> Option<String> {
    if !quick_name_ok(name) {
        return None;
    }
    let mut leaf = name.to_string();
    if super::fatnames::on_fat(dest) && super::fatnames::needs_rename(&leaf) {
        leaf = super::fatnames::sanitize(&leaf);
    }
    Some(unique(dest, &leaf))
}

// The first free name: the leaf, then "stem (n).ext", so a second paste in the same second lands beside the first.
fn unique(dest: &Path, leaf: &str) -> String {
    if dest.join(leaf).symlink_metadata().is_err() {
        return leaf.to_string();
    }
    let (stem, ext) = match leaf.rfind('.') {
        Some(at) => (&leaf[..at], &leaf[at..]),
        None => (leaf, ""),
    };
    for n in 2..PASTE_UNIQUE_END {
        let next = format!("{} ({}){}", stem, n, ext);
        if dest.join(&next).symlink_metadata().is_err() {
            return next;
        }
    }
    leaf.to_string()
}

// One paste, start to line: the clipboard argv runs, the free name is chosen after it answers, the bytes stream into a per-paste O_EXCL temp, and a no-replace rename publishes it.
fn execute(dest: &Path, name: &str, argv: &[String]) -> (String, Option<Entry>) {
    let fail = |msg: &str| (error_line(&op_err("pasteimage", name, msg)), None);
    if argv.is_empty() {
        return fail("the clipboard could not be read");
    }
    if !quick_name_ok(name) {
        return fail("a name cannot be empty, . or .., or contain /.");
    }
    let bytes = match read_paste(argv) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return (error_line_with_code(&op_err("pasteimage", name, EMPTY_MSG), EMPTY_CODE), None),
        Err(e) => return fail(&e),
    };
    let Some(leaf) = valid_name(name, dest) else {
        return fail("a name cannot be empty, . or .., or contain /.");
    };
    // Per paste, so two pastes in flight never share a temp and neither unlinks the other's.
    static PASTE_TMP_NEXT: AtomicUsize = AtomicUsize::new(0);
    let tmp = dest.join(format!(".flea-paste-{}-{}.tmp", std::process::id(), PASTE_TMP_NEXT.fetch_add(1, Ordering::SeqCst)));
    let write = (|| -> Result<std::path::PathBuf, String> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o666).open(&tmp)
            .map_err(|e| format!("could not write a temp file: {}", io_message(&e)))?;
        file.write_all(&bytes).map_err(|e| format!("could not write a temp file: {}", io_message(&e)))?;
        // A FAT destination has no hard links, so the temp moves into its name and never replaces one.
        let mut target = dest.join(&leaf);
        for _ in 0..PASTE_LAND_TRIES {
            match crate::backend::renamecompat::rename_noreplace(&tmp, &target) {
                Ok(()) => return Ok(target),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => target = dest.join(unique(dest, &leaf)),
                Err(e) => return Err(format!("could not write {}: {}", leaf, io_message(&e))),
            }
        }
        Err(format!("{} is already there", leaf))
    })();
    let target = match write {
        Ok(target) => target,
        Err(msg) => {
            std::fs::remove_file(&tmp).ok();
            return fail(&msg);
        }
    };
    let line = super::opsreq::made_line(true, &target.to_string_lossy());
    (line, Some(Entry { op: "paste".to_string(), steps: vec![Step::Created { path: target }] }))
}

// The clipboard's PNG bytes, or why there are none; the reader runs beside the reaper because a big image fills the pipe.
fn read_paste(argv: &[String]) -> Result<Vec<u8>, String> {
    let mut child = crate::backend::locale::command(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "nothing on this system could paste an image; wl-clipboard is not installed".to_string()
            } else {
                format!("the clipboard could not be read: {}", io_message(&e))
            }
        })?;
    let mut out = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; PASTE_READ_CHUNK];
        while let Some(pipe) = out.as_mut() {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    bytes.extend_from_slice(&chunk[..n]);
                    if bytes.len() as u64 > IMAGE_MAX_BYTES {
                        break;
                    }
                }
            }
        }
        bytes
    });
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|e| format!("the clipboard could not be read: {}", io_message(&e)))? {
            Some(status) => {
                let bytes = reader.join().unwrap_or_default();
                // The size first: an image past the cap broke the reader and killed the pipe, so its status never succeeds.
                if bytes.len() as u64 > IMAGE_MAX_BYTES {
                    return Err("the clipboard image is too large to paste".to_string());
                }
                // A tool that exits non-zero holds no image, which reads as the empty clipboard.
                if !status.success() {
                    return Ok(Vec::new());
                }
                return Ok(bytes);
            }
            None => {
                if start.elapsed() > PASTE_TIMEOUT {
                    child.kill().ok();
                    let _ = child.wait();
                    return Err("the clipboard did not answer".to_string());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn io_message(e: &std::io::Error) -> String {
    e.to_string()
}

#[cfg(test)]
#[path = "pasteimage_tests.rs"]
mod paste_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn bad_names_are_refused_before_anything_runs() {
        let d = TestDir::new("pasteimage-names");
        let dest = d.dir("dest");
        for bad in ["", ".", "..", "a/b", "a\0b"] {
            assert!(valid_name(bad, &dest).is_none(), "{:?} is not a file name", bad);
        }
        assert!(valid_name("Pasted image 2026-09-23 at 10.14.32.png", &dest).is_some());
    }

    #[test]
    fn a_second_paste_in_the_same_second_lands_beside_the_first() {
        let d = TestDir::new("pasteimage-unique");
        let dest = d.dir("dest");
        std::fs::write(dest.join("a.png"), "taken").unwrap();
        assert_eq!(unique(&dest, "a.png"), "a (2).png");
        assert_eq!(unique(&dest, "b.png"), "b.png");
        assert_eq!(unique(&dest, "a"), "a");
    }

    #[test]
    fn an_image_on_the_clipboard_lands_as_a_named_file() {
        let d = TestDir::new("pasteimage-lands");
        let dest = d.dir("dest");
        let leaf = "Pasted image 2026-09-23 at 10.14.32.png".to_string();
        let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        let escapes: String = png.iter().map(|b| format!("\\{:03o}", b)).collect();
        let script = format!("printf '{}'", escapes);
        let (line, entry) = execute(&dest, &leaf, &["sh".to_string(), "-c".to_string(), script]);
        assert!(line.contains(r#""t":"made""#), "a paste answers made, got {}", line);
        assert!(line.contains("Pasted image 2026-09-23 at 10.14.32.png"), "got {}", line);
        assert_eq!(std::fs::read(dest.join(&leaf)).unwrap(), png);
        assert!(entry.is_some(), "a paste journals its file so z undoes it");
        // The per-paste temp is renamed away, so no .flea-paste- leftover stays behind.
        assert!(std::fs::read_dir(&dest).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".flea-paste-")), "the temp is gone");
    }

    #[test]
    fn an_empty_clipboard_is_a_code_not_a_file() {
        let d = TestDir::new("pasteimage-empty");
        let dest = d.dir("dest");
        let (line, entry) = execute(&dest, "a.png", &["true".to_string()]);
        assert!(line.contains(r#""code":"emptyclipboard""#), "the empty clipboard rides a code, got {}", line);
        assert!(!line.contains("y copies"), "the sentence has one copy, in the UI: {}", line);
        assert!(entry.is_none());
        assert!(std::fs::symlink_metadata(dest.join("a.png")).is_err(), "nothing was written");
    }

    #[test]
    fn a_failing_tool_reads_as_the_empty_clipboard() {
        let d = TestDir::new("pasteimage-failing");
        let dest = d.dir("dest");
        let argv = ["sh".to_string(), "-c".to_string(), "printf PNGDATA; exit 1".to_string()];
        let (line, entry) = execute(&dest, "a.png", &argv);
        assert!(line.contains(r#""code":"emptyclipboard""#), "a non-zero exit rides the empty code even with bytes, got {}", line);
        assert!(entry.is_none());
        assert!(std::fs::symlink_metadata(dest.join("a.png")).is_err(), "nothing was written");
    }

    #[test]
    fn a_missing_tool_names_itself() {
        let d = TestDir::new("pasteimage-notool");
        let dest = d.dir("dest");
        let (line, _) = execute(&dest, "a.png", &["flea-definitely-not-a-program".to_string()]);
        assert!(line.contains("wl-clipboard is not installed"), "got {}", line);
    }
}
