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
    pub emblem: String, // the row's sync status, see backend/emblem.rs; empty when no sync tool tagged it
}

// The st_mode file-type bits, plus the two types a thumbnail request can reach: a regular file, or a symlink whose target is stat'd when the row is asked for.
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;

// A fifo, socket or device node named like a video would block a decoder for its whole timeout, so no such row is offered; see AGENTS.md "Thumbnail requests".
pub fn thumbnailable(mode: u32) -> bool {
    mode & S_IFMT == S_IFREG || mode & S_IFMT == S_IFLNK
}

// The pass runs serially until it has spent SLOW_PASS_MS; see AGENTS.md "Two-phase listing".
pub const SLOW_PASS_MS: f64 = 10.0;
const STAT_THREADS: usize = 8;

// Phase 2 stats only what a window asked for, see AGENTS.md "Two-phase listing".
pub fn stat_range(base: &Path, l: &Listing, start: usize, count: usize) -> (Vec<Meta>, f64) {
    stat_range_at(base, l, start, count, SLOW_PASS_MS)
}

// slow_ms is the pass cost above which the remainder goes to threads; tests pass 0 or infinity.
fn stat_range_at(base: &Path, l: &Listing, start: usize, count: usize, slow_ms: f64) -> (Vec<Meta>, f64) {
    stat_range_with(base, l, start, count, slow_ms, meta_one)
}

// The test seam: stat names the per-row function, so a counting or slow stub observes the trigger.
fn stat_range_with(
    base: &Path,
    l: &Listing,
    start: usize,
    count: usize,
    slow_ms: f64,
    stat: impl Fn(&Path, &str) -> Meta + Sync,
) -> (Vec<Meta>, f64) {
    let t = Instant::now();
    let end = start.saturating_add(count).min(l.len());
    let start = start.min(end);
    let mut out = Vec::with_capacity(end - start);
    for i in start..end {
        out.push(cached_or(base, l, i, &stat));
        let elapsed_ms = t.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms >= slow_ms && end - (i + 1) > 1 && l.threaded_cached(base) {
            out.extend(stat_parallel_with(base, l, i + 1, end, &stat));
            break;
        }
    }
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

// A prefetched gio row answers from its store, a symlink paying one follow-stat; anything else runs the injected stat.
fn cached_or(base: &Path, l: &Listing, i: usize, stat: &(impl Fn(&Path, &str) -> Meta + Sync)) -> Meta {
    if let Some(g) = l.gio_for(i) {
        // corner: a cached symlink pays meta_one's one follow-stat so a linked folder draws as one.
        let target_is_dir = l.spans.get(i).is_some_and(|s| s.is_symlink)
            && base.join(l.name(i)).metadata().map(|t| t.is_dir()).unwrap_or(false);
        return Meta { size: g.size, mtime: g.mtime, mode: g.mode, target_is_dir, target: l.gio_target(g.name_off).to_string(), dev: l.base_dev, emblem: String::new() };
    }
    stat(base, l.name(i))
}

// Contiguous chunks, joined in order, so the rows come back exactly as the serial walk returns them.
fn stat_parallel_with(
    base: &Path,
    l: &Listing,
    start: usize,
    end: usize,
    stat: &(impl Fn(&Path, &str) -> Meta + Sync),
) -> Vec<Meta> {
    let threads = STAT_THREADS.min(end - start);
    let chunk = (end - start).div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                let from = (start + k * chunk).min(end);
                let to = (from + chunk).min(end);
                (from, to, s.spawn(move || (from..to).map(|i| cached_or(base, l, i, stat)).collect::<Vec<Meta>>()))
            })
            .collect();
        let mut out = Vec::with_capacity(end - start);
        for (from, to, h) in handles {
            // corner: a thread that panicked still owes its rows, so they report zeroes like a vanished row.
            out.extend(h.join().unwrap_or_else(|_| (from..to).map(|_| zeroes()).collect()));
        }
        out
    })
}

