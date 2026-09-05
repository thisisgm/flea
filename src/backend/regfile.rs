// The one way this backend opens a row's own path, because a meta request names whatever the listing
// named and two of those kinds cannot be read at all: an open of a fifo with no writer never returns,
// and a directory opens fine and then reads EISDIR.
use std::fs::File;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

// open(2) O_NONBLOCK, which is this value on both Linux architectures flea is built for.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("O_NONBLOCK needs a verified value for this architecture");

pub fn open_regular(path: &Path) -> Option<File> {
    // Refused before any open, so a fifo's blocked writer is never woken by a reader that will not read.
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        return None;
    }
    // O_NONBLOCK covers the window after that stat: a row swapped for a fifo in it would never return here.
    let f = std::fs::OpenOptions::new().read(true).custom_flags(O_NONBLOCK).open(path).ok()?;
    // corner: this second check is what closes that window, and no test can schedule the swap that opens it.
    if !f.metadata().map(|m| m.is_file()).unwrap_or(false) {
        return None;
    }
    Some(f)
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
}
