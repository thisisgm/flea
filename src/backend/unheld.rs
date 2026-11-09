// Items a destination cannot hold inside a copied tree: each is named and skipped, and the walk goes on.
use crate::backend::copyfile::At;
use crate::error::{from_io, FleaError};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

// The marker a refusal carries up to copy_dir_entries, which records it there; copy_any never lets it out.
pub(crate) const UNHELD: &str = "unheld";

pub(crate) const LINKS: &str = "this drive cannot hold links";
pub(crate) const NODES: &str = "this drive cannot hold special files";
const NAME: &str = "this drive does not allow that name";
const LONG: &str = "that name is too long for this drive";
const TAKEN: &str = "this drive treats that name as the same as another in the folder";

const EPERM: i32 = 1;
const EEXIST: i32 = 17;
const EINVAL: i32 = 22;
const ENAMETOOLONG: i32 = 36;
const ENOSYS: i32 = 38;
const EILSEQ: i32 = 84;
const EOPNOTSUPP: i32 = 95;
// Linux refuses a path of this many bytes with ENAMETOOLONG whatever drive holds it, so the error then says nothing about the drive.
const PATH_MAX: usize = 4096;
// Linux statfs magic for vfat and exfat, the two drives with no link and no mknod operation; see linux/magic.h.
const VFAT_MAGIC: i64 = 0x4D44;
const EXFAT_MAGIC: i64 = 0x2011BAB0;

// What a copy was creating when the destination refused it.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Link,
    Node,
    Entry,
}

// The most refusals one copy names; past it they are only counted, so a tree of a million links stays bounded.
pub const NAMED: usize = 100;

thread_local! {
    // The current copy_any's first NAMED refusals, each a destination path and its reason, and how many there were.
    static REFUSED: RefCell<(Vec<(PathBuf, String)>, usize)> = const { RefCell::new((Vec::new(), 0)) };
    // Whether the current copy_any writes the names below its top as Rename gives them, which decides what a taken name folds with.
    static RENAMING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// vfat or exfat, whose drivers have no link or mknod operation and fold names, so EPERM and EEXIST there mean the drive.
fn fat_drive(parent: &Path) -> bool {
    #[cfg(test)]
    if FORCE_LINKLESS.with(|f| f.get()) {
        return true;
    }
    // A test's vfat root answers as tests/fatcopy-shim.c does: statfs reads vfat for a path that exists below it.
    #[cfg(test)]
    if let Some(root) = FAT_ROOT.with(|r| r.borrow().clone()) {
        return crate::backend::fsinfo::magic_of(parent).is_some() && std::fs::canonicalize(parent).is_ok_and(|p| p.starts_with(&root));
    }
    matches!(crate::backend::fsinfo::magic_of(parent), Some(VFAT_MAGIC) | Some(EXFAT_MAGIC))
}

// A drive whose driver restricts names: vfat, exfat or ntfs3 mounted with windows_names, so EINVAL and EILSEQ there mean the drive.
fn restricts_names(held: &Path, parent: &Path) -> bool {
    fat_drive(held) || crate::backend::fsname::has_windows_names(parent)
}

// vfat and exfat match names without case and without trailing dots or spaces, and a copy under Rename writes a name as sanitize gives it.
fn folded(name: &std::ffi::OsStr) -> String {
    let landed = crate::backend::fatnames::inside_leaf(name, RENAMING.with(|r| r.get()));
    landed.to_string_lossy().trim_end_matches(['.', ' ']).to_lowercase()
}

// A taken name is the drive folding two source names into one only when the source holds such a sibling.
fn folds_in(source: &Path) -> bool {
    let (Some(dir), Some(name)) = (source.parent(), source.file_name()) else { return false };
    let want = folded(name);
    std::fs::read_dir(dir).map(|entries| entries.flatten().any(|e| e.file_name() != name && folded(&e.file_name()) == want)).unwrap_or(false)
}

// Sample input: (Link, EPERM, At { at: "/proc/self/fd/7/l", named: "/media/stick/a/l" }, Some(source)) on vfat answers Some("this drive cannot hold links").
fn reason(kind: Kind, e: &std::io::Error, dst: At, inside: Option<&Path>) -> Option<String> {
    let named = dst.named;
    // statfs asks the held path, which exists while a staged folder's named parent does not; the mount table is read by name.
    let (held, parent) = (dst.at.parent().unwrap_or(dst.at), named.parent().unwrap_or(named));
    let errno = e.raw_os_error()?;
    match (kind, errno) {
        (Kind::Link, EOPNOTSUPP | ENOSYS) => Some(LINKS.to_string()),
        (Kind::Link, EPERM) if fat_drive(held) => Some(LINKS.to_string()),
        (Kind::Node, EOPNOTSUPP | ENOSYS) => Some(NODES.to_string()),
        (Kind::Node, EPERM) if fat_drive(held) => Some(NODES.to_string()),
        // The drive is named by statfs, the same call fat_drive reads, so both agree about which drive this is.
        (_, EINVAL | EILSEQ) if restricts_names(held, parent) => {
            let leaf = named.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let fs = crate::backend::fsinfo::magic_of(held).map(crate::backend::fsinfo::name_for).unwrap_or_default();
            let windows = crate::backend::fsname::has_windows_names(parent);
            Some(crate::backend::fsname::refuse_name(&leaf, &fs, windows).unwrap_or_else(|| NAME.to_string()))
        }
        (_, ENAMETOOLONG) if named.as_os_str().len() < PATH_MAX => Some(LONG.to_string()),
        // Inside a folder this copy created, a taken name is a fold only on vfat or exfat beside a source sibling; else another writer's.
        (_, EEXIST) if fat_drive(held) && inside.is_some_and(folds_in) => Some(TAKEN.to_string()),
        _ => None,
    }
}

// A create's failure: what the drive cannot hold carries the marker; inside is the source entry within a tree, else None.
pub(crate) fn refused(kind: Kind, e: &std::io::Error, dst: At, inside: Option<&Path>) -> FleaError {
    match reason(kind, e, dst, inside) {
        Some(msg) => marked(dst.named, msg),
        None => from_io("copy", &flea::bytename::path_text(dst.named), e),
    }
}

// The same marker for a refusal decided before any syscall, as the 4 GiB limit is.
pub(crate) fn marked(named: &Path, msg: String) -> FleaError {
    FleaError {
        where_: UNHELD.to_string(),
        path: flea::bytename::path_text(named).into_owned(),
        msg,
    }
}

// One copy_any starts with an empty list, so nothing an earlier copy on this thread refused is reported again.
pub(crate) fn begin(renaming: bool) {
    REFUSED.with(|r| *r.borrow_mut() = (Vec::new(), 0));
    RENAMING.with(|r| r.set(renaming));
}

// One refusal: the first NAMED are kept by path, every one is counted.
pub(crate) fn note(e: FleaError) {
    REFUSED.with(|r| {
        let mut r = r.borrow_mut();
        r.1 += 1;
        if r.0.len() < NAMED {
            r.0.push((flea::bytename::exact(e.path), e.msg));
        }
    });
}

// What the last copy_any on this thread refused, taken once by the transfer that reports it.
pub fn take() -> Vec<(PathBuf, String)> {
    REFUSED.with(|r| std::mem::take(&mut *r.borrow_mut()).0)
}

// A refusal at the top is the item's own failure, said as a plain copy error.
pub(crate) fn plain(e: FleaError) -> FleaError {
    if e.where_ != UNHELD {
        return e;
    }
    FleaError { where_: "copy".to_string(), ..e }
}

// Sample output: "sub/a.link could not be copied: this drive cannot hold links", or the count with the first one.
pub(crate) fn summary(top: &Path, moving: bool) -> Option<FleaError> {
    REFUSED.with(|r| {
        let refused = r.borrow();
        let (first, why) = refused.0.first()?;
        let inside = first.strip_prefix(top).unwrap_or(first).to_string_lossy().to_string();
        let msg = match (refused.1, moving) {
            (1, true) => format!("{inside} could not be moved: {why}"),
            (1, false) => format!("{inside} could not be copied: {why}"),
            (n, true) => {
                format!("{n} items inside could not be moved; the first is {inside}: {why}")
            }
            (n, false) => {
                format!("{n} items inside could not be copied; the first is {inside}: {why}")
            }
        };
        Some(FleaError {
            where_: "copy".to_string(),
            path: flea::bytename::path_text(top).into_owned(),
            msg,
        })
    })
}

// Production answers Ok; a test arms a refusal by leaf name, or EPERM for every link, so a drive needs no mount.
pub(crate) fn fault(named: &Path, kind: Kind) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(errno) = test_fault(named, kind) {
        return Err(std::io::Error::from_raw_os_error(errno));
    }
    let _ = (named, kind);
    Ok(())
}

