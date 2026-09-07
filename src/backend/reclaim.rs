// The subtree walk behind a reclaim request: find regenerable directories, size each one, rank
// by size. The walk is the search walk's shape — a slice of directories or one sized match per
// step, so a cancel is never behind more than one tree — and the ranking is the same span
// permutation a search performs, by bytes instead of score.
use crate::backend::dirsize;
use crate::backend::listing::Listing;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::Instant;

// One tick reads this many directories or sizes this many matches, the same bound the search
// walk and the dirsize walker run under.
const DIRS_PER_TICK: usize = 4;

// The exact base names a reclaim reports, case sensitive. The list is build systems and package
// managers' regenerable trees: everything here is weight a package manager or a build can put
// back, and moving one to the trash is recoverable twice over — flea's undo, then a rebuild.
pub const TARGETS: &[&str] = &[
    // JavaScript and its frameworks.
    "node_modules",
    ".next",
    ".nuxt",
    ".output",
    ".turbo",
    ".parcel-cache",
    // Rust, Go-style build outputs, and the generic two.
    "target",
    "dist",
    "build",
    "out",
    // Python and its tooling.
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    // JVM, Terraform, Dart, Sass, coverage runners.
    ".gradle",
    ".terraform",
    ".dart_tool",
    ".sass-cache",
    "coverage",
];

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    // Names only, d_type and one lstat per directory for the device check.
    Discover,
    // One matched tree per step, through the same walker the dirsize row answers use.
    Size,
}

pub struct Reclaim {
    root: PathBuf,
    // The root's own device. A directory on any other filesystem is never descended into, so a
    // scan rooted at home stops at every mount rather than reading a network share by accident.
    dev: u64,
    // Directories still to read, each a path relative to root; the empty string is root itself.
    pending: Vec<String>,
    phase: Phase,
    // Each match's path relative to root, in push order. Push order is row order until rank()
    // permutes the spans, and sizes rides beside it under the same rule.
    found: Vec<String>,
    // (bytes, partial) per match, in push order. partial is dirsize's own 2000 ms floor mark.
    sizes: Vec<(u64, bool)>,
    // How many matches are sized so far, one per step of the Size phase.
    sized: usize,
    pub scanned: usize,
    pub bytes: u64,
    pub started: Instant,
}

impl Reclaim {
    pub fn new(root: &str) -> Reclaim {
        Reclaim {
            root: PathBuf::from(root),
            dev: std::fs::metadata(root).map(|m| m.dev()).unwrap_or(0),
            pending: vec![String::new()],
            phase: Phase::Discover,
            found: Vec::new(),
            sizes: Vec::new(),
            sized: 0,
            scanned: 0,
            bytes: 0,
            started: Instant::now(),
        }
    }

    #[cfg(test)]
    pub fn matched(&self) -> usize {
        self.found.len()
    }

    #[cfg(test)]
    pub fn sized(&self) -> usize {
        self.sized
    }

    // Sizes in the listing's current row order: push order before rank, ranked order after.
    pub fn sizes(&self) -> &[(u64, bool)] {
        &self.sizes
    }

    // Returns true when the walk is finished; the caller then writes the terminal line.
    pub fn step(&mut self, listing: &mut Listing) -> bool {
        if self.phase == Phase::Discover {
            for _ in 0..DIRS_PER_TICK {
                match self.pending.pop() {
                    Some(rel) => self.read_one(&rel, listing),
                    None => {
                        self.phase = Phase::Size;
                        break;
                    }
                }
            }
            if self.phase == Phase::Discover {
                return false;
            }
        }
        // The Size phase: one matched tree per call, the same one-at-a-time rule the dirsize
        // walker runs under, so a cancel is never behind more than one subtree.
        if self.sized < self.found.len() {
            let path = self.root.join(&self.found[self.sized]);
            let result = dirsize::walk(&path);
            self.sizes.push((result.bytes, result.partial));
            self.bytes += result.bytes;
            self.sized += 1;
        }
        self.sized >= self.found.len()
    }

