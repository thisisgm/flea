// One buffer plus a span each, see AGENTS.md "Why the listing is an arena".
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use super::scan::{S_IFMT, S_IFREG};

// One gio row's figures in 24 bytes, keyed by its span's name offset, which every sort carries along.
#[derive(Clone, Copy, Debug)]
pub struct GioMeta {
    pub name_off: u32,
    pub mode: u32,
    pub size: u64,
    pub mtime: i64,
}

// A symlink row's target keyed by the same offset; only symlink rows have one.
#[derive(Clone, Debug)]
pub struct GioTarget {
    pub name_off: u32,
    pub target: String,
}

// Enough that a normal directory never reallocates its way up from nothing.
const NAME_RESERVE_BYTES: usize = 1 << 20;
const SPAN_RESERVE: usize = 4096;

#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub off: u32,
    pub len: u32,
    pub is_dir: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Clone)]
pub struct Listing {
    pub outline_open: std::collections::BTreeSet<String>,
    pub outline_watches: Vec<i32>,
    pub names: String,
    pub spans: Vec<Span>,
    // Empty for readdir listings; the gvfs path fills it, so local rows pay no store.
    pub gio_meta: Vec<GioMeta>,
    pub gio_targets: Vec<GioTarget>,
    // The directory's device once per listing; every cached row takes this dev.
    pub base_dev: u64,
    // The slow-pass answer, read from mountinfo once when a pass first crosses SLOW_PASS_MS.
    thread_hint: OnceLock<bool>,
}

impl Listing {
    pub fn new() -> Listing {
        Listing::with_reserve(NAME_RESERVE_BYTES, SPAN_RESERVE)
    }

    // A result list sized to its rows: a short answer's arena stays under the mmap threshold, so it costs no mmap.
    pub fn with_reserve(names: usize, rows: usize) -> Listing {
        Listing { outline_open: Default::default(), outline_watches: Vec::new(), names: String::with_capacity(names), spans: Vec::with_capacity(rows),
            gio_meta: Vec::new(), gio_targets: Vec::new(), base_dev: 0, thread_hint: OnceLock::new() }
    }

    // corner: u32 offsets cap the arena at 4 GiB of names, see AGENTS.md.
    pub fn push(&mut self, name: &str, is_dir: bool) {
        self.push_entry(name, is_dir, false);
    }

    // The new row's index, so a caller adjusting its span never looks it up again.
    pub fn push_entry(&mut self, name: &str, is_dir: bool, is_symlink: bool) -> usize {
        let off = self.names.len() as u32;
        self.names.push_str(name);
        self.spans.push(Span {
            off,
            len: (self.names.len() as u32) - off,
            is_dir,
            is_symlink,
        });
        self.spans.len() - 1
    }

    // The row's wire text: its name, or for a name that is not UTF-8 the exact spelling src/bytename.rs writes.
    pub fn name(&self, i: usize) -> &str {
        let s = &self.spans[i];
        &self.names[s.off as usize..(s.off + s.len) as usize]
    }

    // The exact file a row names under base; every filesystem call on a row goes through here, never base.join(name).
    pub fn path(&self, base: &Path, i: usize) -> PathBuf {
        base.join(flea::bytename::text_path(self.name(i)))
    }

    // The seam: callers ask the listing; spans is public only because sort borrows it.
    pub fn is_dir(&self, i: usize) -> bool {
        self.spans[i].is_dir
    }

    // A cached gio row by its listing index: the span's offset binary searched in build order.
    pub fn gio_for(&self, i: usize) -> Option<&GioMeta> {
        let off = self.spans.get(i)?.off;
        self.gio_meta.binary_search_by_key(&off, |m| m.name_off).ok().map(|at| &self.gio_meta[at])
    }

    // A cached symlink's target by the same offset; empty for every other row.
    pub fn gio_target(&self, name_off: u32) -> &str {
        self.gio_targets.binary_search_by_key(&name_off, |t| t.name_off).ok().map(|at| self.gio_targets[at].target.as_str()).unwrap_or("")
    }

