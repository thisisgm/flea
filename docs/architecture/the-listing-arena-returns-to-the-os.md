## The listing arena returns to the OS

`backend::run` calls `heap::pin_mmap_threshold()` before anything else, which is one
`mallopt(M_MMAP_THRESHOLD, 131072)` through an `extern "C"` in the idiom `src/thp.rs` already
uses. **It is worth 4.8 MiB of the backend child's memory while a second large directory is on
screen, and it costs nothing measurable in time.**

The TUI front end takes a different lever. `tui::run` calls `heap::one_arena()` first, `mallopt(M_ARENA_MAX, 1)`,
because a picture job's thread opened a second glibc arena that kept its pages after the thread ended: measured on
East on the 2000-file media fixture with a video poster at rest, anonymous memory 976 KiB against 768 before the
poster existed, and 620 to 788 with one arena. `src/tui/run_tests.rs` pins the call ahead of the wire's first thread.
`src/heap.rs`'s `helper_threads_share_one_arena` pins the cap: it re-executes the test binary with a probe that runs
before `main`, on the one thread `tui::run` has at that call, and goes red unless `malloc_info` names the main arena
alone after four allocating threads, so a lost cap or a cap of 2 fails it. The probe cannot run inside a test, because
libtest's own thread already holds a second arena there, and a cap of 2 changes nothing once two exist.
With one arena, the blocks a picture's encoding freed (the job's output read from a pipe, the base64 chunks) stay
inside `[heap]` below live ones, so `graphics::send` writes a picture's transmission and then calls
`heap::give_back()`, `malloc_trim(0)`. Measured on East on the same fixture, five runs each: anonymous memory 620 to
640 KiB with it, 756 to 876 without, `[heap]` 428 to 444 against 564 to 680.
`src/heap.rs`'s `a_sent_picture_returns_the_heap_pages_freed_below_live_blocks` frees 1 MiB below a live block and
goes red unless `send` returns at least half of it.

The mechanism, which matters because the obvious measurement cannot see it. glibc's `free`
ratchets `mmap_threshold` upward, to the size of the block, whenever it frees a block that `mmap`
served. A listing's name arena and its span vector both cross the 128 KiB default, so the FIRST
large listing is `mmap`'d and is handed back to the kernel when it is freed. That free is also
what raises the threshold past the arena's own size, so **every large listing after the first is
served from the heap and never leaves the process.** Naming a threshold is what stops the ratchet:
glibc sets `no_dyn_threshold` the moment a program sets one, through the tunable or `mallopt`.

**So the size of the win depends entirely on the sequence, and any single number quoted without
its sequence is wrong.** Measured through the REAL path, a live Flea window driven by `h`,
`g`, `j` and `Return` to list the 100,000-file fixture, leave it, and list it again, with a
negative control on the same tree and the arms alternating over six interleaved pairs. `Pss` from
`smaps_rollup` in KiB, divide by 1024 for MiB:

| When | No lever | Pinned | Saving |
|---|---|---|---|
| first large directory on screen | 4999 | 4949 | 50 KiB, nothing |
| **second large directory on screen** | **9882** | **4957** | **4925 KiB, 4.81 MiB, 6 of 6 pairs disjoint** |
| back on a 10-entry directory | 1633 | 1453 | 180 KiB |

`VmRSS` on the middle row is 11788 against 6862. Read the second row as what it is: **without the
lever, listing a large directory a second time roughly DOUBLES the backend, and with it the second
listing costs what the first did.**

**The field bench will not show this and must not be expected to.** `tools/flea-field-bench`
launches an entrant once and lists one directory, so its `child_pss_kb` column is the first row of
that table and moves by about 50 KiB. This is a session property, not a cold-start one. **The
2026-08-30 field run confirmed it**: the whole branch, this lever plus the shared table parse's
0.11 MiB, moved that column by **0.17 MiB on the scale fixture** (5.00 MiB before, 4.83 after,
identical in all three batches) **and 0.16 MiB on the media fixture** (1.76 before, 1.60 after).
Do not go looking for the 4.81 MiB in that column.

Corroborated on the direct wire with the same THP setting, medians over interleaved pairs:
`big, big, small` ends at 3145 against 1449 (1696 KiB, 12 of 12 pairs, every delta 1628 to 1756);
`big, small, big, small`, which is the sequence the GUI produces, ends at 1631 against 1407.5
(223.5 KiB, 8 of 8) and reads 8851 against 4911.5 while the second large listing is still held;
`big, small` ends at 1475.5 against 1418 with the arms overlapping, which is why the first round of
this task measured that sequence and recorded a null.

