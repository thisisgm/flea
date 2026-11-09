use crate::backend::fsinfo::dev_of;
use crate::backend::listing::Listing;
use crate::backend::proto::listed_line;
use crate::backend::run::{forget_rows, write_window};
use crate::backend::searchreq::finish_search;
use crate::backend::state::{Held, State, Tables};
use crate::backend::thumbs::Pool;
use flea::bytename::ExactText as _;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

// A listing built from paths the client names instead of a directory it scans; see docs/protocol.md
// "listpaths". The base is always "/", so every entry is its absolute path with the leading slash
// removed and base.join(name) reaches the same file again in phase 2.
pub const BASE: &str = "/";

// Sample input: ["/home/gm/Pictures/a.png", "/home/gm/Downloads", "relative/no", "/gone"].
// The order the client gave is the order it gets back: the picker's Recent is newest first and a
// sort here would throw that away. An entry that does not exist is dropped rather than listed
// against a failed stat, which is what keeps a stale history row off the picker instead of drawing
// a row whose size and date are both zero.
pub fn listing_of(paths: &[String]) -> (Listing, f64) {
    let t = Instant::now();
    let mut l = Listing::new();
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    for path in paths {
        // A trust boundary: this list comes from a file every application on the desktop writes.
        let Some(name) = path.strip_prefix('/') else { continue };
        if name.is_empty() {
            continue;
        }
        // The link's own type, the same rule scan() reads off d_type: a symlink to a directory is
        // listed as a file, and a symlink to nothing is still an entry the user put here.
        let exact = flea::bytename::exact(path);
        let owned = exact.clone();
        let meta = match super::iomount::call(&exact, &body, "listpaths", move || {
            fs::symlink_metadata(&owned)
        }) {
            Ok(Ok(meta)) => meta,
            _ => continue,
        };
        let index = l.len();
        l.push(name, meta.is_dir());
        l.spans[index].is_symlink = meta.file_type().is_symlink();
    }
    (l, t.elapsed().as_secs_f64() * 1000.0)
}

