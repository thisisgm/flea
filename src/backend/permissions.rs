// One reviewed object stays open until its dialog closes; mode writes never walk a directory.
use crate::json::{escape, field_str, field_usize};
use crate::oflags::O_NOFOLLOW;
use flea::bytename::ExactText as _;
use std::fs::{File, Metadata, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

mod target;
use target::{mode, Target};

const O_PATH: i32 = 0o10000000;
const AT_EMPTY_PATH: i32 = 0x1000;
// Linux assigns fchmodat2 number 452 on both supported 64-bit architectures.
const SYS_FCHMODAT2: std::os::raw::c_long = 452;
const SPECIAL_BITS: u32 = 0o7000;
extern "C" {
    fn geteuid() -> u32;
    fn syscall(number: std::os::raw::c_long, ...) -> std::os::raw::c_long;
}

#[derive(Default)]
pub struct Permissions {
    held: Option<Reviewed>,
}
struct Reviewed {
    id: usize,
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
}

fn reason(meta: &Metadata, uid: u32) -> String {
    for (bit, label) in [(0o4000, "setuid"), (0o2000, "setgid"), (0o1000, "sticky")] {
        if meta.mode() & bit != 0 {
            return format!("Read-only: {} bit is present.", label);
        }
    }
    if meta.uid() != uid {
        return "Read-only: you are not the owner.".into();
    }
    String::new()
}

// Sample input, /etc/group: "gm:x:1000:gm".
fn group_name(gid: u32) -> String {
    let text = std::fs::read_to_string("/etc/group").unwrap_or_default();
    for line in text.lines() {
        let mut fields = line.split(':');
        let name = fields.next().unwrap_or("");
        if fields.nth(1).and_then(|s| s.parse::<u32>().ok()) == Some(gid) {
            return name.into();
        }
    }
    String::new()
}

// A batch failure carries what stayed applied, or nothing when the batch rolled back whole.
#[derive(Debug)]
pub struct BatchError {
    pub msg: String,
    pub applied: Vec<crate::backend::undo::Step>,
}

#[cfg(test)]
thread_local! {
    static FAIL_AT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static FAIL_RESTORE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// The k-th apply of the next batch fails without removing anything itself.
#[cfg(test)]
pub fn test_fail_at(index: Option<usize>) {
    FAIL_AT.with(|v| v.set(index));
}

// Every rollback of the next batch fails without racing the filesystem.
#[cfg(test)]
pub fn test_fail_restore(fail: bool) {
    FAIL_RESTORE.with(|v| v.set(fail));
}

fn fail_at(index: usize) -> bool {
    #[cfg(test)]
    {
        FAIL_AT.with(|v| v.get()) == Some(index)
    }
    #[cfg(not(test))]
    {
        let _ = index;
        false
    }
}

fn fail_restore() -> bool {
    #[cfg(test)]
    {
        FAIL_RESTORE.with(|v| v.get())
    }
    #[cfg(not(test))]
    {
        false
    }
}

// Metadata::created carries birth time where the filesystem keeps one, and fails where it does not.
pub(crate) fn born_of(meta: &Metadata) -> Option<(u64, u32)> {
    meta.created().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| (d.as_secs(), d.subsec_nanos()))
}

#[cfg(test)]
thread_local! {
    static SWAP_HOOK: std::cell::Cell<Option<fn(&Path)>> = const { std::cell::Cell::new(None) };
}

// The k-th test swap runs between the pathname checks and the open, nowhere else.
#[cfg(test)]
pub fn test_swap_hook(hook: Option<fn(&Path)>) {
    SWAP_HOOK.with(|v| v.set(hook));
}

fn swap_hook(path: &Path) {
    #[cfg(test)]
    {
        if let Some(hook) = SWAP_HOOK.with(|v| v.get()) {
            hook(path);
        }
    }
    #[cfg(not(test))]
    {
        let _ = path;
    }
}

// One shape check for both passes over a batch: a file or folder whose mode this dialog may edit.
fn check_shape(before: &Metadata) -> Result<(), &'static str> {
    if !(before.is_file() || before.is_dir()) {
        return Err("Permissions takes one item, not a link.");
    }
    if before.mode() & SPECIAL_BITS != 0 {
        return Err("Special permissions cannot be edited.");
    }
    Ok(())
}

