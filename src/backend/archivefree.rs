// The free-space refusal: an unpacked size the target cannot hold is refused with both numbers, and an unknown side refuses nothing.
use crate::backend::fsinfo;

// SI with one decimal, rounded before its unit is picked: 999950 -> "1.0 MB", as GLib prints.
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "kB", "MB", "GB", "TB"];
    const STEP: u64 = 1000;
    // Tenths, the one decimal printed, so rounding happens at the precision shown.
    const TENTHS: f64 = 10.0;
    if bytes < STEP {
        return format!("{} B", bytes);
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while unit < UNITS.len() - 1 && (value * TENTHS).round() >= STEP as f64 * TENTHS {
        value /= STEP as f64;
        unit += 1;
    }
    format!("{:.1} {}", value, UNITS[unit])
}

// True only when both sides are known and the need passes what is free; an exact fit still fits.
pub fn refuses(needed: u64, free: u64) -> bool {
    needed > free
}

// The board's own sentence: the archive's leaf, both sizes, and the folder whose free space ran out.
pub fn refusal(archive_leaf: &str, needed: u64, free: u64, target_leaf: &str) -> String {
    format!("Extracting {} needs {}, and {} has {} free.",
            archive_leaf, format_size(needed), target_leaf, format_size(free))
}

// Some(sentence) when the extract cannot fit, None when it fits or a side is unknown; dest's filesystem answers the free space.
pub fn check(archive_leaf: &str, unpacked: u64, dest: &std::path::Path) -> Option<String> {
    if unpacked == 0 {
        return None;
    }
    let parent = dest.parent().unwrap_or(dest);
    let info = fsinfo::read(parent)?;
    // A zero-block statfs is unknown room, as copyfile's precheck reads it, so a gvfs or FUSE share never refuses.
    if info.blocks == 0 || !refuses(unpacked, info.free) {
        return None;
    }
    let target = parent.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".to_string());
    Some(refusal(archive_leaf, unpacked, info.free, &target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    // SI with one decimal, the desktop's own rule and ui/js/Format.js's: 3.4 GB, 1.2 GB, never IEC.
    #[test]
    fn sizes_read_the_way_the_status_bar_already_writes_them() {
        assert_eq!(format_size(3_400_000_000), "3.4 GB");
        assert_eq!(format_size(1_200_000_000), "1.2 GB");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2_400), "2.4 kB");
    }

    #[test]
    fn only_a_real_shortfall_refuses() {
        assert!(refuses(3_400_000_000, 1_200_000_000), "needed past free must refuse");
        assert!(!refuses(1_200_000_000, 3_400_000_000), "needed inside free must pass");
        assert!(!refuses(1_200_000_000, 1_200_000_000), "an exact fit still fits");
        assert!(!refuses(0, 0), "nothing needed refuses nothing");
    }

    // The board's own sentence, with the archive's leaf and both sizes in it.
    #[test]
    fn the_refusal_names_the_archive_and_both_sizes() {
        assert_eq!(refusal("backup.tar.zst", 3_400_000_000, 1_200_000_000, "home"),
                   "Extracting backup.tar.zst needs 3.4 GB, and home has 1.2 GB free.");
    }

    // No fixture fakes a nearly-full filesystem, but every one is nearly full beside u64::MAX.
    #[test]
    fn an_unpacked_size_no_disk_can_hold_is_refused_wherever_it_would_land() {
        let d = TestDir::new("archivefree");
        let sentence = check("huge.tar", u64::MAX, d.path()).expect("u64::MAX fits on no filesystem");
        assert!(sentence.contains("huge.tar"), "the sentence names the archive: {}", sentence);
        assert!(sentence.contains("needs"), "and the needed size: {}", sentence);
    }

    #[test]
    fn nothing_needed_or_nothing_known_refuses_nothing() {
        let d = TestDir::new("archivefreefit");
        assert_eq!(check("empty.tar", 0, d.path()), None, "an empty archive always fits");
        assert_eq!(check("small.tar", 1, d.path()), None, "one byte fits on any test filesystem");
    }

    // procfs answers a statfs with zero blocks and zero free, the shape of a gvfs share that omits its room.
    #[test]
    fn a_filesystem_that_reports_no_blocks_is_unknown_room_and_refuses_nothing() {
        // The statfs must answer, or the unstattable path would return the same None and the zero-block branch goes untested.
        let room = fsinfo::read(std::path::Path::new("/proc/self")).expect("procfs answers a statfs");
        assert_eq!((room.blocks, room.free), (0, 0), "the fixture is a zero-block filesystem");
        assert_eq!(check("a.tar", 10, std::path::Path::new("/proc/self/out")), None);
    }

    #[test]
    fn an_unstattable_target_is_not_evidence_the_extract_cannot_fit() {
        // u64::MAX fits on no filesystem, so None proves the unstattable path, never a fit.
        assert_eq!(check("a.tar", u64::MAX, std::path::Path::new("/nonexistent-dir-for-this-test/out")), None);
    }

    // AA5: the sentence names the target folder's own name, and "/" for the root, never a literal.
    #[test]
    fn the_target_is_named_by_its_own_folder_and_the_root_by_a_slash() {
        let root = check("huge.tar", u64::MAX, std::path::Path::new("/out")).expect("u64::MAX fits nowhere");
        assert!(root.contains(" and / has "), "the root is named /: {}", root);
        assert!(!root.contains("home"), "no literal home: {}", root);
        let d = TestDir::new("archivefreeleaf");
        let leaf = d.path().file_name().unwrap().to_string_lossy().to_string();
        let named = check("huge.tar", u64::MAX, &d.join("out")).expect("u64::MAX fits nowhere");
        assert!(named.contains(&format!(" and {} has ", leaf)), "the folder is named by its own leaf: {}", named);
    }

    // A later version of a path overwrites the earlier one, so only distinct paths add up against the free space.
    #[test]
    fn appended_versions_of_one_path_need_room_for_the_largest_only() {
        if crate::backend::sandboxprobe::skipped() { return; }
        let extract = |tag: &str, names: [&str; 3]| {
            let d = TestDir::new(tag);
            let archive = d.file("x.7z", "stub input");
            let free = fsinfo::read(d.path()).expect("the test filesystem answers a statfs").free;
            let listing = format!("print('----------')\nfor name in {names:?}:\n print('Path = %s\\nSize = {}\\n' % name)\n", free / 2);
            let formats = crate::backend::archive::Formats::test_stub(vec!["/usr/bin/python3".into(), "-c".into(), listing], vec!["/usr/bin/true".into()]);
            let cancel = std::sync::atomic::AtomicBool::new(false);
            crate::backend::archiveops::extract(&formats, &archive, &d.join("out"), &cancel, None).expect_err("the stub tool writes nothing")
        };
        assert_eq!(extract("archfreeversions", ["disk.img"; 3]).msg, "the archive tool wrote nothing");
        assert!(extract("archfreepaths", ["a.img", "b.img", "c.img"]).msg.starts_with("Extracting x.7z needs "));
    }

    // AA6: the size is rounded before its unit is picked, so no value reads as 1000.0 of the unit below.
    #[test]
    fn a_size_rounds_before_it_picks_its_unit() {
        assert_eq!(format_size(999_950), "1.0 MB");
        assert_eq!(format_size(999_949), "999.9 kB");
        assert_eq!(format_size(999_950_000), "1.0 GB");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_000), "1.0 kB");
    }
}
