//! The disk facts a drop needs, read off the UI thread: the device of the folder the sources live in, the destination's device, whether that source folder can be written, and the refusal only resolved paths show.
use flea::bytename::view;
use flea::ui_logic_drag_drop::Refusal;
use std::ffi::{c_char, c_int, CString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Device numbers are 0 when unknown, which copies (Drag.verbFor).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub src_dev: u64,
    pub dest_dev: u64,
    pub deletable: bool,
    /// The destination, reached through a link, is the sources' own folder or lies inside one of them.
    pub refused: Option<Refusal>,
}

fn dev(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |meta| meta.dev())
}

// access(2) W_OK, as the backend's own dir_writable asks it.
fn writable(path: &Path) -> bool {
    const W_OK: c_int = 2;
    extern "C" {
        fn access(path: *const c_char, mode: c_int) -> c_int;
    }
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: c is a NUL-terminated path that outlives the call.
    unsafe { access(c.as_ptr(), W_OK) == 0 }
}

// The texts' two refusals asked again of the resolved folders, in the texts' order. A source keeps its own name, so a dragged link is never followed.
fn through_links(sources: &[String], dest: &Path) -> Option<Refusal> {
    let dest = dest.canonicalize().ok()?;
    let mut folder = (PathBuf::new(), PathBuf::new());
    let mut already = false;
    for source in sources {
        let path = view(source);
        let (Some(asked), Some(name)) = (path.parent(), path.file_name()) else {
            continue;
        };
        if folder.0 != asked {
            let real = asked.canonicalize().unwrap_or_else(|_| asked.to_owned());
            folder = (asked.to_owned(), real);
        }
        let inside = dest
            .strip_prefix(&folder.1)
            .is_ok_and(|rest| rest.iter().next() == Some(name));
        if inside {
            return Some(Refusal::InsideDrag);
        }
        already |= folder.1 == dest;
    }
    already.then_some(Refusal::AlreadyHere)
}

/// Blocking stat, access and realpath; callers run it on the background executor.
pub fn read(sources: &[String], dest: &str) -> Facts {
    let source = sources.first().map(view);
    let folder = source.as_deref().and_then(Path::parent);
    let dest = view(dest);
    Facts {
        src_dev: folder.map_or(0, dev),
        dest_dev: dev(&dest),
        deletable: folder.is_some_and(writable),
        refused: through_links(sources, &dest),
    }
}
