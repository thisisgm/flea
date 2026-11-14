// The subtree walk behind a search request, ticked a slice at a time so a cancel is never blocked.
use crate::backend::fuzzy::{rank_order, Fuzzy};
use crate::backend::listing::Listing;
use flea::bytename::ExactText as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

// One tick reads this many directories before the loop looks at its channel again, the same "one at a time" idea dirsize uses.
const DIRS_PER_TICK: usize = 4;

pub struct Search {
    root: PathBuf,
    // Folded once by Fuzzy, because the comparison runs once per entry and the query never changes mid-walk.
    fuzzy: Fuzzy,
    // Lowercased tag:<name> terms the query carried; every match must carry all of them.
    tags: Vec<String>,
    hidden: bool,
    // Directories still to read, each a path relative to root; the empty path is root itself.
    pending: Vec<PathBuf>,
    // One score per pushed match, in push order, so ranking is a permutation of the listing's spans.
    scores: Vec<i32>,
    pub scanned: usize,
    pub started: Instant,
}

impl Search {
    pub fn new(root: &str, query: &str, hidden: bool) -> Search {
        let (tags, text) = split_query(query);
        Search {
            root: flea::bytename::exact(root),
            fuzzy: Fuzzy::new(&text),
            tags,
            hidden,
            pending: vec![PathBuf::new()],
            scores: Vec::new(),
            scanned: 0,
            started: Instant::now(),
        }
    }

    // Returns true when the walk is finished; the caller then writes the terminal line.
    pub fn step(&mut self, listing: &mut Listing) -> bool {
        for _ in 0..DIRS_PER_TICK {
            match self.pending.pop() {
                Some(rel) => self.read_one(&rel, listing),
                None => return true,
            }
        }
        self.pending.is_empty()
    }

    fn read_one(&mut self, rel: &Path, listing: &mut Listing) {
        let dir = if rel.as_os_str().is_empty() { self.root.clone() } else { self.root.join(rel) };
        let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
        // One bound for opendir, getdents and d_type together, so a dead server costs one deadline, never one per entry.
        let owned = dir.clone();
        let entries = match super::iomount::call(&dir, &body, "search", move || read_entries(&owned)) {
            Ok(entries) => entries,
            _ => return,
        };
        let tagged = if self.tags.is_empty() { Vec::new() } else { super::searchtags::read_tags(&dir, &body, &entries, self.hidden) };
        for (i, (raw, is_dir)) in entries.into_iter().enumerate() {
            // corner: a dot-prefixed name is dropped before it is counted, matching scan.rs's own hidden rule.
            if !self.hidden && super::searchindex::is_hidden(&raw) {
                continue;
            }
            self.scanned += 1;
            let child = rel.join(&raw);
            let text = child.exact_text();
            // The tags were read in bounded chunks for the names this walk can match; an unread entry carries none.
            if !self.tags.is_empty() && !names_carry(&tagged[i], &self.tags) {
                if is_dir {
                    self.pending.push(child);
                }
                continue;
            }
            // The candidate is the whole relative path, not the base name, so one query can span a
            // separator: "dwnhelp" reaches "downloads/helper.txt", see docs/protocol.md "search".
            if let Some(score) = self.fuzzy.score(&flea::bytename::display(&text)) {
                // The row's name is its path relative to the root, exact for a name that is not UTF-8; the query meets only its lossy spelling.
                listing.push(&text, is_dir);
                self.scores.push(score);
            }
            // corner: a symlink reports its own type here, so a link to a directory is never descended and no loop is possible.
            if is_dir {
                self.pending.push(child);
            }
        }
    }

