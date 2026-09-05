// Recreating a fifo at the destination, which is how a copy carries one across without opening it.
use crate::error::{from_io, FleaError};
use std::ffi::{c_char, CString};
use std::path::Path;

// The mode File::create gives the regular files copy_file writes, so a copied fifo is no more permissive.
const FIFO_MODE: u32 = 0o666;

// std has no wrapper for the call that makes a fifo, so the syscall is declared here as ops.rs declares renameat2.
extern "C" {
    fn mkfifo(path: *const c_char, mode: u32) -> i32;
}

// A fifo is recreated and never opened, because reading one blocks until a writer appears and then until it leaves.
pub fn copy_fifo(dst: &Path) -> Result<(), FleaError> {
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
    // EEXIST rather than a clobber, the same promise create_new and RENAME_NOREPLACE make elsewhere.
    if unsafe { mkfifo(c_dst.as_ptr(), FIFO_MODE) } != 0 {
        return Err(from_io("copy", &dst.to_string_lossy(), &std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::backend::copyfile::{copy_any, Progress};
    use crate::backend::fifotest::{mkfifo, peek, within, FifoWriter};
    use crate::backend::opsreq::{run_duplicate, OpMsg};
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::FileTypeExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

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
}