**The lever cannot cost time on the first large listing and is a small win on the second, which is
what was measured and is neither a null nor a clean sweep.** `mmap` faulting pages in one at a time
is a different profile from a warmed heap and could have cost something, so this was chased rather
than assumed.

**On the first large listing the two arms are provably allocation-identical**: the same threshold is
in force in both and the smaps probe returned byte-identical mappings, so the lever cannot be acting
on that listing at all and no difference appearing there can be caused by it. A difference does
appear, 0.26 to 0.33 ms slower pinned, medians 25.75 ms plain against 25.99 ms pinned, and **it is a
build-layout artefact rather than a behavioural one**: it localises entirely in the `sort` phase,
1.95 to 1.99 ms unpinned against 2.18 to 2.26 ms pinned, in every batch **including both self-pair
controls**, an arm measured against a copy of itself. A difference that survives a self-pair control
is not caused by the change under test.

**The per-pair sign split is a range across batches and not a constant, and every majority favours
the plain arm**: 6 to 6 in the first batch, then 9 of 12 and 11 of 16 in two later batches of the
same comparison. That is stated plainly rather than softened, because it is not what the conclusion
rests on: those are counts on the first-listing row, which is the row the arms cannot differ on, and
the self-pair control above is what discounts them.

**On the second large listing, the only listing where the arms genuinely differ, the pinned arm is
FASTER**: by 0.923 ms in 11 of 12 pairs, against a self-pair control on that same row of 4 of 8 and
a median of -0.119 ms. The earlier medians agree in direction, 24.29 ms plain against 23.74 pinned.
The whole `big, small, big` sequence is 0.647 ms faster pinned, and the earlier three-listing
sequence reads 50.28 against 50.11 ms.

**So there is no speed-for-memory trade to rank against the 4.81 MiB**, and what time movement there
is points the same way as the memory.

**The zero-warning gate is only a partial guard on `src/heap.rs`.** Deleting the `mallopt` call
makes `cargo build` emit five warnings, an unused import plus four never-used items, and the gate
fails on those. But a wrong parameter number or a wrong threshold value warns about nothing and
silently LOSES the 4.81 MiB above, which is why `src/heap.rs` now carries its own test.

**The guard is a re-exec.** `a_freed_large_block_does_not_ratchet_the_mmap_threshold` spawns the test
binary again behind `FLEA_HEAP_PROBE`, ratchets the threshold the way a first large listing does with
one 8 MiB allocation and free, and asserts a 1 MiB allocation landed outside `[heap]` in
`/proc/self/maps`. A fresh process is what makes it not flaky: a heap already holding a free chunk of
that size would answer from the arena for a reason that has nothing to do with the lever. 0 red in 50
runs in each profile.

**The 64 KiB control is what makes it a test at all.** libtest runs a test on a spawned thread even
under `--test-threads=1`, glibc hands that thread its own mmapped arena, and nothing a secondary
arena serves is EVER in `[heap]`, so the probe would have passed for the wrong reason for ever. The
child therefore runs with `MALLOC_ARENA_MAX=1`, which makes `arena_get2` reuse the main arena, and
asserts a below-threshold 64 KiB allocation is INSIDE `[heap]` before it asserts anything else. That
tunable sets `mp_.arena_max` alone and does not touch `no_dyn_threshold`, which the deleted-call
mutation proves rather than asserts.

**The parent asserts the child said `1 passed`, not only that it exited 0.** `--exact` against a
renamed test filters to nothing and libtest reports success on an empty run, so without that second
assertion a rename would leave the guard vacuous and green. Proved by renaming the function and
leaving `CHILD_TEST` behind: exit status 101, "the probe child ran no test, so <the test's path> no longer names one".

**What it catches, measured by mutation on 2026-08-30 and not claimed.** RED for the call deleted,
for an undefined parameter number, and for the threshold raised to 4 MiB. GREEN, so NOT covered, for
the threshold lowered to 64 KiB, because a 1 MiB probe is above both values: what it pins is the
interval `(65536, 1048576]`. GREEN, so NOT covered, for the pin moved after the large free, because
`mallopt` sets the threshold explicitly and so UNDOES a ratchet rather than only preventing one. The
ordering `run.rs` depends on is about memory already given to the arena and is still held up by the
comment there and by this section, not by a test. And it cannot redden for `M_TRIM_THRESHOLD` or
`M_MMAP_MAX`, which freeze the ratchet by accident at a default that happens to equal the pinned
value.

