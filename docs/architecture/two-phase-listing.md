## Two-phase listing

Phase 1 (`scan.rs`) reads only names and `d_type` from raw `getdents64` via `opendir`/`readdir`,
resolving only `DT_UNKNOWN` entries by `fstatat` on the open dir fd with `AT_SYMLINK_NOFOLLOW`,
so knowing whether an entry is a directory costs nothing extra except on typeless filesystems.
Phase 2 (`meta.rs`) calls
`symlink_metadata()`, either for the rows one `window` request names, through
`stat_range()`, or for every row in the directory when a sort needs it, through
`stat_all()`. Sorting by name and directory-first works entirely on phase-1 data;
sorting by size or modification time runs `stat_all()`, one `lstat` per row split across
`available_parallelism()` scoped threads, and `metasort.rs` then orders an index against
the result and gathers the spans behind it. Those stats live for the one request and are
dropped with it, so a listing in name order carries nothing extra and `Span` stays 12
bytes. `kind` orders by MIME group after phase 1, `date` is an alias for `mtime`, and any
other key, `mode` included, reaches neither phase: `ordering.rs` refuses it with the one
sentence that names the four it accepts. A size sort on a listing
`extclass::refuses_folder_sizes` names (network or phone, the classes `DirSizes.js` refuses) walks no folder: `dirsize::sort_walk`
gives each folder its own entry size marked partial, the order a walk past its deadline gives.

A gvfs FUSE path is the exception to both halves. `scan.rs` lists it through one `gio list`
child (`backend/gvfslist.rs`) instead of readdir: the daemon answers names with size, type
and mtime in one pass, where the FUSE path would block in getdents64 and then pay one round
trip per row. That per-row metadata lives in `Listing`'s compact gio store for the listing's life
rather than for one request, and every re-list rebuilds it, which every Flea write triggers;
a remote change raises no inotify on either route, so the watched re-read never fires for
one. The store is one 24-byte `GioMeta` record per gio row (the span's name offset, mode,
size and mtime), pushed in build order so the offsets increase and row lookups binary search
it, plus the directory's device once per listing and a side store of symlink targets keyed by
the same offset, since only symlink rows have one; `build_listing` shrinks both to their length.
The offset rides in the span through every sort, so a sorted listing looks its rows up by the same
key. `stat_range()` answers those rows from the store with no lstat, a symlink row paying the one
follow-stat `meta_one` pays so a linked folder draws as one, and past
`SLOW_PASS_MS` (10 ms) the remainder of a window goes across 8 threads, unless the listing's
device mounts as vfat or exfat, which stay serial: the gate decides lazily at the first slow
pass from `/proc/self/mountinfo` by that device and caches the answer per listing, so a listing
never pays a statfs. A stat on FUSE or a network share is a round trip of a millisecond or more
against microseconds local, so kernel cifs/nfs/sshfs mounts that serve concurrently gain threads
while a warm local window (0.4 ms for 321 rows) never triggers one. The gio call carries
`-h` on every listing and `parse_line`'s dot filter stays the only hidden rule, so both
routes list the same rows; `-h` adds one attribute to the same reply and the progress bound
adds no round trip, which is what F13 and F19 cost on the NAS.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