#[cfg(test)]
fn test_fault(named: &Path, kind: Kind) -> Option<i32> {
    if matches!(kind, Kind::Link) && FORCE_LINKLESS.with(|f| f.get()) {
        return Some(EPERM);
    }
    FAULTS.with(|f| f.borrow().iter().find(|(leaf, _)| Some(leaf.as_ref()) == named.file_name()).map(|(_, e)| *e))
}

#[cfg(test)]
thread_local! {
    static FORCE_LINKLESS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAT_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static FAULTS: RefCell<Vec<(std::ffi::OsString, i32)>> = const { RefCell::new(Vec::new()) };
}

// Sample input: test_force_linkless(true) makes every link answer EPERM and every destination read as vfat.
#[cfg(test)]
pub fn test_force_linkless(force: bool) {
    FORCE_LINKLESS.with(|f| f.set(force));
}

// Sample input: test_fat_root(Some(dir)) makes every existing path below dir read as vfat; None clears it.
#[cfg(test)]
pub fn test_fat_root(root: Option<&Path>) {
    FAT_ROOT.with(|r| *r.borrow_mut() = root.map(|root| std::fs::canonicalize(root).expect("the vfat root exists")));
}

// Sample input: test_refuse(&[("l", 1)]) makes the create of any entry named l answer EPERM; &[] clears it.
#[cfg(test)]
pub fn test_refuse(faults: &[(&str, i32)]) {
    FAULTS.with(|f| *f.borrow_mut() = faults.iter().map(|(leaf, e)| (leaf.into(), *e)).collect());
}

#[cfg(test)]
#[path = "unheld_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "unheld_path_tests.rs"]
mod path_tests;
#[cfg(test)]
#[path = "unheld_summary_tests.rs"]
mod summary_tests;