// The wire side, the way searchreq answers a search: one listing built, announced and windowed.
pub fn answer(
    out: &mut impl Write,
    st: &mut State,
    pool: &Pool,
    tb: &Tables,
    paths: &[String],
    first: usize,
    line: &str,
) {
    // A new listing replaces whatever the walk was filling, so the walk ends before the build starts.
    finish_search(out, st, true);
    let (mut l, read_ms) = listing_of(paths);
    super::picker::filter_listing(&mut l, &tb.mime, line);
    // A history is small, so a re-read over one always names its added plus removed rows.
    let recheck = st.held == Held::ListPaths;
    let changed = if recheck { super::listing::changed_count(&st.listing, &l) } else { 0 };
    // base and listing only move together, exactly as a list moves them.
    st.base = PathBuf::from(BASE);
    st.listing = l;
    // The held listing is a listpaths one now, so a re-read over it names its count.
    st.held = Held::ListPaths;
    forget_rows(st, pool);
    // The sort figure is always zero: nothing here is sorted, see docs/protocol.md "listpaths".
    let listed = listed_line(
        st.listing.len(),
        read_ms,
        0.0,
        dev_of(&st.base),
        &st.base.exact_text(),
        crate::backend::ops::dir_writable(&st.base),
    );
    let listed = if recheck {
        super::proto::with_changed(&listed, changed)
    } else {
        listed
    };
    writeln!(out, "{}", listed).ok();
    // Rides along unasked, the same first-paint saving a list makes.
    write_window(out, st, 0, first, tb);
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dirsizeworker::Worker;
    use crate::backend::state::{State, Tables};
    use crate::backend::testdir::TestDir;
    use crate::backend::thumbs::Pool;
    use std::sync::{mpsc::channel, Arc};

    const FIRST_ROWS: usize = 10;
    const NO_WATCH: i32 = -1;

    // Supply the worker's precomputed rows to the production landing used by directory listings.
    fn worker_listing(path: &str, listing: Listing) -> super::super::iomount::ListOut {
        let base = std::path::Path::new(path);
        let (first_metas, first_ms) = super::super::meta::stat_range(base, &listing, 0, FIRST_ROWS);
        super::super::iomount::ListOut {
            listing, read_ms: 0.0, sort_ms: 0.0, sized: Vec::new(),
            dev: dev_of(base), writable: super::super::ops::dir_writable(base),
            first_metas, first_start: 0, first_ms, watch_wd: NO_WATCH, watch_mount: std::path::PathBuf::new(),
        }
    }

    // One State plus its tables and pool, so every answer test starts from the same harness.
    fn harness(cache: &std::path::Path) -> (State, Tables, Pool) {
        let (tx, _rx) = crate::backend::events::channel();
        let (st, tb) = (State::new(Worker::new(tx)), Tables::load());
        let (results, _done) = channel();
        let pool = Pool::new(1, results, cache.to_path_buf(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        (st, tb, pool)
    }

    // One listing of file rows, so old/new pairs read as data.
    fn listing_of_names(names: &[&str]) -> Listing {
        let mut l = Listing::new();
        for name in names {
            l.push(name, false);
        }
        l
    }

    #[test]
    fn lists_the_paths_that_exist_in_the_order_they_were_given() {
        let d = TestDir::new("listpaths");
        let file = d.join("b.txt");
        let dir = d.join("a-dir");
        fs::write(&file, "x").unwrap();
        fs::create_dir(&dir).unwrap();
        let asked = vec![
            file.to_string_lossy().to_string(),
            dir.to_string_lossy().to_string(),
        ];
        let (l, _) = listing_of(&asked);
        assert_eq!(l.len(), 2);
        assert_eq!(l.name(0), file.to_string_lossy().strip_prefix('/').unwrap());
        assert!(!l.is_dir(0));
        assert!(l.is_dir(1));
    }

    #[test]
    fn drops_a_path_that_is_gone_rather_than_listing_it() {
        let d = TestDir::new("listpaths-gone");
        let kept = d.join("kept.txt");
        fs::write(&kept, "x").unwrap();
        let asked = vec![
            d.join("never-existed.txt").to_string_lossy().to_string(),
            kept.to_string_lossy().to_string(),
        ];
        let (l, _) = listing_of(&asked);
        assert_eq!(l.len(), 1);
        assert_eq!(l.name(0), kept.to_string_lossy().strip_prefix('/').unwrap());
    }

    #[test]
    fn refuses_a_path_that_is_not_absolute_and_refuses_the_root_itself() {
        let (l, _) = listing_of(&["etc/hostname".to_string(), "".to_string(), "/".to_string()]);
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn a_broken_symlink_is_still_the_entry_the_user_put_here() {
        let d = TestDir::new("listpaths-link");
        let link = d.join("dangling");
        std::os::unix::fs::symlink(d.join("no-such-target"), &link).unwrap();
        let (l, _) = listing_of(&[link.to_string_lossy().to_string()]);
        assert_eq!(l.len(), 1);
        assert!(!l.is_dir(0));
    }

    #[test]
    fn an_empty_request_answers_an_empty_listing() {
        let (l, ms) = listing_of(&[]);
        assert_eq!(l.len(), 0);
        assert!(ms >= 0.0);
    }

    #[test]
    fn a_history_reread_names_one_replaced_path_as_two_changed() {
        // One replaced path is one added plus one removed, so the count is 2.
        let d = TestDir::new("listpaths-changed");
        let a = d.join("a.txt");
        let b = d.join("b.txt");
        let c = d.join("c.txt");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        fs::write(&c, "c").unwrap();
        let astr = a.to_string_lossy().to_string();
        let bstr = b.to_string_lossy().to_string();
        let cstr = c.to_string_lossy().to_string();
        let (mut st, tb, pool) = harness(&d.join("cache"));
        st.base = PathBuf::from("/home/gm");
        let mut out = Vec::new();
        answer(&mut out, &mut st, &pool, &tb, &[astr.clone(), bstr.clone()], 10, "");
        let first = String::from_utf8(out).unwrap();
        assert!(!first.lines().next().unwrap().contains("changed"), "a first history listing carries no count: {}", first);
        let mut out = Vec::new();
        answer(&mut out, &mut st, &pool, &tb, &[astr.clone(), cstr.clone()], 10, "");
        let second = String::from_utf8(out).unwrap();
        assert!(second.lines().next().unwrap().contains("\"changed\":2"), "one path replaced reads as two changed: {}", second);
    }

    #[test]
    fn a_same_path_relist_counts_only_when_the_reload_asked() {
        // One renamed row is one added plus one removed, so the count is 2.
        let d = TestDir::new("listpaths-asked");
        let dir = d.join("dir");
        fs::create_dir(&dir).unwrap();
        let path = dir.to_string_lossy().to_string();
        let (mut st, tb, pool) = harness(&d.join("cache"));
        st.base = PathBuf::from(&path);
        st.listing = listing_of_names(&["a", "b", "c"]);
        let mut out = Vec::new();
        super::super::run::adopt_listed(&mut out, &mut st, &pool, &tb, &path, worker_listing(&path, listing_of_names(&["a", "b", "d"])), true);
        let asked = String::from_utf8(out).unwrap();
        assert!(asked.lines().next().unwrap().contains("\"changed\":2"), "an asked re-list names the rename: {}", asked);
        st.base = PathBuf::from(&path);
        st.listing = listing_of_names(&["a", "b", "c"]);
        let mut out = Vec::new();
        super::super::run::adopt_listed(&mut out, &mut st, &pool, &tb, &path, worker_listing(&path, listing_of_names(&["a", "b", "d"])), false);
        let silent = String::from_utf8(out).unwrap();
        assert!(!silent.lines().next().unwrap().contains("changed"), "an unasked re-read stays silent: {}", silent);
    }

    #[test]
    fn a_first_listpaths_after_list_root_carries_no_count() {
        // BASE is "/", so a base-keyed recheck miscounts a root list as a history re-read.
        let d = TestDir::new("listpaths-root");
        let a = d.join("a.txt");
        fs::write(&a, "a").unwrap();
        let astr = a.to_string_lossy().to_string();
        let (mut st, tb, pool) = harness(&d.join("cache"));
        let mut out = Vec::new();
        super::super::run::adopt_listed(&mut out, &mut st, &pool, &tb, "/", worker_listing("/", listing_of_names(&["bin", "etc", "home"])), false);
        let mut out = Vec::new();
        answer(&mut out, &mut st, &pool, &tb, &[astr], 10, "");
        let line = String::from_utf8(out).unwrap();
        assert!(!line.lines().next().unwrap().contains("changed"), "a first listpaths after list / carries no count: {}", line);
    }

    #[test]
    fn a_list_root_after_listpaths_carries_no_count() {
        // A held history is not a root list, so a root re-list over it names no count.
        let d = TestDir::new("listpaths-held-root");
        let a = d.join("a.txt");
        fs::write(&a, "a").unwrap();
        let astr = a.to_string_lossy().to_string();
        let (mut st, tb, pool) = harness(&d.join("cache"));
        let mut out = Vec::new();
        answer(&mut out, &mut st, &pool, &tb, &[astr], 10, "");
        let mut out = Vec::new();
        super::super::run::adopt_listed(&mut out, &mut st, &pool, &tb, "/", worker_listing("/", listing_of_names(&["bin", "etc", "home"])), true);
        assert_eq!(st.held, Held::List, "a directory landing replaces the held history kind");
        let line = String::from_utf8(out).unwrap();
        assert!(!line.lines().next().unwrap().contains("changed"), "a root list over a held history carries no count: {}", line);
    }
}