    // The walk appends in discovery order, so ranking is one permutation of the spans at the end and
    // the name arena never moves. Answers whether the row order changed, because a new order
    // invalidates every outstanding row index the same way a sort does.
    pub fn rank(&self, listing: &mut Listing) -> bool {
        // A listing this walk did not fill by itself is never reordered: the scores would name other rows.
        if self.scores.len() != listing.len() || listing.len() < 2 {
            return false;
        }
        // Take the buffer out so the comparator can borrow it while spans are moved, as sort.rs does.
        let names = std::mem::take(&mut listing.names);
        let spans = &listing.spans;
        let scores = &self.scores;
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by(|&a, &b| {
            let an = &names[spans[a].off as usize..spans[a].off as usize + spans[a].len as usize];
            let bn = &names[spans[b].off as usize..spans[b].off as usize + spans[b].len as usize];
            rank_order(scores[a], an, scores[b], bn)
        });
        let ranked: Vec<_> = order.iter().map(|&i| spans[i]).collect();
        listing.spans = ranked;
        listing.names = names;
        true
    }
}

// Sample input: "tag:Red holiday snaps" answers (["red"], "holiday snaps"); "dwnhelp" answers ([], "dwnhelp").
pub fn split_query(query: &str) -> (Vec<String>, String) {
    let mut tags = Vec::new();
    let mut words = Vec::new();
    for word in query.split(char::is_whitespace).filter(|word| !word.is_empty()) {
        // get answers None when byte 4 splits a character, so a multibyte word is never sliced there.
        let name = word.get(..4).filter(|head| head.eq_ignore_ascii_case("tag:")).map(|_| &word[4..]);
        if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
            tags.push(name.to_lowercase());
        } else {
            words.push(word);
        }
    }
    (tags, words.join(" "))
}

// The one rule for both searches: the names a file has hold every wanted tag, compared case-insensitively.
pub(crate) fn names_carry(have: &[String], tags: &[String]) -> bool {
    tags.iter().all(|want| have.iter().any(|got| super::tags::name_matches(got, want)))
}

