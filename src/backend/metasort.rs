// The two orders the metadata pass makes possible, size and date; see AGENTS.md "Two-phase listing".
use crate::backend::dirsize::{sort_walk, walked_bytes, DirSize, SORT_BUDGET_MS};
use crate::backend::extclass::classify;
use crate::backend::listing::{Listing, Span};
use crate::backend::meta::{stat_all, Stat};
use crate::backend::sort::{name_order, SortBy};
use std::cmp::Ordering;
use std::path::Path;
use std::time::{Duration, Instant};

// A size sort walks folders under one shared deadline and returns those sizes in final row order.
pub fn sort_by_stat(l: &mut Listing, base: &Path, by: SortBy, desc: bool, hidden_last: bool) -> (f64, f64, Vec<Option<DirSize>>) {
    let class = if matches!(by, SortBy::Size) { classify(base) } else { "" };
    sort_by_stat_as(l, base, class, by, desc, hidden_last)
}

// The class is a parameter so a test can sort a local tree as a share; only a size sort reads it.
fn sort_by_stat_as(l: &mut Listing, base: &Path, class: &str, by: SortBy, desc: bool, hidden_last: bool) -> (f64, f64, Vec<Option<DirSize>>) {
    let (stats, mut pass_ms) = stat_all(base, l);
    let walked = match by {
        SortBy::Size => {
            let t = Instant::now();
            let walked = sort_walk(base, class, l, &stats, t + Duration::from_millis(SORT_BUDGET_MS));
            pass_ms += t.elapsed().as_secs_f64() * 1000.0;
            walked
        }
        _ => Vec::new(),
    };
    let t = Instant::now();
    // An index is sorted and the spans gathered after it, so Span stays the 8 bytes every listing pays.
    let mut order: Vec<u32> = (0..l.len() as u32).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (a as usize, b as usize);
        // Hidden entries follow every visible one in both directions, outside the folder grouping, and no partition here reverses.
        if hidden_last {
            match (super::sort::is_hidden(l.name(a)), super::sort::is_hidden(l.name(b))) {
                (true, false) => return Ordering::Greater,
                (false, true) => return Ordering::Less,
                _ => {}
            }
        }
        match (l.is_dir(a), l.is_dir(b)) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            // desc is the exact reverse of asc inside each group, tie-break included, as name's is.
            _ if desc => key_order(l, &stats, &walked, by, b, a),
            _ => key_order(l, &stats, &walked, by, a, b),
        }
    });
    let seed: Vec<Option<DirSize>> = match by {
        SortBy::Size => order.iter().map(|&i| walked[i as usize]).collect(),
        _ => Vec::new(),
    };
    let spans: Vec<Span> = order.iter().map(|&i| l.spans[i as usize]).collect();
    l.spans = spans;
    (pass_ms, t.elapsed().as_secs_f64() * 1000.0, seed)
}

