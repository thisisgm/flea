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

// How the text is stored, read from its byte order mark as src/textdecode.rs decodes it for the preview.
#[derive(Clone, Copy, PartialEq)]
enum Units {
    Bytes,
    Utf16 { big: bool },
}

// The count so far, in the units the preview draws, so a UTF-16 newline's zero byte is not a last line.
struct Tally {
    units: Units,
    // The first byte of a UTF-16 unit whose second byte is in the next read.
    odd: Option<u8>,
    newlines: u64,
    last_was_newline: bool,
    any: bool,
}

impl Tally {
    fn feed(&mut self, bytes: &[u8]) {
        let big = match self.units {
            Units::Bytes => {
                for &b in bytes {
                    self.unit(u16::from(b));
                }
                return;
            }
            Units::Utf16 { big } => big,
        };
        for &b in bytes {
            match self.odd.take() {
                None => self.odd = Some(b),
                Some(first) => self.unit(if big {
                    u16::from_be_bytes([first, b])
                } else {
                    u16::from_le_bytes([first, b])
                }),
            }
        }
    }

    fn unit(&mut self, unit: u16) {
        self.any = true;
        self.last_was_newline = unit == u16::from(b'\n');
        if self.last_was_newline {
            self.newlines += 1;
        }
    }
}

// The mark is not text: UTF-16's two bytes and UTF-8's three are skipped, and a UTF-16 file counts two-byte units.
// Sample input: FF FE 'h' 00 '\n' 00 is one line, where its bytes alone ended on a zero after the newline and read as two.
fn units_of(head: &[u8]) -> (Units, usize) {
    match head {
        [0xff, 0xfe, ..] => (Units::Utf16 { big: false }, 2),
        [0xfe, 0xff, ..] => (Units::Utf16 { big: true }, 2),
        [0xef, 0xbb, 0xbf, ..] => (Units::Bytes, 3),
        _ => (Units::Bytes, 0),
    }
}