// Sample input: items [("/a.txt", "600"), ("/b dir/c.txt", "644")]. One Entry holds every change for one undo.
// Every path is an absolute file or folder, never a link; a short failure rolls back what changed, newest first.
pub fn apply_many(items: &[(PathBuf, String)]) -> Result<Vec<crate::backend::undo::Step>, BatchError> {
    if items.is_empty() {
        return Err(BatchError { msg: "Permissions needs at least one selected item.".into(), applied: Vec::new() });
    }
    let uid = unsafe { geteuid() };
    let mut checked: Vec<(PathBuf, Target)> = Vec::with_capacity(items.len());
    for (path, text) in items {
        let requested = Target::parse(text).map_err(|e| BatchError { msg: e, applied: Vec::new() })?;
        if !path.is_absolute() {
            return Err(BatchError { msg: "Permissions requires an absolute path and request identity.".into(), applied: Vec::new() });
        }
        let before = path.symlink_metadata()
            .map_err(|e| BatchError { msg: format!("Could not inspect permissions: {}.", crate::error::io_message(&e)), applied: Vec::new() })?;
        check_shape(&before).map_err(|msg| BatchError { msg: msg.into(), applied: Vec::new() })?;
        let why = reason(&before, uid);
        if !why.is_empty() {
            return Err(BatchError { msg: why, applied: Vec::new() });
        }
        checked.push((path.clone(), requested));
    }
    let mut steps = Vec::with_capacity(checked.len());
    for (index, (path, requested)) in checked.iter().enumerate() {
        let outcome: Result<crate::backend::undo::Step, String> = (|| {
            if fail_at(index) {
                return Err(format!("Could not inspect permissions: {}.",
                    crate::error::io_message(&std::io::Error::from(std::io::ErrorKind::NotFound))));
            }
            let before = path.symlink_metadata()
                .map_err(|e| format!("Could not inspect permissions: {}.", crate::error::io_message(&e)))?;
            check_shape(&before).map_err(str::to_string)?;
            let (dev, ino, before_bits) = (before.dev(), before.ino(), before.mode() & 0o777);
            let (born, generation) = (born_of(&before), super::filegen::of_path(path));
            let target = requested.resolve(before_bits);
            chmod_pinned(path, dev, ino, (born, generation), before_bits, target)
                .map_err(|e| format!("Could not change mode: {}.", e))?;
            Ok(crate::backend::undo::Step::Mode { path: path.clone(), before: before_bits, after: target, dev, ino, born, generation })
        })();
        match outcome {
            Ok(step) => steps.push(step),
            Err(failure) => {
                let mut stuck = Vec::new();
                for step in steps.iter().rev() {
                    if let crate::backend::undo::Step::Mode { path, before, after, dev, ino, born, generation } = step {
                        if fail_restore() || crate::backend::undo::restore_mode(path, *dev, *ino, (*born, *generation), *after, *before).is_err() {
                            stuck.push(step.clone());
                        }
                    }
                }
                stuck.reverse();
                if stuck.is_empty() {
                    return Err(BatchError { msg: format!("{} No change was applied.", failure), applied: Vec::new() });
                }
                return Err(BatchError {
                    msg: format!("{} {} of {} items were changed; undo restores them.", failure, stuck.len(), items.len()),
                    applied: stuck,
                });
            }
        }
    }
    Ok(steps)
}

// A fixed-mask filesystem answers success and keeps its mode; the held descriptor's mode is read so a swapped path is not.
fn verify_applied_fd(file: &File, path: &Path, requested: u32) -> Result<(), String> {
    let after = file.metadata().map(|m| m.mode() & 0o777).map_err(|e| crate::error::io_message(&e))?;
    if after == requested & 0o777 {
        return Ok(());
    }
    Err(refusal_for(path))
}