**Why `mallopt` in `backend::run` and not `GLIBC_TUNABLES` in `ui/Backend.qml`.** Both work and
both were measured head to head over ten interleaved pairs, 1453.5 KiB against 1450, three and a
half KiB apart with the distributions fully overlapping. `Process` on Quickshell 0.3.1 does carry
`environment` and it does MERGE rather than replace: with the map set, the backend's
`/proc/<pid>/environ` holds the tunable and still holds `HOME`, `PATH`, `XDG_RUNTIME_DIR`,
`WAYLAND_DISPLAY` and `FLEA_BIN`, 26 variables in all, verified live. `mallopt` wins on reach
rather than on numbers: it applies however the backend is started, including `tests/protocol.sh`,
`tests/thumbs.sh` and any second front end, whereas the QML property covers only the process the
GUI spawns; and it keeps an allocator decision in the module whose allocation pattern was measured
rather than in the layer that is meant to be strictly a display. `src/gui.rs` is wrong for either
form, because a tunable set before `cmd.exec()` lands on `qs` as well, whose allocation pattern is
nothing like the backend's and was never measured.

**There is no return value to check.** glibc 2.44 here answers 1 from `mallopt` for every
parameter number, including ones it does not define, measured at `-3`, `-1`, `-99` and `3`, so a
test on the result could not redden on a wrong constant and would be a test that cannot fail. The
evidence for this lever is the interleaved measurement above, taken through the real window with a
negative control, and not a runtime check.

For the backend, `malloc_trim` was considered and not taken: a call site inside the listing path
would reclaim at a moment we choose what pinning the threshold already reclaims by itself. The TUI
does call it, through `heap::give_back` after a picture is sent, for the reason given earlier in
this section.

## A finished listing keeps no slack, but the slack was never resident

`Listing::trim` shrinks the name arena and the span vector to their lengths, called from
`scan::scan_in` once readdir ends, from `picker::filter_listing` after its `retain`, and from
`iomount_listing::scan_dir` after `outline::expand` has added the child rows. Cloning and growing
are over by all three points, so what each discard leaves behind is room nothing will use again.

**Why bother, when the pin above already returns an arena to the OS.** The pin decides where a
block lives, and both blocks cross 128 KiB on a large directory, so they are `mmap`'d and their
untouched tail is not resident. Trimming them therefore does not move `Pss`: 27.4 MiB against
27.4 MiB over three cold runs per arm on the 100,000-file fixture, distributions overlapping. What
it moves is address space. `smaps` on a live backend holding that listing reads the arena at
`4100 KiB size, 2336 KiB rss` and the spans at `1028 KiB size, 784 KiB rss` untrimmed, against
`2336 KiB size, 2336 KiB rss` and `784 KiB size, 784 KiB rss` trimmed: 2.0 MiB of mapped region
gone, at identical `Rss`.

**The sequence where that becomes resident is the unpinned one**, behind any caller that lists
without `backend::run`'s pin, or any build that loses it: a second large listing after the first was
freed, served from the heap because the first free ratcheted `mmap_threshold` past the arena's own
size. Standalone, building, freeing, building again and reading `VmRSS` three times per arm: the
trimmed arm holds 5796 KiB, which is its used bytes, on all three runs, and the untrimmed one ranges
5692, 5796 and 7636 as the allocator does or does not hand it back freed pages. With the threshold
pinned both arms read 3120. The backend binary that `tests/protocol.sh` drives pins, so it collects
nothing from this; a caller that lists in its own process does.

**The trim does not cost the read it sits in.** Eight cold runs per arm through the real wire on a
quiet box, medians: read 116.7 ms trimmed against 118.95 untrimmed, sort 22.77 against 22.52, and
both distributions overlap each other across their whole range. Both blocks stay above the
threshold after shrinking, so glibc serves the shrink with `mremap` rather than a copy.

**Two tests, both proven red by deleting one call each.** `scan::exact_tests`'
`a_finished_listing_holds_no_capacity_past_its_rows` fails with `names keep no slack, left 1048576,
right 439`; `picker::tests`' `filters_keep_directories_and_matches_beyond_the_first_window` fails
with `the 149 filtered rows leave no slots behind, left 4096, right 2`. The names a removed row
leaves in the arena stay, because the kept spans index into them.

**The span itself is the larger half of the same waste, at 4 bytes a row.** A `Span` was two `u32`
fields and two `bool`s, which the alignment rounds to 12 bytes; the length now rides in a `u16` and
the two flags fill the padding, for 8. That is 400 KiB of the span vector gone at 100,000 rows.
`len` cannot be a `u8`: a name is at most `NAME_MAX`, but a search hit and a gvfs row carry a whole
path, and `push_entry` asserts in debug that the name it stored reads back whole. Three call sites
summed `off` and `len` in `u32` and now cast to `usize` instead (`sort.rs`, `search.rs`,
`picker.rs`). `listing.rs`' `a_span_is_eight_bytes_and_the_symlink_flag_costs_none` goes red on the
old layout, and `scan::exact_tests`' `SPAN_BYTES` pins the 8 with it.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
