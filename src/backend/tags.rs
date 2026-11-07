// Dolphin-compatible file tags in user.xdg.tags, read per viewport row and never swept.
use crate::error::FleaError;
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;

// The attribute Dolphin reads and writes; a comma separated list of names.
pub(super) const ATTR_NAME: &str = "user.xdg.tags";
// ENOTSUP on Linux; EOPNOTSUPP shares this value, so one constant refuses either spelling.
const ENOTSUP: i32 = 95;
// ENODATA: the filesystem answers tags, this file just has none.
const ENODATA: i32 = 61;
// A hostile value cannot exceed the filesystem's own limit; this caps the read, never the write.
pub(crate) const MAX_VALUE: usize = 65536;

// Flea's seven names in menu order, which is also the 1 to 7 key order; see ui/js/Tags.js.
pub const NAMES: [&str; 7] = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"];

extern "C" {
    fn lgetxattr(path: *const c_char, name: *const c_char, value: *mut c_void, size: usize) -> isize;
    fn lsetxattr(path: *const c_char, name: *const c_char, value: *const c_void, size: usize, flags: c_int)
        -> c_int;
    fn lremovexattr(path: *const c_char, name: *const c_char) -> c_int;
    fn fsetxattr(fd: c_int, name: *const c_char, value: *const c_void, size: usize, flags: c_int) -> c_int;
}

// Sample input: "red" answers Some("Red"), "Custom" answers None.
pub fn known(name: &str) -> Option<&'static str> {
    NAMES.iter().find(|known| known.eq_ignore_ascii_case(name)).copied()
}

// One stored name answers a lowercased want, so foreign names match in any case.
pub fn name_matches(got: &str, want_lower: &str) -> bool {
    got.to_lowercase() == want_lower
}

// Sample input: b"Red, Custom ,,Blue" answers ["Red", "Custom", "Blue"].
pub fn parse(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == b',')
        .map(|part| String::from_utf8_lossy(part).trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

pub fn join(names: &[String]) -> Vec<u8> {
    names.join(",").into_bytes()
}

// A name no caller can produce still refuses here rather than truncating at the syscall.
fn cpath(path: &Path) -> Result<CString, FleaError> {
    CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| FleaError {
        where_: "tags".into(),
        path: path.to_string_lossy().into(),
        msg: "the file name cannot be represented".into(),
    })
}