// The filesystem name comes from the path, which names the drive and never the verified object.
fn refusal_for(path: &Path) -> String {
    let fs = crate::backend::fsinfo::read(path).map(|info| info.name).unwrap_or_default();
    if fs.is_empty() {
        return "This drive ignores permission changes, so nothing was changed.".into();
    }
    format!("This {fs} drive ignores permission changes, so nothing was changed.")
}

// The held O_PATH descriptor is identity-checked and changed by fchmodat2, never by pathname.
pub(crate) fn chmod_pinned(path: &Path, dev: u64, ino: u64, birth: super::filegen::Birth, expected: u32, target: u32) -> Result<(), String> {
    let current = path.symlink_metadata().map_err(|e| crate::error::io_message(&e))?;
    if current.file_type().is_symlink() {
        return Err("Permissions takes one item, not a link.".into());
    }
    same_identity(&current, super::filegen::of_path(path), (dev, ino), birth, expected)?;
    swap_hook(path);
    let file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_PATH)
        .open(path).map_err(|e| crate::error::io_message(&e))?;
    let meta = file.metadata().map_err(|e| crate::error::io_message(&e))?;
    same_identity(&meta, super::filegen::of_fd(&file), (dev, ino), birth, expected)?;
    fchmod_fd(&file, target)?;
    // tests/fs-matrix.sh c_perms on vfat pins this readback with "refused mode stays put" after a successful no-op chmod.
    verify_applied_fd(&file, path, target)?;
    Ok(())
}

// Same device, inode, mode, birth and inode generation (`now`, read from the same object as `meta`) the caller saw; a replaced file keeps its current mode.
fn same_identity(meta: &Metadata, now: Option<u64>, (dev, ino): (u64, u64), (born, generation): super::filegen::Birth, expected: u32) -> Result<(), String> {
    if meta.dev() != dev || meta.ino() != ino {
        return Err("the item was replaced, so its mode was left in place.".into());
    }
    if meta.mode() & 0o7777 != expected {
        return Err("the mode changed since, so it was left in place.".into());
    }
    // Both files can be born in one timestamp tick, so the inode generation tells them apart where the filesystem keeps it.
    let reborn = matches!((born, born_of(meta)), (Some(was), Some(now)) if was != now);
    if reborn || super::filegen::differs(generation, now) {
        return Err("the item was replaced, so its mode was left in place.".into());
    }
    Ok(())
}

// One empty-path fchmodat2 through a held O_PATH fd, so the mode lands on the reviewed object.
fn fchmod_fd(file: &File, target: u32) -> Result<(), String> {
    if unsafe { syscall(SYS_FCHMODAT2, file.as_raw_fd(), c"".as_ptr(), target, AT_EMPTY_PATH) } != 0 {
        return Err(crate::error::io_message(&std::io::Error::last_os_error()));
    }
    Ok(())
}

impl Permissions {
    // Sample input: {"c":"permissions","op":"inspect","id":1,"path":"/tmp/item"}.
    pub fn handle(&mut self, line: &str) -> String {
        let id = field_usize(line, "id").unwrap_or(0);
        let op = field_str(line, "op").unwrap_or_default();
        let result = match op.as_str() {
            "inspect" => self.inspect(id, &flea::bytename::view(&field_str(line, "path").unwrap_or_default())),
            "apply" => self.apply(id, &field_str(line, "mode").unwrap_or_default()),
            "close" => {
                if self.held.as_ref().map(|h| h.id) == Some(id) {
                    self.held = None;
                }
                Ok(format!(
                    r#"{{"t":"permissions","id":{},"op":"close","ok":true}}"#,
                    id
                ))
            }
            _ => Err("Unknown permissions operation.".into()),
        };
        result.unwrap_or_else(|err| {
            format!(
                r#"{{"t":"permissions","id":{},"op":"{}","ok":false,"error":"{}"}}"#,
                id,
                escape(&op),
                escape(&err)
            )
        })
    }