// Inside one group: the key, then the name, so two equal sizes list the same way every run.
fn key_order(l: &Listing, stats: &[Stat], walked: &[Option<DirSize>], by: SortBy, a: usize, b: usize) -> Ordering {
    let by_key = match by {
        // A folder orders by its walked recursive size, the same number dirsized reports.
        SortBy::Size if l.is_dir(a) => walked_bytes(walked, a).cmp(&walked_bytes(walked, b)),
        SortBy::Size => stats[a].size.cmp(&stats[b].size),
        SortBy::Mtime => stats[a].mtime.cmp(&stats[b].mtime),
        // sort_listing routes name to sort_by_name, so this arm never runs; Equal would still be name order.
        SortBy::Name => Ordering::Equal,
    };
    match by_key {
        Ordering::Equal => name_order(l.name(a).as_bytes(), l.name(b).as_bytes()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dirsize::walk_all;
    use crate::backend::testdir::TestDir;
    use std::time::{Duration, SystemTime};

    // A tree where each key disagrees with the others, the shape tests/protocol.sh builds too: 2 is the
    // larger file and the oldest entry, 3 the smaller and the newest, 11 is older than 1, and 1 holds
    // a 512-byte file so its walked size is above 11's whatever the two directory entries measure.
    // Pushed in an order that is none of the answers.
    fn tree(tag: &str) -> (TestDir, Listing) {
        let d = TestDir::new(tag);
        d.dir("1");
        d.dir("11");
        d.file("1/x", &"x".repeat(512));
        d.file("2", "00000");
        d.file("3", "0");
        stamp(&d.join("2"), 1000);
        stamp(&d.join("11"), 1001);
        stamp(&d.join("1"), 1002);
        stamp(&d.join("3"), 1003);
        let mut l = Listing::new();
        l.push("3", false);
        l.push("1", true);
        l.push("2", false);
        l.push("11", true);
        (d, l)
    }

    fn stamp(path: &Path, seconds: u64) {
        let when = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
        std::fs::File::open(path).unwrap().set_modified(when).unwrap();
    }

    fn names(l: &Listing) -> String {
        (0..l.len()).map(|i| l.name(i)).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn size_lists_directories_by_walked_size_then_files_by_size() {
        let (d, mut l) = tree("sizeasc");
        sort_by_stat(&mut l, d.path(), SortBy::Size, false, false);
        // 1 walks larger than 11, so a build ordering folders by name answers 1 11 3 2.
        assert_eq!(names(&l), "11 1 3 2");
    }

    // Hidden files last on the stat orders with folders still first inside each block.
    #[test]
    fn hidden_last_keeps_dotfiles_after_visible_for_stat_orders() {
        let d = TestDir::new("sizehidden");
        d.dir("Work");
        d.dir(".cache");
        d.file("notes.md", &"x".repeat(64));
        d.file(".bashrc", "x");
        stamp(&d.join("notes.md"), 1003);
        stamp(&d.join(".bashrc"), 1004);
        stamp(&d.join("Work"), 1001);
        stamp(&d.join(".cache"), 1002);
        let pushed = || {
            let mut l = Listing::new();
            for (name, dir) in [("notes.md", false), (".bashrc", false), ("Work", true), (".cache", true)] {
                l.push(name, dir);
            }
            l
        };
        for by in [SortBy::Size, SortBy::Mtime] {
            for desc in [false, true] {
                let mut l = pushed();
                sort_by_stat(&mut l, d.path(), by, desc, true);
                let got = names(&l);
                let first_hidden = got.split(' ').position(|n| n.starts_with('.')).expect("dotfiles");
                let parts: Vec<&str> = got.split(' ').collect();
                assert!(parts[..first_hidden].iter().all(|n| !n.starts_with('.')),
                    "{by:?} desc={desc} keeps every visible entry first: {got}");
                assert!(parts[first_hidden..].iter().all(|n| n.starts_with('.')),
                    "{by:?} desc={desc} keeps every dotfile last: {got}");
                let mut off = pushed();
                sort_by_stat(&mut off, d.path(), by, desc, false);
                assert_ne!(names(&off), got, "{by:?} desc={desc} must differ with the flag off");
            }
        }
        // Folders still lead each block, by walked size and by time alike.
        let mut l = pushed();
        sort_by_stat(&mut l, d.path(), SortBy::Mtime, false, true);
        assert_eq!(names(&l), "Work notes.md .cache .bashrc");
        let mut sized = pushed();
        sort_by_stat(&mut sized, d.path(), SortBy::Size, false, true);
        assert_eq!(names(&sized), "Work notes.md .cache .bashrc");
    }

    #[test]
    fn size_descending_keeps_directories_first_and_reverses_inside_each_group() {
        let (d, mut l) = tree("sizedesc");
        sort_by_stat(&mut l, d.path(), SortBy::Size, true, false);
        assert_eq!(names(&l), "1 11 2 3");
    }

    #[test]
    fn mtime_orders_both_groups_by_time_directories_still_first_in_both_directions() {
        let (d, mut l) = tree("mtime");
        sort_by_stat(&mut l, d.path(), SortBy::Mtime, false, false);
        assert_eq!(names(&l), "11 1 2 3", "a build that lost the grouping answers 2 11 1 3");
        sort_by_stat(&mut l, d.path(), SortBy::Mtime, true, false);
        assert_eq!(names(&l), "1 11 3 2");
    }

    #[test]
    fn equal_keys_fall_back_to_name_order_so_the_listing_is_the_same_every_run() {
        let d = TestDir::new("sizetie");
        let set = ["b", "a", "B", "file_10", "file_2"];
        for n in set {
            d.file(n, "same");
        }
        let mut once = Listing::new();
        let mut again = Listing::new();
        for n in set {
            once.push(n, false);
        }
        for n in set.iter().rev() {
            again.push(n, false);
        }
        sort_by_stat(&mut once, d.path(), SortBy::Size, false, false);
        sort_by_stat(&mut again, d.path(), SortBy::Size, false, false);
        assert_eq!(names(&once), "a B b file_2 file_10", "the tie-break is the name order, digits by value");
        assert_eq!(names(&once), names(&again), "whatever readdir said");
    }

    #[test]
    fn a_vanished_row_sorts_as_empty_and_an_empty_listing_does_not_panic() {
        let d = TestDir::new("sizegone");
        d.file("real", "abc");
        let mut l = Listing::new();
        l.push("real", false);
        // Named to sort after real by name, so this can only pass on size.
        l.push("zzz-gone", false);
        sort_by_stat(&mut l, d.path(), SortBy::Size, false, false);
        assert_eq!(names(&l), "zzz-gone real", "the zeroes stat_range would send sort as the smallest");
        let mut empty = Listing::new();
        let (pass, sort, _) = sort_by_stat(&mut empty, d.path(), SortBy::Mtime, true, false);
        assert_eq!(empty.len(), 0);
        assert!(pass >= 0.0 && sort >= 0.0);
    }

    #[test]
    fn the_gather_moves_spans_and_leaves_every_name_and_flag_intact() {
        let (d, mut l) = tree("gather");
        let mut before: Vec<(String, bool)> = (0..l.len()).map(|i| (l.name(i).to_string(), l.is_dir(i))).collect();
        sort_by_stat(&mut l, d.path(), SortBy::Mtime, true, false);
        let mut after: Vec<(String, bool)> = (0..l.len()).map(|i| (l.name(i).to_string(), l.is_dir(i))).collect();
        before.sort();
        after.sort();
        assert_eq!(before, after, "a sort is a permutation of the rows and nothing else");
    }

    #[test]
    fn folders_order_by_walked_size_with_name_order_reversed() {
        let d = TestDir::new("sizerev");
        d.dir("zz");
        d.dir("aa");
        d.file("aa/big", &"x".repeat(300));
        let mut l = Listing::new();
        l.push("zz", true);
        l.push("aa", true);
        sort_by_stat(&mut l, d.path(), SortBy::Size, false, false);
        assert_eq!(names(&l), "zz aa", "name order alone answers aa zz");
        sort_by_stat(&mut l, d.path(), SortBy::Size, true, false);
        assert_eq!(names(&l), "aa zz");
    }

    #[test]
    fn a_folder_counts_its_whole_subtree_not_its_entry_count() {
        let d = TestDir::new("sizesub");
        d.dir("one");
        d.file("one/big", &"x".repeat(500));
        d.dir("many");
        for i in 0..10 {
            d.file(&format!("many/f{}", i), "0123456789");
        }
        let mut l = Listing::new();
        l.push("many", true);
        l.push("one", true);
        sort_by_stat(&mut l, d.path(), SortBy::Size, false, false);
        assert_eq!(names(&l), "many one", "ten small files still total less than one large file");
        sort_by_stat(&mut l, d.path(), SortBy::Size, true, false);
        assert_eq!(names(&l), "one many");
    }

    #[test]
    fn equal_walked_sizes_fall_back_to_name_order_stably() {
        let d = TestDir::new("sizedirtie");
        d.dir("b");
        d.dir("a");
        let mut once = Listing::new();
        let mut again = Listing::new();
        for n in ["b", "a"] {
            once.push(n, true);
        }
        for n in ["a", "b"] {
            again.push(n, true);
        }
        sort_by_stat(&mut once, d.path(), SortBy::Size, false, false);
        sort_by_stat(&mut again, d.path(), SortBy::Size, false, false);
        assert_eq!(names(&once), "a b", "the tie-break is the name order, digits by value");
        assert_eq!(names(&once), names(&again), "whatever readdir said");
    }

    #[test]
    fn an_unreadable_folder_sorts_by_what_was_counted() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let d = TestDir::new("sizelocked");
        d.dir("big");
        d.file("big/payload", &"x".repeat(1000));
        d.dir("locked");
        d.file("locked/secret", "tiny");
        std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_dir(d.join("locked")).is_ok() {
            // Root bypasses the mode, so this fixture cannot deny anything here.
            std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }
        let mut l = Listing::new();
        l.push("locked", true);
        l.push("big", true);
        let walked = walk_all(d.path(), &l, Instant::now() + Duration::from_secs(60));
        sort_by_stat(&mut l, d.path(), SortBy::Size, true, false);
        let ordered = names(&l);
        std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let locked = walked[0].expect("a locked directory is a floor, never an unknown");
        assert!(locked.partial, "the unreadable row carries partial");
        let big = walked[1].expect("a readable directory carries its walked size");
        assert!(!big.partial, "the readable row carries no partial");
        assert_eq!(ordered, "big locked", "a floor still orders, below what counted more");
    }

    #[test]
    fn walk_all_answers_some_for_directories_and_none_for_files() {
        use std::time::{Duration, Instant};
        let d = TestDir::new("walkallshape");
        d.dir("sub");
        d.file("sub/inner", "x");
        d.file("top", "yyy");
        let mut l = Listing::new();
        l.push("sub", true);
        l.push("top", false);
        l.push("gone-dir", true);
        let walked = walk_all(d.path(), &l, Instant::now() + Duration::from_secs(60));
        assert_eq!(walked.len(), 3);
        assert!(walked[0].is_some(), "a directory row carries its walked size");
        assert!(walked[1].is_none(), "a file row carries nothing to sort a folder by");
        let missing = walked[2].expect("a vanished directory is a floor, never an unknown");
        assert!(missing.partial, "nothing was counted past the missing entry itself");
    }

    #[test]
    fn an_exhausted_budget_floors_every_folder() {
        use std::time::{Duration, Instant};
        let d = TestDir::new("sizebudget");
        d.dir("one");
        d.file("one/big", &"x".repeat(500));
        d.dir("two");
        d.file("two/big", &"x".repeat(500));
        let mut l = Listing::new();
        l.push("one", true);
        l.push("two", true);
        let walked = walk_all(d.path(), &l, Instant::now() - Duration::from_secs(1));
        assert_eq!(walked.len(), 2);
        for (i, w) in walked.iter().enumerate() {
            let w = w.expect("an over-budget folder is a floor, never an unknown");
            assert!(w.partial, "row {} carries partial under an exhausted budget", i);
        }
        let full = crate::backend::dirsize::walk(&d.join("one"));
        assert!(!full.partial, "the full walk sets the total this floors against");
        assert!(walked[0].unwrap().bytes < full.bytes, "the floor counts less than the subtree total");
        let full = crate::backend::dirsize::walk(&d.join("two"));
        assert!(walked[1].unwrap().bytes < full.bytes, "the second floor too");
    }

    // The listing sorted by a walk whose deadline had already passed, the order a share must keep.
    fn expired_order(d: &TestDir, l: &mut Listing) -> String {
        let walked = walk_all(d.path(), l, Instant::now() - Duration::from_secs(1));
        let mut rows: Vec<usize> = (0..l.len()).filter(|&i| l.is_dir(i)).collect();
        rows.sort_by_key(|&i| (walked_bytes(&walked, i), l.name(i).to_string()));
        rows.iter().map(|&i| l.name(i)).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn a_size_sort_on_a_share_walks_no_folder_and_keeps_the_expired_order() {
        let (d, mut l) = tree("sizenet");
        let expired = expired_order(&d, &mut l);
        let (_, _, seed) = sort_by_stat_as(&mut l, d.path(), "network", SortBy::Size, false, false);
        let folders: Vec<DirSize> = seed.iter().flatten().copied().collect();
        assert_eq!(folders.len(), 2, "both folders carry an answer");
        assert!(folders.iter().all(|f| f.partial), "a refused walk answers a floor, never a total");
        let own = std::fs::symlink_metadata(d.join("1")).unwrap().len();
        // "1" holds a 512-byte file, so a walk would count it; the floor is the folder entry alone.
        assert!(folders.iter().any(|f| f.bytes == own), "the floor is the entry size, not the subtree");
        assert_eq!(names(&l).split(' ').filter(|n| *n == "1" || *n == "11").collect::<Vec<_>>().join(" "), expired);
    }

    #[test]
    fn a_phone_is_refused_like_a_share_and_a_local_class_still_walks() {
        for class in ["network", "phone"] {
            let (d, mut l) = tree("sizephone");
            let (_, _, seed) = sort_by_stat_as(&mut l, d.path(), class, SortBy::Size, false, false);
            assert!(seed.iter().flatten().all(|f| f.partial), "{} walks nothing", class);
        }
        for class in ["", "usb"] {
            let (d, mut l) = tree("sizelocal");
            let (_, _, seed) = sort_by_stat_as(&mut l, d.path(), class, SortBy::Size, false, false);
            assert!(seed.iter().flatten().all(|f| !f.partial), "class {:?} still walks", class);
            assert_eq!(names(&l), "11 1 3 2", "and orders by the walked size");
        }
    }
}