    // The first row that is a plain file, never a folder, symlink or special file a gvfs row names by its mode.
    pub fn first_plain_file(&self) -> Option<&str> {
        (0..self.len())
            .find(|&i| !self.spans[i].is_dir && !self.spans[i].is_symlink && self.gio_for(i).map_or(true, |m| m.mode & S_IFMT == S_IFREG))
            .map(|i| self.name(i))
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    // Locate only names already in this listing; no filesystem access or symlink resolution.
    pub fn index_of(&self, base: &Path, path: &Path) -> Option<usize> {
        let name = relative_name(base, path)?;
        (0..self.len()).find(|&index| self.name(index) == name)
    }

    // Where a list's first window starts: row 0, unless the row to select sorts past that window, which then holds it near its middle.
    // Sample input: on 668 rows with first 330, "/d/north-300.txt" at row 667 answers 338 and a name at row 167 answers 0.
    pub fn first_start(&self, base: &Path, select: Option<&str>, first: usize) -> usize {
        match select.and_then(|path| self.index_of(base, Path::new(path))) {
            Some(at) if first > 0 && at >= first => at.saturating_sub(first / 2).min(self.len() - first),
            _ => 0,
        }
    }

    pub fn indices_of<'a>(&self, base: &Path, paths: &'a [String]) -> Vec<(&'a str, usize)> {
        let mut wanted: HashMap<std::borrow::Cow<'a, str>, &str> = paths.iter().filter_map(|path|
            relative_name(base, Path::new(path)).map(|name| (name, path.as_str()))).collect();
        let mut found = Vec::new();
        for index in 0..self.len() {
            if let Some(path) = wanted.remove(self.name(index)) { found.push((path, index)); }
            if wanted.is_empty() { break; }
        }
        found
    }
}

// The row text a path names under base, whether the path is wire text or the exact bytes, so both meet as bytes.
fn relative_name<'a>(base: &Path, path: &'a Path) -> Option<std::borrow::Cow<'a, str>> {
    use std::borrow::Cow;
    if !base.is_absolute() || !path.is_absolute() {
        return None;
    }
    let normal = |relative: &Path| {
        relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    };
    match path
        .to_str()
        .map(flea::bytename::text_path)
        .unwrap_or(Cow::Borrowed(path))
    {
        Cow::Borrowed(exact) => {
            let relative = exact.strip_prefix(base).ok()?;
            normal(relative).then(|| flea::bytename::path_text(relative))
        }
        Cow::Owned(exact) => {
            let relative = exact.strip_prefix(base).ok()?;
            normal(relative).then(|| Cow::Owned(flea::bytename::path_text(relative).into_owned()))
        }
    }
}

// Sample input: Some("vfat") answers false, Some("nfs") and None answer true.
pub fn threaded_for_fstype(fstype: Option<&str>) -> bool {
    !matches!(fstype, Some("vfat") | Some("exfat"))
}

impl Listing {
    // Cached per listing, and only a slow pass asks: one stat of the folder and one mountinfo read, never a statfs.
    pub fn threaded_cached(&self, base: &Path) -> bool {
        *self.thread_hint.get_or_init(|| {
            let dev = std::fs::metadata(base).map(|m| std::os::unix::fs::MetadataExt::dev(&m)).unwrap_or(0);
            let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
            decide(dev, &body)
        })
    }

    // The gate the window worker carries beside its rows, read off the already-parsed mount table with no stat.
    pub fn threaded_cached_with(&self, body: &str) -> bool {
        *self.thread_hint.get_or_init(|| decide(self.base_dev, body))
    }
}

// Added plus removed names between two listings of one path, so a reload names rows changed rather than net delta.
pub fn changed_count(old: &Listing, new: &Listing) -> usize {
    use std::collections::HashSet;
    let mut missing: HashSet<&str> = HashSet::with_capacity(old.len());
    for i in 0..old.len() {
        missing.insert(old.name(i));
    }
    let mut added = 0;
    for i in 0..new.len() {
        if !missing.remove(new.name(i)) {
            added += 1;
        }
    }
    added + missing.len()
}