    fn inspect(&mut self, id: usize, path: &Path) -> Result<String, String> {
        self.held = None;
        if id == 0 || !path.is_absolute() {
            return Err("Permissions requires an absolute path and request identity.".into());
        }
        let before = path
            .symlink_metadata()
            .map_err(|e| format!("Could not inspect permissions: {}.", crate::error::io_message(&e)))?;
        if !(before.is_file() || before.is_dir()) {
            return Err("Permissions takes one item, not a link.".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(O_NOFOLLOW | O_PATH)
            .open(path)
            .map_err(|e| format!("Could not open selected item for permissions: {}.", crate::error::io_message(&e)))?;
        let meta = file.metadata().map_err(|e| crate::error::io_message(&e))?;
        if meta.dev() != before.dev() || meta.ino() != before.ino() {
            return Err("Selected item changed; reopen Permissions.".into());
        }
        let why = reason(&meta, unsafe { geteuid() });
        let response = format!(
            r#"{{"t":"permissions","id":{},"op":"inspect","ok":true,"path":"{}","directory":{},"mode":"{:04o}","uid":{},"gid":{},"owner":"{}","group":"{}","reason":"{}"}}"#,
            id,
            escape(&path.exact_text()),
            meta.is_dir(),
            meta.mode() & 0o7777,
            meta.uid(),
            meta.gid(),
            escape(&super::owner::name(meta.uid())),
            escape(&group_name(meta.gid())),
            escape(&why)
        );
        self.held = Some(Reviewed {
            id,
            path: path.into(),
            dev: meta.dev(),
            ino: meta.ino(),
            file,
        });
        Ok(response)
    }

    fn apply(&mut self, id: usize, text: &str) -> Result<String, String> {
        let requested = mode(text)?;
        let held = self
            .held
            .as_ref()
            .filter(|h| h.id == id)
            .ok_or("Selected item changed; reopen Permissions.")?;
        let meta = held.file.metadata().map_err(|e| crate::error::io_message(&e))?;
        let current = held
            .path
            .symlink_metadata()
            .map_err(|_| "Selected item moved or disappeared; reopen Permissions.")?;
        if current.dev() != held.dev
            || current.ino() != held.ino
            || current.file_type().is_symlink()
        {
            return Err("Selected item changed; reopen Permissions.".into());
        }
        if meta.dev() != held.dev || meta.ino() != held.ino {
            return Err("Selected item changed; reopen Permissions.".into());
        }
        let why = reason(&meta, unsafe { geteuid() });
        if !why.is_empty() {
            return Err(why);
        }
        if meta.mode() & SPECIAL_BITS != 0 {
            return Err("Special permissions cannot be edited.".into());
        }
        // O_PATH can review mode 0000; empty-path fchmodat2 changes that held object without reopening a pathname.
        fchmod_fd(&held.file, requested)
            .map_err(|e| format!("Could not change mode: {e}. No change was applied."))?;
        // The syscall answers success on a fixed-mask filesystem too, so a mismatch refuses here.
        if let Err(refusal) = verify_applied_fd(&held.file, &held.path, requested) {
            return Err(format!("Could not change mode: {refusal} No change was applied."));
        }
        Ok(format!(
            r#"{{"t":"permissions","id":{},"op":"apply","ok":true,"mode":"{:04o}"}}"#,
            id, requested
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::Permissions as Mode;
    use std::os::unix::fs::PermissionsExt;
    use crate::backend::testdir::TestDir;
    #[test]
    fn a_mode_the_filesystem_ignored_is_refused_with_the_filesystem_named() {
        let d = TestDir::new("permissions-verify");
        let path = d.file("item", "a");
        std::fs::set_permissions(&path, Mode::from_mode(0o644)).unwrap();
        let file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_PATH).open(&path).unwrap();
        assert!(verify_applied_fd(&file, &path, 0o644).is_ok(), "a mode that landed verifies");
        let err = verify_applied_fd(&file, &path, 0o600).expect_err("a mode that never landed must refuse");
        assert!(err.contains("ignores permission changes"), "the refusal names the cause: {err}");
        let fs = crate::backend::fsinfo::read(&path).map(|i| i.name).unwrap_or_default();
        assert!(!fs.is_empty(), "the fixture sits on a named filesystem");
        assert!(err.contains(&fs), "the refusal names the filesystem: {err}");
    }
    // Sample input: fd opened on a 0600 file, path naming a 0644 file elsewhere.
    #[test]
    fn a_verify_reads_the_mode_from_its_descriptor_not_its_path() {
        let d = TestDir::new("permissions-fd-verify");
        let held_path = d.file("held", "a");
        let other = d.file("other", "b");
        std::fs::set_permissions(&held_path, Mode::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&other, Mode::from_mode(0o644)).unwrap();
        let file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_PATH).open(&held_path).unwrap();
        assert!(verify_applied_fd(&file, &other, 0o600).is_ok(), "the descriptor's mode verifies against another path's name");
        let other_file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_PATH).open(&other).unwrap();
        assert!(verify_applied_fd(&other_file, &other, 0o600).is_err(), "the other file's own descriptor sees its mode and refuses");
    }
    #[test]
    fn inspect_failures_report_plain_causes() {
        let d = TestDir::new("permissions-plain-error");
        let mut permissions = Permissions::default();
        let missing = permissions.handle(&format!(
            r#"{{"c":"permissions","op":"inspect","id":1,"path":"{}"}}"#,
            escape(&d.join("missing").to_string_lossy())
        ));
        assert_eq!(field_str(&missing, "error").unwrap(), "Could not inspect permissions: item not found.");
        let parent = d.dir("locked");
        let path = d.file("locked/file", "retained contents");
        d.assert_contains(&parent);
        std::fs::set_permissions(&parent, Mode::from_mode(0o0)).unwrap();
        let refused = permissions.inspect(2, &path);
        std::fs::set_permissions(&parent, Mode::from_mode(0o700)).unwrap();
        assert_eq!(refused.unwrap_err(), "Could not inspect permissions: permission denied.");
        assert_eq!(std::fs::read(path).unwrap(), b"retained contents");
        assert!(permissions.held.is_none());
    }
    #[test]
    fn unreadable_owned_file_can_be_repaired() {
        let d = TestDir::new("permissions-unreadable");
        let path = d.file("item", "a");
        std::fs::set_permissions(&path, Mode::from_mode(0o0)).unwrap();
        let mut p = Permissions::default();
        p.inspect(1, &path).unwrap();
        p.apply(1, "600").unwrap();
        assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o600);
    }
    #[test]
    fn ordinary_modes_only() {
        for text in ["644", "000", "777", "0644"] {
            assert!(mode(text).is_ok());
        }
        for text in ["", "64", "888", "4755", " 644", "0644 ", "00000", "-1"] {
            assert!(mode(text).is_err(), "{}", text);
        }
    }
    #[test]
    fn directory_change_does_not_traverse() {
        let d = TestDir::new("permissions-directory");
        let dir = d.dir("dir");
        let child = dir.join("child");
        std::fs::write(&child, "untouched").unwrap();
        let before = child.metadata().unwrap().mode();
        let mut p = Permissions::default();
        assert!(p.inspect(1, &dir).is_ok());
        assert!(p.apply(1, "0700").is_ok());
        assert_eq!(dir.metadata().unwrap().mode() & 0o777, 0o700);
        assert_eq!(child.metadata().unwrap().mode(), before);
    }
    #[test]
    fn stale_identity_symlinks_and_invalid_modes_fail_without_writes() {
        let d = TestDir::new("permissions-identity");
        let path = d.file("item", "a");
        let mut p = Permissions::default();
        p.inspect(1, &path).unwrap();
        assert!(p.apply(2, "600").is_err());
        assert!(p.apply(1, "4755").is_err());
        std::fs::rename(&path, d.join("old")).unwrap();
        d.file("item", "replacement");
        let before = path.metadata().unwrap().mode();
        assert!(p.apply(1, "600").is_err());
        assert_eq!(path.metadata().unwrap().mode(), before);
        let link = d.join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(p.inspect(2, &link).is_err());
        assert!(p.apply(1, "600").is_err());
        assert!(p.inspect(3, &d.join("missing")).is_err());
        assert!(p.inspect(0, &path).is_err());
        assert!(p.inspect(4, Path::new("relative")).is_err());
    }
    #[test]
    fn special_bits_and_non_owner_are_read_only() {
        let d = TestDir::new("permissions-special");
        let path = d.file("item", "a");
        let mut p = Permissions::default();
        for (bits, label) in [(0o4644, "setuid"), (0o2644, "setgid"), (0o1644, "sticky")] {
            std::fs::set_permissions(&path, Mode::from_mode(bits)).unwrap();
            assert!(p.inspect(1, &path).unwrap().contains(label));
            assert!(p.apply(1, "644").is_err());
            assert_eq!(path.metadata().unwrap().mode() & 0o7777, bits);
        }
        std::fs::set_permissions(&path, Mode::from_mode(0o644)).unwrap();
        let meta = path.metadata().unwrap();
        assert!(reason(&meta, meta.uid().wrapping_add(1)).contains("not the owner"));
    }
    #[test]
    fn a_swapped_symlink_is_refused_and_leaves_the_victim() {
        let d = TestDir::new("permissions-swap");
        let item = d.file("item", "a");
        std::fs::set_permissions(&item, Mode::from_mode(0o644)).unwrap();
        let victim = d.file("id_rsa", "secret");
        std::fs::set_permissions(&victim, Mode::from_mode(0o640)).unwrap();
        let before = item.symlink_metadata().unwrap();
        let (dev, ino) = (before.dev(), before.ino());
        let born = born_of(&before);
        std::fs::remove_file(&item).unwrap();
        std::os::unix::fs::symlink(&victim, &item).unwrap();
        let refused = chmod_pinned(&item, dev, ino, (born, None), 0o644, 0o600);
        assert!(refused.is_err(), "a path swapped for a link is refused, got {:?}", refused);
        assert!(item.symlink_metadata().unwrap().file_type().is_symlink(), "the swap is still a link");
        assert_eq!(victim.metadata().unwrap().mode() & 0o777, 0o640, "the swap target keeps its mode");
    }
    // The seam swaps the path after the lstat checks, so the open and its rechecks face the new file.
    fn swap_to_same_mode_file(path: &Path) {
        let bits = path.symlink_metadata().map(|m| m.mode() & 0o777).unwrap_or(0o644);
        std::fs::remove_file(path).unwrap();
        std::fs::write(path, "replacement").unwrap();
        std::fs::set_permissions(path, Mode::from_mode(bits)).unwrap();
    }
    // The victim shares the item's inode, so only the no-follow open tells the swapped link apart.
    fn swap_to_link_of_hardlink(path: &Path) {
        let victim = path.parent().unwrap().join("victim");
        std::fs::remove_file(path).unwrap();
        std::os::unix::fs::symlink(&victim, path).unwrap();
    }
    #[test]
    fn a_file_swapped_after_the_checks_is_refused() {
        let d = TestDir::new("permissions-swap-after-checks");
        let item = d.file("item", "a");
        std::fs::set_permissions(&item, Mode::from_mode(0o644)).unwrap();
        let before = item.symlink_metadata().unwrap();
        let (dev, ino) = (before.dev(), before.ino());
        let born = born_of(&before);
        // The old file stays open across the swap, so the swap cannot reuse its inode.
        let held = File::open(&item).unwrap();
        test_swap_hook(Some(swap_to_same_mode_file));
        let refused = chmod_pinned(&item, dev, ino, (born, None), 0o644, 0o600);
        test_swap_hook(None);
        drop(held);
        assert!(refused.is_err(), "a file swapped after the checks is refused, got {:?}", refused);
        assert_eq!(item.metadata().unwrap().mode() & 0o777, 0o644, "the replacement keeps its mode");
    }
    #[test]
    fn a_symlink_swapped_after_the_checks_is_refused_at_open() {
        let d = TestDir::new("permissions-swap-link-after-checks");
        let item = d.file("item", "a");
        std::fs::set_permissions(&item, Mode::from_mode(0o644)).unwrap();
        let victim = d.path().join("victim");
        std::fs::hard_link(&item, &victim).unwrap();
        let before = item.symlink_metadata().unwrap();
        let (dev, ino) = (before.dev(), before.ino());
        let born = born_of(&before);
        // The old file stays open across the swap, so the swap cannot reuse its inode.
        let held = File::open(&item).unwrap();
        test_swap_hook(Some(swap_to_link_of_hardlink));
        let refused = chmod_pinned(&item, dev, ino, (born, None), 0o644, 0o600);
        test_swap_hook(None);
        drop(held);
        assert!(refused.is_err(), "a link swapped after the checks is refused, got {:?}", refused);
        assert!(item.symlink_metadata().unwrap().file_type().is_symlink(), "the swap is still a link");
        assert_eq!(victim.metadata().unwrap().mode() & 0o777, 0o644, "the swap target keeps its mode");
    }
    #[test]
    fn a_different_birth_time_is_refused() {
        let d = TestDir::new("permissions-birth");
        let a = d.file("a.txt", "a");
        std::fs::set_permissions(&a, Mode::from_mode(0o644)).unwrap();
        let before = a.symlink_metadata().unwrap();
        let born = match born_of(&before) { Some(b) => b, None => return };
        let wrong = (born.0.wrapping_add(1), born.1);
        let refused = chmod_pinned(&a, before.dev(), before.ino(), (Some(wrong), None), 0o644, 0o600);
        assert!(refused.is_err(), "a file with another birth time is refused, got {:?}", refused);
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o644, "the mode stays until the identity matches");
    }
    #[test]
    fn apply_many_changes_every_file_and_undoes_once() {
        let d = TestDir::new("permissions-many");
        let a = d.file("a.txt", "a");
        let b = d.file("b.txt", "b");
        let c = d.file("c.txt", "c");
        for p in [&a, &b, &c] {
            std::fs::set_permissions(p, Mode::from_mode(0o644)).unwrap();
        }
        let steps = apply_many(&[(a.clone(), "600".to_string()), (b.clone(), "600".to_string()), (c.clone(), "600".to_string())]).expect("three ordinary files");
        assert_eq!(steps.len(), 3);
        for p in [&a, &b, &c] {
            assert_eq!(p.metadata().unwrap().mode() & 0o777, 0o600);
        }
        let mut journal = crate::backend::undo::Journal::new();
        journal.push(crate::backend::undo::Entry { op: "permissions".to_string(), steps });
        assert_eq!(journal.undo().expect("one undo restores all three"), "permissions");
        for p in [&a, &b, &c] {
            assert_eq!(p.metadata().unwrap().mode() & 0o777, 0o644);
        }
    }
    #[test]
    fn a_mid_batch_failure_rolls_back_and_says_nothing_changed() {
        let d = TestDir::new("permissions-rollback");
        let a = d.file("a.txt", "a");
        let b = d.file("b.txt", "b");
        for p in [&a, &b] {
            std::fs::set_permissions(p, Mode::from_mode(0o644)).unwrap();
        }
        test_fail_at(Some(1));
        let result = apply_many(&[(a.clone(), "600".to_string()), (b.clone(), "600".to_string())]);
        test_fail_at(None);
        let err = result.unwrap_err();
        assert!(err.msg.contains("No change was applied"), "unexpected message: {}", err.msg);
        assert!(err.applied.is_empty(), "a batch that rolled back whole journals nothing");
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o644, "the first change was rolled back");
        assert_eq!(b.metadata().unwrap().mode() & 0o777, 0o644, "the failed item was never changed");
    }
    #[test]
    fn a_failed_rollback_journals_what_stayed_applied() {
        let d = TestDir::new("permissions-stuck");
        let a = d.file("a.txt", "a");
        let b = d.file("b.txt", "b");
        for p in [&a, &b] {
            std::fs::set_permissions(p, Mode::from_mode(0o644)).unwrap();
        }
        test_fail_at(Some(1));
        test_fail_restore(true);
        let result = apply_many(&[(a.clone(), "600".to_string()), (b.clone(), "600".to_string())]);
        test_fail_at(None);
        test_fail_restore(false);
        let err = result.unwrap_err();
        assert!(err.msg.contains("1 of 2 items were changed"), "unexpected message: {}", err.msg);
        assert_eq!(err.applied.len(), 1, "the step that would not go back stays journalled");
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o600, "the stuck change is still applied");
        let mut journal = crate::backend::undo::Journal::new();
        journal.push(crate::backend::undo::Entry { op: "permissions".to_string(), steps: err.applied });
        assert_eq!(journal.undo().expect("one undo restores what stayed applied"), "permissions");
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o644);
    }
    #[test]
    fn undo_leaves_a_file_put_at_the_name_since_in_place() {
        let d = TestDir::new("permissions-replaced");
        let a = d.file("a.txt", "old");
        std::fs::set_permissions(&a, Mode::from_mode(0o644)).unwrap();
        let steps = apply_many(&[(a.clone(), "600".to_string())]).expect("one ordinary file");
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o600);
        // The old file stays open across the swap, so the replacement takes another inode on any filesystem.
        d.replace_file(&a, "new secret");
        std::fs::set_permissions(&a, Mode::from_mode(0o600)).unwrap();
        let mut journal = crate::backend::undo::Journal::new();
        journal.push(crate::backend::undo::Entry { op: "permissions".to_string(), steps });
        let err = journal.undo().expect_err("a replaced file keeps nothing to restore to");
        assert!(err.msg.contains("left in place"), "{}", err.msg);
        assert_eq!(std::fs::metadata(&a).unwrap().mode() & 0o777, 0o600,
            "undo widened a file it never changed");
    }
    #[test]
    fn undo_refuses_a_mode_with_special_bits_added_since() {
        let d = TestDir::new("permissions-setuid");
        let a = d.file("a.txt", "old");
        std::fs::set_permissions(&a, Mode::from_mode(0o644)).unwrap();
        let steps = apply_many(&[(a.clone(), "600".to_string())]).expect("one ordinary file");
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o600);
        std::fs::set_permissions(&a, Mode::from_mode(0o4600)).unwrap();
        let mut journal = crate::backend::undo::Journal::new();
        journal.push(crate::backend::undo::Entry { op: "permissions".to_string(), steps });
        let err = journal.undo().expect_err("a setuid added since keeps nothing to restore to");
        assert!(err.msg.contains("left in place"), "{}", err.msg);
        assert_eq!(std::fs::metadata(&a).unwrap().mode() & 0o7777, 0o4600, "undo cleared a setuid it never set");
    }
    #[test]
    fn undo_restores_the_rest_when_one_of_several_was_replaced() {
        let d = TestDir::new("permissions-skips-one");
        let a = d.file("a.txt", "a");
        let b = d.file("b.txt", "b");
        for p in [&a, &b] {
            std::fs::set_permissions(p, Mode::from_mode(0o644)).unwrap();
        }
        let steps = apply_many(&[(a.clone(), "600".to_string()), (b.clone(), "600".to_string())])
            .expect("two ordinary files");
        // The old file stays open across the swap, so the replacement takes another inode on any filesystem.
        d.replace_file(&a, "replacement");
        std::fs::set_permissions(&a, Mode::from_mode(0o600)).unwrap();
        let mut journal = crate::backend::undo::Journal::new();
        journal.push(crate::backend::undo::Entry { op: "permissions".to_string(), steps });
        let err = journal.undo().expect_err("one replaced path is skipped with a note");
        assert!(err.msg.contains("left in place"), "{}", err.msg);
        assert_eq!(std::fs::metadata(&a).unwrap().mode() & 0o777, 0o600, "the replacement keeps its mode");
        assert_eq!(b.metadata().unwrap().mode() & 0o777, 0o644, "the untouched file still restores");
    }
}

#[cfg(test)]
#[path = "permissions_wording_tests.rs"]
mod wording;