fn attr() -> CString {
    CString::new(ATTR_NAME).expect("the attribute name holds no NUL")
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn io_err(where_: &str, path: &Path, code: i32) -> FleaError {
    crate::error::from_io(where_, &path.to_string_lossy(), &std::io::Error::from_raw_os_error(code))
}

#[cfg(test)]
thread_local! { pub(crate) static SEARCH_TAG_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

// Sample input: a file Dolphin tagged "Red,Blue" answers Some of those bytes; untagged answers None.
pub fn read_raw(path: &Path) -> Result<Option<Vec<u8>>, FleaError> {
    #[cfg(test)]
    SEARCH_TAG_CALLS.with(|calls| calls.set(calls.get() + 1));
    let cpath = cpath(path)?;
    let name = attr();
    let size = unsafe { lgetxattr(cpath.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        return match errno() {
            ENODATA => Ok(None),
            code => Err(io_err("tags", path, code)),
        };
    }
    if size as usize > MAX_VALUE {
        return Err(FleaError {
            where_: "tags".into(),
            path: path.to_string_lossy().into(),
            msg: "the stored tags are too large to read".into(),
        });
    }
    // Sized by the probe above; a concurrent write growing it past that fails ERANGE below, one shrinking it answers short.
    let mut buf = vec![0u8; size as usize];
    let got = unsafe { lgetxattr(cpath.as_ptr(), name.as_ptr(), buf.as_mut_ptr() as *mut c_void, buf.len()) };
    if got < 0 {
        return Err(io_err("tags", path, errno()));
    }
    buf.truncate((got as usize).min(buf.len()));
    Ok(Some(buf))
}

pub fn read(path: &Path) -> Result<Vec<String>, FleaError> {
    #[cfg(test)]
    super::tagreadprobe::observe(path);
    Ok(read_raw(path)?.map(|raw| parse(&raw)).unwrap_or_default())
}

#[cfg(test)]
thread_local! {
    // Tests drive the refusal without a foreign filesystem; the seam is per thread and absent from a release build.
    pub static FORCE_UNSUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    // Counts support probes on this thread, so a test pins how often a directory is asked.
    pub static PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    // Counts copy_tags calls on this thread, so a test pins the copy_node branch.
    pub static COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn forced_unsupported() -> bool {
    FORCE_UNSUPPORTED.with(|f| f.get())
}

#[cfg(not(test))]
fn forced_unsupported() -> bool {
    false
}

fn probe_code(path: &Path) -> Result<(), i32> {
    #[cfg(test)]
    PROBES.with(|probes| probes.set(probes.get() + 1));
    let cpath = match cpath(path) {
        Ok(path) => path,
        // A name that cannot be represented fails later with its own sentence, not here.
        Err(_) => return Ok(()),
    };
    let name = attr();
    let size = unsafe { lgetxattr(cpath.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
    if size >= 0 {
        return Ok(());
    }
    match errno() {
        ENODATA => Ok(()),
        code => Err(code),
    }
}

// None when tags may be written; Some refusal naming the filesystem when they cannot.
pub fn support_error(path: &Path) -> Option<FleaError> {
    if forced_unsupported() {
        return Some(refusal(path));
    }
    match probe_code(path) {
        Ok(()) => None,
        Err(code) if code == ENOTSUP => Some(refusal(path)),
        // A missing or unreadable file fails later with its own sentence when it is read.
        Err(_) => None,
    }
}

// The once-per-directory probe: false only where the filesystem refuses user attributes, so a transient error never hides the row.
pub fn dir_supports(path: &Path) -> bool {
    if forced_unsupported() {
        return false;
    }
    !matches!(probe_code(path), Err(code) if code == ENOTSUP)
}

// Sample refusal: where "settags", naming the filesystem that cannot hold the attribute.
fn refusal(path: &Path) -> FleaError {
    let fs = crate::backend::fsinfo::read(path).map(|info| info.name).unwrap_or_else(|| "unknown".into());
    FleaError {
        where_: "settags".into(),
        path: path.to_string_lossy().into(),
        msg: format!("{} does not support file tags, so nothing was changed", fs),
    }
}

fn write_raw(path: &Path, value: &[u8]) -> Result<(), FleaError> {
    let cpath = cpath(path)?;
    let name = attr();
    let done =
        unsafe { lsetxattr(cpath.as_ptr(), name.as_ptr(), value.as_ptr() as *const c_void, value.len(), 0) };
    if done == 0 {
        Ok(())
    } else {
        Err(io_err("settags", path, errno()))
    }
}

fn remove(path: &Path) -> Result<(), FleaError> {
    let cpath = cpath(path)?;
    let name = attr();
    let done = unsafe { lremovexattr(cpath.as_ptr(), name.as_ptr()) };
    if done == 0 {
        return Ok(());
    }
    match errno() {
        // Already absent is the state asked for.
        ENODATA => Ok(()),
        code => Err(io_err("settags", path, code)),
    }
}

// Sample input: ["Red", "Custom"] writes b"Red,Custom"; [] removes the attribute.
pub fn write_names(path: &Path, names: &[String]) -> Result<(), FleaError> {
    if names.is_empty() {
        remove(path)
    } else {
        write_raw(path, &join(names))
    }
}

// Restores one file's previous attribute exactly, absent included, which is what undo records.
pub fn restore(path: &Path, before: &Option<Vec<u8>>) -> Result<(), FleaError> {
    match before {
        Some(bytes) => write_raw(path, bytes),
        None => remove(path),
    }
}

// Sample output: a file tagged Red and Custom answers ["Red","Custom"], so Properties shows a foreign word too.
pub fn names_json(path: &Path) -> String {
    let names = read(path).unwrap_or_default();
    format!("[{}]", names.iter().map(|name| format!("\"{}\"", crate::json::escape(name))).collect::<Vec<_>>().join(","))
}

// A cross-device copy carries the tags onto the new item; a destination without user xattrs keeps it and hears nothing.
pub fn copy_tags(src: &Path, dst: &Path) {
    #[cfg(test)]
    COPIES.with(|calls| calls.set(calls.get() + 1));
    if let Ok(Some(bytes)) = read_raw(src) {
        let _ = write_raw(dst, &bytes);
    }
}

// The same through the destination's descriptor, which reaches an O_TMPFILE inode that has no name yet.
pub fn copy_tags_onto(src: &Path, dst: &std::fs::File) {
    use std::os::unix::io::AsRawFd;
    #[cfg(test)]
    COPIES.with(|calls| calls.set(calls.get() + 1));
    if let Ok(Some(bytes)) = read_raw(src) {
        let name = attr();
        unsafe { fsetxattr(dst.as_raw_fd(), name.as_ptr(), bytes.as_ptr() as *const c_void, bytes.len(), 0) };
    }
}

// Test seam: the copy_tags calls this thread made since the last read.
#[cfg(test)]
pub fn test_take_copies() -> usize {
    COPIES.with(|calls| calls.replace(0))
}

pub use super::tagsedit::apply;

#[cfg(test)]
pub(crate) static SEAM: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn names_are_canonical_whatever_case_the_caller_uses() {
        assert_eq!(known("red"), Some("Red"));
        assert_eq!(known("GRAY"), Some("Gray"));
        assert_eq!(known("Custom"), None);
    }

    #[test]
    fn a_tag_the_wire_cannot_hold_is_refused_in_a_sentence_that_names_why() {
        assert_eq!(apply(&[], "add", "a,b").unwrap_err().msg, "a tag cannot contain a comma");
        assert_eq!(apply(&[], "add", "  ").unwrap_err().msg, "a tag name cannot be empty or hold control characters");
        assert_eq!(apply(&[], "add", "a\nb").unwrap_err().msg, "a tag name cannot be empty or hold control characters");
    }

    #[test]
    fn parsing_keeps_foreign_names_and_drops_only_empties() {
        assert_eq!(parse(b"Red, Custom ,,Blue"), vec!["Red", "Custom", "Blue"]);
        assert!(parse(b"").is_empty());
        assert!(parse(b",,,").is_empty());
    }

    #[test]
    fn read_and_write_round_trip_in_a_testdir() {
        let d = TestDir::new("tagsround");
        let file = d.file("a.txt", "body");
        assert_eq!(read(&file).unwrap(), Vec::<String>::new());
        write_names(&file, &["Red".to_string(), "Blue".to_string()]).unwrap();
        assert_eq!(read(&file).unwrap(), vec!["Red", "Blue"]);
    }

    #[test]
    fn a_foreign_name_survives_a_toggle_of_a_flea_one() {
        let d = TestDir::new("tagsforeign");
        let file = d.file("a.txt", "body");
        write_names(&file, &["Custom".to_string(), "Red".to_string()]).unwrap();
        let toggled = apply(&read(&file).unwrap(), "toggle", "Red").unwrap();
        assert_eq!(toggled, vec!["Custom"]);
        write_names(&file, &toggled).unwrap();
        assert_eq!(read(&file).unwrap(), vec!["Custom"]);
    }

    #[test]
    fn add_is_case_insensitive_and_remove_is_quiet_when_absent() {
        assert_eq!(apply(&[], "add", "BLUE").unwrap(), vec!["Blue"]);
        assert_eq!(apply(&["Red".to_string()], "add", "red").unwrap(), vec!["Red"]);
        assert_eq!(apply(&["Red".to_string()], "remove", "Blue").unwrap(), vec!["Red"]);
        assert!(apply(&["Red".to_string()], "clear", "Red").unwrap().is_empty());
    }

    #[test]
    fn remove_drops_only_the_named_tag() {
        assert_eq!(
            apply(&["Custom".to_string(), "Red".to_string()], "remove", "Red").unwrap(),
            vec!["Custom"]
        );
    }

    #[test]
    fn a_bad_operation_or_tag_refuses_before_any_syscall() {
        assert!(apply(&[], "paint", "Red").is_err());
        assert!(apply(&[], "add", "").is_err());
        assert!(apply(&[], "add", "a,b").is_err());
    }

    #[test]
    fn copy_keeps_tags_and_an_untagged_source_leaves_the_destination_alone() {
        let d = TestDir::new("tagscopy");
        let src = d.file("src.txt", "body");
        let dst = d.join("dst.txt");
        std::fs::write(&dst, "body").unwrap();
        write_names(&src, &["Green".to_string()]).unwrap();
        copy_tags(&src, &dst);
        assert_eq!(read(&dst).unwrap(), vec!["Green"]);
        let plain = d.file("plain.txt", "body");
        let plain_dst = d.join("plain-dst.txt");
        std::fs::write(&plain_dst, "body").unwrap();
        copy_tags(&plain, &plain_dst);
        assert_eq!(read(&plain_dst).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn restore_puts_back_present_and_absent_alike() {
        let d = TestDir::new("tagsrestore");
        let file = d.file("a.txt", "body");
        write_names(&file, &["Red".to_string()]).unwrap();
        restore(&file, &None).unwrap();
        assert_eq!(read(&file).unwrap(), Vec::<String>::new());
        restore(&file, &Some(b"Blue".to_vec())).unwrap();
        assert_eq!(read(&file).unwrap(), vec!["Blue"]);
    }

    #[test]
    fn a_forced_unsupported_filesystem_refuses_naming_itself() {
        let _guard = SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let d = TestDir::new("tagsrefuse");
        let file = d.file("a.txt", "body");
        FORCE_UNSUPPORTED.with(|f| f.set(true));
        let err = support_error(&file).expect("the forced seam refuses");
        FORCE_UNSUPPORTED.with(|f| f.set(false));
        assert_eq!(err.where_, "settags");
        assert!(err.msg.contains("does not support file tags"), "{}", err.msg);
        assert!(err.msg.contains("nothing was changed"), "{}", err.msg);
        assert_eq!(read(&file).unwrap(), Vec::<String>::new(), "the refusal wrote nothing");
    }

    #[test]
    fn a_supported_filesystem_answers_no_refusal() {
        let _guard = SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let d = TestDir::new("tagssupport");
        let file = d.file("a.txt", "body");
        assert!(support_error(&file).is_none());
    }

    // The real probe with no forced seam: procfs answers ENOTSUP, so the row hides there.
    #[test]
    fn the_real_probe_refuses_procfs_without_the_forced_seam() {
        let dir = Path::new("/proc");
        assert!(!dir_supports(dir), "lgetxattr user xattrs on procfs answers ENOTSUP");
        let err = support_error(dir).expect("procfs cannot store tags");
        assert_eq!(err.where_, "settags");
        assert!(err.msg.contains("does not support file tags"), "{}", err.msg);
    }
}