fn meta_one(base: &Path, name: &str) -> Meta {
    // corner: a row that vanished between listing and stat reports zeroes, see AGENTS.md.
    match base.join(name).symlink_metadata() {
        Ok(m) => {
            // corner: only a symlink pays a second stat, and only so its icon can be a folder; see AGENTS.md "Icons in the row".
            let is_link = m.file_type().is_symlink();
            let target_is_dir = is_link && base.join(name).metadata().map(|t| t.is_dir()).unwrap_or(false);
            // corner: only a symlink pays the readlink, on the same row that already pays the second stat.
            let target = if is_link { std::fs::read_link(base.join(name)).map(|t| t.to_string_lossy().to_string()).unwrap_or_default() } else { String::new() };
            Meta { size: m.size(), mtime: m.mtime(), mode: m.mode(), target_is_dir, target, dev: m.dev(), emblem: crate::backend::emblem::read_emblem(base, name) }
        }
        Err(_) => zeroes(),
    }
}

// mode 0 needs no flag beside it: a real st_mode always carries its file-type bits, so
// 0 is outside the domain and is itself the "I could not look" marker for the whole row.
fn zeroes() -> Meta {
    Meta { size: 0, mtime: 0, mode: 0, target_is_dir: false, target: String::new(), dev: 0, emblem: String::new() }
}

// One window row's cached gio facts, owned so a remote worker answers from the store with no arena clone.
pub struct WindowRow {
    pub name: String,
    pub cached: Option<CachedRow>,
}
// The store's answer for one row, owned with it: size, mtime and mode ride along, the link target too.
pub struct CachedRow {
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub target: String,
    pub is_symlink: bool,
    pub base_dev: u64,
}
// The window's own rows and nothing else: a remote worker stats these with no listing clone.
pub fn window_rows(l: &Listing, start: usize, count: usize) -> Vec<WindowRow> {
    let end = start.saturating_add(count).min(l.len());
    let start = start.min(end);
    (start..end).map(|i| WindowRow {
        name: l.name(i).to_string(),
        cached: l.gio_for(i).map(|g| CachedRow {
            size: g.size,
            mtime: g.mtime,
            mode: g.mode,
            target: l.gio_target(g.name_off).to_string(),
            is_symlink: l.spans.get(i).is_some_and(|s| s.is_symlink),
            base_dev: l.base_dev,
        }),
    }).collect()
}
// Stats only the rows handed over, serial then threaded past SLOW_PASS_MS, so many slow stats overlap while one hung stat still holds the window to the call's deadline.
pub fn stat_window_rows(base: &Path, rows: &[WindowRow], threaded: bool) -> (Vec<Meta>, f64) {
    stat_window_rows_with(base, rows, SLOW_PASS_MS, threaded, window_meta_one)
}
// slow_ms and the serial gate ride along, so vfat and exfat stay serial; tests pass 0 or infinity.
fn stat_window_rows_with(base: &Path, rows: &[WindowRow], slow_ms: f64, threaded: bool, stat: impl Fn(&Path, &WindowRow) -> Meta + Sync) -> (Vec<Meta>, f64) {
    let t = Instant::now();
    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        out.push(stat(base, row));
        let elapsed_ms = t.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms >= slow_ms && rows.len() - (i + 1) > 1 && threaded {
            out.extend(stat_window_parallel(base, rows, i + 1, &stat));
            break;
        }
    }
    (out, t.elapsed().as_secs_f64() * 1000.0)
}
// One row's answer: a cached row from its snapshot, any other by stat, a cached symlink paying one follow-stat.
fn window_meta_one(base: &Path, row: &WindowRow) -> Meta {
    match &row.cached {
        Some(g) => {
            // corner: a cached symlink pays meta_one's one follow-stat so a linked folder draws as one.
            let target_is_dir = g.is_symlink && base.join(&row.name).metadata().map(|t| t.is_dir()).unwrap_or(false);
            Meta { size: g.size, mtime: g.mtime, mode: g.mode, target_is_dir, target: g.target.clone(), dev: g.base_dev, emblem: String::new() }
        }
        None => meta_one(base, &row.name),
    }
}
// Contiguous chunks joined in order, so the rows come back exactly as the serial walk returns them.
fn stat_window_parallel(base: &Path, rows: &[WindowRow], start: usize, stat: &(impl Fn(&Path, &WindowRow) -> Meta + Sync)) -> Vec<Meta> {
    let threads = STAT_THREADS.min(rows.len() - start);
    let chunk = (rows.len() - start).div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                let from = (start + k * chunk).min(rows.len());
                let to = (from + chunk).min(rows.len());
                (from, to, s.spawn(move || (from..to).map(|i| stat(base, &rows[i])).collect::<Vec<Meta>>()))
            })
            .collect();
        let mut out = Vec::with_capacity(rows.len() - start);
        for (from, to, h) in handles {
            // corner: a thread that panicked still owes its rows, so they report zeroes like a vanished row.
            out.extend(h.join().unwrap_or_else(|_| (from..to).map(|_| zeroes()).collect()));
        }
        out
    })
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
    stat_all_with(base, l, stat_one)
}