    fn read_one(&mut self, rel: &str, listing: &mut Listing) {
        let dir = if rel.is_empty() { self.root.clone() } else { self.root.join(rel) };
        // corner: an unreadable directory is skipped in silence, exactly as scan.rs's phase one
        // skips an unreadable entry and search's walk skips an unreadable subtree.
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            self.scanned += 1;
            // d_type is free and answers is_dir with no stat, matching scan.rs's phase one. A
            // symlink reports its own type, so a link named like a target is never matched and
            // never descended, and no loop is possible.
            if !entry.file_type().map(|f| f.is_dir()).unwrap_or(false) {
                continue;
            }
            let child = if rel.is_empty() { name.to_string() } else { format!("{}/{}", rel, name) };
            // A directory on another filesystem is refused before anything else about it is
            // considered: a mount that happens to be named like a target would otherwise be
            // listed and then sized across the boundary, network share included.
            let entry_dev = entry.metadata().map(|m| m.dev()).unwrap_or(0);
            if self.dev != 0 && entry_dev != 0 && entry_dev != self.dev {
                continue;
            }
            // corner: there is no hidden flag on a reclaim — .venv and __pycache__ are the point.
            if TARGETS.contains(&name.as_ref()) {
                // A match is reported but never descended into: its own size already covers
                // anything nested, and a monorepo's nested node_modules would answer twice.
                listing.push(&child, true);
                self.found.push(child);
                continue;
            }
            self.pending.push(child);
        }
    }

    // The walk appends in discovery order, so ranking is one permutation of the spans at the end
    // and the name arena never moves. Answers whether the row order changed, because a new order
    // invalidates every outstanding row index the same way a sort does. The size table is
    // permuted by the same order, so sizes() keeps naming rows by index afterwards.
    pub fn rank(&mut self, listing: &mut Listing) -> bool {
        // A walk cancelled mid-discovery or mid-sizing leaves rows without sizes; they stay in
        // discovery order and the client's dirsize requests size them on demand instead.
        if self.sizes.len() != listing.len() || listing.len() < 2 {
            return false;
        }
        // Take the buffer out so the comparator can borrow it while spans are moved, as
        // search's rank and sort.rs both do.
        let names = std::mem::take(&mut listing.names);
        let spans = &listing.spans;
        let sizes = &self.sizes;
        let mut order: Vec<usize> = (0..spans.len()).collect();
        // Heaviest first, which is the whole point of the ranking. The tie is the raw byte
        // compare, so one walk over one tree answers in exactly one order.
        order.sort_by(|&a, &b| {
            sizes[b].0.cmp(&sizes[a].0).then_with(|| {
                let an = &names[spans[a].off as usize..(spans[a].off + spans[a].len) as usize];
                let bn = &names[spans[b].off as usize..(spans[b].off + spans[b].len) as usize];
                an.cmp(bn)
            })
        });
        let ranked: Vec<_> = order.iter().map(|&i| spans[i]).collect();
        let sized: Vec<_> = order.iter().map(|&i| sizes[i]).collect();
        listing.spans = ranked;
        listing.names = names;
        self.sizes = sized;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    // Hard rule 9: every path below is inside a sandbox TestDir made and removes itself.
    fn root(d: &TestDir) -> &str {
        d.path().to_str().expect("the sandbox path is utf-8")
    }

    fn walk_all(root: &str) -> (Listing, Reclaim) {
        let mut r = Reclaim::new(root);
        let mut l = Listing::new();
        while !r.step(&mut l) {}
        r.rank(&mut l);
        (l, r)
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
    fn regenerable_directories_match_by_exact_base_name() {
        let d = TestDir::new("reclaim");
        d.dir("proj/node_modules/left-pad");
        d.file("proj/node_modules/left-pad/index.js", "x");
        d.dir("rust/target/debug");
        d.file("rust/target/debug/app", "x");
        d.dir(".venv/lib");
        d.dir("node_module");
        d.file("node_module/readme", "");
        d.dir("builds");

        let (l, _) = walk_all(root(&d));
        // The singular and the plural are decoys, and a match's inside is never listed.
        assert_eq!(sorted_names(&l), [".venv", "proj/node_modules", "rust/target"]);
        for i in 0..l.len() {
            assert!(l.is_dir(i), "{} keeps the directory bit", l.name(i));
        }
    }

    #[test]
    fn a_match_reports_one_row_for_the_whole_tree_it_covers() {
        let d = TestDir::new("cover");
        d.dir("node_modules/a/b");
        d.file("node_modules/a/b/big.bin", &"x".repeat(10_000));
        d.dir("node_modules/nested/node_modules");

        let (l, r) = walk_all(root(&d));
        // The nested target is not a second row: the outer one's size already covers it.
        assert_eq!(names(&l), ["node_modules"]);
        assert_eq!(r.sizes().len(), 1);
        assert!(r.sizes()[0].0 >= 10_000, "the size is the whole tree, got {}", r.sizes()[0].0);
        assert!(!r.sizes()[0].1);
    }

    #[test]
    fn results_arrive_ranked_heaviest_first() {
        let d = TestDir::new("rank");
        d.dir("small/node_modules");
        d.dir("big/target");
        d.file("big/target/libbig.a", &"x".repeat(100_000));

        let (l, _) = walk_all(root(&d));
        assert_eq!(names(&l), ["big/target", "small/node_modules"]);
    }

    #[test]
    fn a_symlink_named_like_a_target_is_never_matched_nor_descended() {
        let d = TestDir::new("loop");
        d.dir("real/node_modules");
        d.file("real/node_modules/x", "");
        std::os::unix::fs::symlink(d.join("real"), d.join("node_modules")).unwrap();

        let (l, _) = walk_all(root(&d));
        assert_eq!(sorted_names(&l), ["real/node_modules"]);
    }

    #[test]
    fn a_directory_on_another_filesystem_is_never_descended_into() {
        let d = TestDir::new("cross");
        d.dir("proj/target");
        d.file("proj/target/app", "x");
        d.dir("proj/node_modules/left-pad");
        d.file("proj/node_modules/left-pad/index.js", "x");
        d.dir("proj/src");
        d.file("proj/src/main.rs", "fn main() {}");

        let mut r = Reclaim::new(root(&d));
        // Pretend every directory read out of the root lives on another device, the way a
        // mounted share reports; nothing below the root may be descended into.
        r.dev = r.dev.wrapping_add(1);
        let mut l = Listing::new();
        while !r.step(&mut l) {}
        assert_eq!(l.len(), 0);
        assert!(r.scanned > 0, "the root itself was still read");
    }

    #[test]
    fn a_missing_root_finishes_with_nothing_rather_than_failing() {
        let mut r = Reclaim::new("/definitely/not/here");
        let mut l = Listing::new();
        assert!(r.step(&mut l));
        assert_eq!(l.len(), 0);
        assert_eq!(r.scanned, 0);
    }

    #[test]
    fn a_step_reads_a_bounded_slice_so_a_cancel_is_never_blocked() {
        let d = TestDir::new("slice");
        for i in 0..DIRS_PER_TICK + 3 {
            d.dir(&format!("d{}", i));
        }
        let mut r = Reclaim::new(root(&d));
        let mut l = Listing::new();
        // The root read queues every child, so the first step cannot also drain them.
        assert!(!r.step(&mut l));
    }

    #[test]
    fn the_size_phase_walks_one_match_per_step() {
        let d = TestDir::new("pace");
        for name in ["node_modules", "target", ".venv"] {
            d.dir(&format!("{}/pkg", name));
            d.file(&format!("{}/pkg/index.js", name), "x");
        }
        let mut r = Reclaim::new(root(&d));
        let mut l = Listing::new();
        // The root read finds all three and the phase flips in the same tick, so the first step
        // already sizes one match and each later step sizes exactly one more.
        assert!(!r.step(&mut l));
        assert_eq!(r.matched(), 3);
        assert_eq!(r.sized(), 1);
        assert!(!r.step(&mut l));
        assert_eq!(r.sized(), 2);
        assert!(r.step(&mut l));
        assert_eq!(r.sized(), 3);
        assert_eq!(l.len(), 3);
    }

    #[test]
    fn an_unreadable_subtree_is_skipped_and_marks_its_size_partial() {
        let d = TestDir::new("denied");
        d.dir("node_modules/locked");
        d.file("node_modules/locked/x", "");
        d.dir("target");
        std::fs::set_permissions(
            d.join("node_modules/locked"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();

        let (l, r) = walk_all(root(&d));
        std::fs::set_permissions(
            d.join("node_modules/locked"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        // The match is still reported, its size is what was readable, and the client is told it
        // is a floor rather than an exact number, which is dirsize's own rule.
        assert_eq!(sorted_names(&l), ["node_modules", "target"]);
        let sizes = r.sizes();
        let locked = sizes.iter().find(|&&(b, _)| b > 0).expect("the readable tree was sized");
        assert!(locked.1, "the denied subtree marks the size partial");
    }
}