// One directory as (name, is_dir) pairs, the name as the kernel spelled it; d_type is free and answers is_dir with no stat.
// Sample input: a directory holding "a.txt" and a "sub" directory.
fn read_entries(dir: &std::path::Path) -> Vec<(std::ffi::OsString, bool)> {
    // corner: an unreadable directory answers empty in silence, as scan.rs's phase one skips an unreadable entry.
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out; };
    for entry in rd.flatten() {
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        out.push((entry.file_name(), is_dir));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    // Hard rule 9: every path below is inside a sandbox TestDir made and removes itself.
    fn root(d: &TestDir) -> &str {
        d.path().to_str().expect("the sandbox path is utf-8")
    }

    fn walk_all(root: &str, query: &str, hidden: bool) -> (Listing, Search) {
        let mut s = Search::new(root, query, hidden);
        let mut l = Listing::new();
        while !s.step(&mut l) {}
        s.rank(&mut l);
        (l, s)
    }

    // Ranked order, which is the order the client is answered in.
    fn names(l: &Listing) -> Vec<String> {
        (0..l.len()).map(|i| l.name(i).to_string()).collect()
    }

    fn sorted_names(l: &Listing) -> Vec<String> {
        let mut v = names(l);
        v.sort();
        v
    }

    #[test]
    fn a_multibyte_word_splits_without_panicking() {
        assert_eq!(split_query("abc\u{e9}"), (Vec::new(), "abc\u{e9}".to_string()));
        assert_eq!(split_query("ta\u{1f5}:x"), (Vec::new(), "ta\u{1f5}:x".to_string()));
        assert_eq!(split_query("\u{e9}\u{e9}x TAG:Red"), (vec!["red".to_string()], "\u{e9}\u{e9}x".to_string()));
    }

    #[test]
    fn matches_are_paths_relative_to_the_root() {
        let d = TestDir::new("rel");
        d.dir("tools/benches");
        d.file("tools/benches/bench-run.sh", "");
        d.file("unrelated.txt", "");

        let (l, s) = walk_all(root(&d), "bench", false);
        assert_eq!(sorted_names(&l), ["tools/benches", "tools/benches/bench-run.sh"]);
        assert_eq!(s.scanned, 4);
        // The directory match keeps its directory bit, so the client can navigate into it.
        for i in 0..l.len() {
            if l.name(i) == "tools/benches" {
                assert!(l.is_dir(i));
            }
        }
    }

    #[test]
    fn a_query_spanning_a_separator_finds_the_file() {
        let d = TestDir::new("span");
        d.dir("downloads");
        d.file("downloads/helper.txt", "");

        let (l, _) = walk_all(root(&d), "dwnhelp", false);
        assert_eq!(names(&l), ["downloads/helper.txt"]);
    }

    #[test]
    fn results_arrive_ranked_rather_than_in_discovery_order() {
        let d = TestDir::new("rank");
        d.dir("bench");
        d.file("bench/unrelated-notes.txt", "");
        d.file("bench.txt", "");

        let (l, _) = walk_all(root(&d), "bench", false);
        // All three carry the query whatever readdir said: the two whose own name is the query lead
        // on score, the shorter of those two leads on the tie, and the parent-only match comes last.
        assert_eq!(names(&l), ["bench", "bench.txt", "bench/unrelated-notes.txt"]);
    }

    #[test]
    fn hidden_is_false_by_default_and_true_descends_dot_directories() {
        let d = TestDir::new("hidden");
        d.dir(".git");
        d.file(".git/bench.log", "");
        d.file("bench.txt", "");

        let (visible, vs) = walk_all(root(&d), "bench", false);
        assert_eq!(sorted_names(&visible), ["bench.txt"]);
        assert_eq!(vs.scanned, 1);

        let (all, _) = walk_all(root(&d), "bench", true);
        assert_eq!(sorted_names(&all), [".git/bench.log", "bench.txt"]);
    }

    #[test]
    fn a_symlink_to_a_parent_directory_is_never_descended() {
        let d = TestDir::new("loop");
        d.dir("sub");
        d.file("sub/bench.txt", "");
        std::os::unix::fs::symlink(d.path(), d.join("sub/up")).unwrap();

        let (l, _) = walk_all(root(&d), "bench", false);
        assert_eq!(sorted_names(&l), ["sub/bench.txt"]);
    }

    #[test]
    fn a_missing_root_finishes_with_nothing_rather_than_failing() {
        let mut s = Search::new("/definitely/not/here", "x", false);
        let mut l = Listing::new();
        assert!(s.step(&mut l));
        assert_eq!(l.len(), 0);
        assert_eq!(s.scanned, 0);
    }

    #[test]
    fn a_step_bounds_each_directory_once_never_once_per_entry() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("stepcalls");
        for i in 0..DIRS_PER_TICK + 1 {
            d.dir(&format!("d{}", i));
        }
        let mut s = Search::new(root(&d), "zzz", false);
        let mut l = Listing::new();
        // The root read queues every child, so the first step cannot also drain them.
        assert!(!s.step(&mut l));
        assert_eq!(crate::backend::iomount::test_calls(), DIRS_PER_TICK, "one bound per directory read, never one per entry");
    }

    #[test]
    fn a_step_reads_a_bounded_slice_so_a_cancel_is_never_blocked() {
        let d = TestDir::new("slice");
        for i in 0..DIRS_PER_TICK + 3 {
            d.dir(&format!("d{}", i));
        }
        let mut s = Search::new(root(&d), "zzz", false);
        let mut l = Listing::new();
        // The root read queues every child, so the first step cannot also drain them.
        assert!(!s.step(&mut l));
    }

    #[test]
    fn a_listing_the_walk_did_not_fill_is_never_reordered() {
        let s = Search::new("/definitely/not/here", "x", false);
        let mut l = Listing::new();
        l.push("b.txt", false);
        l.push("a.txt", false);
        assert!(!s.rank(&mut l));
        assert_eq!(names(&l), ["b.txt", "a.txt"]);
    }

    #[test]
    fn a_tag_term_keeps_only_files_carrying_it() {
        let d = TestDir::new("tagsearch");
        d.file("holiday.jpg", "");
        d.file("notes.txt", "");
        super::super::tags::write_names(&d.join("holiday.jpg"), &["Red".to_string()]).unwrap();
        let (l, _) = walk_all(root(&d), "tag:Red", false);
        assert_eq!(names(&l), ["holiday.jpg"]);
    }

    #[test]
    fn a_tag_term_combines_with_the_fuzzy_text_and_ignores_case() {
        let d = TestDir::new("tagcombo");
        d.file("holiday.jpg", "");
        d.file("red-herring.jpg", "");
        super::super::tags::write_names(&d.join("holiday.jpg"), &["Red".to_string()]).unwrap();
        let (l, _) = walk_all(root(&d), "TAG:red holiday", false);
        assert_eq!(names(&l), ["holiday.jpg"]);
        // red-herring.jpg fuzzy-matches "herring" but carries no Red tag, so the tag half refuses it.
        let (none, _) = walk_all(root(&d), "TAG:red herring", false);
        assert!(none.len() == 0);
    }

    #[test]
    fn a_tag_query_splits_terms_from_text() {
        assert_eq!(split_query("tag:Red holiday snaps"), (vec!["red".to_string()], "holiday snaps".to_string()));
        assert_eq!(split_query("dwnhelp"), (Vec::<String>::new(), "dwnhelp".to_string()));
        assert_eq!(split_query("tag:"), (Vec::<String>::new(), "tag:".to_string()));
    }

    #[test]
    fn a_foreign_non_ascii_tag_matches_in_any_case() {
        let d = TestDir::new("tagforeigncase");
        d.file("holiday.jpg", "");
        d.file("notes.txt", "");
        super::super::tags::write_names(&d.join("holiday.jpg"), &["\u{c4}rger".to_string()]).unwrap();
        let (lower, _) = walk_all(root(&d), "tag:\u{e4}rger", false);
        assert_eq!(names(&lower), ["holiday.jpg"]);
        let (upper, _) = walk_all(root(&d), "tag:\u{c4}RGER", false);
        assert_eq!(names(&upper), ["holiday.jpg"]);
    }

    #[test]
    fn a_tagged_file_with_a_non_utf8_name_is_still_found() {
        use std::os::unix::ffi::OsStrExt;
        let d = TestDir::new("tagnonutf8");
        let path = d.path().join(std::ffi::OsStr::from_bytes(b"bad\xff.jpg"));
        std::fs::write(&path, "").unwrap();
        super::super::tags::write_names(&path, &["Red".to_string()]).unwrap();
        let (l, _) = walk_all(root(&d), "tag:Red", false);
        assert_eq!(l.len(), 1, "the tag is read at the entry's own path, never at a lossy copy of its name");
    }

    #[test]
    fn a_folder_whose_name_is_not_utf8_is_descended() {
        use std::os::unix::ffi::OsStrExt;
        let d = TestDir::new("dirnonutf8");
        let folder = d.path().join(std::ffi::OsStr::from_bytes(b"bad\xffdir"));
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("bench.txt"), "").unwrap();
        let (l, s) = walk_all(root(&d), "bench", false);
        assert_eq!(
            names(&l),
            [
                flea::bytename::path_text(std::path::Path::new(std::ffi::OsStr::from_bytes(
                    b"bad\xffdir/bench.txt"
                )))
                .into_owned()
            ],
            "the folder is read at its own path, never at a lossy copy of its name"
        );
        assert_eq!(s.scanned, 2);
    }

    #[test]
    fn tag_reads_ride_bounded_chunks_not_one_call_per_entry() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("tagbound");
        d.dir("sub");
        d.file("sub/holiday.jpg", "");
        d.file("notes.txt", "");
        super::super::tags::write_names(&d.join("sub/holiday.jpg"), &["Red".to_string()]).unwrap();
        let (l, _) = walk_all(root(&d), "tag:Red", false);
        assert_eq!(names(&l), ["sub/holiday.jpg"]);
        assert_eq!(crate::backend::iomount::test_calls(), 4, "root and sub each cost the name read plus one chunk, never one per entry");
    }
}
