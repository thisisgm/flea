// The one way this backend opens a row's own path, because a meta request names whatever the listing
// named and two of those kinds cannot be read at all: an open of a fifo with no writer never returns,
// and a directory opens fine and then reads EISDIR.
use crate::oflags::O_NONBLOCK;
use std::fs::File;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub fn open_regular(path: &Path) -> Option<File> {
    // Refused before any open, so the only fifo whose blocked writer this wakes is one swapped in after this stat.
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        return None;
    }
    open_if_regular(path, 0).ok()
}

// The open itself, which a copy also takes with O_NOFOLLOW: O_NONBLOCK covers the window after the
// caller's own stat, so a row swapped for a fifo inside it returns here instead of never returning.
pub fn open_if_regular(path: &Path, extra_flags: i32) -> std::io::Result<File> {
    open_if_regular_with_meta(path, extra_flags).map(|(f, _)| f)
}

// The same open, handing back the fstat its own check took, so a caller needing the mode never stats it twice.
pub fn open_if_regular_with_meta(path: &Path, extra_flags: i32) -> std::io::Result<(File, std::fs::Metadata)> {
    let f = std::fs::OpenOptions::new().read(true).custom_flags(O_NONBLOCK | extra_flags).open(path)?;
    // corner: this second check is what closes that window, and no test can schedule the swap that opens it.
    let meta = f.metadata().map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"))?;
    if !meta.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"));
    }
    Ok((f, meta))
}

// A whole document, bounded: missing is NotFound, not a regular file is InvalidInput (never opened), too big or not UTF-8 is InvalidData.
pub fn read_text(path: &Path, limit: u64) -> std::io::Result<String> {
    use std::io::{Error, ErrorKind, Read};
    let read = || {
        if !std::fs::metadata(path)?.is_file() {
            return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
        }
        let (file, meta) = open_if_regular_with_meta(path, 0)?;
        let mut text = String::new();
        if meta.len() > limit || file.take(limit + 1).read_to_string(&mut text)? as u64 > limit {
            return Err(Error::new(ErrorKind::InvalidData, format!("larger than {limit} bytes")));
        }
        Ok(text)
    };
    read().map_err(|e| Error::new(e.kind(), format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::fifotest::{mkfifo, within};
    use crate::backend::testdir::TestDir;
    use std::os::unix::io::AsRawFd;

    // Sample input, /proc/self/fdinfo/7: "pos:\t0\nflags:\t02004000\nmnt_id:\t29\nino:\t1234\n".
    fn open_flags(f: &File) -> i32 {
        let text = std::fs::read_to_string(format!("/proc/self/fdinfo/{}", f.as_raw_fd())).expect("fdinfo");
        let row = text.lines().find(|l| l.starts_with("flags:")).expect("a flags row");
        let octal = row.split_whitespace().nth(1).expect("a flags value");
        i32::from_str_radix(octal, 8).expect("octal flags")
    }

    #[test]
    fn the_open_that_closes_the_stat_window_is_the_non_blocking_one() {
        let d = TestDir::new("regfileflags");
        let f = open_regular(&d.file("real.txt", "body\n")).expect("a regular file is what this opens");
        assert_eq!(open_flags(&f) & O_NONBLOCK, O_NONBLOCK, "without O_NONBLOCK the racy open is the hang this exists to stop");
    }

    #[test]
    fn nothing_but_a_regular_file_is_ever_handed_back() {
        let d = TestDir::new("regfilekinds");
        let fifo = d.join("pipe");
        mkfifo(&fifo);
        assert!(within("open_regular", move || open_regular(&fifo)).is_none(), "a fifo is refused, and before the open rather than inside it");
        assert!(open_regular(&d.dir("sub")).is_none(), "a directory opens fine and reads EISDIR");
        assert!(open_regular(&d.join("never-existed")).is_none(), "a row that vanished has nothing to open");
        let real = d.file("real.txt", "body\n");
        std::os::unix::fs::symlink(&real, d.join("toreal")).unwrap();
        assert!(open_regular(&d.join("toreal")).is_some(), "the open follows a link, so the check has to as well");
    }

    #[test]
    fn a_document_read_is_bounded_and_names_what_it_refused() {
        use std::io::ErrorKind;
        let d = TestDir::new("regfiletext");
        let fifo = d.join("pipe");
        mkfifo(&fifo);
        let kind = |r: std::io::Result<String>| r.map_err(|e| e.kind());
        assert_eq!(kind(within("read_text", move || read_text(&fifo, 64))), Err(ErrorKind::InvalidInput));
        assert_eq!(kind(read_text(Path::new("/dev/zero"), 64)), Err(ErrorKind::InvalidInput), "a device is never opened");
        assert_eq!(kind(read_text(&d.join("never-existed"), 64)), Err(ErrorKind::NotFound));
        assert_eq!(kind(read_text(&d.file("big.txt", &"x".repeat(65)), 64)), Err(ErrorKind::InvalidData));
        std::fs::write(d.join("bin"), [0xff_u8, 0xfe]).unwrap();
        assert_eq!(kind(read_text(&d.join("bin"), 64)), Err(ErrorKind::InvalidData));
        assert_eq!(kind(read_text(&d.file("fits.txt", &"x".repeat(64)), 64)), Ok("x".repeat(64)));
    }
}