// vfat and exfat stay serial; a device mountinfo does not name threads as today.
fn decide(dev: u64, body: &str) -> bool {
    threaded_for_fstype(super::mountinfo::fstype_for_dev(dev, body).as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symlink_type_uses_the_existing_span_padding() {
        #[allow(dead_code)]
        struct OriginalSpan { off: u32, len: u32, is_dir: bool }
        assert_eq!(std::mem::size_of::<Span>(), std::mem::size_of::<OriginalSpan>());
    }

    #[test]
    fn stores_and_returns_names_in_order() {
        let mut l = Listing::new();
        l.push("alpha.txt", false);
        l.push("bin", true);
        assert_eq!(l.len(), 2);
        assert_eq!(l.name(0), "alpha.txt");
        assert_eq!(l.name(1), "bin");
        assert!(!l.is_dir(0));
        assert!(l.is_dir(1));
    }

    #[test]
    fn the_first_plain_file_skips_folders_symlinks_and_special_files() {
        let mut l = Listing::new();
        l.push("bin", true);
        l.push("pipe", false);
        l.push("link", false);
        l.spans[2].is_symlink = true;
        l.push("notes.txt", false);
        l.gio_meta.push(GioMeta { name_off: l.spans[1].off, mode: 0o10644, size: 0, mtime: 0 });
        l.gio_meta.push(GioMeta { name_off: l.spans[3].off, mode: 0o100644, size: 3, mtime: 0 });
        assert_eq!(l.first_plain_file(), Some("notes.txt"));
        let mut folders = Listing::new();
        folders.push("only", true);
        assert_eq!(folders.first_plain_file(), None, "a folder of folders has no file to open");
    }

    #[test]
    fn a_new_listing_is_empty() {
        let l = Listing::new();
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn locates_existing_names_without_resolving_files() {
        let base = Path::new("/listing");
        let mut listing = Listing::new();
        listing.push("first.txt", false);
        listing.push("sub/selected.txt", false);
        assert_eq!(listing.index_of(base, Path::new("/listing/sub/selected.txt")), Some(1));
        listing.spans.reverse();
        assert_eq!(listing.index_of(base, Path::new("/listing/sub/selected.txt")), Some(0));
        for path in ["", "sub/selected.txt", "/listing", "/listing/missing", "/listing-other/first.txt", "/listing/../listing/first.txt"] {
            assert_eq!(listing.index_of(base, Path::new(path)), None, "{path}");
        }
        assert_eq!(listing.index_of(Path::new(""), Path::new("/first.txt")), None);
    }

    #[test]
    fn a_select_past_the_first_window_moves_the_window_onto_it() {
        let base = Path::new("/d");
        let mut listing = Listing::new();
        for i in 0..668 { listing.push(&format!("row-{i:03}"), false); }
        assert_eq!(listing.first_start(base, None, 330), 0);
        assert_eq!(listing.first_start(base, Some("/d/row-167"), 330), 0, "a row inside the first window keeps row 0");
        assert_eq!(listing.first_start(base, Some("/d/row-329"), 330), 0);
        assert_eq!(listing.first_start(base, Some("/d/row-330"), 330), 165, "the first row past it sits mid-window");
        assert_eq!(listing.first_start(base, Some("/d/row-400"), 330), 235);
        assert_eq!(listing.first_start(base, Some("/d/row-667"), 330), 338, "the last row clamps to a full window");
        for path in ["/d/missing", "/e/row-400", "row-400", ""] {
            assert_eq!(listing.first_start(base, Some(path), 330), 0, "{path}");
        }
        assert_eq!(listing.first_start(base, Some("/d/row-400"), 0), 0, "a list asking no rows starts nowhere else");
        assert_eq!(listing.first_start(base, Some("/d/row-400"), 1), 400);
    }

    #[test]
    fn batched_lookup_scans_names_once_and_deduplicates_matches() {
        let mut listing = Listing::new();
        for name in ["a", "sub/b", "quote\"\n"] { listing.push(name, false); }
        let paths = ["/listing/sub/b", "/listing/missing", "/listing/a", "/listing/a",
            "/listing/../listing/a", "/listing-other/a", "a", "/listing/quote\"\n"]
            .map(String::from);
        assert_eq!(listing.indices_of(Path::new("/listing"), &paths),
            vec![("/listing/a", 0), ("/listing/sub/b", 1), ("/listing/quote\"\n", 2)]);
        assert!(listing.indices_of(Path::new("/listing"), &[]).is_empty());
        assert!(listing.indices_of(Path::new("relative"), &paths).is_empty());
    }

    #[test]
    fn a_raw_named_folder_and_row_meet_their_exact_paths_and_never_their_twins() {
        use flea::bytename::ExactText as _;
        use std::os::unix::ffi::OsStrExt;
        let base = Path::new(std::ffi::OsStr::from_bytes(b"/d\xff"));
        let raw = base.join(std::ffi::OsStr::from_bytes(b"z\xfe"));
        let mut listing = Listing::new();
        for name in ["x", &*raw.file_name().unwrap().exact_text()] {
            listing.push(name, false);
        }
        let paths = [
            raw.exact_text().into_owned(),
            "/d\u{fffd}/x".to_string(),
            base.join("z\u{fffd}").exact_text().into_owned(),
        ];
        assert_eq!(
            listing.indices_of(base, &paths),
            vec![(paths[0].as_str(), 1)],
            "a twin folder or twin row never matches"
        );
        assert_eq!(listing.path(base, 1), raw);
    }

    #[test]
    fn handles_names_with_awkward_bytes() {
        let mut l = Listing::new();
        l.push("two\nlines", false);
        l.push("quote\"inside", false);
        l.push("café", false);
        assert_eq!(l.name(0), "two\nlines");
        assert_eq!(l.name(1), "quote\"inside");
        assert_eq!(l.name(2), "café");
    }

    #[test]
    fn one_buffer_holds_every_name() {
        let mut l = Listing::new();
        for i in 0..1000 {
            l.push(&format!("file_{}.txt", i), false);
        }
        assert_eq!(l.len(), 1000);
        assert_eq!(l.name(999), "file_999.txt");
    }

    #[test]
    fn a_vfat_or_exfat_device_stays_serial_while_anything_else_threads() {
        let vfat = "30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw\n";
        let nfs = "31 1 0:45 / /media/nas rw - nfs nas:/share rw\n";
        let fuse = "32 1 0:46 / /media/cloud rw - fuse.rclone remote: rw\n";
        let mut l = Listing::new();
        l.base_dev = 0x811;
        assert!(!l.threaded_cached_with(vfat), "a slow vfat pass stays serial");
        let mut l = Listing::new();
        l.base_dev = 45;
        assert!(l.threaded_cached_with(nfs), "a slow nfs pass keeps today's threads");
        let mut l = Listing::new();
        l.base_dev = 46;
        assert!(l.threaded_cached_with(fuse), "a slow fuse pass keeps today's threads");
        let l = Listing::new();
        assert!(l.threaded_cached_with(vfat), "no match threads as today");
    }

    #[test]
    fn only_vfat_and_exfat_fstypes_stay_serial() {
        assert!(!threaded_for_fstype(Some("vfat")), "vfat stays serial");
        assert!(!threaded_for_fstype(Some("exfat")), "exfat stays serial");
        assert!(threaded_for_fstype(Some("fuse")), "fuse keeps today's threads");
        assert!(threaded_for_fstype(Some("cifs")), "cifs keeps today's threads");
        assert!(threaded_for_fstype(Some("nfs")), "nfs keeps today's threads");
        assert!(threaded_for_fstype(Some("ext4")), "ext4 keeps today's threads");
        assert!(threaded_for_fstype(None), "unknown keeps today's threads");
    }

    #[test]
    fn a_same_path_relist_counts_added_plus_removed_names() {
        let mut old = Listing::new();
        for name in ["a", "b", "c"] {
            old.push(name, false);
        }
        let mut same = Listing::new();
        for name in ["a", "b", "c"] {
            same.push(name, false);
        }
        assert_eq!(changed_count(&old, &same), 0);
        let mut renamed = Listing::new();
        for name in ["a", "b", "d"] {
            renamed.push(name, false);
        }
        assert_eq!(changed_count(&old, &renamed), 2, "one added plus one removed");
        let mut grown = Listing::new();
        for name in ["a", "b", "c", "d", "e"] {
            grown.push(name, false);
        }
        assert_eq!(changed_count(&old, &grown), 2);
    }
}
