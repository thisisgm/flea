// A file's inode generation from name_to_handle_at, which tells a file reborn on a freed inode within one birth tick from the original; no handle reads None.
use super::undocodec::{get, n, parse_u64};
use crate::jsondoc::Json;
use std::ffi::CStr;
use std::fs::File;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

const AT_FDCWD: c_int = -100;
const AT_EMPTY_PATH: c_int = 0x1000;
const MAX_HANDLE_SZ: usize = 128;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

// What tells a recreated file from the one a step recorded once it reuses the freed inode: birth time, then inode generation.
pub(crate) type Birth = (Option<(u64, u32)>, Option<u64>);

#[repr(C)]
struct FileHandle {
    bytes: u32,
    kind: c_int,
    data: [u8; MAX_HANDLE_SZ],
}

extern "C" {
    fn name_to_handle_at(dirfd: c_int, path: *const c_char, handle: *mut FileHandle, mount_id: *mut c_int, flags: c_int) -> c_int;
}

fn read(dirfd: c_int, path: &CStr, flags: c_int) -> Option<u64> {
    let mut handle = FileHandle { bytes: MAX_HANDLE_SZ as u32, kind: 0, data: [0; MAX_HANDLE_SZ] };
    let mut mount: c_int = 0;
    if unsafe { name_to_handle_at(dirfd, path.as_ptr(), &mut handle, &mut mount, flags) } != 0 {
        return None;
    }
    let held = &handle.data[..(handle.bytes as usize).min(MAX_HANDLE_SZ)];
    Some(handle.kind.to_le_bytes().iter().chain(held).fold(FNV_OFFSET, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)))
}

// The generation of the entry at `path` itself, a link never followed.
pub(crate) fn of_path(path: &Path) -> Option<u64> {
    let owned = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    read(AT_FDCWD, &owned, 0)
}

// The generation of an open file, so a check after an open judges the object it holds.
pub(crate) fn of_fd(file: &File) -> Option<u64> {
    read(file.as_raw_fd(), c"", AT_EMPTY_PATH)
}

// Two generations split two files only when both are known and unequal.
pub(crate) fn differs(was: Option<u64>, now: Option<u64>) -> bool {
    matches!((was, now), (Some(was), Some(now)) if was != now)
}

// An unknown inode generation writes nothing, so a record without one stays byte for byte what it always was.
pub(crate) fn with_generation(mut pairs: Vec<(&str, Json)>, generation: Option<u64>) -> Vec<(&str, Json)> {
    if let Some(generation) = generation {
        pairs.push(("gen", n(generation)));
    }
    pairs
}

