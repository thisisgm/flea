// One bounded line count, which is everything the preview column's "Lines" fact is built from. It
// sits outside metareq.rs because it shares nothing with the archive, media and subprocess machinery
// there, and because that file is at its size cap.
use crate::backend::regfile::open_regular;
use std::io::Read;
use std::path::Path;

// A line count reads at most this much of a file, so a huge log costs one bounded read and says so.
pub const LINE_BUDGET: u64 = 1 << 20;

// What one bounded count learned. partial and failed are separate because they are separate facts:
// a count can stop early on a file it read fine, and a file it could not open has no count at all.
pub struct LineCount {
    pub lines: u64,
    pub partial: bool,
    pub failed: bool,
}

// A file with no trailing newline still has a last line, so the count is newlines plus one for any
// bytes after the final one; an empty file has no lines at all.
pub fn count_lines(path: &Path) -> LineCount {
    let mut f = match open_regular(path) {
        Some(f) => f,
        // Denied, not a regular file, or the row vanished. Any of them would read as "empty" at zero.
        None => return LineCount { lines: 0, partial: false, failed: true },
    };
    let mut buf = vec![0u8; 64 * 1024];
    let mut read: u64 = 0;
    let mut newlines: u64 = 0;
    let mut last_was_newline = true;
    let mut any = false;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        any = true;
        for &b in &buf[..n] {
            if b == b'\n' {
                newlines += 1;
                last_was_newline = true;
            } else {
                last_was_newline = false;
            }
        }
        read += n as u64;
        if read >= LINE_BUDGET {
            return LineCount { lines: newlines, partial: true, failed: false };
        }
    }
    if !any {
        return LineCount { lines: 0, partial: false, failed: false };
    }
    let lines = newlines + if last_was_newline { 0 } else { 1 };
    LineCount { lines, partial: false, failed: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::fifotest::{mkfifo, peek, within, FifoWriter};
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    #[test]
    fn a_file_that_cannot_be_opened_is_not_an_empty_one() {
        let d = TestDir::new("linecountdenied");
        let empty = count_lines(&d.file("empty.txt", ""));
        let path = d.file("denied.txt", "a\nb\nc\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let denied = count_lines(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!((empty.lines, empty.partial, empty.failed), (0, false, false), "an empty file really has no lines");
        assert_eq!((denied.lines, denied.partial, denied.failed), (0, false, true), "a file that could not be opened has no count at all");
    }

    // Counted on a thread with a bound, because the defect under test is an open that never returns.
    fn bounded(path: PathBuf) -> LineCount {
        within("a count", move || count_lines(&path))
    }

    fn no_count(path: PathBuf, why: &str) {
        let c = bounded(path);
        assert_eq!((c.lines, c.partial, c.failed), (0, false, true), "{}", why);
    }

    #[test]
    fn nothing_but_a_regular_file_is_ever_opened_for_a_count() {
        let d = TestDir::new("linecountkinds");
        let fifo = d.join("pipe");
        mkfifo(&fifo);
        no_count(fifo, "opening a fifo with no writer never returns");
        std::os::unix::fs::symlink("pipe", d.join("topipe")).unwrap();
        no_count(d.join("topipe"), "the open follows the link, so the check has to as well");
        no_count(d.dir("sub"), "a directory opens fine and reads EISDIR, which used to answer zero lines");
        let sock = d.join("sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        no_count(sock, "a socket is not a file to count");
        no_count(d.join("never-existed"), "a row that vanished still has no count");
        // The guard is this narrow so a real text file is still counted, which is what this file is for.
        assert_eq!(bounded(d.file("real.txt", "a\nb\nc\n")).lines, 3);
        // And through a link to one, the case that separates the two candidate calls: the open follows it.
        std::os::unix::fs::symlink("real.txt", d.join("toreal")).unwrap();
        let through = bounded(d.join("toreal"));
        assert_eq!((through.lines, through.partial, through.failed), (3, false, false));
    }

    #[test]
    fn a_fifo_that_has_a_writer_keeps_every_byte_a_count_did_not_read() {
        let d = TestDir::new("linecountfed");
        let p = d.join("pipe");
        mkfifo(&p);
        // feeding returns only once the bytes are in the pipe, or this case degenerates into the writerless one.
        let mut writer = FifoWriter::feeding(&p, "a\nb\nc\n", &d.join("wrote"));
        let counted = bounded(p.clone());
        let left = peek(p);
        let stopped = writer.stop();
        assert_eq!((counted.lines, counted.partial, counted.failed), (0, false, true));
        assert_eq!(left, b"a\nb\nc\n".to_vec(), "a count must never eat a pipe someone else is reading");
        assert!(stopped, "the writer this test started is killed by its own pid");
    }
}
