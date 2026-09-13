use crate::backend::listing::Listing;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;

pub struct Meta {
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub target_is_dir: bool,
    // Where a symlink points, verbatim and unresolved, empty for every other kind of row; see
    // docs/protocol.md "rows". The link's own bytes, so a relative target stays relative.
    pub target: String,
    // The filesystem this row lives on, so a drop can tell a move within one volume from a copy
    // across two the way Finder does. Free here: the stat that fills the fields above already read it.
    pub dev: u64,
}

// The st_mode file-type bits, plus the two types a thumbnail request can reach: a regular file, or a symlink whose target is stat'd when the row is asked for.
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;

// A fifo, socket or device node named like a video would block a decoder for its whole timeout, so no such row is offered; see AGENTS.md "Thumbnail requests".
pub fn thumbnailable(mode: u32) -> bool {
    mode & S_IFMT == S_IFREG || mode & S_IFMT == S_IFLNK
}

// Phase 2 stats only what a window asked for, see AGENTS.md "Two-phase listing".
pub fn stat_range(base: &Path, l: &Listing, start: usize, count: usize) -> (Vec<Meta>, f64) {
    let t = Instant::now();
    let end = start.saturating_add(count).min(l.len());
    let start = start.min(end);
    let mut out = Vec::with_capacity(end - start);
    for i in start..end {
        // corner: a row that vanished between listing and stat reports zeroes, see AGENTS.md.
        match base.join(l.name(i)).symlink_metadata() {
            Ok(m) => {
                // corner: only a symlink pays a second stat, and only so its icon can be a folder; see AGENTS.md "Icons in the row".
                let is_link = m.file_type().is_symlink();
                let target_is_dir = is_link
                    && base.join(l.name(i)).metadata().map(|t| t.is_dir()).unwrap_or(false);
                // corner: only a symlink pays the readlink, on the same row that already pays the second stat.
                let target = if is_link {
                    std::fs::read_link(base.join(l.name(i)))
                        .map(|t| t.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                out.push(Meta {
                    size: m.size(),
                    mtime: m.mtime(),
                    mode: m.mode(),
                    target_is_dir,
                    target,
                    dev: m.dev(),
                })
            }
            // mode 0 needs no flag beside it: a real st_mode always carries its file-type bits, so
            // 0 is outside the domain and is itself the "I could not look" marker for the whole row.
            Err(_) => out.push(Meta {
                size: 0,
                mtime: 0,
                mode: 0,
                target_is_dir: false,
                target: String::new(),
                dev: 0,
            }),
        }
    }
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

// What a sort by size or date reads for every row: the same lstat stat_range makes, without the
// symlink's second stat, because an order needs no icon.
#[derive(Clone, Copy)]
pub struct Stat {
    pub size: u64,
    pub mtime: i64,
}

// The metadata pass: every row once, in listing order, so the caller's index i is row i. Split
// across the cores because the pass is IO-bound cold, where the KB measured 1005 ms serial against
// 282 ms on twelve threads at 100k rows; available_parallelism follows the affinity mask, so
// `taskset -c 0` is how the serial figure is taken from the same binary.
pub fn stat_all(base: &Path, l: &Listing) -> (Vec<Stat>, f64) {
    let t = Instant::now();
    let n = l.len();
    let mut out = vec![Stat { size: 0, mtime: 0 }; n];
    let workers = std::thread::available_parallelism().map(|w| w.get()).unwrap_or(1);
    // Ceiling division, so every row lands in exactly one chunk; max(1) keeps chunks_mut off zero.
    let per_worker = n.div_ceil(workers).max(1);
    std::thread::scope(|s| {
        for (k, slots) in out.chunks_mut(per_worker).enumerate() {
            let first = k * per_worker;
            s.spawn(move || {
                for (j, slot) in slots.iter_mut().enumerate() {
                    *slot = stat_one(base, l.name(first + j));
                }
            });
        }
    });
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

// corner: a row that vanished between listing and stat reports zeroes, the same zeroes stat_range sends.
fn stat_one(base: &Path, name: &str) -> Stat {
    match base.join(name).symlink_metadata() {
        Ok(m) => Stat { size: m.size(), mtime: m.mtime() },
        Err(_) => Stat { size: 0, mtime: 0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::listing::Listing;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::symlink;

    fn fixture(tag: &str) -> (TestDir, Listing) {
        let d = TestDir::new(tag);
        d.file("three.txt", "abc");
        d.file("empty.txt", "");
        let mut l = Listing::new();
        l.push("three.txt", false);
        l.push("empty.txt", false);
        (d, l)
    }

    #[test]
    fn returns_size_for_each_row_in_the_range() {
        let (d, l) = fixture("range");
        let (metas, _) = stat_range(d.path(), &l, 0, 2);
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].size, 3);
        assert_eq!(metas[1].size, 0);
        assert!(metas[0].mtime > 0);
    }

    #[test]
    fn a_range_past_the_end_is_clamped_not_a_panic() {
        let (d, l) = fixture("clamp");
        let (metas, _) = stat_range(d.path(), &l, 1, 500);
        assert_eq!(metas.len(), 1);
        let (metas, _) = stat_range(d.path(), &l, 99, 10);
        assert!(metas.is_empty());
    }

    #[test]
    fn only_a_regular_file_or_a_symlink_is_offered_as_thumbnailable() {
        let (d, l) = fixture("mode");
        let (metas, _) = stat_range(d.path(), &l, 0, 1);
        assert!(thumbnailable(metas[0].mode), "a regular file must be offered");
        assert!(thumbnailable(0o120777), "a symlink is stat'd when it is asked for");
        assert!(!thumbnailable(0o010644), "a fifo blocks a decoder for its whole timeout");
        assert!(!thumbnailable(0o140644), "a socket is not a file to decode");
        assert!(!thumbnailable(0o020644), "a character device is not a file to decode");
        assert!(!thumbnailable(0o040755), "a directory has no thumbnail");
        assert!(!thumbnailable(0), "a row that vanished reports mode 0");
    }

    #[test]
    fn a_row_that_vanished_reports_zeroes_instead_of_failing() {
        let (d, mut l) = fixture("vanished");
        l.push("never-existed.txt", false);
        let (metas, _) = stat_range(d.path(), &l, 0, 3);
        assert_eq!(metas.len(), 3);
        assert_eq!(metas[2].size, 0);
        assert_eq!(metas[2].mode, 0);
        // The claim mode 0 rests on: a row that was stat'd can never answer 0, so the two never blur.
        assert_ne!(metas[0].mode, 0, "a real row always carries its file-type bits");
        assert_ne!(metas[1].mode, 0, "including the empty file, whose size really is 0");
    }

    #[test]
    fn only_a_symlink_whose_target_is_a_directory_reports_target_is_dir() {
        let d = TestDir::new("linktarget");
        d.dir("realdir");
        d.file("real.txt", "abc");
        symlink(d.join("realdir"), d.join("linkdir")).unwrap();
        symlink(d.join("real.txt"), d.join("linkfile")).unwrap();
        symlink(d.join("nowhere"), d.join("brokenlink")).unwrap();
        let mut l = Listing::new();
        // Pushed in the order stat_range answers in, which is the listing's order and not a sort.
        l.push("realdir", true);
        l.push("linkdir", false);
        l.push("linkfile", false);
        l.push("brokenlink", false);
        let (metas, _) = stat_range(d.path(), &l, 0, 4);
        assert_eq!(metas.len(), 4);
        assert!(!metas[0].target_is_dir, "a real directory is not a symlink to one, so is_dir already covers it");
        assert!(metas[1].target_is_dir, "a symlink to a directory is the one row that draws as a folder");
        assert!(!metas[2].target_is_dir, "a symlink to a regular file is not a folder");
        assert!(!metas[3].target_is_dir, "a broken symlink resolves to nothing, which is not a folder");
        assert_eq!(metas.iter().filter(|m| m.target_is_dir).count(), 1, "exactly one of the four");
        assert_eq!(metas[0].target, "", "a real directory has no target and pays no readlink");
        assert_eq!(metas[1].target, d.join("realdir").to_string_lossy(), "the link's target is the link's own bytes");
        assert_eq!(metas[2].target, d.join("real.txt").to_string_lossy());
        assert_eq!(metas[3].target, d.join("nowhere").to_string_lossy(), "a broken link still names where it points");
    }

    #[test]
    fn the_pass_stats_every_row_once_in_listing_order() {
        let d = TestDir::new("statall");
        let mut l = Listing::new();
        // 25 rows: no core count on a desktop divides it, so the last chunk is short and every
        // boundary between two workers' chunks is exercised whatever available_parallelism says.
        for i in 0..25 {
            let name = format!("f{}", i);
            d.file(&name, &"x".repeat(i));
            l.push(&name, false);
        }
        let (stats, _) = stat_all(d.path(), &l);
        assert_eq!(stats.len(), 25);
        for (i, s) in stats.iter().enumerate() {
            assert_eq!(s.size as usize, i, "row {} was written {} bytes long", i, i);
            assert!(s.mtime > 0, "row {} carries a real mtime", i);
        }
    }

    #[test]
    fn the_pass_lstats_so_a_dangling_link_has_a_size_and_a_vanished_row_has_zeroes() {
        let d = TestDir::new("statallgone");
        symlink("never-existed", d.join("dangling")).unwrap();
        let mut l = Listing::new();
        l.push("dangling", false);
        l.push("never-existed", false);
        let (stats, _) = stat_all(d.path(), &l);
        assert_eq!(stats.len(), 2);
        // The same lstat stat_range makes, so the order agrees with the s the column shows for the link.
        assert_eq!(stats[0].size as usize, "never-existed".len(), "a link's size is its target path");
        assert_eq!((stats[1].size, stats[1].mtime), (0, 0), "the zeroes stat_range would send");
        let (none, _) = stat_all(d.path(), &Listing::new());
        assert!(none.is_empty(), "an empty listing spawns no work and answers nothing");
    }
}