// Sample input: {"gen":18446744073709551615} decodes to Some(Some(u64::MAX)); a record with no "gen" key decodes to Some(None).
pub(crate) fn decode_generation(pairs: &[(String, Json)]) -> Option<Option<u64>> {
    match get(pairs, "gen") {
        None => Some(None),
        Some(value) => parse_u64(value).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn a_file_reads_the_same_generation_every_time_and_a_missing_one_reads_none() {
        let d = TestDir::new("filegen-stable");
        let file = d.file("a.txt", "body");
        let (first, second) = (of_path(&file), of_path(&file));
        assert_eq!(first, second, "one file, one generation");
        assert_eq!(of_fd(&File::open(&file).unwrap()), first, "the open file reads what its path read");
        assert_eq!(of_path(&d.join("absent")), None);
    }

    #[test]
    fn only_two_known_unequal_generations_differ() {
        assert!(differs(Some(1), Some(2)));
        assert!(!differs(Some(1), Some(1)));
        assert!(!differs(None, Some(2)) && !differs(Some(1), None) && !differs(None, None), "an unknown side never refuses");
    }

    // Two files that both exist cannot share an inode, so they never share a generation either.
    #[test]
    fn two_live_files_read_different_generations() {
        let d = TestDir::new("filegen-two");
        let (a, b) = (d.file("a.txt", "a"), d.file("b.txt", "b"));
        let (ga, gb) = (of_path(&a), of_path(&b));
        if ga.is_none() || gb.is_none() {
            eprintln!("skipped: the filesystem or sandbox gives no file handle");
            return;
        }
        assert_ne!(ga, gb);
    }
    fn set_mode(path: &Path, bits: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap();
    }

    #[test]
    fn a_different_inode_generation_is_refused_though_inode_and_birth_match() {
        use crate::backend::permissions::{born_of, chmod_pinned};
        use std::os::unix::fs::MetadataExt;
        let d = TestDir::new("filegen-mode");
        let a = d.file("a.txt", "a");
        set_mode(&a, 0o644);
        let before = a.symlink_metadata().unwrap();
        let Some(generation) = of_path(&a) else { return };
        let (dev, ino, born) = (before.dev(), before.ino(), born_of(&before));
        let refused = chmod_pinned(&a, dev, ino, (born, Some(generation.wrapping_add(1))), 0o644, 0o600);
        assert_eq!(refused, Err("the item was replaced, so its mode was left in place.".to_string()));
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o644, "the mode stays until the identity matches");
        chmod_pinned(&a, dev, ino, (born, Some(generation)), 0o644, 0o600).expect("the same generation still applies");
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_file_recreated_on_the_freed_inode_keeps_its_mode() {
        use crate::backend::permissions::{born_of, chmod_pinned};
        use std::os::unix::fs::MetadataExt;
        let d = crate::backend::inodereuse::sandbox("filegen-reuse");
        let a = d.file("a.txt", "a");
        let recreated = crate::backend::inodereuse::recreate(&d, &a, "other", || {
            let before = a.symlink_metadata().unwrap();
            (before.dev(), before.ino(), (born_of(&before), of_path(&a)))
        });
        let (dev, ino, birth) = recreated.saved;
        let after = a.symlink_metadata().unwrap();
        if recreated.reused_inode {
            assert_eq!((after.dev(), after.ino()), (dev, ino), "the new file really reuses the recorded inode");
            assert!(crate::backend::undo::born_differs(birth.0, born_of(&after)) || differs(birth.1, of_path(&a)), "the reused inode has a distinct birth or generation");
        } else {
            assert_ne!((after.dev(), after.ino()), (dev, ino), "the fallback has a distinct inode");
        }
        set_mode(&a, 0o644);
        let refused = chmod_pinned(&a, dev, ino, birth, 0o644, 0o600);
        assert_eq!(refused, Err("the item was replaced, so its mode was left in place.".to_string()));
        assert_eq!(a.metadata().unwrap().mode() & 0o777, 0o644, "its mode stays");
    }

    #[test]
    fn a_mode_step_keeps_its_generation_through_the_journal_and_a_record_without_one_reads_none() {
        let step = |key: &str| format!(r#"{{"v":3,"gen":0,"undo":[{{"op":"chmod","steps":[{{"k":"pm","path":"/a","before":420,"after":384,"dev":1,"ino":2,"born":null{key}}}]}}],"redo":[]}}"#);
        let with = crate::backend::undocodec::decode(&step(&format!(r#","gen":{}"#, u64::MAX))).expect("a step with a generation decodes");
        assert!(matches!(&with.undo[0].steps[0], crate::backend::undo::Step::Mode { generation: Some(u64::MAX), .. }), "the generation comes back exact");
        assert!(crate::jsondoc::render(&crate::backend::undocodec::encode(&with)).contains(&format!(r#""gen": {}"#, u64::MAX)));
        let without = crate::backend::undocodec::decode(&step("")).expect("an older step decodes");
        assert!(matches!(&without.undo[0].steps[0], crate::backend::undo::Step::Mode { generation: None, .. }));
        assert_eq!(crate::jsondoc::render(&crate::backend::undocodec::encode(&without)).matches(r#""gen""#).count(), 1, "only the push counter, no generation key");
    }
}