// The test seam: a prefetched gio listing answers from its store with no stat at all.
fn stat_all_with(base: &Path, l: &Listing, stat: impl Fn(&Path, &str) -> Stat + Sync) -> (Vec<Stat>, f64) {
    let t = Instant::now();
    let n = l.len();
    if !l.gio_meta.is_empty() {
        let out: Vec<Stat> = (0..n).map(|i| match l.gio_for(i) {
            Some(g) => Stat { size: g.size, mtime: g.mtime },
            None => stat(base, l.name(i)),
        }).collect();
        return (out, t.elapsed().as_secs_f64() * 1000.0);
    }
    let mut out = vec![Stat { size: 0, mtime: 0 }; n];
    let workers = std::thread::available_parallelism().map(|w| w.get()).unwrap_or(1);
    // Ceiling division, so every row lands in exactly one chunk; max(1) keeps chunks_mut off zero.
    let per_worker = n.div_ceil(workers).max(1);
    let stat_ref = &stat;
    std::thread::scope(|s| {
        for (k, slots) in out.chunks_mut(per_worker).enumerate() {
            let first = k * per_worker;
            s.spawn(move || {
                for (j, slot) in slots.iter_mut().enumerate() {
                    *slot = stat_ref(base, l.name(first + j));
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

    #[test]
    fn the_threaded_stat_returns_the_serial_rows_in_order() {
        let d = TestDir::new("statparallel");
        let mut l = Listing::new();
        for n in 0..57 {
            let name = format!("f{:02}", n);
            std::fs::write(d.join(&name), vec![b'x'; n]).expect("fixture");
            l.push(&name, false);
        }
        let (serial, _) = stat_range_at(d.path(), &l, 3, 50, f64::INFINITY);
        let (threaded, _) = stat_range_at(d.path(), &l, 3, 50, 0.0);
        assert_eq!(serial.len(), 50);
        let sizes = |m: &[Meta]| m.iter().map(|x| (x.size, x.mode, x.mtime)).collect::<Vec<_>>();
        assert_eq!(sizes(&threaded), sizes(&serial));
        assert_eq!(threaded[0].size, 3, "row 3 first, in listing order");
        assert_eq!(threaded[49].size, 52);
    }

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

    #[test]
    fn a_slow_pass_goes_to_threads_while_a_fast_one_stays_serial() {
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};
        let d = TestDir::new("statelapsed");
        let mut l = Listing::new();
        for n in 0..16 {
            l.push(&format!("f{:02}", n), false);
        }
        // Fast: microseconds for all 16 rows, far under the 10 ms budget, so no thread spawns.
        let seen_fast: Arc<Mutex<HashSet<std::thread::ThreadId>>> = Arc::new(Mutex::new(HashSet::new()));
        let seen = Arc::clone(&seen_fast);
        let fast = move |_: &Path, name: &str| {
            seen.lock().unwrap().insert(std::thread::current().id());
            Meta { size: name[1..].parse().unwrap_or(0), mtime: 1, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0, emblem: String::new() }
        };
        let (metas, _) = stat_range_with(d.path(), &l, 0, 16, SLOW_PASS_MS, fast);
        assert_eq!((metas.len(), metas[0].size, metas[15].size), (16, 0, 15));
        assert_eq!(seen_fast.lock().unwrap().len(), 1, "a fast pass must not pay for threads");
        // Slow: one 25 ms row already spends the budget, so the remainder goes across threads.
        let seen_slow: Arc<Mutex<HashSet<std::thread::ThreadId>>> = Arc::new(Mutex::new(HashSet::new()));
        let seen = Arc::clone(&seen_slow);
        let slow = move |_: &Path, name: &str| {
            seen.lock().unwrap().insert(std::thread::current().id());
            std::thread::sleep(std::time::Duration::from_millis(25));
            Meta { size: name[1..].parse().unwrap_or(0), mtime: 1, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0, emblem: String::new() }
        };
        let (metas, _) = stat_range_with(d.path(), &l, 0, 16, SLOW_PASS_MS, slow);
        assert_eq!((metas.len(), metas[0].size, metas[15].size), (16, 0, 15));
        assert!(seen_slow.lock().unwrap().len() > 1, "a slow pass must share the remainder across threads");
    }

    #[test]
    fn a_slow_vfat_pass_stays_serial_while_a_slow_local_pass_threads() {
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};
        let d = TestDir::new("statfatserial");
        // Sample mountinfo: the stick is vfat on 8:17, the card exfat on 8:33, the nas nfs on 0:45.
        let body = "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw\n31 1 8:33 / /media/card rw - exfat /dev/sdc1 rw\n32 1 0:45 / /media/nas rw - nfs nas:/share rw\n";
        let mk = |dev: u64| {
            let mut l = Listing::new();
            for n in 0..16 {
                l.push(&format!("f{:02}", n), false);
            }
            l.base_dev = dev;
            l.threaded_cached_with(body);
            l
        };
        let run = |dev: u64| {
            let seen: Arc<Mutex<HashSet<std::thread::ThreadId>>> = Arc::new(Mutex::new(HashSet::new()));
            let slow = {
                let seen = Arc::clone(&seen);
                move |_: &Path, name: &str| {
                    seen.lock().unwrap().insert(std::thread::current().id());
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    Meta { size: name[1..].parse().unwrap_or(0), mtime: 1, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0, emblem: String::new() }
                }
            };
            let (metas, _) = stat_range_with(d.path(), &mk(dev), 0, 16, SLOW_PASS_MS, slow);
            assert_eq!((metas.len(), metas[0].size, metas[15].size), (16, 0, 15));
            let n = seen.lock().unwrap().len();
            n
        };
        assert_eq!(run(0x811), 1, "a slow vfat pass must not pay for threads");
        assert_eq!(run(0x821), 1, "a slow exfat pass must not pay for threads");
        assert!(run(0x801) > 1, "a slow ext4 pass keeps today's threads");
        assert!(run(45) > 1, "a slow nfs pass keeps today's threads");
    }

    #[test]
    fn a_sorted_gio_listing_reads_cached_figures_with_no_stat() {
        // Rows name files absent from the test dir, so any stat would answer zeroes.
        let d = TestDir::new("gio-sort-cached");
        let text = "smb://h/share/b.txt\t30\t(regular)\ttime::modified=300\nsmb://h/share/a.txt\t10\t(regular)\ttime::modified=100\nsmb://h/share/c.txt\t20\t(regular)\ttime::modified=200\n";
        let build = || crate::backend::gvfslist::build_listing(text, false, d.path().to_str().unwrap()).unwrap();
        let (stats, _) = stat_all_with(d.path(), &build(), |_: &Path, _: &str| panic!("a cached sort must not stat"));
        assert_eq!(stats.iter().map(|s| (s.size, s.mtime)).collect::<Vec<_>>(), [(30, 300), (10, 100), (20, 200)]);
        for by in [crate::backend::sort::SortBy::Size, crate::backend::sort::SortBy::Mtime] {
            let mut l = build();
            crate::backend::metasort::sort_by_stat(&mut l, d.path(), by, false, false);
            assert_eq!((0..l.len()).map(|i| l.name(i)).collect::<Vec<_>>(), ["a.txt", "c.txt", "b.txt"]);
            // The spans moved, so each row's figures must still be its own name's.
            let figures: Vec<(u64, i64)> = (0..l.len()).map(|i| l.gio_for(i).map(|g| (g.size, g.mtime)).unwrap_or_default()).collect();
            assert_eq!(figures, [(10, 100), (20, 200), (30, 300)], "the offset key follows its row through the sort");
        }
    }

    #[test]
    fn a_prefetched_listing_answers_without_any_stat() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let d = TestDir::new("statcached");
        let text = "smb://h/share/a.txt\t10\t(regular)\ttime::modified=1790537811 unix::mode=33216\nsmb://h/share/sub\t4096\t(directory)\ttime::modified=1790537811 unix::mode=16832\nsmb://h/share/link\t5\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=a.txt time::modified=1790537811 unix::mode=41471\n";
        let l = crate::backend::gvfslist::build_listing(text, false, d.path().to_str().unwrap()).unwrap();
        let calls = AtomicUsize::new(0);
        let (metas, _) = stat_range_with(d.path(), &l, 0, 3, SLOW_PASS_MS, |_: &Path, _: &str| {
            calls.fetch_add(1, Ordering::SeqCst);
            zeroes()
        });
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no cached row may reach the stat function");
        assert_eq!((metas[0].size, metas[0].mode), (10, 0o100700));
        assert_eq!((metas[1].size, metas[1].mode), (4096, 0o40700));
        let dev = std::os::unix::fs::MetadataExt::dev(&std::fs::metadata(d.path()).unwrap());
        assert_eq!((metas[2].target.as_str(), metas[2].dev), ("a.txt", dev), "the directory's own device, stat'ed here");
        let all_calls = AtomicUsize::new(0);
        let (stats, _) = stat_all_with(d.path(), &l, |_: &Path, _: &str| {
            all_calls.fetch_add(1, Ordering::SeqCst);
            Stat { size: 0, mtime: 0 }
        });
        assert_eq!(all_calls.load(Ordering::SeqCst), 0, "the size/date pass must read the cache too");
        assert_eq!((stats.len(), stats[0].size, stats[1].size), (3, 10, 4096));
    }

    #[test]
    fn window_rows_move_only_the_window_and_stat_the_same() {
        let d = TestDir::new("windowrows");
        d.file("a.txt", "a");
        std::fs::write(d.join("b.txt"), "bb").unwrap();
        let mut l = Listing::new();
        l.push("a.txt", false);
        l.push("b.txt", false);
        l.push("c.txt", false);
        let rows = window_rows(&l, 1, 1);
        assert_eq!(rows.len(), 1, "a remote worker moves one window, never the arena");
        assert_eq!(rows[0].name, "b.txt");
        assert!(rows[0].cached.is_none(), "a readdir row carries no cache");
        let (range, _) = stat_range(d.path(), &l, 0, 3);
        let (moved, _) = stat_window_rows(d.path(), &window_rows(&l, 0, 3), true);
        assert_eq!(moved.len(), range.len());
        for (m, r) in moved.iter().zip(range.iter()) {
            assert_eq!((m.size, m.mtime, m.mode), (r.size, r.mtime, r.mode), "the moved rows stat the same");
        }
        assert_eq!((moved[0].size, moved[1].size), (1, 2));
        assert_eq!((moved[2].size, moved[2].mode), (0, 0), "a vanished row still reports zeroes");
    }

    // A slow window goes to threads past SLOW_PASS_MS, a fast one stays serial, and a gated-off one never threads.
    #[test]
    fn a_slow_window_threads_while_a_fast_one_and_a_gated_one_stay_serial() {
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};
        let d = TestDir::new("windowelapsed");
        let rows: Vec<WindowRow> = (0..16).map(|n| WindowRow { name: format!("f{:02}", n), cached: None }).collect();
        let run = |threaded: bool, sleep_ms: u64| {
            let holds: Arc<Mutex<HashSet<std::thread::ThreadId>>> = Arc::new(Mutex::new(HashSet::new()));
            let seen = Arc::clone(&holds);
            let stub = move |_: &Path, row: &WindowRow| {
                seen.lock().unwrap().insert(std::thread::current().id());
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                Meta { size: row.name[1..].parse().unwrap_or(0), mtime: 1, mode: 0o100644, target_is_dir: false, target: String::new(), dev: 0, emblem: String::new() }
            };
            let (metas, _) = stat_window_rows_with(d.path(), &rows, SLOW_PASS_MS, threaded, stub);
            assert_eq!((metas.len(), metas[0].size, metas[15].size), (16, 0, 15));
            let threads = holds.lock().unwrap().len();
            threads
        };
        assert_eq!(run(true, 0), 1, "a fast window must not pay for threads");
        assert!(run(true, 25) > 1, "a slow window must share the remainder across threads");
        assert_eq!(run(false, 25), 1, "a gated-off window stays serial however slow");
    }

    #[test]
    fn window_rows_answer_a_cached_row_from_its_snapshot() {
        let d = TestDir::new("windowcached");
        let text = "smb://h/share/a.txt\t10\t(regular)\ttime::modified=300 unix::mode=33188\n";
        let l = crate::backend::gvfslist::build_listing(text, false, d.path().to_str().unwrap()).unwrap();
        let rows = window_rows(&l, 0, 1);
        assert!(rows[0].cached.is_some(), "a gio row rides to the worker in the snapshot");
        let (metas, _) = stat_window_rows(d.path(), &rows, true);
        assert_eq!((metas[0].size, metas[0].mtime, metas[0].mode), (10, 300, 33188));
    }
}
