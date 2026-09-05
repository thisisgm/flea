// Recreating a node at the destination, which is how a copy carries a fifo, a socket or a device
// across without opening one.
use crate::error::{from_io, FleaError};
use std::ffi::{c_char, CString};
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

// std has no wrapper for the call that makes a node, so the syscall is declared here as ops.rs declares renameat2.
extern "C" {
    fn mknod(path: *const c_char, mode: u32, dev: u64) -> i32;
}

// A node is recreated and never opened: a fifo's open waits for a writer, a socket's answers ENXIO,
// and a device would stream until the destination filesystem was full.
pub fn copy_node(src: &Metadata, dst: &Path) -> Result<(), FleaError> {
    let c_dst = match CString::new(dst.as_os_str().as_encoded_bytes()) {
        Ok(c) => c,
        // corner: a path with an interior NUL cannot reach a syscall, and no listing can produce one.
        Err(_) => {
            return Err(FleaError {
                where_: "copy".to_string(),
                path: dst.to_string_lossy().to_string(),
                msg: "path contains an interior NUL".to_string(),
            })
        }
    };
    // The source's own st_mode carries the kind and the permission bits, so a copy is never wider
    // than what it copied, and mknod refuses a taken name with EEXIST as create_new does elsewhere.
    // corner: a device node needs CAP_MKNOD, so an unprivileged copy of one reports EPERM and streams nothing.
    if unsafe { mknod(c_dst.as_ptr(), src.mode(), src.rdev()) } != 0 {
        return Err(from_io("copy", &dst.to_string_lossy(), &std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::backend::copyfile::{copy_any, copy_file, Progress};
    use crate::backend::fifotest::{mkfifo, peek, within, FifoWriter};
    use crate::backend::opsreq::{run_duplicate, OpMsg};
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    // The real path a duplicate takes: opsdispatch spawns this and reads back one OpMsg.
    fn duplicated(path: &Path) -> OpMsg {
        let (tx, rx) = std::sync::mpsc::channel();
        let owned = path.to_string_lossy().to_string();
        within("a duplicate", move || run_duplicate(owned, tx));
        rx.try_recv().expect("run_duplicate answers before it returns")
    }

    // Bounded too: before the fifo branch this call is the hang, and the suite must fail rather than stop.
    fn copied(src: PathBuf, dst: PathBuf) -> Result<(), String> {
        within("a copy", move || {
            let flag = AtomicBool::new(false);
            let mut sink = |_: u64, _: u64| {};
            let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None };
            copy_any(&src, &dst, &mut p).map_err(|e| e.msg)
        })
    }

    #[test]
    fn a_duplicated_tree_carries_a_fifo_across_instead_of_hanging_on_it() {
        let d = TestDir::new("copyfifotree");
        let tree = d.dir("tree");
        std::fs::write(tree.join("a.txt"), "a").expect("a.txt");
        mkfifo(&tree.join("pipe"));
        match duplicated(&tree) {
            OpMsg::Duplicated { ok, err, .. } => assert!(ok, "a fifo in the tree must not fail the copy: {}", err),
            _ => panic!("a duplicate answers with Duplicated and nothing else"),
        }
        let clone = d.join("tree copy");
        assert_eq!(std::fs::read_to_string(clone.join("a.txt")).expect("the file beside it"), "a");
        let kind = clone.join("pipe").symlink_metadata().expect("the fifo is in the clone").file_type();
        assert!(kind.is_fifo(), "a fifo is recreated as a fifo, the way cp -a and rsync -a carry one");
    }

    #[test]
    fn copying_a_fifo_never_reads_the_bytes_a_writer_left_in_it() {
        let d = TestDir::new("copyfifofed");
        let src = d.join("pipe");
        let dst = d.join("copied");
        mkfifo(&src);
        let mut writer = FifoWriter::feeding(&src, "a\nb\nc\n", &d.join("wrote"));
        let outcome = copied(src.clone(), dst.clone());
        let left = peek(src);
        let stopped = writer.stop();
        outcome.expect("a fifo with a writer is copied, not read");
        assert_eq!(left, b"a\nb\nc\n".to_vec(), "a copy must never eat a pipe someone else is reading");
        assert!(dst.symlink_metadata().expect("the copy").file_type().is_fifo());
        assert!(stopped, "the writer this test started is killed by its own pid");
    }

    #[test]
    fn a_fifo_copy_refuses_a_destination_that_is_already_taken() {
        let d = TestDir::new("copyfifoclobber");
        let src = d.join("pipe");
        mkfifo(&src);
        let taken = d.file("taken", "already here");
        let e = copied(src, taken.clone()).expect_err("mkfifo must refuse a name that is taken");
        assert!(e.contains("os error 17"), "EEXIST is what refuses it, got {}", e);
        assert_eq!(std::fs::read_to_string(&taken).expect("untouched"), "already here");
    }
    // ~/.gnupg and every agent directory hold one, so an ordinary copy meets a socket; opening one
    // answers ENXIO, which before this failed the whole tree around it.
    #[test]
    fn a_socket_in_a_tree_is_carried_across_instead_of_failing_the_copy() {
        let d = TestDir::new("copysockettree");
        let tree = d.dir("tree");
        std::fs::write(tree.join("a.txt"), "a").expect("a.txt");
        let listener = std::os::unix::net::UnixListener::bind(tree.join("S.agent")).expect("a bound socket");
        let clone = d.join("tree copy");
        let outcome = copied(tree, clone.clone());
        drop(listener);
        outcome.expect("a socket must not fail the tree around it");
        assert_eq!(std::fs::read_to_string(clone.join("a.txt")).expect("the file beside it"), "a");
        let kind = clone.join("S.agent").symlink_metadata().expect("the socket is in the clone").file_type();
        assert!(kind.is_socket(), "a socket is recreated as a socket, the way cp -a carries one");
    }

    // Bounded on purpose: /dev/zero never ends and its size reads 0, so streaming it fills the
    // destination filesystem. The cancel goes in on the first report, capping the run at one CHUNK.
    // corner: as root mknod recreates the device node instead of failing, and neither answer streams.
    #[test]
    fn a_character_device_is_never_streamed_into_the_destination() {
        let d = TestDir::new("copychardev");
        let dst = d.join("zero");
        let flag = AtomicBool::new(false);
        let mut chunks = 0u32;
        let mut sink = |_: u64, _: u64| {
            chunks += 1;
            flag.store(true, Ordering::Relaxed);
        };
        let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None };
        let _ = copy_any(Path::new("/dev/zero"), &dst, &mut p);
        assert_eq!(chunks, 0, "a device node is recreated, never read: it has no end and would fill the filesystem");
        assert!(
            !dst.symlink_metadata().map(|m| m.file_type().is_file()).unwrap_or(false),
            "and no regular file of zeroes is written in its place"
        );
    }

    #[test]
    fn a_node_is_recreated_no_wider_than_its_source() {
        let d = TestDir::new("copyfifomode");
        let src = d.join("pipe");
        mkfifo(&src);
        std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o600)).expect("a private fifo");
        let dst = d.join("copied");
        copied(src, dst.clone()).expect("copy");
        let mode = dst.symlink_metadata().expect("the copy").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a constant 0666 widens a private fifo that cp -a and rsync -a preserve");
    }
    // copy_any routes every kind it can name away from copy_file, so a fifo arriving there was
    // swapped in after that stat. A plain read-only open of one parks in open(2) until a writer appears.
    #[test]
    fn a_source_swapped_to_a_fifo_after_the_stat_is_refused_rather_than_waited_on() {
        let d = TestDir::new("copyfifoswap");
        let src = d.join("pipe");
        mkfifo(&src);
        let dst = d.join("dst.bin");
        let target = dst.clone();
        let e = within("copy_file on a fifo", move || {
            let flag = AtomicBool::new(false);
            let mut sink = |_: u64, _: u64| {};
            let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None };
            copy_file(&src, &target, 0, &mut p).map_err(|e| e.where_)
        })
        .expect_err("a fifo source is refused, not waited on");
        assert_eq!(e, "copy");
        assert!(!dst.exists(), "and the refusal lands before the destination is created");
    }
}