// A file with no trailing newline still has a last line, so the count is newlines plus one for any
// text after the final one; an empty file has no lines at all.
pub fn count_lines(path: &Path) -> LineCount {
    let mut f = match open_regular(path) {
        Some(f) => f,
        // Denied, not a regular file, or the row vanished. Any of them would read as "empty" at zero.
        None => {
            return LineCount {
                lines: 0,
                partial: false,
                failed: true,
            }
        }
    };
    let mut buf = vec![0u8; 64 * 1024];
    let mut read: u64 = 0;
    let mut tally = Tally {
        units: Units::Bytes,
        odd: None,
        newlines: 0,
        last_was_newline: true,
        any: false,
    };
    let mut first = true;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let mut skip = 0;
        if first {
            // A mark split across two reads is not worth a second read: a regular file answers its first read whole.
            (tally.units, skip) = units_of(&buf[..n]);
            first = false;
        }
        tally.feed(&buf[skip..n]);
        read += n as u64;
        if read >= LINE_BUDGET {
            // Issue 96, nixfred: the budget bounds the work, it does not decide the answer. A file
            // ending exactly at it was read whole, and one more byte is what tells the two apart.
            let mut past = [0u8; 1];
            match f.read(&mut past) {
                Ok(0) => break,
                // A byte back, or a read that failed, both leave this count a floor and not a total.
                _ => {
                    return LineCount {
                        lines: tally.newlines,
                        partial: true,
                        failed: false,
                    }
                }
            }
        }
    }
    if !tally.any {
        return LineCount {
            lines: 0,
            partial: false,
            failed: false,
        };
    }
    let lines = tally.newlines + if tally.last_was_newline { 0 } else { 1 };
    LineCount {
        lines,
        partial: false,
        failed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::fifotest::{mkfifo, peek, within, FifoWriter};
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    // Issue 96, nixfred: a file of exactly the budget had been read whole, and the early return
    // reported the newline count as a floor, which for one unterminated line of a megabyte was zero.
    #[test]
    fn a_file_of_exactly_the_budget_is_counted_whole() {
        let d = TestDir::new("linecountbudget");
        let exact = count_lines(&d.file("exact.txt", &"a".repeat(LINE_BUDGET as usize)));
        let ended =
            count_lines(&d.file("ended.txt", &("a".repeat(LINE_BUDGET as usize - 1) + "\n")));
        let over = count_lines(&d.file("over.txt", &"a".repeat(LINE_BUDGET as usize + 1)));
        assert_eq!(
            (exact.lines, exact.partial),
            (1, false),
            "a megabyte with no newline is one whole line"
        );
        assert_eq!(
            (ended.lines, ended.partial),
            (1, false),
            "and so is one that ends on the budget"
        );
        assert_eq!(
            (over.lines, over.partial),
            (0, true),
            "one byte more and the count is a floor again"
        );
    }

    // FX-PREVIEW follow-up: Quick Look drew "hello utf16" as 1 line while the footer said 2.
    #[test]
    fn a_marked_file_counts_the_lines_its_preview_draws() {
        let d = TestDir::new("linecountmarks");
        let count = |name: &str, bytes: &[u8]| {
            let path = d.join(name);
            std::fs::write(&path, bytes).unwrap();
            let c = count_lines(&path);
            (c.lines, c.partial, c.failed)
        };
        let little: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("hello utf16\n".bytes().flat_map(|b| [b, 0]))
            .collect();
        let big: Vec<u8> = [0xfe, 0xff]
            .into_iter()
            .chain("a\nb".bytes().flat_map(|b| [0, b]))
            .collect();
        assert_eq!(
            count("le.txt", &little),
            (1, false, false),
            "one UTF-16 line ending on its newline"
        );
        assert_eq!(
            count("be.txt", &big),
            (2, false, false),
            "two big-endian lines, the last unterminated"
        );
        assert_eq!(
            count("crlf.txt", &[0xff, 0xfe, b'a', 0, b'\r', 0, b'\n', 0]),
            (1, false, false)
        );
        // A unit whose high byte is 0x0A is a letter, not a newline: U+0A41 is Gurmukhi, then B.
        assert_eq!(
            count("gurmukhi.txt", &[0xff, 0xfe, 0x41, 0x0a, b'B', 0]),
            (1, false, false)
        );
        assert_eq!(
            count("lemark.txt", &[0xff, 0xfe]),
            (0, false, false),
            "a mark alone is no text"
        );
        assert_eq!(count("utf8mark.txt", b"\xef\xbb\xbf"), (0, false, false));
        assert_eq!(count("utf8.txt", b"\xef\xbb\xbfa\nb\n"), (2, false, false));
        assert_eq!(
            count("plain.txt", b"a\n\0"),
            (2, false, false),
            "with no mark every byte still counts"
        );
    }

    #[test]
    fn a_file_that_cannot_be_opened_is_not_an_empty_one() {
        let d = TestDir::new("linecountdenied");
        let empty = count_lines(&d.file("empty.txt", ""));
        let path = d.file("denied.txt", "a\nb\nc\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let denied = count_lines(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            (empty.lines, empty.partial, empty.failed),
            (0, false, false),
            "an empty file really has no lines"
        );
        assert_eq!(
            (denied.lines, denied.partial, denied.failed),
            (0, false, true),
            "a file that could not be opened has no count at all"
        );
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
        no_count(
            d.join("topipe"),
            "the open follows the link, so the check has to as well",
        );
        no_count(
            d.dir("sub"),
            "a directory opens fine and reads EISDIR, which used to answer zero lines",
        );
        let sock = d.join("sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        no_count(sock, "a socket is not a file to count");
        no_count(
            d.join("never-existed"),
            "a row that vanished still has no count",
        );
        // The guard is this narrow so a real text file is still counted, which is what this file is for.
        assert_eq!(bounded(d.file("real.txt", "a\nb\nc\n")).lines, 3);
        // And through a link to one, the case that separates the two candidate calls: the open follows it.
        std::os::unix::fs::symlink("real.txt", d.join("toreal")).unwrap();
        let through = bounded(d.join("toreal"));
        assert_eq!(
            (through.lines, through.partial, through.failed),
            (3, false, false)
        );
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
        assert_eq!(
            (counted.lines, counted.partial, counted.failed),
            (0, false, true)
        );
        assert_eq!(
            left,
            b"a\nb\nc\n".to_vec(),
            "a count must never eat a pipe someone else is reading"
        );
        assert!(
            stopped,
            "the writer this test started is killed by its own pid"
        );
    }
}
