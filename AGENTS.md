# Flea

The fastest GUI file manager on Linux, keyboard first, native to Omarchy.
P0 is the local browser, and remotes, search and disk operations have since landed on
top of it. Encryption and Flea's own terminal interface are later phases and are not in
this tree yet: `flea --tui` says so and exits 2.

## The five load-bearing rules

1. **Every per-file operation stays scoped to the viewport.** Icons, MIME sniffing and
   thumbnails all stay inside the visible rows. The whole margin over the field is that
   the competition stats every entry in the directory and Flea stats only the ones a
   screen can hold. Measured: the field stats 100,000 entries, Flea's first window
   stats 350. `ui/Pane.qml` owns the viewport-sized held window and calls
   `ui/Backend.qml`'s `window(start, count)` request only when that window drifts.

2. **The QML model is an integer count, never a list.** `ui/Pane.qml` binds its
   `ListView` to `model: root.total`, so it instantiates and recycles only the delegates
   the viewport needs. A `Repeater` over a fetched window was tried first and dropped
   220 of 480 frames, because swapping the window destroys every delegate it held.

3. **`list` answers with the first screenful, unasked.** `ui/Backend.qml` sends one
   `list` request and emits its `listed` and initial `rows` replies; `ui/Pane.qml`
   consumes those replies as a pair before accepting the new held window. Making the
   client ask for it separately costs a full round trip that lands mid first-paint.
   Measured: 64 ms if asked for afterward, 4 ms riding along with the listing.

4. **Prewarm stays disabled until it is both safe and faster.** The `flea --prewarm`
   producer exists, but the proposed UI reader measured a 485 ms median against 439 ms
   for the live path. After its removal, ignored experimental generation measured
   469 ms against 437 ms live. Production `ui/shell.qml` intentionally calls only
   `pane.open(start)`, and `tools/flea-first-paint` preserves the comparison. The reader
   was rejected as slower and stale-capable; it remains disabled until the wire carries
   the requested path and a new measurement proves a real win.

5. **Vulkan now, lazy multimedia later.** `ui/shell.qml` sets
   `QSG_RHI_BACKEND=vulkan`, which costs 2.4x less memory than the OpenGL default and
   initialises 35 ms faster, with identical frame timing. Preview and QtMultimedia
   are now in the tree and the laziness held: `ui/PreviewMedia.qml` is the only file
   that imports QtMultimedia, reached through a `Loader` built by the first press of
   play, because QtMultimedia costs 20 MB before it plays anything.

6. **A hidden view is not a free view.** `visible: false` does NOT stop a QML view doing
   model work: it keeps its geometry, stays bound to the listing, and pays per row. All
   three views were instantiated unconditionally and hidden with `visible:` alone, and
   the cost was measured at **110 bytes a row with one view instantiated, 232 with two
   and 262 with three**, on the 100,000 file fixture. Gating the two non-default views
   behind `Loader`s was MEASURED to recover 14.7 MB at 100k and take the listing from 264
   to 147 bytes a row, and was then **REVERTED and is not in the tree**. The repair needs
   `Pane.qml` split first, because two `Loader`s inline exceed the 400-line hard cap, and
   that split broke `tests/ui.sh` to 20 of 30 and then 27 of 30: worse after a fix is the
   signal to stop. It is a deferred ticket with a measurement attached rather than a
   shipped change. The recovery figure is also a list-view number and not a whole-product
   one, taken on a build whose other two views did not render. **The same fact
   had already appeared as a keyboard bug** and been patched at the symptom: a hidden
   view holding focus swallowed the keys the visible one should have had, which is why
   `focus:` is toggled explicitly beside `visible:`. A hidden view that can steal a
   keystroke is a hidden view that is fully alive.

7. **The media bracket got a real rival on 2026-09-03, and two columns felt it.** Measuring
   `strata`'s work properly moved it from unranked into the table, and Flea's placements moved
   with it: memory went from fifth of six to **sixth of seven** at 112.6 MiB against strata's
   **70.3 MiB**, and the CPU lead, which read 3.61x over `nemo`, is now **1.28x over strata**,
   1.42 s against 1.82 s. Both columns Flea wins are still won, settled listing first of six and
   CPU first of seven, and the 100,000 file bracket did not move at all. The operator ruled this
   acceptable with an optimization pass to follow, so it is a deferred ticket and not a defect.
   Item 6 above is the first place to look: its `Loader` gating was MEASURED to recover 14.7 MB at
   100k and is not in the tree, and memory is the column that actually lost a place. Read its
   caveat first, because that figure is a list-view number rather than a whole-product one, and
   the split it needs broke `tests/ui.sh` twice.

## Two-phase listing

Phase 1 (`scan.rs`) reads only names and `file_type()`, which is backed by `d_type` in
the `getdents64` buffer libstd already read to produce the directory iterator, so
knowing whether an entry is a directory costs nothing extra. Phase 2 (`meta.rs`) calls
`symlink_metadata()`, either for the rows one `window` request names, through
`stat_range()`, or for every row in the directory when a sort needs it, through
`stat_all()`. Sorting by name and directory-first works entirely on phase-1 data;
sorting by size or modification time runs `stat_all()`, one `lstat` per row split across
`available_parallelism()` scoped threads, and `metasort.rs` then orders an index against
the result and gathers the spans behind it. Those stats live for the one request and are
dropped with it, so a listing in name order carries nothing extra and `Span` stays 12
bytes. A key that is none of the three, `kind` and `mode` included, reaches neither
phase: `sort.rs`'s `parse_sort_by()` refuses it with the one sentence that names the
three it accepts.

## Why the listing is an arena

`Listing` (`listing.rs`) holds one `String` with every entry's name written back to
back, plus a `Vec<Span>` of `{off: u32, len: u32, is_dir: bool}` indexing into it. A
100,000-entry directory then costs one allocation for the names and one for the spans,
instead of 100,000 individual `String` allocations. Sorting moves 12-byte `Span` values
(the compiler pads the struct to 12 bytes for `u32` alignment), never the name bytes
themselves, so `sort_by_name` never touches the arena at all. Offsets are `u32`, so the
arena tops out at 4 GiB of names, which no directory reaches but a later search phase
reusing this arena could.

**The comparator is macOS Finder's, not a byte compare, and it costs about 2.3x.** Three properties
apply per segment as two names are walked together: a run of digits compares by value so `file_2`
comes before `file_10`; letters compare case-insensitively so `Apple` sits beside `apple` instead of
in an upper-case block above it; and leading zeros do not change a value, so `file_01` and `file_1`
are equal and fall through to a tie-break. **The tie-break is a raw byte compare and it is not
optional**: case-insensitivity makes `README` and `readme` equal, and a stable sort would then
preserve readdir order, which is not reproducible across runs.

A digit run is compared without parsing, because a 40-digit run overflows every integer type: skip
leading zeros, longer remaining run wins, then compare the digits as bytes. **corner: ASCII only.**
Finder also collates a diacritic beside its base letter, which needs a collation table, and this
tree ships zero dependencies with `Cargo.lock` byte-identical as a gate, so a byte above `0x7f`
falls back to byte order, which for UTF-8 is code point order. A name that is not UTF-8 never
reaches the comparator at all: the arena is a `String` and `scan.rs:16` already converts lossily at
the readdir, with its own corner tag there.

Measured on this box, release build, seven runs each, medians: **100,000 rows 2.02 ms to 4.62 ms,
2,000 rows 0.225 ms to 0.586 ms.** So the listing hot path pays about 2.6 ms once on the largest
directory this product is benched against, which is inside the noise of a settle time measured in
hundreds of milliseconds. Zero allocation per comparison: `u8::to_ascii_lowercase` inline, no
`String`, nothing collected.

**The name sort is stable on purpose, and `sort_unstable_by` is a measured LOSS here.** Plan 5's
Task 5b made the substitution, proved it output-identical, and reverted it on time.

The order argument, for whoever revisits this. The comparator's key is `(is_dir, name)`, it
returns `Equal` only when both fields match, and POSIX gives each entry in a directory a unique
name, so on names that are valid UTF-8 the order is total and the substitution cannot change the
output. The one hole is that `scan.rs` pushes `file_name().to_string_lossy()`, so two distinct
non-UTF-8 names in one directory can collide on the same lossy string and tie; see the `scan.rs`
entry under "Deliberate corners". That hole is harmless, because two rows that tie there carry
the same name, the same `is_dir` and the same failed stat, which makes them byte-identical
objects on the wire, so no permutation of them is observable. It was exercised rather than
argued: a fixture with three file names and two directory names colliding on two lossy strings
sorted identically under both. **The output was byte-identical in all ten comparisons taken**,
four trees plus that fixture, ascending and descending, 100,000 rows down to 7, with the only
difference anywhere being the `rows` line's own `ms` stat-timing field, which is a measurement.

**The reason to revert is time.** btrfs returns `readdir` in creation order, which is largely
name order, so the stable sort's run detection is what this product's directories actually hand
it. The 100,000-file fixture comes back in 5 ascending runs and `sort_by` costs a 2.09 ms median
against `sort_unstable_by`'s 14.50 ms, twelve interleaved pairs with the arms alternating and no
run of either arm overlapping the other. The media fixture (401 ascending runs over 2001 entries)
reads 0.227 against 0.243 ms and `/usr/lib` (388 over 4734) 0.698 against 0.727 ms, nine pairs
each. **Those two are a not-a-loss and not a win**: the medians favour stable by 0.016 and
0.029 ms, but the distributions overlap heavily, media's A maximum of 0.289 sitting above B's
minimum of 0.228 and `/usr/lib`'s B minimum of 0.668 below A's minimum of 0.689. The conclusion
rests on the scale fixture and the shuffled control alone. Only a deliberately shuffled
20,000-file directory (9989 runs)
favoured unstable, 3.64 against 5.08 ms, and nothing on this box produces that shape by itself.
The 600 KiB of stable-sort scratch that motivated the change never showed up either: peak RSS
moved 4 KiB on the scale fixture, because the scratch reuses pages the arena's own growth had
already faulted in.

**The 7x magnitude is doubly fixture-dependent, and the direction is not.** Beyond the run count,
every name in that fixture shares the 15-character prefix `some_file_name_`, so each comparison
chews 15 identical bytes before it can diverge. Both arms pay that, but the arm doing several
times more comparisons pays several times more of it. A directory of short, early-diverging names
would narrow the ratio without changing its sign.

## The listing arena returns to the OS

`backend::run` calls `heap::pin_mmap_threshold()` before anything else, which is one
`mallopt(M_MMAP_THRESHOLD, 131072)` through an `extern "C"` in the idiom `src/thp.rs` already
uses. **It is worth 4.8 MiB of the backend child's memory while a second large directory is on
screen, and it costs nothing measurable in time.**

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
leaving `CHILD_TEST` behind: exit status 101, "the probe child ran no test, so CHILD_TEST no longer
names this one".

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

`malloc_trim` was considered and not taken. It is a second `extern "C"` plus a call site inside the
listing path, spent to reclaim at a moment we choose what pinning the threshold already reclaims
by itself.

## Prewarm

`flea --prewarm <path> <first> <dest>` (`launcher/prewarm.rs`) runs the same scan, sort
and stat-range as the backend's `list` command, then writes exactly the two lines
`--backend` would have printed to stdout, a `listed` line and a `rows` line, to `dest`
instead. This producer and its secure file contract remain available for measurement,
but production ignores `FLEA_PREWARM`: the rejected reader was slower and the file
carries no requested path with which to reject stale content (rule 4 above).

## Predictable path writes

`dest` sits in a shared runtime directory, so its path is guessable and could be
pre-planted as a symlink. `write_prewarm` writes to `<dest>.<pid>.tmp`: it unlinks any
leftover of its own first, then opens with `O_CREAT | O_EXCL` so a planted path cannot
be followed, and the pid in the temp name keeps two concurrent launchers off each
other's file. The file is created at mode `0o600` in the `open()` call itself, never
chmod'd afterward, so there is no window where it is readable by anyone but the owner.
`fs::rename` runs last, so a reader only ever sees a complete file or none at all. On
any failure, only the temp file is removed; `dest` is left exactly as it was, because
unlinking a caller-supplied path is not this function's job even when that path came
from a broken caller. The exit status is the contract: a caller must check it before
trusting whatever `dest` currently holds.

## Modes

`main.rs` dispatches on argv before anything else runs, but only `--backend` is fully insulated
from the flag parsing below: it is matched anywhere in argv and always wins. `--prewarm` and
`--open` are matched only in their exact well-formed shape, `args.len() == 5` and
`args.len() == 3` with the flag in argv[1], so a MALFORMED one is not caught here at all. It
falls through to the parsing below and leaves by the unknown-flag branch, which names the flag
and exits 2; `flea --open` with no path and `flea --open a b` are both that case. The looseness
predates this branch for `--prewarm` and this branch extended it to `--open`. `--open` takes exactly one path and exits with the whole of its contract: `0` is a
successful handoff, `2` is anything that could not be opened and carries one elided
sentence, and `3` means the resolved target is a directory and carries no output at all. A
directory is refused rather than handed on because `xdg-mime query default inode/directory`
here is `org.gnome.Nautilus.desktop`, so handing one to `xdg-open` from inside a file
manager opens a different file manager; the caller navigates instead. See "Opening a file".

`--default` and `--default off` are matched the same way, in their own exact shape
(`args.len() == 2`, and `args.len() == 3` with `args[2] == "off"`), dispatching to
`defaults::claim()` and `defaults::release()`. Unlike `--prewarm` and `--open`, a malformed
`--default` does not fall through to the unknown-flag branch: a third check catches any argv
with `args[1] == "--default"` that matched neither shape and names its own usage error,
`--default takes nothing, or off`, before exiting 2. `claim()` refuses and writes nothing when
Flea's own desktop entry, `com.thisisgm.flea.desktop` (`defaults::DESKTOP_ID`), is not installed
under `$XDG_DATA_HOME` (or `~/.local/share`) or any of `$XDG_DATA_DIRS` (default
`/usr/local/share:/usr/share`): the packaged entry is the proof the pacman package landed, and
pointing `xdg-mime` or Hyprland's bindings at an uninstalled binary would be a claim on nothing.
Past that check it rewrites two independent per-user files through `userfile::replace_file` (see
"Predictable path writes"): the `inode/directory` MIME default via `xdg-mime`, and the
additive, markered block `hyprkeys::claim()` adds to Omarchy's `~/.config/hypr/bindings.lua` for
the two file-manager chords. `--default off` reverses both, each half a no-op when it was never
claimed; either half's failure is reported without blocking the other, see `defaults::report`.

What remains chooses between the terminal interface and the window with two
booleans, `want_tui` and `want_gui`, not an enum: there are four modes total, each dispatched
exactly once, and a type nobody matches on twice would be ceremony.

`--tui` and `--gui` are mutually exclusive; giving both is a usage error naming the conflict,
never a coin flip. With neither flag, the choice is `interactive`: both stdin and stdout must
be a real terminal, not just one, so that a pipeline never receives the terminal interface.
`flea | head` gives stdin a tty and stdout a pipe, so `interactive` is false and the window
branch runs; what the rule prevents is the terminal interface writing escape codes into that
pipe, which is exactly what the `--tui` refusal below it says. `--tui` without a terminal on
both handles exits 2; `--gui` without `WAYLAND_DISPLAY` or `DISPLAY` refuses rather than
trying and failing inside `qs`.

Telling that `&&` apart from an `||` needs a tty on exactly one handle and no suite here has
a pty, so that distinction is untested; the no-flag case in `./tests/modes.sh` pins only
which branch the default takes.

`paths::ui_dir()` finds the UI the same way `FLEA_BIN` finds the backend binary (see "Where
the backend binary comes from" below): `FLEA_UI` first, then the packaged
`/usr/share/flea/ui`, then the dev tree's `ui/` found by walking up three parents from the
running binary (`target/debug/flea` to the repository root). Each candidate is confirmed by
checking for its `shell.qml` before being accepted, and an empty `FLEA_UI` is rejected before
that check because `PathBuf::from("")` would test the working directory, so a stale or empty
`FLEA_UI` falls through instead of handing `qs` a broken path.

`gui::exec_qs` calls `Command::exec`, replacing this process instead of spawning a child, so
`flea` never leaves an orphaned pid behind and signals sent to it reach `qs` directly. It also
calls `prctl(PR_SET_THP_DISABLE)` immediately before that `exec`, because the setting is
preserved across `exec` and this is the last point that can hand it to `qs`; see "Transparent
huge pages" below for what it is worth and what it cost.

## Module map

- `main.rs` dispatches on argv: `--backend` runs the command loop, `--prewarm <path>
  <first> <dest>` writes the prewarm file, `--open <path>` hands one file to the desktop's
  handler, `--default [off]` claims or releases the OS-level default, and anything else
  picks the terminal interface or the window by the `--tui`/`--gui` flags and the tty
  state, see "Modes".
- `paths.rs` resolves the UI directory and whether a display is available.
- `gui.rs` execs `qs` against the resolved UI directory.
- `thp.rs` the one `prctl(PR_SET_THP_DISABLE)` declaration, `disable()` and `enable()`.
- `open.rs` hands one file to `xdg-open`, see "Opening a file".
- `defaults.rs` claims or releases the OS-level default: the desktop-entry install check,
  the `inode/directory` MIME default via `xdg-mime`, and reporting each half, see "Modes".
- `hyprkeys.rs` adds or removes the additive, markered block in Omarchy's
  `~/.config/hypr/bindings.lua` that binds the two file-manager chords to Flea, see "Modes".
- `userfile.rs` resolves `$HOME` and `$XDG_CONFIG_HOME` and rewrites a per-user file through
  an exclusive temp plus rename, see "Predictable path writes".
- `error.rs` the one error type, naming the failing operation and input.
- `json.rs` the whole of this tree's JSON: read one named field out of a line, escape one string into one.
- `backend/mod.rs` declares the forty-six backend modules plus the test-only `testdir.rs`, nothing else.
- `backend/listing.rs` the arena-backed `Listing`.
- `backend/aliases.rs` resolves a MIME alias to its canonical name, see "MIME aliases".
- `backend/scan.rs` phase 1: readdir plus `file_type()`.
- `backend/sort.rs` parses the sort key, holds the name order, and holds the one sentence a key it does not know answers with.
- `backend/meta.rs` phase 2: stat a row range for a `window`, or every row for a size or date sort.
- `backend/metasort.rs` the size and date orders: sorts an index over the metadata pass, then
  gathers the spans behind it, see "Two-phase listing".
- `backend/owner.rs` resolves a uid to its login name from `/etc/passwd` alone, for the meta reply.
- `backend/mime.rs` resolves a file name to a MIME type from the shared globs2 database.
- `backend/icons.rs` resolves a MIME type to a freedesktop icon name from generic-icons.
- `backend/md5.rs` computes the thumbnail cache's filename digest, see "Thumbnail cache".
- `backend/thumbspec.rs` parses the `.thumbnailer` files on the search path into a table of
  validated specs, see "Thumbnailer specs".
- `backend/thumbargv.rs` turns one spec plus one input into the exact argv a child is exec'd
  with, see "Thumbnailer specs".
- `backend/thumbcache.rs` names and reads entries in the shared thumbnail cache, see
  "Thumbnail cache".
- `backend/thumbwrite.rs` creates the temp, stamps the PNG and writes the fail marker.
- `backend/sandbox.rs` wraps a thumbnailer's argv in bwrap and prlimit, see "Thumbnail sandbox".
- `backend/child.rs` runs one argv under a deadline and says whether it succeeded, failed or
  never started, which is the whole of what decides a `fail/` marker, see "Thumbnail pool".
- `backend/thumbs.rs` the bounded, cancellable thumbnail pool, see "Thumbnail pool".
- `backend/proto.rs` the wire types, the request dispatch and the one-line responses.
- `backend/rows.rs` serialises one window of rows and its per-response Kind dictionary.
- `backend/thumbreq.rs` the thumbnail request policy: cache lookup, queueing, cancel and
  result reporting, see "Thumbnail requests".
- `backend/run.rs` the command loop, see "Thumbnail requests". stdin is read on its own
  thread and the pool answers on its own channel, and a forwarder thread joins the two, so
  client requests and worker results arrive on one `recv`, because `std` has no `select`.
- `heap.rs` pins glibc's mmap threshold for the backend, see "The listing arena returns to the OS".
- `launcher/mod.rs` re-exports `prewarm`, nothing else.
- `launcher/prewarm.rs` writes the listing and first screenful before the UI starts.
- `ui/shell.qml` owns the pragmas, window, startup path and read-only IPC seam.
- `ui/Backend.qml` is the only QML component that talks to the Rust child, and carries
  `thumb` and `thumbcancel` out and `thumbed` in alongside `list`, `window` and `sort`.
- `ui/Theme.qml` owns the singleton palette, type and spacing tokens from the Omarchy
  theme plus the user override.
- `ui/Pane.qml` owns one directory view, its integer model, held window, actions, the
  settle timer and the row-to-thumbnail map, see "Thumbnail requests in the GUI".
- `ui/PaneWire.qml` is where every reply from outside the window lands: the backend's, and
  those of the three foreign programs the pane runs (`ui/Opener.qml`, `ui/ShareLink.qml`,
  `ui/Taildrop.qml`); it owns no state and writes only through the pane handed in.
- `ui/Header.qml` renders the column header band and its rule, and owns nothing else: it
  was lifted out of `Pane.qml` at the 400-line hard cap and has no behaviour.
- `ui/Row.qml` renders one row delegate: the icon slot, which a thumbnail replaces in
  place, the PlainText name and the semantic colours.
- `ui/Opener.qml` is the only component that launches a foreign program, by running
  `flea --open`, see "Opening a file".
- `ui/ContextMenu.qml` is the one pane-owned right-click popup and its single Open action.
- `ui/StatusBar.qml` renders the path, row counts and transient messages.
- `ui/TabBar.qml` is the window's tab strip, hidden with no height until a second tab exists.
- `ui/ChromeBar.qml` renders the top chrome, and owns the path bar: the same strip typed into
  rather than drawn, opened by `:`, `Ctrl+L` or a double click on the path.
- `keys.toml` is the one key table, and `tools/flea-keymap-gen` turns it into `Keymap.js`.
- `ui/js/Keymap.js` is the generated key-to-action lookup and imports no QML.
- `ui/js/Format.js` is the pure size, date and permission formatter.
- `ui/js/Errors.js` turns a backend failure into the one sentence the status bar shows, and
  imports no QML; it was lifted out of `Pane.qml` at the hard cap and has no behaviour of
  its own.
- `ui/js/Thumbs.js` is the pure row-to-path map and the visible-row request plan, and
  imports no QML so `./tests/js.sh` can redden on a mutation.
- `ui/js/Tabs.js` is the directory-tab snapshots and the t/w/1-9 policy, and imports no QML. A
  snapshot carries a cursor and a selection only across a switch that re-lists nothing, because an
  index names a row and a re-read can put a different file behind the same number.
- `ui/js/Trash.js` is the dd pair's arm-and-fire policy, split out of `Focus.js` at its cap.
- `ui/js/Scale.js` is the interface scale's step, clamp and sentence; `ui/ViewState.qml` stores it
  and `ui/Theme.qml` multiplies its own tokens by it, so no surface reads the chord itself.
- `ui/js/PathBar.js` is what a typed path line means: the tilde, the relative name, the
  `file://` URI, the interior `.` and `..`, and what Tab makes of one directory's names. Pure,
  so `tests/js/pathbar.js` drives all of it; the field itself is `ui/ChromeBar.qml`'s.

## Where the backend binary comes from

`ui/Backend.qml` spawns the child as `[Quickshell.env("FLEA_BIN") || "flea", "--backend"]`.
A relative path cannot be right, because `Process` resolves it against the working
directory of the `qs` process rather than against the config directory, and `qs` is started
from several different directories here, including a non-interactive ssh whose cwd is
`$HOME`. Measured with `command: ["./target/release/flea", "--backend"]` and `qs` started
from `$HOME`: the child never ran, and the log read `Process failed to start, likely
because the binary could not be found. Command: QList("./target/release/flea",
"--backend")`.

`workingDirectory` does not rescue it. It can only relocate a relative path that is wrong
in the packaged layout anyway, where the binary installs to `/usr/bin/flea` and the UI to
`/usr/share/flea/ui/`, so no single working directory makes `./target/release/flea`
correct in both layouts.

The default is therefore the bare name `flea`, which is exactly what a packaged install
puts on PATH, and `FLEA_BIN` is the development seam that points at `target/release/flea`
in this tree. Every script that starts `qs` for a test exports it. It is a deployment seam
and not a tuning knob: a user never sets it.

## Every component needs a qmldir line

`ui/qmldir` is the type authority for that directory, and **the qualifier decides whether a
reference goes through it**. An unqualified name resolves only through the qmldir, so a bare
`Backend` is a type only because a line declares one, however correct `Backend.qml` itself is.
A namespace-qualified name does not: under `import "." as Flea` an undeclared sibling still
resolves by file name, so `Flea.StatusBar` loads `StatusBar.qml` with no qmldir line at all.
`shell.qml` carries both `import "."` and `import "." as Flea`, so both spellings are in scope
and only the qualified one falls back to the file on disk. Deleting the `Header`, `Row` or
`StatusBar` line therefore does nothing whatever with the caches cleared, not an error and not
a warning, while deleting the `Backend` line breaks the load outright: `StatusBar` and
`Backend` are both used from `shell.qml`, and the qualifier is the only thing separating them.
Adding `ui/Backend.qml` alone left the config refusing to load, and the entire message
was `ERROR: Failed to load configuration` followed by `caused by @shell.qml[25:13]: Backend
is not a type`, which names the line that uses the type and says nothing about the qmldir.
Deleting that one line from a working tree reproduces it exactly and putting it back clears
it.

**Every component added to `ui/` still needs its own line**, `<Name> 1.0 <Name>.qml`,
alongside `singleton Theme 1.0 Theme.qml`. The qualified form loading undeclared is not a
licence to skip it: a singleton has no unqualified fallback and `Theme` is referenced bare
everywhere, and the first bare reference anyone writes to an undeclared component fails the
whole config load with a message that points at the use and never at the qmldir.

## Where the UI patterns come from

`ui/ContextMenu.qml` is a plain overlay `Item`, not a Controls `Popup`. It fills the pane, sits
at `z: 1` above the list, and stays invisible until `openAt` places its frame and sets `opened`.
A backdrop `MouseArea` gives close-on-press-outside and the action's `Keys.onPressed` gives
close-on-escape, which together are what the popup's `closePolicy` used to provide. `Pane.qml`
reads `menu.opened` and never `menu.visible`, because a full-pane overlay's visibility is not the
menu's state.

That was the only Controls import in the tree, and it cost 10 to 12 ms of warm startup and 2.2 MB
of PSS, measured across nine interleaved pairs and slower in 9 of 9. Dropping it moved the warm
product path from about 205 ms to about 193 ms, faster in 16 of 18 pairs over two nine-pair runs
with the arms swapped between them.

Closing a popup restored focus to the item that had it for free. The overlay does not, so `openAt`
records the sibling holding active focus and `close` hands it back. Without that the list stopped
answering the keyboard after Escape, which is exactly what `./tests/ui.sh menu` catches by pressing
a cursor key after Escape. The pointer convention the menu answers is
`/usr/share/omarchy/shell/Ui/WidgetButton.qml:94-116`, where left and right arrive through one
handler and the button decides. No QML is imported from `shell/Ui`: the pointer convention was
copied, not imported. `qs.Commons` is a different matter: not a public API, but already
load-bearing tree-wide (Theme, NetworkDialog, SidebarRow, ContextMenu and Preview all import it
for Style and Color), so an `omarchy update` reshaping it is a standing, tracked risk rather
than a rule any one file breaks.

## How the list renders

`ui/Pane.qml`'s `ListView` sets `clip: true` because the top row of a wheel-scrolled viewport is
only partly inside the list, and the rest of it draws over the column header: the cursor's accent
band, its edge bar and the row's filename all landed on top of the header labels and the header
rule. `tests/ui.sh cursor` counts the accent pixels in the header band to hold that closed.

**It also sets `reuseItems: true`, and that is safe here only because `ui/Row.qml` has no
`Component.onCompleted`.** Every property the delegate draws is a binding that reads `index`, so a
row leaving the small `cacheBuffer` is re-bound rather than destroyed and rebuilt. Imperative
setup in the delegate does NOT re-run on reuse, so anything added there later has to move to a
binding or to `ListView.onReused` or this line has to go. Measured in Plan 5 Task 5a on the
100,000-file fixture, nine interleaved pairs each driving 2400 wheel detents then 400 single-row
`j` steps: the fling cost 1.60 s of process CPU without reuse and 1.46 s with it, reuse winning
every one of the nine pairs, and the row-by-row steps cost 0.94 s against 0.72 s, again nine pairs
out of nine, about a quarter less on the churn case reuse exists for. Input to rows for one `G`
jump, stamped inside the UI, went from a median 39 ms to 37, reuse winning eight pairs and tying
one. It is not free, and the direction reverses here: peak PSS after that fling rose from
76,146 KiB to 77,299 KiB, reuse the WORSE arm in all nine pairs, about 1.1 MiB of pooled
delegates, `pss_kb` over 1024. **That cost does not reach either
field column**, because the benchmark samples PSS from launch to settle and never scrolls:
first-paint PSS moved by a median 0.16 MiB on the scale fixture over five interleaved pairs, with
per-pair deltas of 16 to 626 KiB against a 455 KiB spread inside the base arm, and by 0.06 MiB on
the media fixture over nine, where reuse is the higher arm in 6 of 9 and the lower in 3, and the
57 KiB median-to-median sits well inside the base arm's own 727 KiB spread. Call it a fifth of a
megabyte at worst where the fling costs 1.1.

The correctness hunt was the point rather than the timing. A delegate that failed to re-bind shows
the wrong icon on a row, so the check pairs delegate state against model state through the
read-only seam: `rowIcon(i)` is what the row draws and `thumbFile(i)` is what the pane holds for
that index, and they must agree for every instantiated row. A purpose-built fixture of 100 jpegs
and 100 text files interleaved by name puts both kinds on every screen. Zero violations across
eight dumps, with and without reuse, after a hard fling and after scrolling out past the buffer and
back to the top. The dumper and the comparison both live at `~/bench/t5a-rows.sh` and
`~/bench/t5a-checkrows.py`, outside the repo, so the next session re-runs them rather than
rebuilding them.

**The comparison was proved able to fail twice, and the second control is the one to copy.** A
mutation of a saved dump, swapping two rows' drawn icons and relabelling a thumbnailed row as
`.txt`, reddens it with three violations. Stronger, and run live on the box: an arm whose delegate
takes `thumb` in `Component.onCompleted` instead of binding it, which is exactly what a real reuse
break looks like, produced **21 violations across the 21 thumbnailed rows of the screen while the
qs log still reported `LOGCLEAN`**. **The log check alone would never have caught a live reuse
break**, so anything that touches delegate reuse runs this comparison and not a clean log.

## The list model

Task 8's registered decision rule, written before any measurement was taken through the product
path: ship `ScriptModel` plus Qt's documented `ItemSelectionModel` only if BOTH an interleaved
fling on the scale fixture drops no more frames than the integer model does within the
run-to-run spread AND the scale `Pss` regression sits inside a 1 MiB budget; otherwise keep
`model: root.total` (rule 2 above) and hand-roll selection as an index set. A windowless probe
had already measured `ScriptModel` holding 100,000 integers at about 3.7 MB more `Pss` than the
same process holding nothing, so the prediction, registered in advance, was decline on memory.
This section is that prediction checked against the real application rather than a probe.

**Two scratch UI trees, one shared binary, because the model lives only in QML.** Arm I is the
shipped `ui/` unmodified, `ui/List.qml`'s `model: pane.total`. Arm S patches `ui/List.qml` to
`model: rowModel`, a `ScriptModel` filled through one `setRowCount(n)` function that assigns `[]`
before the new array, because a single disjoint replace of 100,000 values diffs quadratically
(about 294 s in the windowless probe; clear-then-fill there cost 17 ms). Both arms launched
`flea --gui` against the same `target/release/flea`, so only `FLEA_UI` differed. Nine interleaved
pairs on the scale fixture (`/home/flea-sandbox/flea-bench-btrfs`, 100,000 files), `Pss` read from
`smaps_rollup` in KiB after settling to a 256 KiB band held for three 300 ms samples, order
alternated pair to pair:

| | Arm I median (KiB) | Arm S median (KiB) | delta |
|---|---|---|---|
| Pss | 76,308 | 78,064 | **+1,756 KiB, 1.71 MiB** |
| Pss_Anon | 37,400 | 39,052 | +1,652 KiB |
| USS (Private_Clean + Private_Dirty) | 54,988 | 56,704 | +1,716 KiB |

Arm S cost more `Pss` in **9 of 9 pairs**, fully disjoint from Arm I in every one (per-pair delta
567 to 1,925 KiB, no overlap). This box permanently holds a resident `quickshell`, which is the
sharing condition `Pss` divides against.

**Ruling: `ScriptModel` is declined on memory**, per the registered rule: 1.71 MiB median against
a 1 MiB budget, over budget in 9 of 9 pairs. The fling half of the gate was not run. The ship rule
needs both halves to pass and this half already fails on its own with no overlap across nine
pairs, so a second box-time measurement on the scale fixture cannot change an outcome the first
one already settled; this project already used the same reasoning to stop the row-mark gate once
its own decisive axis (the toolchain) made the rest of that measurement moot. `ui/List.qml` keeps
`model: pane.total`, unchanged. Task 9's selection is a hand-rolled index set in
`ui/js/Selection.js`, not `ItemSelectionModel`.

## File budget

`tools/flea-file-budget` scans `src`, `ui` and `tests` for `.rs`, `.qml` and `.js`
files. Rust and QML get a 250-line soft budget and a 400-line hard cap; JS gets 200
soft and 300 hard. Going over the hard cap fails the tool; going over the soft budget
only warns. The budget is a smell detector, not a target. Every count below is
`wc -l` on the file, and every test-module count runs from its `#[cfg(test)]` line to
the end of the file; run the tool rather than trusting these if the two disagree. **Three of them
had gone stale by a whole plan and were re-derived from `wc -l` in Plan 5 Task 5a**, so when you
touch a file here, re-derive its count from the artefact rather than adjusting the nearest
number.

**Every count in this section is a SNAPSHOT, not a live figure, and eleven of the eighteen had
drifted by 2026-09-01: `src/heap.rs` was claimed at 15 and is 100, `ui/Row.qml` at 166 and is 310,
`ui/Theme.qml` at 264 and is 187.** Nothing was over the hard cap when that was checked; only the
record was wrong. So read the numbers below as the reasoning's own context, never as the current
size of anything, and run `tools/flea-file-budget` for what a file measures today. The warning
directly above this paragraph was already there and was violated eleven times in the paragraphs
beneath it, which is why this one states the failure rather than repeating the instruction.

`src/backend/proto.rs` was 241 lines when this was written, under both budgets: 71 are the wire types,
the request dispatch and the four one-line responses, the other 170 the test module. It has
been split twice, each time at the seam that leaves each file a single job. **At 399 of the
400 hard cap** `src/json.rs` took the whole of this tree's JSON, the field scanner as well
as the escaper: the scanner is protocol-agnostic by construction, the decoding half of the
encoder already living there, and the same mutations still redden the four tests that moved
with it. **At 489, after the dirsize and Kind wave pushed it through the cap**, the rows
serialiser moved out to `src/backend/rows.rs`: every other response is one `format!` line,
and the one that loops, allocates and owns the Kind dictionary was the file's whole growth.
The old rule that every consumer imports the wire layer from `proto` alone went with it:
`run.rs` and `prewarm.rs` now import `rows_line` from `rows` and everything else from
`proto`, the same shape as the `thumbspec`/`thumbargv` split. Ten serialiser tests and the
literal-database `dbs()` helper moved with `rows_line` unchanged; the request and one-line
response tests stayed.

`src/backend/rows.rs` is 251 lines by `wc -l`, one over the soft budget and well under the
hard cap: 88 are `rows_line` and its Kind dictionary, the other 163 the test module. It is
one job: serialise one window of rows, with the per-response dictionary that sends each
distinct Kind string once and an integer per row. `kind_for` lives here because the
dictionary is its only caller.

`src/heap.rs` is 15 lines by `wc -l`, well inside both budgets: two named constants, one
`extern "C"` declaration and one call, in the same shape as `src/thp.rs`.

`src/json.rs` is 187 lines, under both budgets. 101 are the scanner and the escaper and 86
are the test module. It is one job in two directions: pull one
named field out of one JSON line, and escape one string into one. Nothing here builds or
validates a document, because the wire is one object per line and never anything else.

`src/backend/thumbspec.rs` is 312 lines, over the soft budget and under the hard cap: 190 the
discovery, the desktop-entry parser and `is_runnable`, the other 122 the test module. Those
three are one security boundary and are read together, so they stay in one file: splitting the
validation from the exec line it validates would be the wrong cut. **`argv` moved out**, to
`src/backend/thumbargv.rs`, when this file reached 398 of the 400 hard cap. That is a different
job: the table answers "which validated program declares this type", and `argv` answers "what
exact argv does that program get for this input". `argv` reads `Spec.exec` and nothing else here,
and the seam is where the two-URI rule lives, which is easier to see in a file that holds only
the substitution. Every test in the table file builds its fixture through the one-line
`one(body)` helper rather than repeating a seven-line `from_entries` call.

`src/backend/thumbargv.rs` is 121 lines, well inside both budgets: 24 the substitution, 97 the
test module. **That ratio is four to one and it is right**, not a smell: this file is a security
boundary, and every placeholder, the refusal, the absolutisation and both spellings of the input
need a real file, a real symlink and a real directory on disk before they prove anything. A
reader arriving without that context will read the ratio as top heavy; it is the cost of testing
a boundary against the filesystem rather than against a mock.

`src/backend/child.rs` is 231 lines by `wc -l`, inside both budgets and so not one of the eight
files the tool warns about, with its `#[cfg(test)]` at line 96, so 95 lines of implementation and
136 of tests. It runs one argv
under a deadline and reports `Ran::Succeeded`, `Ran::Failed` or `Ran::NotStarted`. It came out of
`thumbs.rs` at 397 of the 400 hard cap, and it completes a three-part story each of whose parts is one file:
`thumbargv` builds the inner argv, `sandbox` wraps it, `child` runs the result. Nothing in it
knows about thumbnails, which is why the pool's `JOB_TIMEOUT` stays in `thumbs.rs` and is passed
in.

`ui/Pane.qml` is 398 lines by `wc -l`, over the soft budget and 2 lines under the hard cap. It stood at
exactly 400 of 400 and could not gain a line, which is why `ui/Header.qml` came out of it
first and alone, before any behaviour was added; it then took on the settle timer, the
thumbnail row map, the opener wiring, the input-to-rows stamps and the first-screen settle, and
still ends under the cap. **There is almost no room left**: the next change of any size lifts a helper into
`ui/js/Thumbs.js` or takes another band out the way the header went, and does not lift the
cap. It stays one
component because the integer model, held window, list-reply pairing, focus and action
dispatch share one state owner; splitting them would add a state boundary in the exact
path that prevents untagged backend replies from crossing directory navigation.

`ui/Header.qml` is 70 lines, well inside both budgets. It is the column header band, its
labels and its rule, lifted out of `Pane.qml` whole and carrying no behaviour of its own. The
cut shipped as its own commit exactly so that no behaviour change could hide inside a move.

`ui/js/Errors.js` is the second such cut, made for the same reason and shipped the same way.
`Pane.sentence` was already a pure function of two strings with no QML in it, so it moved to
`ui/js/` whole and gained the suite every other file there has, `tests/js/errors.js`, which is
what makes the move provable rather than merely asserted.

`ui/Row.qml` is 166 lines, under both budgets. It gained the icon `Image` and its
`sourceSize`, the two read-only aliases the icon checks assert through, and `iconSource`,
which answers a thumbnail URL when the pane holds one for this row and the OEM two-step icon
lookup otherwise.

`ui/Opener.qml` is 41 lines, well inside both budgets: one `Process`, one `open()` and two
signals. `flea --open` decides and exits in milliseconds, so one process serves every open,
and this is the only component in the tree that launches a foreign program.

`ui/js/Thumbs.js` is 77 lines by `wc -l` against a 200-line soft budget, and its suite
`tests/js/thumbs.js` is 66. It is arithmetic over the row map and nothing else, with no QML
import, which is what lets `./tests/js.sh` mutate it and watch a check redden.

`src/backend/thumbwrite.rs` is 280 lines, over the soft budget and well under the hard cap: 179
the temp creation, the chunk writing and the marker, the other 101 the test module. It went over
the soft budget when `write_stamped` gained the strip pass that keeps a published entry to one of
each key it owns, and the test that pins it walks the chunks twice, once first-match and once
last-match, because agreeing is the whole property.

`src/backend/thumbcache.rs` is 312 lines, over the soft budget and well under the hard
cap. 176 of those are the test module, which carries its own on-disk fixtures: `lookup`'s
ordering and its `Thumb::MTime` guard are both invisible to a pure-function test, so they
are proved against real files written under a per-pid temp root through `Cache::at`. The
implementation stays one file because the URI builder, the digest-named paths and the
`tEXt` reader are three parts of one cache key: a change to any one of them silently stops
matching entries the rest of the desktop wrote, and the tests that pin that against a real
entry have to see all three.

`src/backend/run.rs` is 327 lines by `wc -l`, over the soft budget and under the hard cap, and has
no test module at all: it is the command loop, the dirsize walk and the window writes, and every
one of its behaviours is proved through the real binary by `tests/protocol.sh` and
`tests/thumbs.sh`. The two small structs it carries, `Tables` for the four databases read
once per process and `State` for everything the loop mutates, exist so a handler takes six
arguments instead of the twelve the flat form reached; their fields are `pub(crate)` because
the thumbnail request policy in `src/backend/thumbreq.rs` is the same loop's other half. **It
reached 451 of the 400 hard cap in the dirsize wave and was split there**: this file already
described it as "the command loop and the thumbnail request policy", two jobs, and the cut is
that sentence.

`src/backend/thumbreq.rs` is 133 lines by `wc -l`, inside both budgets and, like `run.rs`,
without a test module: `tests/protocol.sh` and `tests/thumbs.sh` prove it through the real
binary. It is the thumbnail request policy alone, the per-row cache lookup, the queue-once
map, cancel and result reporting, moved out of `run.rs` whole. Nothing in it touches the
event loop, and `run.rs` keeps `forget_rows` because a listing change invalidates dirsize
state too, not only thumbnails.

`src/backend/thumbs.rs` is 393 lines by `wc -l`, over the soft budget and under the hard cap. Its
`#[cfg(test)]` is at line 250, so 144 of those are the test module, and that is where the cost sits: proving cancellation without a
race needs a worker pinned inside a real child, and the round trip through a real thumbnailer,
the real sandbox and the rename is the only test that proves the subsystem end to end. The
implementation is the other 249 lines, after the PNG writing and the temp creation moved out to
`src/backend/thumbwrite.rs` and running one child under a deadline moved out to
`src/backend/child.rs`. What is left is the queue, the worker and one job's lifecycle, and it
stays one file because the queue, the pop and `cancel` share one invariant: splitting the worker
from the queue it pops would put the two halves of that invariant in different files.

`ui/Theme.qml` is 264 lines, over the soft budget and under the hard cap. Theme and
user-override parsing plus palette and token application stay together as the single
theme owner. Splitting its pure parsers is deferred because that change needs its own
automated regression coverage.

## The key table is generated

`keys.toml` at the repository root is the single source of truth for every binding.
`ui/js/Keymap.js` is generated from it by `tools/flea-keymap-gen` and must not be hand
edited: change `keys.toml`, run the tool, commit its output.

`docs/images/glyphs.svg` is generated the same way, by `tools/flea-glyph-sheet` from the `PATHS`
table in `ui/js/Icons.js`. It has no diff guard, so re-run the tool whenever a mark joins or
leaves that table: the sheet kept drawing `send` twelve minutes after `e4b4911` took it out, and
never drew `drive`, `filter` or `lock` at all.

Three guards cover it, and each catches what the others cannot. `./tests/keymap-gen.sh`
regenerates into a temp file and diffs that against the committed one, so an edit to
`keys.toml` with no regeneration and a hand edit to `Keymap.js` both turn it red, and the
diff names the stale file either way. The same script then hands every `Qt.Key_*` the
generator emitted to a `qml6` probe and fails naming any that is `undefined`, because a
mistyped `key = "Downn"` generates `key === Qt.Key_Downn`, which is false forever and diffs
perfectly clean. `tests/js/keymap.js`, run by `./tests/js.sh`, asserts the lookups
themselves, so it catches a binding wired to the wrong action; it is a hand-maintained list,
so it says nothing about a binding nobody wrote an assertion for.

The reasoning the hand-written `Keymap.js` used to carry lives in `keys.toml` now, one
comment above the block it explains: control-modified keys are checked first, so plain `d`
can mean cut while ctrl-d pages, and the character is authoritative for letters, so shift-g
is distinguishable from `g`. The generated file carries only the header saying where it came
from.

A `[[shift]]` table joined this for Task 9's shift+arrow selection extend. Unlike `[[ctrl]]`,
whose block returns `""` on no match so an unbound ctrl-combo never falls through to a plain
binding, the generated `[[shift]]` block has no such return: a shift-modified letter is
already distinguished by its own capital text (the `G` idiom above), so an unmatched
shift+key needs to fall through to the ordinary key and text switches rather than being
swallowed. The table exists only to give the arrow keys, which carry no text at all, a
shift-modified meaning.

The generator's block parser is a plain `while read` loop over `name = value` lines, not the
multi-statement `awk` the plan sketched, because the project's code rules send a
multi-statement `awk` back for exactly that rewrite and the loop came out shorter and needs
no second language. Do not restore the `awk`.

A `[[pointer]]` table joined this on 2026-09-02, when a single left click stopped opening a
row and the second tap started to. It declares what each button, modifier and tap count does
in the listing, in the columns view's two neighbour columns and in the rail, and the generator
emits it as `Keymap.POINTER`. `ui/js/Tap.js` is the only code that decides any of it and
`tests/js/tap.js` drives every listing and neighbour row of the table through that file, so
neither side can move without the other. `tests/ui.sh` case `click` then drives real clicks at
the window, which is the half a JavaScript suite cannot reach: it is what says a delegate hands
`Tap.tapped` the tap count and the modifiers the click actually carried.

Its effect field is `does` and not `action`, and that is load bearing. `tools/flea-acceptance`
derives its whole key checklist with one `sed` for `^action = ` over this file, with no table
scoping, so an `action =` in a pointer block would put an item on that checklist that no key
can press. The generator refuses an empty or unreadable `[[pointer]]` table for the same
reason `[digits]` is expanded by hand in that battery: a checklist derived from an empty table
passes by having nothing in it.

The tool emits JavaScript only. Plan 6 adds the Rust output together with the terminal key
type that consumes it; a generated module with no caller is dead code, so the second output
waits for its consumer.

## Testing

- **The warning gate is all four cargo invocations**, not two: `cargo build`,
  `cargo build --release`, `cargo test` and `cargo test --release`, each after a `cargo clean`,
  each expected to print zero warnings. `cargo build` cannot see anything inside `#[cfg(test)]`,
  so a two-invocation gate hides every unused import and every dead helper in a test module. Two
  of them lived here for three review rounds, one of them predating the whole thumbnail plan,
  because the gate said "build" and nobody ran the other half.
- `cargo test` runs the unit tests inside every module.
- **`./tests/run-all.sh` is the one command, and it exists because nothing executed any suite
  at all.** Before `96186ff` this tree carried twelve suites, no runner and no CI: every
  cross-reference to a suite, here and in `README.md` and in tool and source comments, was
  prose naming it rather than a line running it, and `PKGBUILD`'s `check()` runs
  `cargo test --release` alone, which it still does. **`96186ff`'s own message is wrong about
  this and cannot be rewritten, because the branch is shared:** its subject says nine suites
  were uninvoked and its body says seven, and the derived answer is zero of twelve. The runner
  builds both cargo profiles, since `protocol.sh` drives the debug binary and `thumbs.sh` the
  release one, runs the nine suites that need nothing but a shell, and reads each suite's OWN
  exit code, never a pipeline's. It then names `ui.sh`, `drag.sh` and `bench.sh` with what each
  needs, so a suite it cannot run stays visible instead of being forgotten a second time.
- **`./tests/drag.sh` is the internal drag's characterisation suite, 9 checks**, and it has to
  be run by hand: no runner invokes it. It was written against the drag's behaviour BEFORE the
  platform-drag rewrite, so it is the net that catches what the rewrite changes, and it earned
  that immediately: the first cut of the rewrite failed R3, `expected [kept] got [GONE]`,
  because `Drag.active = true` runs a nested event loop in which **the window receives no key
  events at all**, so the `Keys.onPressed` handler carrying ctrl never fired and every
  ctrl-copy silently became a move. The file still arrived, so nothing looked wrong.
- **It drives the pointer through uinput, never `omarchy-drive drag`**, which cannot drive a Qt
  client at all: it interpolates through `hl.dsp.cursor.move`, which emits `wl_pointer.motion`
  with no `wl_pointer.frame`, and Qt dispatches buffered pointer events only on `frame`. A drag
  test written on that tool passes vacuously.
- **Its coverage is 3 of 4 races and the fourth is named rather than assumed.** Sabotaging
  `canDrop` to accept file rows still passes 9 of 9, because the backend refuses a non-directory
  `dest` independently, so that QML guard can break invisibly; and the race it covers needs a
  stolen grab, which a synthetic pointer cannot produce. The rewrite arguably retires that race,
  since the compositor now owns the drop decision, but that is reasoning and is not counted as
  coverage.
- `tools/flea-field-bench` is the cold-cache comparison against the installed field and the
  instrument behind `docs/baseline-2026-08-30b.md`. **Run it from a copy outside this tree**, because
  it launches the tree it measures; `~/bench/` is where it runs here. It needs the sudo password on
  stdin for `drop_caches`, and it launches Flea through `$FLEA_BIN --gui`, never `qs` directly:
  measuring Flea by a path no user takes cost a whole field run, since `qs` alone misses the
  `prctl` in `exec_qs` and reads 104 MB against the real path's 45 MB.
- **The field was re-taken cold on 2026-08-30 against the honest settle detector and
  `docs/baseline-2026-08-30b.md` is the current one.** It supersedes `docs/baseline-2026-08-30.md`,
  which supersedes `docs/baseline-2026-08-29-plan5.md`. Two things changed under it and both move
  numbers: the settle signal now includes children, and `tumbler` was installed on the box.
- **The settle signal is no longer one process's `utime + stime`.** A run is settled when, for a
  whole 500 ms, neither the watched set's `utime + stime + cutime + cstime` nor the summed
  `utime + stime + cutime + cstime` of its whole LIVE descendant tree has changed. The watched set
  is the entrant plus its declared helper, and `flea` is the only entrant declaring one
  (comm `flea`, token `--backend`). The descendant walk is breadth first and runs only on a poll
  where the watched set earned no tick, because it costs a read per thread and per descendant.
  **`cutime` alone is not enough**: it holds only what a watched pid has REAPED, so a decode still
  running is invisible to it, and Flea's decode is two levels down under `bwrap`.
- **`cpu_tree_s` is a new column at the END of each row**, so an old parser still works. It carries
  the watched set's tree total; `cpu_s` still reports the entrant process alone. On the media fixture
  they diverge hard: pcmanfm reads 38.83 against 90.00 and dolphin 3.29 against 68.59.
- **`thumbs_by_format` is a newer column at the END of each row**, and it is why a count can be
  compared at all. A thumbnailer with no plugin registered for a MIME type never attempts the file
  and writes no failure marker, so a silent skip and work-not-done are the same zero in a total.
  The column reads `jpg=601;png=189;webp=0;heic=0;unknown=0`, one entry per extension the fixture
  holds, zeros included, because `webp=0` is the capability claim and a missing row reads as
  untested. `unknown` counts a produced key the fixture cannot explain, which means the cache was
  not clean and the row is not comparable. The key sets themselves land in `<out>.keys/`, so a run
  can be classified after the fact; a count cannot. `tools/flea-bench-keys` builds the map and does
  the classification, and it refuses a fixture holding any name that would need percent-encoding,
  because a shell cannot reproduce the backend's GLib literal set and a wrong key would report
  every entrant as producing zero. Measured against dolphin on a five-file probe: 5 files, 5 new
  keys, `jpg=3;png=2;unknown=0`, which is what says the derivation matches a real thumbnailer and
  not merely itself.
- **`ranked` is the last column and it is the one a report must read first.** `yes` is a
  comparable row. `unranked: nothing reached the thumbnail cache so its work was not
  measured` is a GUI entrant that finished a media run having left that cache empty, and its
  `thumbs_n` and `thumbs_by_format` then read `unmeasurable` rather than 0. **That is a failed
  measurement and never a capability claim**, because a cache count cannot tell an entrant that
  drew nothing from one that persists nothing: strata renders each thumbnail into a scratch
  directory it deletes on drop and keeps the image in memory, and its 0 here was published as
  "strata thumbnails nothing" in three artefacts for a whole release while it drew six of the
  eight formats offered. The raw cache reading survives in `thumbs_by_dir`, which is a true
  statement about the cache and a false one about the entrant. `tools/flea-bench-capability` is
  the instrument that answers ability, and it measures an entrant named in its `TRANSIENT_ID` by
  live watch instead of by cache count. The harness names the refusal on stderr the way it names
  an absent entrant and keeps going, because one unmeasurable entrant is not a reason to lose the
  other thirteen. `n/a: tui in <terminal>` is the whole TUI bracket: a
  TUI previews the cursor file and never fills a grid, and the terminal decides whether it can draw
  an image at all, so those rows carry `n/a` in `thumbs_n` rather than a 0 that reads as slow.
- **`tools/flea-field-bench` measures its own `TRANSIENT_ID` by the same live watch, so such a row
  is ranked rather than refused.** Two watches, because neither answers both questions: an
  `inotifywait` on `/tmp` counts each `result.png` closing under `TRANSIENT_PREFIX`, and an
  `inotifywait` on the fixture directory names the file that earned it, because bwrap binds the
  input into the helper's namespace without moving the dentry and the open still reports against
  the fixture's own watch. Both are armed above `drop_caches`, so the walk's own cache warming is
  dropped with everything else, and both are read at the moment `count_thumbs` would have run, so
  the count and the timing come from one pass. strata thumbnails one file at a time, so an open and
  the render it earns are paired: the run refuses its own numbers if the two logs differ by more
  than the one render in flight. The cache reading still lands in `thumbs_by_dir`, and a 0 from the
  live watch is a real 0 rather than the cache's silence.
- **`ONLY=<entrant>` scopes the kill list as well as the run.** A full-field run closes every file
  manager on the box by name and refuses to start beside one, which is correct for a field run and
  wrong for re-measuring one row on a box somebody is using: the operator's own windows are not the
  harness's to close. With `ONLY` set, both the kill list and the start-up refusal cover only that
  entrant's own processes. A re-measured row is a row from a different day and the manifest says so
  in its own words, the way `docs/bench/media-rc-2044.manifest.md` does for strata.
- **Two rules for the report, which the harness cannot enforce.** The thumbnail count prints beside
  every timing number in every table, not only in the CSV: a settle time without its count is not
  citable for this field. And where two counts differ by an order of magnitude the row says so
  rather than implying a comparison, because Flea thumbnails the viewport by design and dolphin
  thumbnails the directory, which on the media fixture was 36 against 790.
- **The manifest opens before the first entrant and closes after the last.** The open block carries
  the versions, the fixture counted from itself, the thumbnailable denominator with the extension it
  excluded named, the load average, and the payload assertion recorded as having passed rather than
  only its number. The close block carries what the run turned out to be: each entrant's format
  capability stated from what it produced rather than inferred from the library table, and every row
  the harness refuses to rank, including a bracket-wide refusal when the ranked rows differ in
  thumbnail work by ten times or more. On the media fixture the denominator is **1800**: 2000 visible
  entries less 200 `.txt`. It is not 2008; that count came from a seeded fixture an interrupted run
  left behind, and a rebuild produces 2001 with the marker.
- **The TUI bracket runs as-intended, in kitty, on the bench's own configuration.** foot is sixel
  only while every entrant here that previews at all prefers the Kitty protocol, so the old
  bracket's `thumbs_n 0` across all eight was the harness's terminal choice and not a result about
  the entrants. `tools/flea-bench-tui/` is the `XDG_CONFIG_HOME` handed to every TUI entrant, copied
  into a `mktemp -d` per run so the operator's own `~/.config` is neither read nor written and the
  repo never collects the state files entrants leave. Provenance per entrant is in that directory's
  README; the manifest records the materialised files verbatim, because lf reads `previewer` as a
  path and does not expand a variable in it, measured, so the run substitutes the real path in.
  **Verified live, one screenshot each: yazi, ranger, lf, nnn and superfile all draw an image.**
  mc, broot and xplr have no image preview feature at all, which is a capability finding and not a
  configuration gap.
- **`preview_ms` is the TUI bracket's number, and a thumbnail count is not.** A TUI draws the cursor
  file in a pane and never fills a grid, so it is timed to its first preview instead. It runs in its
  own launch after every other number is taken, so the settle pass never pays for a screen capture,
  and it points the entrant at a named fixture image, because the name that sorts first here is a
  `.mp4` and almost nothing previews a video. The signal is **unique colours** in a band of the
  screen: text is a handful of palette entries however much of it there is, a photograph is tens of
  thousands. Two consecutive polls confirm it, so a torn frame mid-redraw cannot pass.
  Measured on the real 2000-file fixture: no preview reads broot 6, mc 472, ranger-without-a-preview
  536, xplr 538; a preview reads yazi 121598, spf 420753, nnn 431030, lf 447285. The threshold of
  5000 clears the busiest text UI by 9x and the smallest real preview by 24x.
  **Two earlier signals were wrong and both were caught by running an entrant with no image preview
  feature at all.** Hashing the band tripped the moment any TUI painted, so xplr reported a preview
  at 694 ms. PNG compression density then separated them on a three-file directory and did not
  survive the real fixture: xplr's file table over 2000 entries reads 163 against yazi's 307, under
  2x. **The entrant with no feature is the control this metric needs, and it has to run on the
  fixture the bench actually uses.**
- **`ranger` is launched with `--selectfile` for the preview pass, and only ranger.** A bare file
  path makes ranger open the file with its configured opener and exit, so its window was gone before
  the first poll and it scored `-1` while previewing perfectly well on a directory.
- **The measured bracket, end to end on the media fixture:** yazi 745 ms, lf 733, nnn 702,
  superfile 686, ranger 1127; mc, broot and xplr `-1`, which is correct because none of the three
  has an image preview feature at all. Single runs, so read them as magnitudes.
- **An idle box is a run condition and the bench now enforces it.** `require_bench_idle` refuses a
  one-minute load average over `BENCH_MAX_LOAD_HUNDREDTHS`, default 50. 150 was the first value
  and it was measured too loose: a scale run started at 1.10 and its mapped column came out
  205 ms above the previous baseline with nothing in the diff that could account for it. This
  document carried 150 as the rule long after the tool had dropped to 50, which is the mistake
  recorded as the rule. **`cargo test` is a run
  condition too**: `a_trip_partway_through_a_listing_still_carries_nothing` asserts three reads are
  instant against a 100 ms budget, so a suite running inside a bench window fails there and reads as
  an archive regression that is not one. Do not overlap them.
- **The instrument carries a known uniform lag of about 23 ms**, one poll period, on every entrant
  whose descendants earn CPU and are then reaped, and on none without. It moves a settle later,
  never earlier. It is conservative and it ships deliberately; see `docs/baseline-2026-08-30b.md`.
- **`KILL_TARGETS` includes `tumblerd`, and it has to.** tumbler is D-Bus activated through
  `systemd --user`, so it is never a descendant of the entrant that triggered it, it outlives that
  entrant, and it writes into the shared thumbnail cache. A survivor lands inside the next entrant's
  cold arm. `require_bench_idle` reads the same list, so the harness now refuses to start while
  tumblerd is alive; that is the designed behaviour of a shared list.
- **`foot|-e` is a substring match, so any Omarchy TUI window open at start time blocks the field by
  name.** `omarchy-launch-tui` runs `xdg-terminal-exec --app-id=$APP_ID -e "$1"`, whose command line
  contains `-e`, so `require_bench_idle` names it and refuses. Close it, or the run will not start.
- **`EXPECT_FILES` is the payload count and the dotfiles are judged by name, because one count
  cannot do both jobs.** The payload is what the fixture is for and does not move when a marker is
  added: **scale 100000, media 2000, matched 1700**, checked with `ls -U`. Every dotfile is then
  matched against `.flea-*` (a fixture marker) or `.seed.*` (an encoder seed) and a stray is named
  rather than folded into a total. A total was tried and it broke the whole harness the moment
  `.flea-bench-fixture` landed, because a count that must be re-derived by hand after an unrelated
  change is the same defect as a bare `ls` with a different constant.
- **`ls -A` on each fixture is 100001, 2001 and 1701**, one marker each. `tools/flea-media-fixture`
  deletes its seven `.seed.*` encoder seeds at the end of a successful build, so a fixture that still
  carries them was built by an interrupted run: **the resident media fixture had all seven and was
  reconciled to the builder on 2026-09-01**, which is why an earlier note here recorded 2008. The
  builder is the authority, and the `.seed.*` arm of the dotfile check stays so a fixture from an
  interrupted build is still named rather than rejected.
- **The media fixture's visible composition, derived with `ls -U | sed 's/.*\.//' | sort | uniq -c`:**
  600 jpg, 400 mp4, 200 webp, 200 txt, 200 png, 200 heic, 100 webm, 100 mkv. **The thumbnailable
  denominator is 1800**, the 2000 visible less the 200 txt. It does not move with the seeds either
  way, because both terms come from the payload. A classifier run is what will confirm the txt
  exclusion rather than assuming it.
- **dolphin restores a KDE session on every launch and it poisoned every dolphin row ever published
  here.** `~/.config/session/dolphin_dolphin_dolphin` held a tab naming
  `file:///home/flea-sandbox/flea-bench-btrfs`, the 100,000-file fixture, so dolphin listed that in a
  background tab on top of whatever it was being measured on. Measured on a 24-file directory:
  5417 ms settled, 8.99 s CPU and 177714 kB PSS with the file present, against 1625 ms, 0.68 s and
  86678 kB with it parked. **It is invisible on the scale fixture**, where the restored tab is the
  directory under test, which is why nobody caught it.
- **The `thumbs` column counts every size directory under `~/.cache/thumbnails`, not only
  `large/`.** Which directory an application writes to follows the pixel size it asks for, so a
  `large/`-only count reads a structural zero for an entrant that asks for a smaller one. Measured
  on the 2,000-file media fixture: **pcmanfm generates 605 and dolphin 534, both into `normal/`**,
  and both read zero under the old count. `baseline-2026-08-30b.md` reads 606 and 552 for the same
  two, dolphin's rise being its session tab parked. `docs/baseline-2026-08-29-plan5.md` and the two before it
  all carry the uncorrected column. yazi writes to `/tmp/yazi-1000` and never touches the shared
  cache at all. **thunar's zero was `tumbler` not being installed, and `tumbler` 4.20.2-1 was
  installed on 2026-08-30**: it now writes 221 into `normal/` on the media fixture and its settle
  went from 958 ms to about 12600.
  **nemo's `thumbnail-limit` default is 1,048,576 BYTES and applies to every type**, measured with a
  scratch directory and a redirected `XDG_CACHE_HOME`: it thumbnailed both 361,383-byte jpgs and
  both 96,096-byte webps and none of the heic, png, webm, mp4 or mkv above the limit, so its 60 on
  the media fixture are small images and not one of them is a video.
- **The cache count is not what the user sees, and for two entrants they disagree.** Measured on a
  24-file directory holding three of each fixture type, screen against cache: **pcmanfm renders jpg,
  webp and heic and persists none of the three**, 18 on screen against 9 in the cache, so a
  cache-only reading marks it down for work it did. **nautilus shows a play glyph on every mkv** and
  **dolphin shows an image glyph on every heic**, and both of those the cache does agree with. flea
  and thunar are the only two entrants that render all seven media types.
- **`pss_kb()` reads the rollup with `mapfile`, never line by line.** A `while read` loop over
  `/proc/<pid>/smaps_rollup` re-renders the seq_file between lines and mixes fields from two
  instants: measured against a process allocating 512 KiB every 2 ms it produced a USS above its own
  `Pss`, which the kernel cannot produce, in 15 of 120 reads, worst by 186 kB, while `mapfile` gave
  0 of 300 and is about 4.5x cheaper. The `Pss` column was never affected, because it is the first
  field in the file.
- **`cpu_s` is the entrant process's own `utime + stime`**, fields 14 and 15 of `/proc/<pid>/stat`
  and not `cutime + cstime`, so no child's CPU is in it. For Flea that excludes the Rust backend and
  every thumbnailer child, and the column therefore compares front ends and not total work.
- **A PSS row is only comparable to a row taken in the same interleaved run**, and the 4.1 MiB
  that `docs/baseline-2026-08-29-plan5.md` says appeared across Tasks 4 to 7 of that branch is
  mostly this rather than the product. Five interleaved cold pairs of `30b7fde` against `7dfc0d1`
  on the 100,000 file fixture put the real cost of those four tasks at **234 kB** of raw `pss_kb`,
  medians 54740 against 54974, and the per-mapping medians out of `/proc/<pid>/smaps` put all of
  it in `[anonymous]` and the QtQml JS GC heap, which is what one component, one JavaScript
  library and two timers cost. The rest is a level shift between two measurement sessions: that
  baseline's "after the icon column" row was written at 14:03 and its Task 8 before-arm at 18:07,
  and re-measured now the 18:07 row reproduces within 349 kB while the 14:03 row sits 3540 kB
  below what its own tree measures today. Whatever else is resident moves the figure too, because
  Pss divides a shared page by its sharers: the same tree reads 54606 kB alone against 54067 kB
  with one thunar resident, and 54754 kB alone against 53147 kB with thunar, pcmanfm and nemo
  resident, each pair interleaved in its own run, and `QT_QPA_PLATFORMTHEME` is worth 7310 to
  9087 kB of it. Neither names the 3540 kB, which is still unexplained. **The maximum the
  harness records is not the reason**: on this fixture the maximum and the reading taken at the
  settle agree within 1 kB in 13 of 13 launches, and the value still holds five seconds later.
- The `mime` and `icons` unit tests call `load()`, so they read the box's installed
  `shared-mime-info`: a package update, not a code change, is what will redden them.
- `./tests/protocol.sh` drives the built `--backend` binary over real stdin and
  asserts the exact stdout contract, including the prewarm file.
- `./tests/modes.sh` drives the real `flea` binary and asserts the mode contract:
  refusal messages, the mutual-exclusion usage error and its message, the no-flag
  default landing on the window branch, and that `--backend` and `--prewarm` still dispatch. The `--tui --gui` case checks the message
  as well as the exit code, because the no-tty refusal below it exits 2 too, so the exit
  code alone cannot prove the mutual-exclusion guard fired. It cannot exercise the window
  itself, since that needs a graphical session; see "Modes" above.
- The transparent huge page check inside `./tests/modes.sh` puts a stub `qs` on `PATH` that
  prints `THP_enabled` out of its own `/proc/self/status`, so what it asserts is the kernel's
  view of the process `exec_qs` launched and not the return value of a function. It reddens if
  the `prctl` is removed and it reddens if the call fails; a second check asserts the stub
  reported at all, so a broken stub cannot be mistaken for a passing gate. See "Transparent
  huge pages" under "Deliberate corners".
- The `--open` checks inside `./tests/modes.sh` put a stub `xdg-open` on `PATH` that reports
  its own argv and its own `THP_enabled`, which is what pins symlink resolution and the
  handoff. The huge page half needs a second stub, because a bare `flea --open` runs in a
  process where nothing disabled huge pages, so `thp::enable()` is a no-op there and that
  check cannot tell it from an empty function: the paired case launches `flea --gui` against a
  stub `qs` that reports what it inherited and then execs `flea --open`, and only that one
  reddens when the `thp::enable()` call is deleted. Both were measured against a deletion.
- `.process_group(0)` in `open::open` is pinned by the same stub, which prints its own pid
  beside field five of `/proc/self/stat`, its process group, and reports `PGID MATCH` only when
  the two are equal, which they are only after `setpgid(0, 0)`. Deleting the call reads
  `PGID MISMATCH pid=1180968 pgid=884`, where 884 is the invoking shell's group. Deleting it
  also raises an `unused import: std::os::unix::process::CommandExt` warning, but **that
  warning is not the guard**: it disappears the moment anything else in the file needs
  `CommandExt`, while the check keeps working.
- The two `--open` error paths each need a check on the sentence, not just the status, because
  the unknown-flag branch also exits 2 and also prints no errno: against the pre-Task-4 binary
  the status and errno checks of both pairs pass unchanged, and only `could not be opened` and
  `nothing on this system could be asked` tell an implemented `--open` from an absent one.
- `./tests/budget.sh` asserts `tools/flea-file-budget` itself rejects an oversized
  file and passes a clean tree.
- `./tools/flea-acceptance` is the everything-works battery, and **its checklist is derived at run
  time, never written from memory**: every request in `docs/protocol.md`, every action in
  `keys.toml` (the `[digits]` range included, which binds nine keys no `action =` line names), every
  entry in `ui/ContextMenu.qml`, every artboard in the design canvas, every sidebar entry kind, and
  every SUPER chord in Omarchy's own `bindings/clipboard.lua`. A derivation that comes back empty is
  a hard failure, because an empty checklist passes. The 18 requests are **driven live** through the
  real backend, each in a directory of its own because rename, trash and duplicate mutate, and they
  need no display. Keys, menu entries and the Omarchy chords need a real window, so `--drive` opens
  one and presses everything in a single pass: this box has one Hyprland session and several lanes
  queue for it. A bare run reported `18 passed, 0 failed, 95 derived but undriven` at `5bd514d`,
  which is the honest state for a run that pressed nothing. **Cite the commit beside any count
  taken from this tool.** The tally moves with the branch, so a bare number outlives the run that
  produced it: this tool printed 70 undriven at `aa9e482`, then 75 for the four commits from
  `90cd964`, once the keymap lane's Finder chords took the key group from 39 to 44, then 76 from
  `2dc6169`, once New Folder added the twelfth menu row. Every one of those was correct when it was
  printed, which is the whole point: the number did not become wrong, it stopped being current.
  Two rules the driven half keeps: every check reads a
  value before the chord and after it and refuses the case where the before value already equals
  what it is about to require, because an assertion that cannot fail is worse than none; and a key
  whose effect no reader outside the app can see is reported UNDRIVEN with the reason and the one
  line that would fix it, never given a check that would pass on a dead app. Three things its own
  first run taught it: a request answered from a worker needs the
  quit held back or `search`, `dirsize` and `meta` fail as though the product were broken; a shared
  sandbox lets `rename` move `a.txt` out from under `duplicate`; and a cancel's effect is not
  observable on a four-file directory, so the cancels assert that the backend accepted the request,
  answered no error and is still answering, which is coverage of the request and not of the
  cancellation. The canvas lives in the omarchy repo, so a read-only copy sits at
  `$REPO/../flea-canvas/` on the box and the failure names it rather than skipping the group.
- `./tests/bench.sh` gates `tools/flea-bench-manifest`, the version and environment record the
  field bench writes beside its CSV, and the two values the harness derives from its own entrant
  table to feed it. It launches nothing and builds its fixture in a `mktemp -d`. It exists because
  the field bench had no gate at all, and its first check is the defect that found: an entrant row
  arriving indented missed its own case arm, so Flea reported itself as `quickshell 0.3.1`, which
  is what `pacman -Qo` says about its launcher.
- `./tests/keymap-gen.sh` asserts the committed `ui/js/Keymap.js` is byte-for-byte what
  `tools/flea-keymap-gen` produces from `keys.toml`; see "The key table is generated" above.
- `./tests/thumbs.sh` drives the release binary against the media fixture and asserts the
  thumbnail contract end to end: a video row and an image row each generate their own
  thumbnail, a text row answers with no file, every requested row is answered, a second
  request is answered from the cache in under 5 ms rather than generated again, and a
  result for a superseded listing is dropped. It also asserts the four fixes this subsystem's
  whole-branch review found: a fifo is refused at once instead of burning a worker's timeout, a
  file under a symlinked directory still thumbnails, a missing `bwrap` refuses the job rather than
  running it unconfined, and a cancel-all does not strand the rows it cancelled. It needs
  `--release` built, because a debug
  decode is slow enough to make the cache-hit check meaningless. **It writes into the
  operator's real shared cache and removes exactly what it created**, by the same md5 key
  the backend used, and its last three checks are that `~/.cache/thumbnails/large` ended at
  the count it started at, that no `.flea-` temp is left in it, and that no `fail/flea`
  namespace was left behind. **Both counts use `ls -A`.** The temps this subsystem leaks are
  dotfiles, so a bare `ls` cannot see them: the operator's cache showed 94 by `ls` and 104 by
  `ls -A`, and the count check alone stays green against a temp that was present both before and
  after, which is why the explicit `.flea-` check is a separate assertion and not a rewording.
- `./tests/js.sh` runs the pure JavaScript suites through `qml6`: `tests/js/format.js`,
  `tests/js/keymap.js` and `tests/js/thumbs.js`. The thumbs suite is 66 lines against a
  77-line helper and pins the four properties the request policy rests on: `plan` names only
  visible rows carrying `t`, it never re-asks a row the map already holds in any of its three
  states, it drops only rows that are still waiting when they leave the viewport, and
  `remember` evicts at the cap in insertion order. It reddens on a mutation because
  `ui/js/Thumbs.js` imports no QML.
- `./tests/ui.sh` drives the real window through `omarchy-drive` and takes a case name to run
  one of nine: `cursor`, `terminal`, `open`, `menu`, `colour`, `icons`, `thumbs`, `hashcache`
  and `nosweep`. With no argument it runs all nine and then three whole-run checks, a backend
  drain, a log grep and a cache count, so a clean run prints `0 of 12 checks failed`.
  - `open` replaced the old `symlink` case. It puts a stub `xdg-open` on `PATH` and asserts
    all three Enter answers on one listing: a symlink to a file hands the resolved target to
    the handler and does not leave the directory, a symlink to a directory navigates and is
    never handed to the handler, and a broken symlink says one sentence, moves nothing and
    leaves the listing standing.
  - `icons` catches removing the icon slot from `ui/Row.qml` or its theme fallback. Every row
    must answer a non-empty source, a directory and a symlink to a directory must both draw a
    folder, a `.jpg` must draw the image icon, a `.pem` must not draw as an executable, and
    the rendered row pitch read off `itemRect` must still equal `Theme.rowHeight`, so a slot
    that grows its row reddens.
  - `thumbs` catches turning the settle timer into a request per scrolled frame. One screen
    of 200 hard-linked jpegs must cost exactly one request, row 0 must end up drawing a
    `file://` URL out of the cache, the first row past the requested screen must exist and
    must NOT draw one, a fling must add at most two requests and at least one, and the rows
    the cache actually gained must sit between one screenful and the requests times the
    viewport. The pitch is asserted here too, against a thumbnail rather than an icon.
  - `hashcache` catches `encodeURI` in `ui/Row.qml` leaving `#` or `?` literal, which Qt reads
    as a fragment or a query rather than path bytes. It redirects `XDG_CACHE_HOME` into a
    directory whose name holds both and asserts the row's URL carries `%23` and `%3F` and that
    `Image.status` reached `Ready`, because a URL a test can read is not proof Qt could load it.
  - `nosweep` catches any request for a row the client did not name, which is the rule that
    forfeits the project: a fling and a `G`/`g` traversal of the 100,000-file fixture must end
    with the request count at zero, the shared cache the size it started, and no `.flea-` temp.
  It needs `jq` and ImageMagick: the cursor case counts warm pixels in the header band of a
  real screenshot, because a clipping bug is a pixel fact that no IPC value reports. Its sample
  starts 16 px in, since the Hyprland corner arc shows wallpaper through the window's own
  top-left pixels.
- Each case starts a fresh `/tmp/flea.log`, so `launch()` rolls the previous case's log into
  one run log and the end-of-run check greps that. Grepping the live log only ever saw the
  last case, which is how a `TypeError` in an earlier case went unreported.
- `qmllint ui/*.qml`, run from the repository root, is a gate, and it is the only instrument here
  that reads across a file boundary. Three defects in one evening were invisible to reading and
  found only by it: an orphaned `Format.js` import, an unused import in `ui/Pane.qml`, and a
  single-parameter signal handler a refactor left behind after giving the signal a second
  parameter, which would have read the sender as its back flag and tabbed backwards forever.
  That last one is the shape worth remembering: the handler is correct in isolation and wrong
  only against a signal one file away, and the refactor asserted its new form was PRESENT
  without asserting the old form was ABSENT, which is the question a refactor actually asks.
  - **It does not come back clean and it cannot be made to.** `import qs.Commons` does not
    resolve under qmllint, so `Theme.color` and `Theme.spacing` degrade to `QObject` and two
    categories fill with false positives: **`missing-property` 605 and `unqualified` 331**, out
    of 8255 lines total (`tools/flea-qmllint-gate` and `cat ui/*.qml | wc -l` at `9cecda3`; an
    earlier version of this line said 590, 317 and 3601, and 548 and 301 before that). Those two
    track how large `ui/*.qml` is, so a later delta in them alone
    is tree growth, not a regression. Building a `qs` alias tree and passing `-I` was measured
    and is NOT worth the temp directory: `unqualified` falls to 215 and `unresolved-type` clears,
    but `missing-property` RISES to 577.
  - **So the gate is the actionable categories, by name**, which together run under fifty:
    `import` 13, `unused-imports` 0, `signal-handler-parameters` 0, `unresolved-type` 6, and
    `property-override` 0 (the one pre-existing case, `ui/OmarchyMark.qml:13` overriding
    `scale`, left the tree with that file). Every real defect found tonight was in one of these. A
    gate on total silence would be permanently red and would be ignored within a day. A prior
    version of this line also named `index` 12: that count came from the same grep miscounting
    an array subscript in a printed source line, `root.rows[index]`, as a category; `index` is
    not a real qmllint id, and `--json`, which `tools/flea-qmllint-gate` reads, never emits it.
  - **`signal-handler-parameters` is gated at 0 because one exact message is filtered first.**
    qmllint cannot resolve `QProcess::ExitStatus`, so every `Process` `onExited` in this tree
    raises `Type QProcess::ExitStatus of parameter exitStatus in signal called exited was not
    found`, thirteen today, and it raises it identically at 0, 1 and 2 declared parameters, so
    that count was never measuring arity. `tools/flea-qmllint-gate` (`90c5ec2`, corrected in
    `320f840`) drops the findings carrying that message, matched on the message and deliberately
    not on the handler name, so a genuine arity slip on an `onExited` itself still fails; that was
    proven by injecting one there and one on another signal. The thirteen are printed as context
    beside the other two false positives rather than hidden, and the gated count is 0.
  - **Name the blind spot rather than pretending it away:** a genuine `missing-property` defect
    is invisible here, buried in 605 false ones. That is the price of the seam, not a reason to
    skip the instrument, and it is why qmllint supplements the suites rather than replacing them.
  - **The third defect above is a second blind spot, and its sentence over-claims.** Injecting
    exactly that shape into a sandbox copy, `onRenameCommitted: function (idx) { ... }` against a
    two-parameter signal, moved no category at all: every count byte-identical before and after.
    **qmllint flags a handler with too MANY formal parameters, never too few**, because an
    under-declared handler is legal JavaScript. So a signal that gains a parameter and leaves a
    handler under-declared is not caught here, and nothing in this tree guards it. What that
    refactor actually tripped is not recoverable: `aa21bad` landed `tabbed(var from, bool back)`
    with every handler already at two parameters, so the fix happened in the working tree and left
    no commit to read. The shape qmllint can flag, and the one the same refactor could equally have
    produced at the inner `signal tabbed` re-declaration, is the inverse: a two-parameter handler
    on a signal still declared with one. Read the sentence above as the consequence that was
    avoided, not as the category that caught it, and do not cite it as one.
  - **It is not on the non-interactive ssh PATH.** `command -v qmllint` prints nothing and exits 1
    over ssh; the binary lives at `/usr/lib/qt6/bin/qmllint`. A gate written as
    `qmllint ui/*.qml | grep -E '...'` runs that "command not found" through the grep, finds no
    match, and reports clean. `tools/flea-qmllint-gate` is the correct form: it asserts the binary
    at its absolute path before running anything, reads `--json` instead of the text output so a
    printed source line's array-index syntax cannot be mistaken for a category by a bracket grep,
    and checks the actionable categories against a ceiling rather than an exact count, so a fix
    can lower one without turning the gate red.
- `./tools/flea-bench` reproduces the backend half of the acceptance table against a
  100,000-file directory.
- `firstRowsAt()` on that same `IpcHandler` reports the `Date.now()` of the first `rows`
  response that carried any row, as a decimal string. It is what the warm product path is
  measured against: the harness records its own start time, waits for the rows, then reads
  the timestamp once after the fact, so no polling tax lands inside the measurement. It
  returns a string and not a `real` deliberately: a millisecond epoch is a thirteen digit
  double, and IPC formats a double to six significant digits, which would report
  `1.78799e+12`. Both arms of any A/B run must carry it.
- `inputToRows()` on that same `IpcHandler` reports two `Date.now()` instants separated by a
  space: `Pane.inputAt`, stamped at the top of `Keys.onPressed` before the key is dispatched, and
  `Pane.rowsAt`, stamped in `onRows` the first time a window arrives that covers the cursor. Their
  difference is input-to-rows latency on the application's own clock. It exists because the obvious
  harness, press a key then poll `rowAt` until it answers, charges its own cost to the application:
  one `omarchy-drive ipc` call measured 213 ms idle on this box and 287, 323, 389 and 482 ms under
  4, 6, 8 and 12 worker thumbnail bursts, and `ready()`, which returns a constant and reads no UI
  state, cost the same as `rowAt` at every one of those loads. Press the key, wait past the
  interval, then read both instants in ONE call after the fact. A `rowsAt` of `0` is not a sample:
  it means no window arrived, which is a key that needed no fetch. Every key resets the pair, so it
  always describes the last one. Idle it reads 33 to 48 ms with a median of 38 over the 192 baseline
  samples of the eight-round battery.

- The UI has no accessibility tree, so the one `IpcHandler` in `ui/shell.qml` is the seam
  every UI test reads state through. It reports and never acts: a test that could mutate
  state through the seam it asserts on would not be testing the keyboard path. Assert
  through `omarchy-drive wait ipc -p ui <target> <fn> <value> --timeout 0`, never a bare
  `qs ipc call`, which exits 0 on both `Target not found.` and `Function not found.`
- Wrapping the content in a plain `QQuickWindow` instead of Quickshell's `FloatingWindow`
  was tried as a way to get a real accessibility tree, and did not work: the app registers
  on the AT-SPI bus but `omarchy-drive tree` returns a bare `[application] quickshell` node
  with no children. Do not spend the ten minutes again.

Measured on the dev box against 100,000 files on btrfs. The plan predicted roughly 26 ms
read, 2.5 ms sort and 1.5 ms window stat; read and sort landed there and the window
stat came in far faster, so the measured figure is the baseline, not the estimate.

- Warm: read 23.1 to 23.3 ms, sort 2.0 to 2.4 ms, window stat 0.42 to 0.53 ms, 350
  rows in the first window.
- Cold, page cache dropped: read 132 to 164 ms, sort about 2.1 ms, window stat 73.3 ms.
  A range, not one figure: an earlier independent measurement on this box recorded
  146.4 to 165.9 ms for the same work, the same order of variance.
- The cold window stat is the number that shows a cold run really was cold. A warm
  read next to a cold stat means something warmed the directory before the measurement.
- The benchmark refuses to run on tmpfs, because `drop_caches` cannot evict it and a
  cold number there would be a fabrication. `/tmp` on this box is tmpfs, which is why
  the default target moved to `/home/flea-sandbox`, a sibling of $HOME rather than a child of it, per hard rule 9.
- Cold procedure: `sudo sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches'`, then
  `COLD=1 ./tools/flea-bench`. A plain `./tools/flea-bench` run warms the directory
  before it measures, so it cannot produce a cold number; `COLD=1` skips that check.

## Fixtures

There are four, they measure different halves of the product, and **none may live on
tmpfs**. `drop_caches` cannot evict tmpfs pages, so a cold number taken there is a
fabrication. `/tmp` on this box is tmpfs, which is why all three default to `/home/flea-sandbox`, on the same btrfs filesystem as home but outside it, and
all three scripts refuse to build anywhere `findmnt` calls tmpfs or cannot name at all.

| Fixture | Built by | Holds | Measures |
|---|---|---|---|
| `/home/flea-sandbox/flea-bench-btrfs` | `tools/flea-bench`, warm run | 100,000 empty `.txt` files | listing: readdir, sort, window stat |
| `/home/flea-sandbox/flea-media-btrfs` | `tools/flea-media-fixture` | 2,000 real media files, 4.7 GB | thumbnail generation and icon resolution |
| `/home/flea-sandbox/flea-matched-btrfs` | `tools/flea-matched-fixture` | the media fixture minus heic and mkv, 1,700 files, 4.0 GB | the same comparison with every entrant at equal capability |
| a `mktemp -d` of its own | `tools/flea-gvfs-fixture` | a zip mounted through `gvfsd-archive` | the NETWORK rail and anything needing a real FUSE mount, at zero privilege |

The listing fixture contains no media at all, so it cannot exercise a thumbnailer. The
media fixture is small enough that its listing time says nothing. Report both.

The GVFS fixture is the odd one out: it measures nothing and exists so a mount can be driven
without a share, a server or a password. It carries the wedge-stub shapes for `gio` and `lsblk`,
because both rails froze on an unbounded listing and the suite missed it: `case_sharebrowser`
stubs `gio mount -l` to print nothing, and **a stub that cannot hang cannot catch a hang**.

**The third one exists because an entrant that declines a format finishes sooner for it.** heic is
dropped because dolphin renders none and mkv because nautilus and pcmanfm render none, which is
measurement and not guesswork; the 200 txt stay, because every entrant renders zero of them. It is
`cp --reflink=auto` of the media fixture's own files, one to one, so it spans 1,700 distinct
physical copies and a cold read touches each once. Neither it nor the media fixture may be published
alone: the full one is what a user gets, the matched one removes the discount a declined format buys.
**It equalises capability and not work**, because each entrant still chooses how much of the directory
to sweep, so a ratio taken from it carries the same work-rate caveat the media table does.

- The media mix is per 20 files: 6 JPEG, 2 PNG, 2 WebP, 2 HEIC, 4 MP4, 1 MKV, 1 WebM and
  2 plain text. At the default `COUNT=2000` that is 600/200/200/200/400/100/100/200. The
  text files are the long tail, so icon resolution is exercised beside the thumbnails.
- Overrides are `DIR` and `COUNT`. `COUNT` must be at least 20 or the mix drops a format.
- **A media run drops the thumbnail cache as well as the page cache.** A warm
  `~/.cache/thumbnails` makes a generation benchmark measure nothing at all:

  ```bash
  rm -rf ~/.cache/thumbnails
  sudo sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches'
  ```

- Every file is byte-identical on a rebuild. `magick plasma:fractal` is random by default,
  so the seed frame passes `-seed 7`, and `-strip` drops the timestamp chunk that made the
  PNG bytes differ while the pixels matched. `ffmpeg` takes `-fflags +bitexact` so the
  Matroska muxer stops writing a date and a random segment id, and the VP9 seed takes
  `-threads 1`, because row threading makes the encode differ run to run. `-fflags` is a
  per-file option, so it goes before the OUTPUT: on the input it parses, changes nothing,
  and leaves the MKV and WebM seeds differing on every rebuild.
- One seed per format, then plain copies: the build runs seven encoders, not two thousand.
  Per-file `heif-enc` alone would cost 1.6 s times 200. WebM is the one seed a remux cannot
  make, since no WebM container accepts the H.264 stream, so it is a real VP9 encode.
- The copies pass `--reflink=never`. On btrfs a plain `cp` shares extents, and a cold read
  of 400 reflinked clips would hit one physical copy 400 times.
- A build took 84 s, 208 s and 327 s across three runs on this box, and CPU time was 39 s
  in all three, so the spread is time waited rather than time computed and it is not the
  encoders; no mechanism beyond that was established. Deleting a previous fixture is not
  the cause: `rm -rf` of the whole 4.7 GB takes 0.5 s.
- The fixture directory carries a `.flea-media-fixture` marker, and the script refuses to
  `rm -rf` any existing `DIR` that does not have one. That is the only thing standing
  between a mistyped `DIR` and the deletion, so do not remove the marker by hand. It is a
  dotfile, so the fixture lists 2,000 visible entries and 2,001 in total.

### MKV has no thumbnailer until Plan 4 resolves aliases

`mime::Db::lookup` reads `/usr/share/mime/globs2` and returns the canonical name.
`xdg-mime query filetype` on this box does not read that database at all: with `DE=generic`
it runs `file --brief --dereference --mime-type`, so it sniffs content with libmagic's own
name table. `gio`, which does read the XDG database, agrees with `Db::lookup` on all eight
fixture types. Two of them are alias pairs, and they fall on opposite sides:

| Extension | `Db::lookup` and `gio` | `file` and `xdg-mime` | Thumbnailer declares |
|---|---|---|---|
| `.heic` | `image/heif` | `image/heic` | `image/heif`, the canonical name |
| `.mkv` | `video/matroska` | `video/x-matroska` | `video/x-matroska`, the alias |

`/usr/share/mime/aliases` carries both pairs, `image/heic image/heif` and
`video/x-matroska video/matroska`, alias first and canonical second. So no single spelling
matches every thumbnailer on this box, and Plan 4's matcher has to compare the alias
equivalence class rather than the string. Until it does, Plan 5 shows an icon rather than a
thumbnail for MKV. Nothing about this is a defect in `mime.rs`: alias resolution belongs to
the code that matches thumbnailers, which Plan 4 owns.

## MIME aliases

`/usr/share/mime/aliases` is one pair per line, the alias first and its canonical name
second, separated by a single space. On this box it holds 357 lines with no comment line,
no blank line, no tab and no line carrying a third field, so `split_once(' ')` reads all of
it. `Aliases::from_str` still skips a blank line, a `#` line and a line with no space,
because the file is shipped by `shared-mime-info` and its shape is not Flea's to promise.

A thumbnailer may declare either side of a pair, and the two pairs that matter here fall on
opposite sides:

| Pair in `aliases` | Thumbnailer | Declares |
|---|---|---|
| `image/heic image/heif` | `glycin-heif.thumbnailer` | `image/heif`, the canonical name |
| `video/x-matroska video/matroska` | `ffmpegthumbnailer.thumbnailer` | `video/x-matroska`, the alias |

So neither "always canonicalise" nor "always alias" works: a rule fixed in either direction
misses one of the two container types. The rule is that **both** sides pass through
`Aliases::canonical` before they are compared, which maps each spelling onto the
equivalence class it names. The matcher owns that comparison; this module owns only the
table.

The table is many aliases to one canonical name, so it must never be inverted. `image/heif`
is the canonical name for three aliases here, `image/heic`, `image/heic-sequence` and
`image/heif-sequence`, and an inverted table would keep only whichever of the three came
last in the file. Reversing the insert is one of the two mutations this module's tests are
proved against.

`canonical` returns a borrow of its input when there is no alias, so a caller that
canonicalises every row pays no allocation for the common case. An unknown type and the
empty string both come back unchanged rather than erroring, and a missing file yields an
empty table rather than a panic, because a box without `shared-mime-info` should fall back
to icons rather than fail to list.

`Aliases::load` measured a p50 of 78 to 82 microseconds across three release runs of 50
iterations each, min 78 and max 195, against `mime::Db::load`'s 390 to 679.

## Thumbnail cache

The freedesktop thumbnail spec names every cache entry by the MD5 of the file's `file://`
URI: `~/.cache/thumbnails/large/<md5>.png`, with the same rule for `normal/` and for the
`fail/` markers every application on this box honours. There is no way to read or write the
shared cache without computing that digest, and reading it is the whole point, so
`backend/md5.rs` is a hand-written MD5.

It is hand written because the crate has zero Cargo dependencies and `std` ships no hash
that produces this digest. It is MD5 because the spec says MD5, not because MD5 was chosen:
the digest is a cache filename, never an authentication or integrity check, so its collision
weakness costs nothing here. A review finding that flags MD5 as a weak hash is answered with
that fact rather than with a different algorithm, which would simply miss every entry the
rest of the desktop wrote.

`hex` takes the URI bytes and returns the 32-character lowercase digest. Its tests are RFC
1321's own four vectors, the 55, 56 and 64 byte lengths that straddle the padding boundary,
and one real filename that exists in this box's cache,
`4b971187c53a6a6ff1925d1147d8dacf.png`, which is what proves the implementation
interoperates rather than merely being self-consistent. The two mutations it is proved
against are a zeroed padding byte and a big-endian length append; the second leaves the
empty-string vector green, because a zero length has the same eight bytes in either order.

`backend/thumbcache.rs` reads that cache. The root is `$XDG_CACHE_HOME/thumbnails`, or
`$HOME/.cache/thumbnails` when the variable is unset or empty, which is what runs over a
non-interactive ssh here. Under it, `large/<md5>.png` holds the 256 px thumbnails and
`fail/<application>/<md5>.png` holds the markers that say a file could not be thumbnailed.
`fail/` is namespaced by application because one program failing on a file says nothing
about another; Flea's namespace is `flea`, and the only namespace this box already carries
is `gnome-thumbnail-factory`. `Cache::new` computes that root, and `Cache::at` takes one,
so a test writes its markers into a temporary directory and never into the shared cache.

`lookup` checks `fail` before `large`, deliberately. A file that failed to thumbnail may
still have an old thumbnail sitting in `large` from before it was edited, and checking
`large` first would hand that stale image back and then retry the generation that already
failed. Three permanent tests pin this, all through `Cache::at` on a temp root: one writes
both entries for a single URI and asserts `Hit::Failed`, one writes only a `large` entry
and looks it up under a different mtime for `Hit::Miss`, and one looks the same entry up
under its own mtime for `Hit::Ready` naming the file it found. The third is not redundant:
without it the second passes on an implementation that returns `Miss` for everything.

An entry is only a hit when its `Thumb::MTime` text equals the source file's mtime. That is
the spec's whole staleness rule: the thumbnail carries the modification time of the file it
was made from, so an edited file misses and gets regenerated rather than showing its old
picture. A missing, unparsable or different `Thumb::MTime` is a miss, never a hit.

The metadata lives in uncompressed PNG `tEXt` chunks, and only those are read. `zTXt` and
`iTXt` would need a DEFLATE decompressor, which this crate has no dependency for, and no
thumbnailer shipped on this box writes them; every one of the 94 entries here is `tEXt`
written by `GNOME::ThumbnailFactory`. The chunk walk steps length, type, payload and the
4-byte CRC, stops at `IDAT` or `IEND` because no text follows the pixels, and returns
`None` rather than panicking on any truncation.

**A published entry carries exactly one of each key this application owns.** `stamp` inserts
`Thumb::URI`, `Thumb::MTime` and `Software` straight after `IHDR`, so a first-match reader such as
`png_text` sees ours. That is not enough: `ffmpegthumbnailer` writes a `Thumb::URI` of its own, a
bare canonical path with no `file://` scheme, and it survives later in the same file, so a
last-match reader saw a URI that is not the cache key. We own the entry we publish and two of one
key in it is a defect whichever one a reader picks, so `write_stamped` strips any existing `tEXt`
chunk carrying one of those three keys before inserting its own. Keys the thumbnailer owns are
left exactly where it put them: `ffmpegthumbnailer`'s `Thumb::Mimetype`, `Thumb::Movie` and
`Thumb::Size` all survive untouched. The strip pass is a rewriter and not a validator, so bytes
that do not parse as a chunk are copied through rather than dropped.

**Two URIs are built from one file, and collapsing them breaks half the thumbnailers.** The
CACHE KEY is `uri_for(job.path)`, the URI of the path the user actually named, because that is
what every other application on the box keys on and what `Thumb::URI` stamped into the PNG has
to equal or nobody can read the entry back, this tree included. The CHILD-FACING URI is
`uri_for(abs)`, built inside `thumbargv::argv` from the canonical path, because that is the one
path `sandbox::wrap` binds into the namespace. `%i` and `%u` are two spellings of that one
input; the cache key is a different thing that happens to have the same type. Giving the child
the raw-path URI while binding the canonical path lost every `%u` thumbnailer under a symlink,
which on this box is 54 of the 172 types the loaded table serves, every still image through
glycin and everything evince handles, against video, audio, office and xournalpp on `%i`. It
failed silently for the glycin types, because `glycin-thumbnailer` prints
`Error opening file ...: No such file or directory` and **exits 0**, writing a zero-byte file,
so the job read as a success and only died later when `stamp` refused the empty PNG. `argv`
therefore builds the child URI itself and takes no URI argument at all, so the two cannot be
handed the same value by mistake.

`uri_for` builds the URI the digest is taken over, and its literal set is GLib's, not RFC
3986's unreserved set. This was measured on the box rather than read off the spec: a probe
of every printable ASCII byte through `GLib.filename_to_uri` leaves `!$&'()*+,-.:=@_~`
and the alphanumerics literal, along with the `/` that separates components and is
structure, and escapes space, `"`, `#`, `%`, `;`, `<`, `>`, `?`, `[`,
`\`, `]`, `^`, `` ` ``, `{`, `|`, `}`. Non-ASCII is escaped one UTF-8 byte at a time. The
narrower RFC set is not merely stricter, it is wrong: this box's cache holds
`67471ae1929105f9f387addc0af2eb20.png` for
`file:///home/gm/Downloads/CleanShot%202026-08-17%20at%2018.25.25@2x.png`, whose `@`
GNOME left literal, and percent-encoding it yields a different digest and a permanent
miss. `a_uri_matches_the_one_gnome_wrote_for_a_real_entry` pins that entry, and the same
rule is what keeps the very common `file (1).jpg` shape addressable.

A warm hit on this box costs single-digit microseconds in a release build, roughly 9 for
the median across five runs, against roughly 24 in a debug build, for one failed open on
the `fail` path plus a 9912-byte read and the chunk walk. The maximum in a 50-iteration
batch runs 4 to 5 times the median on scheduler noise, so treat this as a magnitude and
remeasure rather than quoting a constant.

### The writer, `backend/thumbwrite.rs`

The writer lives next to this reader on purpose: the two agreeing is the only property either
of them has, so a change to one that is not made to the other is invisible until the desktop
stops matching Flea's entries. `png_text` above reads a `tEXt` chunk; `thumbwrite.rs` writes
one, and `a_stamped_png_reads_back_through_the_cache_parser` is the test that pins them
together against a real file on disk.

It owns the whole of publishing one entry safely: `exclusive_temp` opens
`.flea-<pid>-<16 hex>.png` with `create_new` at 0600, `stamp` reads back the bare PNG a
thumbnailer wrote and inserts three `tEXt` chunks directly after IHDR, and `write_marker`
puts the same three chunks in a 67-byte 1x1 greyscale PNG for the `fail/` namespace. The keys
are `Thumb::URI`, `Thumb::MTime` and `Software`.

**The CRC-32 is in-tree because the cache is shared.** A `tEXt` chunk needs a CRC that `std`
does not have, and the alternative, skipping the chunks and keeping the mtime in a Flea-owned
sidecar, was rejected: `lookup` reads `Thumb::MTime` out of the PNG itself, so a cache Flea
writes but cannot re-read is worthless, and a `fail/` entry no other application honours
defeats the only reason for using the shared cache. It is twenty lines, its table is built
once in a `OnceLock` because it runs three times per stamped file, and
`the_crc_matches_the_standard_vector` pins it to `crc32("123456789") == 0xCBF43926`.

**`ONE_PIXEL` was generated and CRC checked on this box, not transcribed.** 67 bytes, three
chunks, `PNG 1x1 8-bit Grayscale`. An earlier hand-transcribed version was 66 bytes with a
wrong IDAT CRC that ImageMagick silently tolerated, so do not tidy that array. Both it and a
real generated entry were walked chunk by chunk against python's `binascii.crc32`; `xxd` is
not installed here, so use `python3` or `od`.

## Thumbnailer specs

`backend/thumbspec.rs` reads the freedesktop `.thumbnailer` files, which say which program
turns a file of a given MIME type into a PNG. The search path is `$XDG_DATA_HOME` then every
entry of `$XDG_DATA_DIRS`, each with `thumbnailers/` appended. Over a non-interactive ssh on
this box both variables are empty, so the defaults apply: `~/.local/share/thumbnailers`,
which does not exist here, then `/usr/local/share/thumbnailers`, then
`/usr/share/thumbnailers`, which holds all nine shipped files.

Files are parsed in search-path order, and within one directory in sorted path order, so a
duplicate declaration always resolves the same way; see "Deliberate corners".

**Only the `[Thumbnailer Entry]` group is read.** The field scan used to have no group awareness
at all, so an `Exec=` under any other heading was honoured and the last one in the file won. That
is a key naming a program to run, read out of a file in a user-writable directory, which is the
one place in this subsystem where being quiet is not an option. Keys before the first group and
keys in any other group are now skipped, and a file that never opens the group declares nothing.

The first directory on that path is inside the user's home and is writable without root, so
a `.thumbnailer` file is untrusted input naming a program to run. Two rules follow. First,
the program is validated before it can ever be run: `is_runnable` refuses anything that is
not a regular file with an execute bit, resolving a bare name through `PATH` and an absolute
name directly. Second, the argv `argv` returns is exec'd directly and never through a shell,
so a file named `a; rm -f canary.jpg` reaches the child as one argument and its semicolon
means nothing. Both were proved on the box: the same string through `sh -c` deletes the
canary and through a direct exec does not.

`TryExec` is optional, and **both it and the `Exec` program are validated**, not one or the
other. `com.github.xournalpp.xournalpp.thumbnailer` ships without a `TryExec`, so the first
token of `Exec` has to be checked anyway; checking only `TryExec` when it was present meant a
file could name `TryExec=/bin/true` and any `Exec` at all and still report `t:true`, which put a
row on the wire that no thumbnailer can serve. All nine shipped files
resolve here, and five of them name their program by bare name rather than by absolute path
(`xournalpp-thumbnailer`, `evince-thumbnailer`, `ffmpegthumbnailer` twice,
`gsf-office-thumbnailer`); only the four `glycin` files give `/usr/bin/glycin-thumbnailer`.
A bare name is why `is_runnable` searches `PATH`, and why anything that later runs these
programs with a scrubbed environment has to supply one.

`argv`, in `backend/thumbargv.rs`, substitutes four placeholders: `%i` the input path, `%u` its
`file://` URI, `%o` the output path and `%s` the pixel size. The input is put through `canonicalize` first, so a
relative path or one carrying `..` reaches the child absolute and cannot be read as a flag.
It returns `Option<(PathBuf, Vec<String>)>` and answers `None` when that call fails, which is
the file being deleted between the listing and the thumbnail, a permission denied on an
intervening directory, or a symlink loop. Refusing is the whole point: passing the raw path
through on failure would hand the child a non-absolute string in exactly the case the guarantee
exists for. A caller turns `None` into a failed thumbnail, never into a run.

**One call produces the bind, `%i` and `%u`, and that is the whole point of the signature.** The
sandbox binds one input read-only by the path its caller gives it, and the child opens whatever
`%i` or `%u` names, so all three have to be the same path; see "Thumbnail cache" for why the
cache key deliberately is not. This has been got wrong twice, in opposite directions, and both
were silent.

First the bind and `%i` diverged: `run_one` passed the raw `job.path` to `sandbox::wrap` while
`argv` canonicalised it for `%i`, so a file reached through a symlink or a symlinked parent was
bound under one name and opened under another. Inside the shipped wrapper that is
`ffmpegthumbnailer -i .../real/clip.mp4` against a bind of `.../link/clip.mp4`, which exits 255
with `Could not open input file`, and it wrote a permanent `fail/` marker each time.

Fixing only that inverted the defect onto the larger half. `%u` was still built by the caller
from the raw path while the bind had become canonical, so **54 of the 172 types the loaded table
serves** stopped thumbnailing under a symlink, while the 118 `%i` types, video and audio through
`ffmpegthumbnailer`, office through `gsf-office-thumbnailer` and three note formats through
`xournalpp-thumbnailer`, started working.

**Count the table, not the declaration lines.** The five `%u` files carry 57 `MimeType=` entries
between them, which is the number a `grep` of the field gives and the number this paragraph
wrongly quoted twice. The table holds 54, because `image/tiff` is declared by both
`glycin-image-rs` and `evince` and because two of evince's comic-book pairs canonicalise
together, `application/x-cbz` onto `application/vnd.comicbook+zip` and `application/x-cbr` onto
`application/vnd.comicbook-rar`. **57 is the raw declaration count and 54 is what the table
serves**, and every claim below is about the table. Re-derive it by loading `Thumbnailers::load`
and counting `by_mime` by `exec[0]` rather than by counting declarations; the fix report carries
the command.

The 54 split **22 glycin and 32 evince**, and the two halves failed differently. The 22 glycin
types failed silently: `glycin-thumbnailer --input file://<raw>` inside the real sandbox prints
`Error opening file ...: No such file or directory` and **exits 0** with a zero-byte output, so
the child read as a success and the job only died later when `stamp` refused the empty PNG. The
32 evince types failed permanently: `evince-thumbnailer` takes the ordinary non-zero exit,
measured at **254** on the same shape, and each one wrote a `fail/` marker, the same damage the
first direction did to video. So the regression lost 22 types silently and 32 permanently, and
either way the row never got a thumbnail and burned a full sandbox spawn on every viewport pass.
The zero-byte case is now recorded as a failure in its own right; see "Thumbnail pool".

`argv` therefore takes NO URI argument. It canonicalises once and derives the returned path,
`%i` and `%u` from that one value, which makes the divergence unrepresentable rather than
merely fixed. `tests/thumbs.sh` now carries four symlink checks, a video and an image through a
symlinked parent directory and a video and an image through a symlink to the file itself,
because the single video check it had before is exactly why the second defect shipped.

Matching is alias-aware in both directions: the declared type and the queried type both pass
through `Aliases::canonical` before they are compared, because the two pairs that matter fall
on opposite sides of the declarations, see "MIME aliases".

## Thumbnail sandbox

`backend/sandbox.rs` builds the argv that confines a thumbnailer child. Untrusted bytes
reach media decoders there, which is the richest CVE seam in this subsystem, so the child
gets no network, no writable path outside its output directory, and a bounded amount of CPU
and address space. The module only builds the argv; nothing in it runs a process, and the
argv is exec'd directly and never through a shell.

**The sandbox is mandatory, and its absence fails the job closed.** `sandbox::available()` looks
for `bwrap` and `prlimit` on `PATH`, and when either is missing `run_one` returns `Outcome::Failed`
before spawning anything: the row answers with an empty `file` and nothing is written to the shared
cache. It used to run the UNWRAPPED argv instead, with no log line, no `fail` marker and no wire
signal, which was reproduced by pointing `PATH` at an empty directory and watching glycin produce a
real 60 kB thumbnail from an untrusted file with no namespace, no network isolation, no rlimits and
full access to `$HOME`. The composed case is the point: with the sandbox up, a hostile
`~/.local/share/thumbnailers/x.thumbnailer` is contained, and with `bwrap` absent that same file was
unconfined code execution as the user. One missing package must not turn the first into the second.
**No `fail/` marker is written for this**, on the same reasoning as a vanished input under
"Thumbnail pool": a missing package is not a broken file, and a marker keyed by URI plus mtime would
stop every application on the box from ever thumbnailing that file again. The backend also checks
once at startup and prints one line on stderr, so the cause is stated rather than silent; it is not
fatal, because listing directories does not need the sandbox. `tests/thumbs.sh` runs the real binary
with `PATH` pointed at an empty directory and asserts the empty `file`, no cache entry and no marker.

The shape is `prlimit --cpu=30 --as=1073741824 bwrap <flags> <inner>`. **`prlimit` is the
outermost program because `bwrap` has no rlimit option**, verified against `bwrap --help`
on bubblewrap 0.11.2 here. Setting the limits from Rust would need raw `setrlimit`, which
`std` does not expose and which the zero-dependency rule forbids reaching for through
`libc`, so stock `/usr/bin/prlimit` from util-linux 2.42.2 carries them. `prlimit` is
outermost because it keeps the argv one flat exec and covers `bwrap`'s own setup as well
as the child, not because the limits would otherwise miss the decoder: rlimits survive
fork and exec, so the reverse nesting was measured to kill the same spin and to report
the same `/proc/self/limits`.

The two numbers: `--cpu=30` seconds, because a 1080p decode is well under a second of CPU
here and 30 s is a runaway rather than a slow file; `--as=1073741824`, one GiB, because
`ffmpegthumbnailer` peaks in the tens of megabytes on the media fixture and a gigabyte of
address space is a decompression bomb rather than a big video.

The flags, and why each is there:

- `--unshare-all` drops every namespace, which is what removes the network.
- `--die-with-parent` means a wedged decoder cannot outlive the backend.
- `--new-session` detaches the controlling terminal so the child cannot inject input with
  `TIOCSTI`.
- `--clearenv` empties the environment, so no `LD_PRELOAD`, no `XDG_*`, no session bus.
- `--ro-bind /usr /usr` and `--ro-bind /etc /etc` give the decoder its libraries and its
  loader configuration, read-only.
- `--symlink usr/lib /lib`, `/lib64`, `--symlink usr/bin /bin`, `/sbin` reproduce this
  box's merged-`/usr` layout. `ls -ld /lib /lib64 /bin /sbin` shows all four are symlinks
  into `usr`, so binding `/usr` plus these four lines is the whole filesystem the child
  needs.
- `--proc /proc` and `--dev /dev` are the minimal kernel interfaces a decoder expects.
- `--tmpfs /tmp` gives it scratch space that is discarded with the namespace.
- `--ro-bind <input> <input>` is the one file it may read.
- `--bind <out> <out>` is the one path it may write, which production makes the temp file itself.

**A bare program name resolves without a `PATH`.** Five of the nine shipped `.thumbnailer`
files name their program by bare name, see "Thumbnailer specs", and `--clearenv` means the
child has no `PATH` at all: `env` inside the sandbox prints only `PWD=/`. `bwrap` execs
through `execvp`, whose glibc fallback when `PATH` is unset is `confstr(_CS_PATH)`, measured
here as `/bin:/usr/bin`. All four installed bare-name thumbnailers live in `/usr/bin`, so
they resolve, and a bare-name `ffmpegthumbnailer` under the shipped flags produces a real
PNG from the video fixture. **No `--setenv PATH` is needed and none is set.** The limit
worth knowing is that `/usr/local/bin` is not on that fallback path, so a thumbnailer
installed there would fail to resolve by bare name; nothing relevant is installed there
today.

**Measured overhead, nine interleaved pairs with the arm order alternating.** Against a
trivial `/usr/bin/true` child, which isolates the wrapper from the decode, the full
`prlimit` plus `bwrap` wrapper costs in the low single-digit milliseconds: the nine deltas
ran 3.3 to 4.1 ms with a median near 3.7 ms, and `prlimit`'s own share of that was 0.2 to
0.9 ms, median near 0.4 ms. That leaves `bwrap` at roughly 3 ms, consistent with the 3.2 ms
recorded for `bwrap` alone, so adding `prlimit` costs well under a millisecond. Against a
real `glycin-thumbnailer` run the same nine pairs gave 1.2 to 5.8 ms, median near 2.5 ms,
but the decode itself is about 45 ms and drifts by as much as the effect, so the isolated
number is the one to quote. Either way the wrapper is under a tenth of one thumbnail's
cost. Do not treat any of these as a citable constant; two batches of the same harness on
this box have disagreed by more than 2x. `systemd-run --user` was measured at 23.8 ms and
rejected on that number.

**Confinement results, all measured on the box.** It produces a real 256 px PNG through the
full argv; the canary file outside the output directory survives and a write to it fails
with `No such file or directory`; `/home` is not visible at all and the root holds only
`bin dev etc lib lib64 proc sbin tmp usr`; `getent hosts example.com` exits 2 with no
resolution; `ulimit -v` inside reads 1048576 KiB, so the address-space limit is applied;
and a spin under `--cpu=2` is killed rather than returning 0. **The `sh -c` in those probes
is a test OF the sandbox, not production code.** Production execs the argv directly, and
`grep -rn 'sh -c' src/` finds nothing.

**The writable path is whatever the caller binds, and production binds one file.** `wrap`'s
third argument is the single path mounted `--bind`, and `thumbs.rs` passes the pre-created
temp file rather than its parent directory, so that temp is the only path the child can
write. Binding a single pre-created file was measured to work for both `glycin-thumbnailer`
and `ffmpegthumbnailer`, both writing a valid PNG through a `--bind <out>/t.png <out>/t.png`,
and `glycin-thumbnailer` was re-checked against a 1x1 PNG when the pool started using it. A
caller may still pass a directory, which is the looser bind and what a thumbnailer that wrote
to a temporary name and renamed would need; only those two thumbnailers were probed.

## Thumbnail pool

`backend/thumbs.rs` is the only thing that generates a thumbnail, and it generates one
only for a `Job` a caller submits. **Nothing in it enumerates a directory**, at any
priority: the load-bearing rule that every per-file operation stays scoped to the viewport
is the reason the product beats the field, and a sweep here would forfeit it.

`Pool::new(workers, results, root, aliases, specs)` spawns `workers.max(1)` threads that pop from
one `Mutex<VecDeque<Job>>` under a `Condvar` and report every finished job down one `Sender<Done>`.
The worker count is the caller's, not a constant here: it is a scheduling policy that belongs
with the request policy in `run.rs`, and the only thing this module insists on is that zero
workers is not a pool. The MIME alias table, the thumbnailer specs and the `Cache` live in one
`Arc<Tables>` shared by every worker, so the caller's one parse and the one cache root reach four
threads as a single handle rather than as four copies. **No worker reads either file on any path
now**: both arrive as `Arc`s from the caller, `Cache::at` only stores the root, and even
`with_specs` builds its tables in the constructor.

**Both tables are the caller's, passed in as `Arc<Aliases>` and `Arc<Thumbnailers>`.** `run.rs`
has already parsed both to answer the `rows` line, and `Pool::new` used to parse them a second
time at startup, so a client that never asks for a thumbnail paid for two tables it could not
use. Sharing that one parse is worth **0.11 MiB of the backend child's PSS**, measured over 12
interleaved pairs on each of three trees with no run overlapping the other arm: 5075 to 4957 KiB
on the 100,000-file fixture, 1525 to 1409 KiB on the media fixture and 1453 to 1345 KiB on a
12-file directory, `Pss` read from `smaps_rollup` in KiB with THP disabled in the child as the
shipped launcher leaves it. `with_specs` still builds its own, because a test names thumbnailers
no shipped file declares.

**The cache root is a constructor argument, not `Cache::new()`.** Production passes
`thumbcache::default_root()`; every test in this module passes a per-pid directory under the
temp dir. The shared cache at `~/.cache/thumbnails` is the operator's, and a suite that
records a `fail/flea/<md5>.png` marker there on every run would poison it for every other
application on the box.

`submit`, `cancel` and `cancel_all` each return the queued `Job`s they dropped, taken under the
same lock that dropped them, so `run.rs` can unmap exactly those rows and answer them rather
than only counting them; see "Thumbnail requests".

`MAX_QUEUE` is 70, two viewports of about 35 rows, and `submit` drops from the FRONT when it
is full. The oldest job is the row furthest from the viewport after a scroll, so it is the one
whose answer nobody is waiting for. `cancel` and `cancel_all` only remove queued jobs; a job
already inside a child runs to its own end or to the timeout.

`JOB_TIMEOUT` is 20 seconds, and `backend/child.rs` enforces it. The child is waited on
exactly, not polled: `pidfd_open` gives a descriptor that becomes readable when the child exits,
and one `poll` on it with the remaining time as its timeout is BOTH the wait and the deadline, so
a child still alive at the deadline is killed and reaped and only that case records a marker in
`fail/`. `poll` returning `POLLIN` says only that the child exited, so the status itself still
comes from `wait`. The two symbols are `extern "C"` declarations in the idiom `src/thp.rs` already
uses, because `std` links the system libc and this project takes zero crates; glibc has shipped a
`pidfd_open` wrapper since 2.36, so no `syscall()` is needed. The descriptor is held in an
`OwnedFd`, which closes it on Drop, so the deadline path and both error paths release it without
anyone remembering to. `EINTR` is a retry and is the only reason the loop goes round twice, and
the retry recomputes the remaining time against the same deadline rather than restarting it, so a
stream of signals cannot hand a hung child an unbounded wait.

**There is deliberately no fallback to a sleep loop.** A `pidfd_open` that fails leaves a child
already running with no way to enforce a deadline, so it is killed and reaped and reported as
`Ran::NotStarted`. A second wait implementation, reachable only on fd exhaustion on a box whose
kernel always has `pidfd_open`, would be code nobody ever runs and therefore code nobody has
tested when it matters.

**A syscall failure is the machine's fault, so it never records a marker.** `Ran::NotStarted` is
the variant whose call-site behaviour is right for it: `run_one` discards the temp and writes
nothing to `fail/`. Reading the variant by its NAME rather than by what the call site does with it
argues the other way, since the child did start and did read the file, and that reading is wrong.
The distinction this enum draws is whether a thumbnailer's verdict on these bytes exists, and a
`pidfd_open`, a `poll` or a `wait` this process could not complete produces no verdict at all,
exactly like a fork that failed under memory pressure.

**Three of the eight ways a job can end record a marker, and the deadline is only one of them.**
That count is the `yes` column of the table below, counted off it rather than carried in from
anywhere else. `run_with_timeout` itself returns `Ran::Failed` for only two of the three, a child
still alive at the deadline and a child that exited non-zero; the third is `run_one`'s. Every way
a job can end:

| Path | Variant | Marker |
|---|---|---|
| `spawn` failed | `NotStarted` | no |
| `pidfd_open` failed | `NotStarted` | no |
| `poll` failed, or was ready with no `POLLIN` | `NotStarted` | no |
| `wait` failed | `NotStarted` | no |
| still alive at the deadline | `Failed` | **yes** |
| exited non-zero | `Failed` | **yes** |
| exited 0 having written nothing | `Succeeded`, then recorded by `run_one` | **yes** |
| exited 0 having written a thumbnail | `Succeeded`, published by `run_one` | no |

`run_one` records on its `Succeeded | Failed` arm (`src/backend/thumbs.rs:211-213`), which a
success reaches only when it wrote nothing, and `tests/thumbs.sh` exercises that arm on every
run. What the recording paths share is that a decoder ran on the file and produced no answer.

The `wait` row was `Failed` until 2026-08-30, which contradicted the rule the rest of this
section states: an `Err` from `wait` is `ECHILD` or nothing, never a verdict, and it wrote a
permanent marker for a file that may be perfectly sound. The tail of `run_with_timeout` is
`verdict`, its own function, so the failing wait is reachable from a test: it spawns a child,
reaps it directly with `waitpid`, and leaves `verdict` a wait with nothing to reap. Against the
old single `_ => Ran::Failed` arm that test fails and the other six in the file pass.

**A machine-caused SIGKILL is NOT told apart here, and the attempt was refused on evidence.**
The proposal was to read `status.signal() == SIGKILL` as `NotStarted`, since the deadline path
kills and returns before `verdict`, so a SIGKILL arriving there is not ours. It fails on three
counts, each measured on this box against the real `sandbox::wrap` argv by
`~/bench/sigkill-probe.py`.

**It cannot see the kill it was proposed for.** bwrap's man page says under `EXIT STATUS` that it
returns the exit status of the initial application process, and its `--json-status-fd` entry
documents the encoding as "n if it exited normally with status n, or 128+n if it was killed by
signal n". So a decoder SIGKILLed inside the sandbox reaches the direct child as **exit 137, not
as a signal**, and the direct child is `prlimit` having exec'd `bwrap`, which is one process
either way.

**A file's own death is the same number.** `prlimit --cpu=N` sets the soft limit EQUAL to the hard
limit, read back from inside the sandbox as `Max cpu time 2 2 seconds`, and with them equal the
kill that arrives is SIGKILL and not SIGXCPU. Measured both ways, a bomb that ignores `SIGXCPU`
and a plain one that does not: **exit 137 for both**, never 152. That is a decompression bomb, a
file verdict that must record, and it is indistinguishable from the OOM kill at the only layer
where either is visible. A discriminator keyed on 152 does not exist to be built.

**The premise is close to unreachable here anyway.** The sandbox caps address space at 1 GiB, so a
memory bomb surfaces as the decoder's own non-zero exit long before the box is short: the probe's
`as_rlimit_bomb` row is exit 1, a Python `MemoryError` and not a kill. The box carries 19 GiB of
RAM and 38 GiB of swap.

Only a SIGKILL of the sandbox launcher itself arrives as `signal=9`, and that process decodes
nothing, allocates nothing and burns no CPU, so it can reach neither rlimit; with
`--die-with-parent` the realistic producer of that signal is something killing Flea's whole
process tree, at which point nothing records anything. The probe's six rows are `inner_sigkill`
exit 137, `as_rlimit_bomb` exit 1, `cpu_rlimit_ignoring_xcpu` exit 137, `inner_exit_1` exit 1,
`outer_sigkill` signal 9 and `cpu_rlimit_plain` exit 137.

**Two discriminators were looked for and rejected, so nobody re-derives them.** `--json-status-fd`
carries the same `128 + n` encoding as the exit status and so adds nothing. `wait4` rusage could
separate a CPU-limit kill from an OOM kill by `utime`, but it needs raw FFI replacing
`Child::wait`, and on this box there is nothing for it to discriminate.

This replaced a `try_wait` plus `sleep(25 ms)` loop on 2026-08-29. It cost up to a full 25 ms of
pure latency per job: measured through the real pool at 4 workers on 35 videos, 61 of 70 `child`
durations sat within 1 ms of a multiple of 25 ms, and after the change 3 of 70 did, which is what
chance alone gives. The `child` median fell from 125.65 ms to 110.49 ms.

**What is recorded in `fail/` and what is not.** A marker means "this content cannot be
thumbnailed at this mtime", and every other application on the box honours it, so it is
written only when a thumbnailer actually ran on a readable file and failed or hung. A missing
thumbnailer for the type is not recorded, and neither is an input that will not canonicalise:
a file that vanished between the listing and the job is not a broken file, and recording it
would stop every application from ever thumbnailing that path again if it came back with the
same mtime, which a `mv` back, an `rsync -t` or a `git checkout` all produce. The failure path
is one `canonicalize` syscall with no child spawned, so the retry it allows is cheap and the
queue bounds it.

**A child that exits 0 having written nothing IS recorded.** `glycin-thumbnailer` exits 0 on
bytes it cannot decode and leaves the output file empty, measured here on 4 kB of `/dev/urandom`
named `.jpg`; a real jpeg truncated to 1 kB, by contrast, still yields a 1456-byte thumbnail and
is a genuine success, so emptiness and not partiality is the test. `run_one` therefore checks the
child's own output at the child boundary, immediately after `Ran::Succeeded`, and treats an empty
file as a failure to record. **The placement is deliberate.** Judging it at `stamp` instead would
be simpler by one line and wrong, because a `stamp` error also covers a write this process could
not complete, which is ours and not the file's; judging the child's own output keeps `stamp`'s
failure meaning "we could not publish" and keeps this one meaning "the decoder ran against these
bytes and produced nothing", which is exactly what a marker says. Before this, such a file was
retried at a full sandbox spawn on every viewport pass, measured at about 25 ms on both the first
and the second request, against about 0.05 ms once the marker answers it. **Stopping the rule at
emptiness is deliberate.** A non-empty but malformed output still discards without a marker,
exactly as before, because no thumbnailer on this box was observed to produce one, and retrying
is the conservative direction against recording a verdict nothing measured.

**Three failures were moved the other way, onto the not-recorded side.** A missing `bwrap` or
`prlimit` records nothing, see "Thumbnail sandbox". And `run_with_timeout` returns three states
rather than a bool: a child that could not be SPAWNED at all is `NotStarted` and records nothing,
because a fork that failed under memory pressure says nothing about the file, and so is a child
this process could not `wait` on, while a child that ran and exited non-zero, or that was killed
at the deadline, is `Failed` and does record.
The rule that separates them is whether a decoder ever looked at the bytes. The seam that
bound the raw path while `argv` handed the child the canonical one, see "Thumbnailer specs",
was in the recording class and wrote a permanent marker for every video under a symlinked
directory; it is closed at the source rather than by widening this rule, because from inside
`run_one` a bad bind and a corrupt file are the same non-zero exit and cannot be told apart.
The `NotStarted` distinction cannot be reached from a test through `run_one`, because
`sandbox::available()` has already checked `prlimit` microseconds earlier and both paths
`discard` without a marker anyway; it is pinned at the `run_with_timeout` level instead.

**The destination is never handed to a child.** The output is a `thumbwrite::exclusive_temp`
name in the target directory, and the entry is published by `rename`, which is atomic. `create_new` is what makes the guard load bearing: with `create(true)` a
symlink planted at that name is followed and the file outside is created, measured here.
Every failing path removes its own temp before returning, so a directory of dangling symlinks
cannot litter the shared cache.

**The sandbox binds the temp FILE, not its directory.** The spec asks for the output temp file
to be the only writable path, and Task 6 measured both real thumbnailers writing in place
through a single-file bind, so `run_one` passes the temp to `sandbox::wrap` and the child can
write nothing else; see "Thumbnail sandbox".

**The temp, the stamping and the marker are `backend/thumbwrite.rs`**, documented under
"Thumbnail cache" beside the `tEXt` reader they have to agree with. This module calls three of
its functions and owns none of them.

**Cancellation is tested against a pinned worker, and the pin is load bearing.** `submit` then
`cancel` on the main thread races the worker's own pop, so a test that just does the two in a
row passes or fails on scheduling: with a 50 ms delay inserted between them it goes red here
while `cancel` is working perfectly. The tests therefore submit a job whose spec is a
`/usr/bin/sleep` of their own unique duration first, wait until that child appears in `/proc`,
and only then submit and cancel the job under test. **The gate is the child's own cmdline and
not its temp file**, because a child that has exec'd has already made every bind: gating on the
temp instead lets the test's own teardown delete it while `bwrap` is still setting up, which
fails the bind, which is a child failure, which records a `fail/` marker and recreates the
directory the test just removed. That reproduced 5 times in 5 under three concurrent suites and
0 times in 8 after the gate moved. `Pool::pending` exists for these tests and is `#[cfg(test)]`.

## Thumbnail trace

`FLEA_THUMB_TRACE`, set and non-empty, makes the backend write one line per reported job to
**stderr**. Off by default, and off it costs a `None` on `Job` and nothing else: the environment
is read once for the process behind a `OnceLock`, and an untraced job reads no clock and carries
no marks.

```
flea: trace row=12 depth=8 queued=182.41 setup=0.31 child=94.77 after=1.02 total=278.51
```

`row` is the listing row the job answers and `depth` is the number of jobs already queued when
this one was submitted, which is what says whether the workers were starved. The four durations
are milliseconds and they SUM to `total`, because all of them are deltas from one `Instant`
captured in `thumbs::trace` at submit rather than four clocks of their own. Each names a
different suspect:

- `queued`, submit to the worker's pop, is queueing.
- `setup`, the pop to the moment the argv is handed to the spawner, is worker startup, the cache
  key, `create_dir_all`, `exclusive_temp`, the argv build and the sandbox wrap.
- `child`, that moment to `run_with_timeout` returning, is the decode. **It is not pure decode.**
  It carries the `bwrap` and `prlimit` fork and exec, and about a millisecond of notice latency,
  because `child.rs` waits on a pidfd; see "Thumbnail pool". Until 2026-08-29 it also carried up
  to a full 25 ms poll step, which is how the poll was caught: every one of 70 sampled `child`
  durations landed on a multiple of 25 ms.
- `after`, the child to the row being flushed to stdout, is `stamp`, the `rename`, the result
  channel, the forwarder hop and the write itself.

**The line is written in `run.rs`, not in the pool**, so `after` covers the whole tail including
the single-threaded report path. The consequence is that it is a line per REPORTED job, and THREE
paths answer a row without reporting it, so a trace-line count is not a job count:

1. A job whose listing was superseded is dropped in `report_done` before it is reported.
2. A row the drain deadline cut short is answered in `drain` rather than by a result.
3. A job the pool evicted for `MAX_QUEUE` overflow is answered by `queue_row`'s own loop over
   what `submit` returned, which writes a bare `thumbed_line` and never enters `report_done`.
   That job carries a live `Trace` it never gets to emit. It is not a defect, since the row IS
   answered, but a debugger reconciling counts will trip on it.

**stderr is not negotiable.** stdout is the wire, `ui/Backend.qml` parses every line of it, and a
trace line there is reported to the user as a line this build cannot read. `tests/thumbs.sh`
pins all three of on-stderr, never-on-stdout and silent-when-unset.

## Thumbnail requests

`backend/run.rs` owns the policy; `backend/thumbs.rs` owns the mechanism. **A thumbnail is
generated only for a row a client named in a `thumb` request.** Nothing in the loop walks
the listing looking for work at any priority, and the proof is on the record: listing the
100,000-file fixture and stat'ing every row through two full windows grows
`~/.cache/thumbnails/large` by zero files and never creates `fail/flea`.

**`THUMB_WORKERS` is 4, and it is a measured ceiling and not a core count.** It lives here and
not in `thumbs.rs` because it is scheduling policy and belongs with the request policy.

**The reason this paragraph used to give was half wrong, and the box said so.** It claimed the
box has 12 cores and that a decoder child is itself multithreaded, so four children already
reach twelve-way parallelism. Neither half survives. The box is an i7-8700T with **6 physical
cores and 12 logical**, two threads per core, which `lscpu` reports and the six distinct entries
in `/sys/devices/system/cpu/cpu*/topology/core_id` confirm. And one `ffmpegthumbnailer` decode of
this fixture costs **0.088 to 0.118 CPU-seconds, median 0.104**, against 0.079 to 0.110 wall
seconds, median 0.095, over 15 warm runs on five videos; the artefact is `~/bench/t4/cpuwall.csv`.
CPU-seconds is the number to quote and not the ratio: a stalled run stretches `real` while
`user + sys` holds, which turns the ratio into noise, and a child decoding three ways would show
0.3 CPU-seconds against the same tenth of a second of wall. One child is one thread and a little.
Four children are about four and a half cores of work, not twelve.

**Throughput keeps rising with width, so throughput is not what picks the number.** Through the
real pool, the media fixture's first 35 videos, this run's own cache entries dropped before every
run, 12 measured runs an arm over three batches of a four-arm Latin square: the median time from
the `thumb` request to the last `thumbed` line is 1108.5 ms at 4 workers, 909.0 at 6, 801.5 at 8
and 709.5 at 12. **That one is monotone at the individual run level**, which is the strongest form
available: the slowest run of each arm beats the fastest run of the next, 1077 against 956, 886
against 817, 790 against 725. Sublinear everywhere, negative nowhere.

**Two things get worse as the pool widens, and both are what a person actually sees.**

The first is the first thumbnail. From those same runs, the median time to the FIRST filled row is
121.5 ms at 4 workers (116 to 127), 123.5 at 6 (111 to 136), 135 at 8 (116 to 142) and 204.5 at 12
(185 to 214). **That one is monotone only in the medians.** At the run level only 8 against 12
separates; the 4, 6 and 8 ranges all overlap, and 10 of 12 8-worker runs land above the 4-worker
maximum of 127 while 2 do not. A wide pool starts its whole first wave at once and the wave
finishes together, so the screen stays empty 83 ms longer at 12 in exchange for filling 399 ms
sooner. The fill curve is a rotation and not a translation, and this is derived from all 35 rows of
the artefact rather than off a sampled table: 4 workers are lowest at rows 1 to 4, 6 workers at 5
and 6, 8 workers at 7, 8, 13 to 16 and 25, and 12 workers at 9 to 12, 17 to 24 and 26 to 35. Read
the other end the same way: 12 workers are LAST at rows 1 to 4, 6 workers at 7 and 8, 262.0 and
273.5 against 248.5 and 255.5, 4 workers everywhere else, and 8 workers are never last at any row.

The second is input latency while the pool is busy, which is the half of the old argument that was
right. **It has to be timed by the UI's own clock.** The first version of this measurement pressed
a key and polled `rowAt` until it answered, and reported 361 / 430.5 / 464 / 635.5 ms at the four
widths; almost all of that was the harness. One `omarchy-drive ipc` call costs 213 ms idle here and
287, 323, 389 and 482 ms under 4, 6, 8 and 12 worker bursts, with 213 again on the return-to-idle
control, and `ready()`, which returns a constant and reads no UI state, costs the same as `rowAt`
at every load. Those numbers are the harness measuring itself. `inputToRows()` exists for this; see
"Testing".

Measured with it: a Flea window on the 100,000-file fixture, `omarchy-drive key --window flea G`
and `g` to jump to the far end and back, a 210-row generation burst at the arm's own width running
beside it, eight rounds an arm with every arm twice in every position. With no burst every arm
reads a median of 38 ms over 48 samples; that is a sanity check and not a result, since each
baseline is taken before the burst process launches, on the identical shipped tree, so no mechanism
exists by which the arms could differ there.

**The last jump of a round can outrun the burst, and every figure below drops it.** The harness
checked burst liveness once per jump PAIR, and a jump costs half a second of quiet window plus a
200 to 480 ms read, so the final sample of a round could be answered after the load had ended. It
shows up in the artefact: the sixth and last sample of the 8-worker arm reads 39, 37, 38, 39, 39,
40, 39, 37 across the eight rounds, which is that arm's own no-burst level, while no other position
of any arm sits near baseline. **The last sample of every round is therefore dropped in every arm,
symmetrically, no arm favoured.** Untrimmed the medians of medians are 48.25, 52.25, 51.5 and
73.25, which is NOT monotone: it puts 8 workers below 6. Trimmed they are **48.0, 51.0, 53.5 and
73.0**, per-round medians spanning 45 to 49, 49 to 57, 50 to 57 and 56 to 83, and the series is
monotone. The trimmed figures are the ones to quote. The harness now checks liveness before each
jump AND again after its read, and reports what it dropped as `offload_dropped`; the numbers here
predate that gate and come from the symmetric trim.

**Each of 6, 8 and 12 is above the 4-worker arm in 8 rounds out of 8** either way, which is
p = 0.004 for a sign test, and at the sample level the trimmed arm is the slower of a cross pair
72.6, 76.6 and 88.8 percent of the time against a null of 50. **6 and 8 are not separated from each
other**, trimmed or not: head to head the 6-worker median is above the 8-worker one in 2 rounds
and below in 6 after the trim, and above in 5, below in 2 and tied in 1 before it, so no ordering
between those two arms is a finding.

**Hyprland is not what is starved**: it holds at 1.8 to 2.5 percent of one core at every width. Do
not read the `qs` process's falling percentage-of-core as starvation either. Its CPU per COMPLETED
jump pair does not fall in the direction that claim needs: 87.2, 96.7, 90.4 and 105.6 ms across the
four widths over all eight rounds, ending higher than it started. The percentage falls because
fewer jumps complete per second, not because anything was denied to it.

**The rule, written before the numbers: take the highest width whose input latency is inside the
noise of the 4-worker arm and whose first thumbnail is too.** On the trimmed series, 12 fails both,
by 25.0 ms of latency and 83 ms of first thumbnail; 8 fails both, by 5.5 ms and 13.5 ms; and 6
passes the first-wave check outright, 123.5 against 121.5 with overlapping ranges, and fails the
latency check by 3.0 ms while sitting above the 4-worker arm in 8 of 8 rounds. Nothing above 4
survives both, so **4 stays**.

**Say the margin out loud, because it is small and the trade is not.** The honest cost of 6
workers is **3.0 ms** of input-to-rows latency on the trimmed series, 51.0 against 48.0, and 4.0 ms
untrimmed, 52.25 against 48.25; the trim is the one described above and it is the figure to quote.
Three milliseconds is under a fifth of a 60 Hz frame. What it would buy is 199.5 ms off the 35th
thumbnail and a tie on the first. The rule disqualified 6 because the cost is measurable, 8 rounds
out of 8, not because it is large, and the 69.5 ms that looked decisive before the instrument was
fixed was the harness timing itself. Anyone revisiting this constant should start from 3.0 against
199.5 and not from the conclusion.

**One thing this measurement does not contain.** The burst runs in a SECOND backend process, so
the window under the input has an idle pool of its own and the contention a real burst adds to
`run.rs`'s single event loop, where every `thumbed` line is written and flushed, is in no number
here. That omission is conservative for the decision, since it can only understate what a wider
pool costs the window it lives in, which is why it is said rather than left out.

**Four answers cost no job at all.** A directory row, a row that is not a regular file, and a
row whose type no validated thumbnailer declares are answered with an empty `file` on the spot,
which is the same condition the `t` flag already reports to the client. A row the shared cache
already holds
at the file's current mtime is answered with that path, and a row recorded as failed at
that mtime with an empty `file`. Only a real miss is queued. A cache hit costs about 12
microseconds end to end on the wire here, against 9 for `Cache::lookup` alone: treat both
as magnitudes and remeasure.

**Only a regular file is ever handed to a thumbnailer.** `scan` fills `is_dir` from `d_type`, so
a fifo, a socket or a device node reads as an ordinary file and its NAME alone decides whether a
thumbnailer declares it. Four fifos called `*.mp4` were enough to take the whole pool down: all
four came back `"t":true`, all four workers blocked on the open for the full 20 second job
timeout, each wrote a permanent `fail/` marker, and every visible row behind them waited. Seventy
such names kill the pool for six minutes, and a filename is the whole trigger, which is the threat
model this section exists for. `thumb_rows` therefore takes the `std::fs::metadata` it already
needed for the mtime and gates on `m.is_file()`, which costs nothing and subsumes the directory
test, and `rows_line` mirrors it through `meta::thumbnailable`, which reads the file-type bits out
of the `st_mode` the window already carries rather than adding a second stat per row. That flag
accepts a symlink as well as a regular file, because the request path stats the target: a symlink
to an image still thumbnails, and a symlink to a fifo is refused when it is asked for.

**The row-to-path map is `State::asked`, and it holds only rows a client asked for.** It is
not a map of the directory: building one would be a per-file pass over the listing, which is
the one thing this product does not do. A `list` or a `sort` clears it and cancels the queue,
because both change which row an index names, so a result that arrives afterwards finds no
entry and is dropped rather than printed against a row it no longer describes. That is this
tree's answer to the generation problem, and it is strictly stronger than a generation counter
carried in the `Job`: a counter bumped on `list` alone would still misreport a row after a
`sort` reordered the listing under an outstanding job. `tests/thumbs.sh` proves it by listing a
second directory between the request and the result and asserting no `thumbed` line at all;
removing the clear from the `list` arm reddens exactly that check and nothing else.

**The map is bounded by the pool, and the loop never trims it.** It holds one entry per queued or
running job, so the pool's own `MAX_QUEUE` plus the worker count already bounds it at 74 and
nothing in `run.rs` removes an entry the pool still owns. An earlier version trimmed the map from
the FRONT past that number, justified as dropping "the entries the pool silently dropped to make
room". The box disproved that: workers pop from the front, so the oldest mappings are the RUNNING
jobs. A single request for 80 rows against the 70-job queue and four workers answered 74 of them
and stranded the first six, four of which finished inside a worker and had their results discarded
because `report_done` found no mapping, which breaks the contract that a client never waits forever
for a row that will not arrive. `submit` therefore returns the `Job`s it dropped rather than a
count, and `queue_row` unmaps each one and answers its row with an empty `file` on the spot.
`tests/thumbs.sh` asks for 80 rows and asserts one `thumbed` line per row; the old trim answers 74.

**The map is also the dedupe.** A path already in it has a job queued or running for this
listing, so `queue_row` returns without submitting a second one. Without that, `rows:[0,0,0]`
queues three identical decodes, because all three iterations run before the first job finishes
and so all three see `Hit::Miss`. The wire stays correct either way, since `report_done` removes
the mapping and the duplicate results are dropped on arrival, but three of a four worker pool
spend tens to hundreds of milliseconds each on work already in flight, which is the exact
resource `MAX_QUEUE` and the worker count exist to ration. `tests/thumbs.sh` proves it without
timing anything: it saturates the four workers, queues a victim row, then floods one row with 75
repeats, and the victim is evicted from the front of a full queue and answers empty unless the
dedupe holds.

**A cancelled row leaves the map with its job.** `thumbcancel` with an explicit `rows` unmaps each
row whose queued job it actually dropped, and `thumbcancel` with an empty or malformed `rows`
cancels everything queued and unmaps every one of those rows too. The asymmetry that made the empty
form poison the map was real and reproduced: `list` and `sort` clear the map through `forget_rows`,
`thumbcancel` did not, so every cancelled mapping went stale, `queue_row`'s dedupe then swallowed
every later request for those paths until the next `list`, and shutdown answered them all with an
empty `file`. A viewport-change handler sends exactly that request, so the headline feature stopped
after the first cancel-all. `tests/thumbs.sh` cancels all of twelve queued rows and asks for them
again; without the unmap only the four already inside a worker come back with a file.

**`submit`, `cancel` and `cancel_all` each return the queued jobs that will now never report**,
taken under the pool's own lock. That is what keeps `State::outstanding` exact and what lets the
loop answer a dropped row instead of stranding it. Reading the queue length and then clearing it in
two calls would race a worker's own pop and leave the count short, which at shutdown means exiting
while a job is still inside a child.

**Shutdown answers the work the client asked for, then cancels.** The plan's text said to
call `cancel_all` before exiting on `quit`; the running code drains first and cancels after,
because `cancel_all` on its own loses the race with the worker that has not popped the job
yet, and the client is told nothing about a row it explicitly requested. Draining also
matters for the shared cache: a worker inside a child owns a `thumbwrite::exclusive_temp`
file there, and only its own return renames that temp into place or removes it. The whole
drain is bounded at `DRAIN_LIMIT`, 25 seconds, which is longer than the pool's own 20-second
job deadline so no single running job can be cut short by it; any row still unanswered when
it runs out is answered with an empty `file`, because the contract is that a client never
waits forever.

**A drain that gives up sweeps this process's own temps.** The one path a worker cannot clean up
after itself is the one where the drain runs out of budget: the process exits and the four worker
threads die inside `run_with_timeout`, after `thumbwrite::exclusive_temp` and before `discard`.
Reproduced with eight hung jobs against four workers under a scratch cache root: four zero-byte
`.flea-<pid>-<hex>.png` files were left behind, which is exactly the ten the operator found in the
real cache. `drain` therefore calls `thumbwrite::sweep_own_temps` on the `large/` directory after
`cancel_all` and only when `outstanding` is still above zero. The order is load bearing: with the
queue emptied first, no worker can start a new job, so the temps on disk at that moment are exactly
the abandoned ones and the sweep cannot race a fresh one. The name carries this process's pid, so
the sweep provably cannot touch a published entry, another application's file, or a concurrent
flea's in-flight temp; a unit test plants all three next to a real temp and asserts only ours goes.
Two alternatives were rejected. Sweeping stale `.flea-*` at STARTUP would put a `readdir` of a
possibly huge shared directory on the product path, which is the one thing this product does not
do, and "stale" needs an age heuristic that races a concurrent flea. Registering each temp in a
shared list would be exact but replaces one function with an invariant that every return path in
`run_one` has to maintain, which is the class of bug this is. Neither this nor any other design
covers a `SIGKILL`; nothing running inside the process can.

**Termination no longer rests on the channel disconnecting.** Before the pool existed the
reader thread held the only `Sender<Event>`, so a reader death ended the loop by
disconnection. The pool's workers and the forwarder now hold senders for the life of the
process, so `rx.recv()` can never return `Err` and the backstop is gone. What replaces it is
that `spawn_reader` sends exactly one terminal event on every exit it has: `Closed` when
stdin ends, `ReadError` when a line will not decode, and nothing at all only when its own
send already failed, which means the loop is gone. All four shutdown paths were exercised
live: `quit`, stdin closed with no `quit`, junk then close, and invalid UTF-8, which still
prints exactly one `{"t":"error","where":"read",...}` line and exits 0.

**The pool is built when the backend starts, not when the first `thumb` arrives, and that
costs.** Measured on the media fixture against the pre-plan tree, the backend child's peak
PSS went from 1004 kB to 1528 kB, three runs each and no overlap, and backend startup went
from about 2.5 ms to about 3.7 ms, slower in 15 of 15 interleaved pairs. Decomposed on the
same fixture: the pre-plan tree runs 1 thread at about 1200 kB, Task 8's reader thread makes
it 2 threads at about 1400 kB, and this task's four workers plus the forwarder make it 7
threads at about 1760 kB. **That measurement predates `8fa64ee`, and the rest of it is gone.** The
second copy of the alias and thumbnailer tables was what carried it, and `Pool::new` now parses
nothing: both tables arrive as `Arc`s from the caller (`src/backend/thumbs.rs:76`,
`src/backend/run.rs:104`), which is the 0.11 MiB recorded above. What eager construction still
costs is four worker threads blocked on a condvar, and a backend whose client never asks for a
thumbnail still pays for those, `flea --backend` driven from a script being exactly that client.
Constructing the pool on the first `thumb` request would give back 0.042 MiB and about 0.32 ms;
it is dropped on its size alone, and the measurement behind those two numbers is in "Backend
memory levers that were measured and dropped".

### Thumbnail requests in the GUI

`ui/Pane.qml` decides which rows are asked about and `ui/js/Thumbs.js` does the arithmetic, as
pure functions with no QML imports so `./tests/js.sh` can redden on a mutation. **`Thumbs.viewport`
is the range rule itself**, the viewport in row indices clamped to the last row of the listing, and
it lives there rather than inline in `requestThumbs` so a mutation reddens `tests/js.sh`: dropping
its clamp fails "a listing shorter than the screen clamps to its last row". **The request
names visible rows and nothing else**, which is the client half of the rule the backend enforces:
nothing prefetches, warms, or asks for the screen ahead.

**`viewport` measures the window in pixels, because a `contentY` that is not row aligned straddles
one more row than the window holds.** `first` is `floor(contentY / rowHeight)` and `last` is
`ceil((contentY + visibleRows * rowHeight) / rowHeight) - 1`, still clamped to `total - 1`. At this
box's constants, a 1312 px list with a 37 px row and `visibleRows` 36, the old
`first + visibleRows - 1` left a 37th row up to 16 px on screen with no thumbnail for every
`contentY mod 37` in 21 to 36. The bound in `tests/ui.sh thumbs` is therefore
`requests * (visibleRows + 1)`. **The extent is derived from `visibleRows` and not from
`list.height`, which the function is not given**, and `visibleRows` is `ceil(height / rowHeight)`,
so the padded extent also over-asks by exactly one row for a `contentY mod 37` in 1 to 20. That is
one decode for the row immediately below the fold, it errs toward the safe direction, and the exact
rule would need the pixel height threaded through for no behaviour anybody can see.

**Two other places compute the same pair and neither is this rule.** `ui/Pane.qml`'s
`ListView.onContentYChanged` clamps the cursor with `first + visibleRows - 1`, where clamping to
fully visible rows is what is wanted, and `requestIfDrifted` uses `firstVisible + visibleRows` with
no `- 1` as a refetch margin heuristic. Do not unify them with `Thumbs.viewport`.

**One settle timer, restarted by every scroll, by every arriving window of rows and by every
viewport change.** A `Timer` runs at `firstSettleMs`, 70 ms, until a listing has issued its first
request and at `settleMs`, 120 ms, after that, and `open()` puts it back to 70 for every new
listing. It is restarted from `ListView.onContentYChanged`, from `onRows` and from
`onVisibleRowsChanged`. While the list is moving, `contentYChanged` fires every
frame and keeps pushing the deadline out, so a fling issues no request until it stops. `tests/ui.sh thumbs` holds that closed with a budget:
one fling across a 200-row fixture must leave the request count between one and two higher than it
started, the upper bound catching a request per scrolled frame and the lower one catching a settle
timer no scroll restarts. Its sampling loop, which watches the cursor move while the count does
not, is corroboration and not the gate. `tests/ui.sh nosweep` asserts a full traversal of the
100,000-file fixture leaves the request count at zero and the shared cache the size it started.
120 ms is a feel decision, confirmed on the box against 60 ms and 250 ms rather than measured.

**The first screen races the compositor's resize, and `firstSettleMs` exists to lose that race on
purpose.** Flea's `FloatingWindow` declares `implicitHeight: 600` at `ui/shell.qml:18`, Hyprland
then tiles it to the full screen, and the viewport a request would name depends on which of the two
arrives first. Before the resize the list is **526 px** tall and `visibleRows` is **15**; both
`ui/Header.qml` and `ui/StatusBar.qml` declare `implicitHeight: Theme.rowHeight`, which the IPC
seam reports as 37 here, so 526 plus 37 plus 37 is exactly the declared 600. After the resize the
list is 1312 px and `visibleRows` is 36.

**The resize latency is a range and the sign is not stable**, measured with `console.log(Date.now())`
stamps inside the UI and read back from the qs log. On a 200-file fixture, where the listing is fast,
the resize landed **4 to 41 ms AFTER the rows in 14 of 20 runs and 34 to 40 ms before them in the
other 6**, and the negatives all clustered at the end of the batch as the box warmed. On the
2001-file media fixture the listing is slow enough that the resize landed **33 to 42 ms before the
rows, 6 of 6**. So neither event can be assumed to be first.

Plan 5 Task 5a first tried removing the timer from the initial listing altogether. The request went
out 9 to 10 ms after `onRows` instead of 134 to 140, and the media `settled` column improved in 9 of
9 pairs by a median 113 ms, **but on a fast listing it named the 15-row viewport and 21 of the 36
rows the user was looking at kept their generic icon until something scrolled**, because nothing
re-requested. `tests/ui.sh thumbs` showed it as `added=51` against base's `added=72`, in 5 of 7 runs
rather than all of them, because it is a race and not a deterministic bug.

**What shipped instead debounces both events rather than betting on either.** `firstSettleMs` is
70 ms, `onVisibleRowsChanged` restarts the settle as well as `onRows`, and `requestThumbs` latches
`settle.interval` to the 120 ms fling debounce the first time it actually runs. Whichever of the two
events is last re-arms the timer, so the request names the final viewport either way. **70 is chosen
against the measured 41 ms worst case with 29 ms of margin**, and the failure mode if that margin is
ever blown is loud rather than silent: the timer fires early, the viewport change re-arms it, and the
first screen takes two requests, which `tests/ui.sh thumbs` fails on. It does not go back to missing
thumbnails.

Measured on the shipped tree: `requests=1` and 36 of 36 rows filled in 8 of 8 runs on the fast
fixture, the request issued at `h=1312 vis=36` with `interval=70` in 10 of 10 probe runs, the
request going out after `onRows` against base's 134 to 140, and the media `settled` column better
in 12 of 18 interleaved pairs across two nine-pair batteries, median to median 40 ms then 24 ms,
pooled median 33 ms per pair. **The time after `onRows` is a range across two batteries and not a
constant**: the second read 83 to 100 ms over 12 runs against the first's 81 to 87, so the honest
span is 81 to 100 ms.

**The media PSS cost this change was first reported to carry did not reproduce, and the honest
statement is that there is no established cost.** The first battery had it 216 KiB higher, 8 of 9;
the second, same comparison and same harness, had it 122 KiB LOWER, 6 of 9. Pooled over 18 pairs it
is 87 KiB higher, 11 of 18, against a within-arm spread of **341 to 586 KiB across all four arms of
the two batteries**. The 437 to 586 first quoted here dropped its own lowest arm, 341, in the
direction that supports the withdrawal it justifies; read it as a range over every arm, not as a
floor. Two batteries flipping the sign majority is the same instability the field benchmark shows,
so treat any sub-megabyte PSS claim from a nine-pair batch as unproven until a second batch agrees
with it.

**`open()` resets the interval, so every listing gets the short settle and not only the first**, and
`requestThumbs` latches only once `work.ask` is non-empty, so a directory it asks nothing about does
not spend the first screen's budget. Both measured, 3 of 3, by logging `settle.interval` on entry
and on exit through three nested directories: images then images then text-only gave entry 70, 70,
70 and exit 120, 120, **70**, with two `thumbRequests` for the three listings.

**Two thresholds, and only one of them is the request count.** Write `g` for the resize's arrival
relative to `onRows`, so `g` is negative when the resize is first. Base fires at `onRows + 120`; the
repair fires at `max(onRows, resize) + firstSettleMs`, which is `onRows + 70` when `g <= 0` and
`onRows + g + 70` otherwise. So the first screen takes **two requests when `g > 70`**, and the
repair is **slower than base when `g > 50`**, by at most 20 ms in the band between. Against the
measured worst `g` of 41 ms that is 29 ms of margin on the first and 9 ms on the second.

**The slower-than-base band does not touch a field column, and that was measured rather than
assumed.** Both field fixtures are slow listings, and `g` came out NEGATIVE in every pair of both
batteries taken. **Read its size as a range across those two batteries and not as a constant:** the
reproduction got 38 to 53 ms negative on the media fixture and 38 to 72 ms negative on the
100,000-file scale fixture, against the first battery's 6 of 6 at 33 to 42 and 37 to 43, so the
honest span is 33 to 53 negative on media and 37 to 72 negative on scale. The scale fixture also
has no thumbnailable row in it, so its request is empty and this timer cannot move its column at
all. The positive-`g` case only appears on a directory small enough to list before the compositor
answers, which is a 200-file fixture here.

**70 is the tunable and 120 is the one-constant rollback.** The yield is exactly `120 - firstSettleMs`
on any slow listing, so setting `firstSettleMs: 120` restores the old behaviour in one line and gives
back the whole latency win. Lowering it widens the two-request margin and narrows the
slower-than-base one; raising it does the reverse. 70 was chosen because both margins clear the
measured 41 ms worst case.

**One known corner comes with it, recorded rather than fixed.** The anti-fling settle is 70 ms
rather than 120 from the listing arriving until a settled viewport actually names an un-asked
thumbnailable row, because `requestThumbs` latches to 120 only when `work.ask` is non-empty
(`ui/Pane.qml:367-368`). A slow wheel scroll started inside that window can issue a request
mid-scroll where it previously would not. **On a text-only listing the window never closes at
all**: nothing is ever asked for, so the interval stays at 70 for that listing's whole life. That
case is verified benign rather than argued: a full 2400-detent traversal reports
`NOSWEEP requests=0` with the cache count unmoved, because `Backend.thumb([])` early-returns and
never sends or counts a request. Either way it is one request naming one viewport, so it breaks no
rule; no test covers the mid-scroll request, and `tests/ui.sh thumbs` cannot reach it because its
fling runs entirely after the latch.

**Do not reach for the two repairs that look obvious and are not.** Gating on the window reaching
its declared size fails, because at 526 px it IS at its declared size. Requesting on the first
`visibleRows` change fails, because on a slow listing that change has already happened before the
rows arrive. And a bare short interval with no `onVisibleRowsChanged` restart is a margin bet
against a 41 ms worst case that was measured at 40 ms twice in twenty runs.

**The request count cannot prove the rule on its own, so the cache does.** `thumbRequests` counts
LINES, not rows, so an implementation that swept a whole directory in one `thumb` naming every row
would score one request and satisfy every count assertion. `tests/ui.sh thumbs` therefore asserts
what `~/.cache/thumbnails/large` grew by against a viewport-derived bound, the requests issued
times `pane.visibleRows`, which the read-only seam reports as `visibleRows()`. Every request names
a subset of one viewport, so the rows generated cannot exceed a screenful a request, and a clean
run sits exactly ON that bound rather than under it. The WINDOW's own height over the row pitch was
tried first and rejected: it exceeds the list's by the header and the status bar, and one request
can never generate more than `MAX_QUEUE` plus the worker count, 74 rows, however large the
directory, so on this box the loose form bounded 74 by 74 and a whole-directory sweep passed it.
The bound is paired with a floor, because `added` is a delta over a cache every application shares
and a warm one makes the ceiling vacuous: the fixture carries the run's pid so its entries cannot
pre-exist, and the floor fails the case if a screenful was never generated at all. Together they
are the only end-to-end witness in the tree that a request names visible rows and nothing else.

**The map has three states and `undefined` is the fourth.** Absent means unknown, `null` means
asked and waiting, a string is the answer, and an empty string is a real answer meaning this row
has no thumbnail. Only the last two are terminal: an answered row is never asked about again for
this listing, and only a row still `null` can be cancelled, because only that row has a job the
backend could drop. `open()` clears the whole map, since an index names a different file after a
new listing.

**The `t` flag over-promises and the renderer absorbs it rather than the wire narrowing.** `t`
is true for every symlink whose name resolves to a declared type, whatever the link points at,
so a symlink to a directory, to a FIFO, to a loop or to nothing at all is asked for like any
other row. Every one of them is answered with an empty `file` in microseconds, because the
request path stats the target and refuses it there, and an empty string is a terminal answer in
the map. `Row.iconSource` reads it as no thumbnail and falls through to the themed icon, so the
row simply keeps the icon it already drew: nothing blinks, nothing waits and no slot is left
holding a gap. That is the deliberate answer to the over-promise recorded in
`docs/protocol.md`, and it is why this plan renders defensively instead of narrowing the flag,
which would be a wire change with its own documentation and test surface.

**`Backend.thumbcancel` never sends an empty `rows`.** That form cancels EVERYTHING queued, so a
viewport-change handler that sent one would strand every row it did not name; `Backend.thumb`
guards the same way so an empty request never leaves the process. `requestThumbs` deliberately
does NOT test the two lengths itself: a caller-side guard makes the guard here dead code, and this
is the class of bug that already cost this project a branch, so the invariant stays on the live
path with exactly one author. `thumbRequests` counts ATTEMPTS, not lines on the wire: the
increment precedes `send`, which may queue the line until the child starts or fail it outright.
It is what the settle gate asserts through the IPC seam.

**`ui/Backend.qml` carries two `// Sample input:` lines and that is deliberate.** `receive`
consumes four shapes, `listed`, `rows`, `error` and `thumbed`, and the two carrying row data are
sampled; each sample is a real line a reader can check the parser against, so folding them into one
comment would cost the thing the comment is for.

**`onThumbed` drops a result while a listing is in flight.** `run.rs` clears `State::asked` while
handling `list` and drops every later result, so on the wire every `thumbed` for a superseded
listing precedes the new `listed` line. The guard drops exactly those. It is argued from the wire
order, not proven by a test: deleting it reddens nothing in a single-directory case.

**`thumbCap` is 240 and bounds a policy bug, not normal use.** Only rows a viewport held are ever
recorded, so seven screens of history at this row height is already generous; the cap exists so a
future handler that records more costs memory slowly rather than without limit.

Three behaviours a later session will meet, each known and none fixed here.

- The cap evicts by insertion order and not by distance from the viewport, so returning to the top
  of a listing after visiting more than 240 rows evicts and re-asks the rows now on screen. It
  self-heals through a cache hit, so it is a flicker and not a loss.
- A window resize that exposes rows without moving `contentY` used to leave them unasked until the
  next scroll. **Task 5a closed this**: `onVisibleRowsChanged` restarts the settle, so a resize is
  debounced exactly like a scroll and the rows it exposes are asked for once the viewport stops
  changing.
- `open()` clears the map, and `Backend.sort` invalidates every row index on the backend side just
  as hard. `sort` has no caller in the shipped UI today; whoever wires the header's sort controls
  has to clear the map there too.

**`case_thumbs`'s own live-sampled fling check is this project's documented known red, and the
root cause is measured rather than assumed.** The check backgrounds `omarchy-drive scroll down
$fling_clicks` and polls `cursor`/`thumbRequests` in a loop, looking for a sample where the cursor
moved and the counter did not. Measured directly on this box: `omarchy-drive scroll down N` takes
at minimum ~140 ms even at the smallest `N` that moves the cursor at all, rising to roughly two
seconds at the fling's own 1500 clicks, and every separate `omarchy-drive ipc` round trip costs a
further ~190 ms on its own. Both figures already exceed the 120 ms settle debounce, so any check
taken through a shell-level IPC call, whether immediately after a scroll command returns or
live-polled during one, races the debounce and reliably loses: by the time the round trip
completes, settle has already fired. This is a property of the CLI tool in this environment, not
of the settle logic it is trying to observe. Task 16's directory-heavy twin to `case_nosweep`
(`ui.sh`, `nosweep-dirs` fixture) hit the identical `during_samples=0` symptom on first write,
reproduced it on unmodified `thumbs` before the twin existed to rule out a Task 16 defect, and
shipped instead with a before/after delta bound (`1 <= delta <= 2`, one settle's worth of requests
per fling rather than a literal zero), the same upper bound this section's own settled-state check
already accepts and for the identical reason.

## Backend memory levers that were measured and dropped

Measured on 2026-08-30 by Plan 5's Task 5b, warm, against a backend driven over its own wire with
THP disabled in the child exactly as the shipped launcher leaves it. Every `Pss` here is
`smaps_rollup`'s `Pss` in KiB; divide by 1024 for MiB, and never subtract these against a figure
taken with a divisor of 1000.

**Read "The listing arena returns to the OS" before this section.** The glibc threshold lever is
NOT in this list: it SHIPPED, and the first round of this task wrongly recorded it here as a null.
The measurement that produced the null opened one large directory and then a small one, and that
sequence is structurally incapable of showing the ratchet, because the ratchet only strands the
NEXT large allocation and there was never a second one. **A null is only as general as the
sequence that produced it.**

**Building the thumbnail `Pool` lazily is worth 0.042 MiB and about 0.32 ms, and both are below
anything that matters.** Once the duplicate table parse is gone (see "Thumbnail pool") all that
eager construction still costs is four worker threads blocked on a condvar. Measured by building the
same tree at `THUMB_WORKERS` 1 instead of 4: three fewer threads move `Pss` from a 1337 KiB median
to 1305 over nine interleaved pairs, about 10.7 KiB a thread, so four are about 43 KiB, which is
0.9 percent of the 4957 KiB the backend holds on the scale fixture. The startup half of the cost
was measured separately over forty interleaved pairs, fork to the first `listed` line: 3.3665 ms
against 3.1281, so three threads are 0.238 ms and four are about 0.32 ms, against a 77 ms startup
budget. The sign split is 35 of 40. The 202,800 KiB of `VmSize` those three threads also give back
is glibc arena and stack reservation with zero resident pages.

**Nothing blocks the laziness, and that was checked before it was dropped.** There is no recorded
reason for the eager construction anywhere: not in `9d26434`, which introduced it, and not in this
file. The nearest constraint is `run.rs`'s note that the workers hold `Sender<Done>` clones so the
forwarder can never see a disconnect, and that survives trivially by holding the original sender
until the pool is built. Nor is there a race, because `worker` only waits on the condvar while the
queue is empty, so a job pushed before a worker has run is found on that worker's first lock. It
is dropped on its size alone.

**`glibc.malloc.arena_max=1` is not a memory lever.** It removes 393 MiB of `VmSize`, 410,988 KiB
down to 17,904, which is arena address-space reservation with zero resident pages. Its effect on
`Pss` was 1374 against 1394 KiB on a **single** unpaired sample, which is one sample and is not
evidence of a direction; a second party's single sample pointed the other way. It is recorded
here only as the control that proves this box reads `GLIBC_TUNABLES` at all.

## Write operations and the undo journal

Seven requests write: `transfer`, `transfercancel`, `trash`, `rename`, `duplicate`, `mkdir` and `undo`.
`docs/protocol.md` carries the wire; this is the part a reader of the code needs that the wire does not
say.

**They name paths, not row indices.** Every read request on this wire (`window`, `thumb`, `dirsize`)
names a row of the current listing, because a viewport is a fact about the listing. A write outlives the
listing it started from: a copy of a large tree is still running when the user navigates away, and a row
index would name a different file by then. So the write requests take absolute paths and the backend
never consults the listing to serve one.

**One of `transfer`, `trash` or `duplicate` runs at a time.** `opsdispatch.rs` holds `Ops::running`, and a second `transfer`,
`trash` or `duplicate` while one is live answers an `error` line rather than queueing. The reason is the
surface, not the backend: the operations design gives transfers the status bar's single transient slot,
so a second concurrent operation would have nowhere to report itself. `rename` and `mkdir` are exempt because
neither spawns at all. An `archive` or a `convert` never claims the slot either: `Ops::claim_id` numbers them
and they run alongside by design, so the cap was never one write of any kind.

**`rename` and `mkdir` run on the loop's thread, the other three spawn.** Each is one syscall, so a
thread would cost more than the work. `trash` shells to `gio` twice for the list diff plus once to
trash; `duplicate` may copy a whole tree; a `transfer` is unbounded. Those three send their results back
through `Event::Op`, joined onto the loop's receiver exactly the way the thumbnail pool's `Event::Thumb`
already is, so the loop stays the only writer of stdout.

**Every write creates its target exclusively, and this is the module's whole safety story.** A file copy
opens with `create_new`, a directory copy and a `mkdir` use `create_dir`, a symlink copy uses `symlink`, and a rename
or a move uses `renameat2` with `RENAME_NOREPLACE`. None of them can destroy a file that is already
there, and none has a check-then-write window another process can slip through. `std::fs::rename`
silently replaces its target on Unix and is never used here.

**The journal records only what an operation created or moved.** `undo.rs`'s `Step` has exactly four
shapes: `Moved` (rename back), `Created` (remove it), `MadeDir` (remove it while it is still empty, because
whatever is inside it now was put there by someone else), `Trashed` (restore it). A path an operation merely
read is never recorded, so an undo cannot delete a file the operation did not put there. An operation
whose step list is empty is not pushed at all, so a refused rename leaves nothing to undo. Steps reverse
newest first, and a failing step stops the rest rather than half-reversing. A copy that fails short of a
cancel (ENOSPC, EPERM, a socket deeper in the tree) leaves the partial destination it created on disk,
because removing it on a transient error would destroy data, and `copyfile.rs` reports that path in
`Progress.partial` so `transfer` and `duplicate` journal it as a `Created` step and the existing undo
path removes it. A destination that already existed is never reported, because nothing was created there.

**The trash URI is captured at trash time, and that is forced by a measured fact.**
`gio trash --restore` refuses an original path (`Location given doesn't start with trash:///`), and two
files trashed from one path both list that same original, so a later lookup cannot tell them apart.
`trash.rs` therefore reads `gio trash --list` before and after the call and keeps the entries that are
new. The operations design says restore takes the original path; it does not, and this is the deviation.

**Which paths were trashed is read off the filesystem, not off `gio`'s exit status**, which covers the
whole batch. A path still present afterwards is counted `failed`.

**A cancel stops the item in flight and removes its partial destination.** `copyfile.rs` checks the
cancel flag between chunks and unlinks what it had written. The operations design says the in-flight
item finishes; it does not here, because a cancel that waits out a multi-gigabyte copy is not a cancel
and a half-written file is not a result anyone asked for. A `quit` or a closed stdin cancels the same
way and waits for the terminal line, so shutting down mid-copy leaves nothing half-written either.

**Testing them is `tests/ops.sh`**, which drives the real binary over a FIFO rather than a pipe: an
operation answers asynchronously, so a piped script would send `undo` before the operation it meant to
reverse had reported. Its sandbox is under `FLEA_FIXTURE_ROOT` and not `/tmp`, because `gio trash`
refuses `/tmp` and `/var/tmp` with "Trashing on system internal mounts is not supported"; `/home` is the
only mount on this box a trash round trip can be exercised on. Hard rule 9's guard is in the script
itself, and the Rust tests use `TestDir`.

## Deliberate corners

- The name arena uses u32 offsets, so it tops out at 4 GiB of names. No directory
  reaches that, but a later search phase reusing this arena could.
- The context menu's backdrop consumes the press that dismisses it, so a left click on a row
  while the menu is open closes the menu without selecting that row; `tests/ui.sh menu`
  covers this and it is behaviour, not a bug.
- `ui/Preview.qml` decodes an untrusted file inside the GUI process on Space, where thumbnail
  decoding runs argv-direct under `bwrap` instead. The difference is consent and blast radius: a
  thumbnail sweep decodes files the user never chose, a preview decodes the one file the user
  pressed Space on, the same trust the user already extends by opening it in any other viewer.
  The no-sweep rule is untouched: a preview reads exactly one path per open and issues no
  thumbnail requests, which `tests/ui.sh nosweep` still covers unchanged.
- `ui/Preview.qml`'s markdown kind is a name-suffix check (`.md`/`.markdown`/`.mkd`), not an icon
  check: `text/markdown`'s own `/usr/share/mime/generic-icons` entry is `x-office-document`, not
  `text-x-generic`, so the row's icon name alone cannot carry it. It is a display-mode choice
  over an already-text-shaped file, not client-side content sniffing.

### Size formatting matches real GLib, not Finder, and not the design spec's assumed rounding

`ui/js/Format.js`'s `size()` is GLib's SI, one-decimal ladder (`g_format_size`), because storage
capacity has no power-of-two basis and hard rule 2 picks the desktop's own convention over
Flea's old IEC one. Finder renders 26,950,000,000 bytes as "26.95 GB". The design spec that
named this fixture quotes that string as a value that "reproduces exactly" and assumed GLib
rounds it up to "27.0 GB"; measured live on this box (glib 2.88.3, `python3 -c "from
gi.repository import GLib; print(GLib.format_size(26950000000))"`, and cross-checked with
`numfmt --to=si 26950000000`) real GLib prints "26.9 GB", not "27.0 GB". The nearest IEEE-754
double to the decimal 26.95 is 26.949999999999999289, strictly below the true midpoint between
26.9 and 27.0, so any correctly-rounded one-decimal formatter, GLib's included, rounds it down.
`tests/js/format.js` pins "26.9 GB" to match the measurement, not the design spec's assumption.
A later reader comparing this test against that spec should trust the live measurement here,
not the spec's quoted string.

### Transparent huge pages

`/sys/kernel/mm/transparent_hugepage/enabled` is `[always]` on this box, and Quickshell is the
most huge-page-inflated process on it. Every `FileView` and every Qt worker gives `qs` another
thread, each thread stack is collapsed onto a 2 MB huge page, and the settled process carried
roughly 70 MB of `AnonHugePages` against roughly 80 MB of anonymous memory: that memory was
rounding, not allocation. `gui::exec_qs` therefore calls `prctl(PR_SET_THP_DISABLE)` immediately
before `Command::exec`. That is the last point that owns the launch and the only one that needs
to, because the setting is preserved across `exec` and inherited by every child. `qs` then
settles around 44 to 45 MB of PSS instead of around 101 to 104 MB, on the media fixture and on
the 100,000-file fixture alike, with `AnonHugePages` at zero.

`prctl` is declared once in `src/thp.rs` as an `extern "C"` symbol rather than pulled in with a crate, because
`std` already links the system libc and this project has no Cargo dependencies. The fixed-arity
declaration is not a tolerated mismatch against a variadic symbol: `objdump` on glibc 2.44 here
shows `prctl` is a raw syscall stub, `endbr64; mov %rcx,%r10; mov $0x9d,%eax; syscall`, so the
prototype matches the machine-level contract exactly. The kernel rejects a non-zero `arg4` and
`arg5` for this option, so passing all five is required rather than merely harmless. **`arg3` is a
flags argument and not a zero-only field**, corrected on 2026-08-30 after this section had asserted
the opposite: `/usr/include/linux/prctl.h` lines 181 to 190 here define
`PR_THP_DISABLE_EXCEPT_ADVISED` as `1 << 1` and say outright that flags apply when disabling, and a
direct probe on this 7.1.9 kernel returns 0 for `arg3 = 0` and for `arg3 = 2` while returning
EINVAL for `arg3 = 1`, for `arg4 = 1` and for `arg5 = 1`. The shipped call passes `arg3 = 0`, the
plain disable, which is what every figure in this section measures. The call is
deliberately not fatal: a non-zero return prints one sentence and the launch continues, since a
larger process is not a reason to refuse to open a window.

`PR_THP_DISABLE_EXCEPT_ADVISED`, `arg3 = 2` on this 7.1 kernel, was built and measured and buys
nothing: 42.08 MB against the shipped call's 42.12 MB, because nothing in the Qt or Mesa stack
advises huge pages. It also leaves `THP_enabled` reading 1, so the shipped test would not pin the
mechanism under it. The simpler call is the right one.

**// corner: the setting is inherited by every descendant, and that is fine only while Flea launches
nothing foreign.** Today the only descendants are the backend and the `bwrap` thumbnailer children,
both measured to pay nothing for it: `ffmpegthumbnailer` on a fixture clip ran 88 to 94 ms with huge
pages on against 89 to 92 ms off over five interleaved pairs, output byte-identical. A foreign program
CAN now be launched from the window, because Enter on a file runs `flea --open`, and the undo the
corner asked for ships with it: `open::open` calls `thp::enable()`, which is
`prctl(PR_SET_THP_DISABLE, 0, ...)`, in the `flea --open` process before it spawns `xdg-open`, so the
opened program inherits huge pages back on. That call lives in `src/open.rs`, after the
`canonicalize` and after the directory refusal and immediately before the spawn, so a path that
never reaches a handler never changes the setting, and `src/thp.rs` holds the one `extern "C"`
declaration that both it and `gui::exec_qs` call. Without it Flea would silently change a
system-wide performance setting for every application launched from it, for that
application's whole life. `tests/modes.sh` pins it through a
stub `qs` that execs `flea --open` against a stub handler reporting its own `THP_enabled`, which is
the only shape of check that can redden when the `thp::enable()` call is deleted. See "Opening a
file" below.

`--backend` never reaches this branch, so a backend started from a shell keeps huge pages on and
its stdout is byte-identical either way. The backend `qs` spawns does inherit the setting, and
that was measured to cost it nothing: over nine interleaved pairs on the 100,000-file fixture the
listing read stayed in the same 22 to 25 ms band in both arms with the per-pair sign split five
to four, and the sort in the same 2.1 to 2.3 ms band.

The price was paid in CPU and not in latency. Over eighteen interleaved pairs at two workload
sizes, the same scripted scroll workload cost `qs` a few percent more CPU with huge pages off,
the same fraction at both sizes rather than a growing one, while the input-to-rows latency of a
jump to the far end of the listing did not move: equal medians at both sizes and an even per-pair
sign split. The extra minor faults are in the low thousands, far too few to account for the CPU,
so what is being paid for is page-walk work rather than allocation.

### Opening a file

`flea --open <path>` is the one route that opens a file with the desktop's own handler, and both
front ends use it rather than launching anything themselves. It exists in Rust because the huge page
setting above is inherited by every descendant, so a program launched from QML would run its whole
life with transparent huge pages disabled without ever asking; `open::open` hands the setting back
before it spawns.

It spawns `xdg-open` and exits, it does not `exec` into it. `/usr/bin/xdg-open` line 977 is
`env "$command" "$@"` inside `search_desktop_file`, with no `exec` and no `&`, so `xdg-open` blocks
for the whole life of the application it launched. An `exec` would therefore leave a Flea-descended
process alive for that whole life, as a child of the `qs` process, which Quickshell may kill when
Flea quits. `Command::spawn` is still argv-direct exec and never a shell, which is the binding
requirement. The child gets `process_group(0)` so that nothing which later kills Flea's process group
reaches the opened application.

The child also gets `/dev/null` on all three descriptors. Quickshell hands `flea --open` pipes and
closes them the moment it exits, so a handler that inherited those pipes dies with `SIGPIPE` on its
first write. Measured on this box: `imv` was spawned and gone inside 250 ms without ever mapping a
window, and the same spawn with `Stdio::null()` opened normally. `tests/modes.sh` pins it by giving
`flea --open` a pipe of its own and reading the stub handler's `/proc/$$/fd/1`.

The path is `std::fs::canonicalize`d, which does two jobs at once: a symlink opens its target rather
than the link, and the absolute result cannot be read as a flag by the child, so a file named
`--output=/etc/x` is passed as `/tmp/.../--output=/etc/x`. A broken symlink fails canonicalization,
which is the error path.

The exit statuses are the whole contract: `0` is a successful handoff, `2` is anything that could not
be opened and carries one elided sentence on stderr, and `3` means the resolved target is a directory
and carries no output at all. A directory is refused rather than handed on because
`xdg-mime query default inode/directory` is `org.gnome.Nautilus.desktop` here, so opening one through
`xdg-open` from inside a file manager opens a different file manager; the caller navigates instead.
`flea --open` with no path and `flea --open a b` both fall through to the unknown-flag branch, which
names the flag and exits 2.

`ui/Opener.qml` is the window side of that contract and the only component in the tree that
launches a foreign program. It holds one `Process`, because `flea --open` decides and exits in
milliseconds, and its `onExited` is the whole mapping: `0` says nothing, `3` raises
`isDirectory` and `Pane` navigates there, and anything else raises `failed` and `Pane` writes one
sentence to the status line. `Pane.openCursor` sends a row with `d` true to `open()` and every
other row to the opener, so a symlink to a directory reaches the opener, comes back 3 and
navigates; `tests/ui.sh open` asserts all three answers on one listing.

**`xdg-open` ignores `Terminal=true`, and that is the box's XDG configuration rather than Flea's
to work around.** `search_desktop_file` reads only `Exec`, `Icon` and `Name`, so a handler that
needs a terminal is run without one. `xdg-mime query default text/plain` is `nvim.desktop` here,
so Enter on a text file spawns a headless `nvim` that maps no window and that the user never
sees. The remedy belongs to the operator and it is `omarchy default editor`, which rewrites the
association. Wrapping the handler in a terminal from inside Flea would mean Flea deciding which
programs are terminal programs, and that is the judgement the desktop's own database exists to
make.

### No type-ahead, and trash is a pair

Removed in v0.1.3 for issue 7. The type-ahead was the fallback for a printable key that
`keys.toml` did not bind, so it only ever worked for the letters nothing else had claimed:
typing `Vid` toggled the selection on `v`, jumped on `i`, and trashed the row on `d`. An
unbound letter now answers `Press / to filter this listing by name.`, which is the filter
that already narrows the held rows without a walk.

Bare `d` is `trashArm`, not `trash`: `ui/js/Trash.js` owns the pair, the first press arms and
says so, a second inside `Trash.ARM_MS` trashes, and `Focus.handleKey` clears
`pane.trashArmedAt` for every other action so an arm never survives the key after it. `Delete`
and `Ctrl+Delete` still go on one press, because neither is a letter a name is typed with. The
sheet draws `dd`, and `tests/js/keymap.js` resolves a doubled cap as that one character.

### Theme roles and sources

`Theme.qml`'s `applyColors` assigns only Flea's eight palette roles and ignores unknown
keys because Omarchy themes contain more roles than Flea uses. Alacritty-derived palettes
contain neither background ladder key, so the measured surface fallback is `selection`.
Shell parsing consumes only `[font]` and `[spacing]` because Flea has no bar, popups,
tooltip or lock screen.

### Plain text filenames

Every filename in `Row.qml` uses `Text.PlainText`. `Text.AutoText` treats markup in a
filename as rich text, so it is never safe for backend-provided names.

### A key bound ahead of its feature says so

The rule, applied on 2026-09-02 after two silent-by-accident keys and one silent-by-design: every
row in `keys.toml` either works or says why it does not, in a sentence written for the user and
never an action name. The list is empty as of v0.1.3: both entries that were on it have landed.
`t`, `w` and `1` to `9` open, close and switch directory tabs, one live listing with a snapshot per
hidden tab and the strip only once there are two; closing the last tab is refused and nine is the
cap because those digits are the jump keys. `:` and `Ctrl+L` open the path bar, whose action is
`pathBar` rather than `palette` (the old name read as a command palette, and `ui/js/Palette.js` is
the theme's colour file); `Focus.handleKey` opens it beside the keymap sheet as a global action and
`ui/ChromeBar.qml` carries the field, answered by `keycase_pathBar` in
`tools/flea-acceptance-drive`, which drives the key and reads `pathBarOpen` off the seam. The generic
`<action> is not built yet.` fallback below those stays as the loud failure for a row the
dispatcher does not answer, and should never be reachable from the shipped table. `Ctrl+Shift+N`
was on this list for one afternoon and is not on it now: `295e757` routed the action through
`Focus.act` to `Ops.newFolder`, which sends the backend's `mkdir` (docs/protocol.md), and
`ui/ContextMenu.qml` carries the same action as its New Folder row.

### Finder's chords, Cmd read as Ctrl, beside the vim keys

Hyprland grabs every `SUPER` chord, so no app ever sees one; Omarchy's own universal clipboard
(`/usr/share/omarchy/default/hypr/bindings/clipboard.lua`) works by injecting `CTRL+C`, `CTRL+V`
and `CTRL+X` into the focused surface with `send_key_state`. So the Finder table lands in
`keys.toml` as `[[ctrl]]` and `[[ctrlshift]]` rows, every one additive to the bare vim key it
doubles, and `tools/flea-keymap-gen` checks the shifted rows first and falls through. Two Finder
conventions are refused on purpose and should not be reopened: Enter opens and does not rename
(every Linux file manager and the vim table open on Enter, and the TUI is generated from this
table), and `Cmd+D` is not duplicate because `Ctrl+D` already pages with `Ctrl+U` as its pair.
`Ctrl+E` goes through `ui/js/Eject.js` `release`: the rail's cursor row when the rail has focus,
otherwise `Mounts.holding`, the mounted removable volume whose path holds the listing, and the
verdict is `Mounts.railMenu`'s either way. `XF86Eject` and the transport keys are not bound here
and cannot be: `media.lua` binds them through `o.bind`, which is a consuming `hl.bind`, so a
client never receives them; the play key runs `omarchy-shell media playPause` against the MPRIS
player the shell tracks, and Flea's Quick Look (`ui/PreviewMedia.qml`, a plain QtMultimedia
`MediaPlayer`) registers no MPRIS name, so it is untouched by it. On the sheet a chord is `^c`,
`^N` with a capital is ctrl-shift, and two rows keep the bare key alone because `f ^f` beside
`find in subtree` (155 px) and `a ^k` beside `add network place` (169 px) overrun the 154 px
column at base-size 14 in JetBrainsMono; the README carries those two chords.

### Prewarm correlation

The current `listed` reply and two-line prewarm file carry no requested path, so the UI
cannot tell whether prewarmed content is stale. Production ignores `FLEA_PREWARM` until
the protocol gains that correlation and a first-paint measurement proves a real win.

### MIME globs

`/usr/share/mime/globs2` on this box holds 1596 lines, two of them the header comment,
leaving 1594 data lines. `mime::Db::from_str` drops 10 of those: 4 use a character class
or `?` (`*.so.[0-9]*`, `*.anim[1-9j]`, `*.[1-9]`, `[0-9][0-9][0-9].vdr`) and 6 carry a
star that is not the leading character, always trailing a literal prefix rather than
buried mid-pattern (`sconscript.*`, `cachegrind.out*`, `callgrind.out*`, `dockerfile.*`,
`makefile.*`, `readme*`). No line in this file actually has a star with literal text on
both sides. The remaining 1584 lines resolve to either a suffix or a literal name entry.

A line carries an optional 4th field, a comma-separated flag list, and `cs` is the only
flag this database uses. Exactly 5 lines carry it: `*.gs`, `perf.data`, `core`, `*.C` and
`*.c`, each immediately followed by an unflagged twin of the same glob and the same MIME
type. `Db` keeps two key spaces per lookup, a case-sensitive one keyed on the glob's
original text and a case-insensitive one keyed on a lowercased copy, and `lookup` probes
the case-sensitive space first. One shared key space cannot work here: the unflagged
`*.C` twin lowercases to the same `.c` key as the flagged `*.c` entry, ties at weight 50,
and wins on file order, which is how every `.c` file resolved to `text/x-c++src` before
this was fixed. The flag is honoured for literal names as well as suffixes, for one rule
rather than two, though no test can tell the difference on this box: both flagged literal
names have twins carrying the same MIME type.

- `thumbspec.rs`: the first file to declare a MIME type keeps it, and `load` sorts each
  directory's files by path as it reads them, so that winner is the same on every box and
  after every reinstall. `image/tiff` is declared by both `evince.thumbnailer` and
  `glycin-image-rs.thumbnailer`, and without the sort the winner was whatever `read_dir`
  happened to yield first, which POSIX does not specify. The sort is per directory and not
  across the whole list, deliberately: sorting the collected list would reorder the search
  path itself, and a `XDG_DATA_HOME` that does not sort before `/usr/share` would then let
  a system thumbnailer beat the user's own, which is backwards. `sorted_by_path` is a
  separate function so a test can prove the ordering without depending on what `read_dir`
  returned.
- `thumbspec.rs`: only a whole-token placeholder substitutes, so `--input=%i` would be
  passed through literally. Every one of the nine shipped files writes its placeholders as
  standalone tokens, so nothing on this box needs the embedded form.
- `thumbspec.rs`: `fields` reads `Exec`, `TryExec` and `MimeType` only inside
  `[Thumbnailer Entry]`, and a duplicate key inside that group is last-wins. All nine shipped
  files open with that group and carry no other, so the group test costs nothing here and
  closes a file that could otherwise hide the program it runs under a second heading.
- `mime.rs`: on a tie in weight for the same key, the first line in file order wins,
  because `insert_heaviest` only replaces a stored entry when the new weight is
  strictly higher. This is exercised by real data: eleven lines share `*.pem` at
  weight 50, and `lookup("cert.pem")` resolves to `application/pkcs7-mime`, the first
  of the eleven in the file.

- `scan.rs`: an entry the directory iterator itself cannot read (`fs::read_dir`'s
  per-entry `Result` came back `Err`) is dropped by `.flatten()` and never reaches the
  listing. An entry that read fine but whose `file_type()` call then fails is kept, as
  a file (`unwrap_or(false)`), because the entry itself is real even if the type
  lookup raced it. Two different failures, two different outcomes, on purpose.
- `meta.rs`: a row that existed during `scan` but is gone by the time `stat_range`
  reaches it (deleted, renamed) reports `size: 0, mtime: 0, mode: 0` rather than
  failing the whole window; one vanished file should not blank the screen.
- `sort.rs`: `parse_sort_by` answers `Err` for any key that is not `"name"`, `"size"` or
  `"mtime"`, and `run.rs` refuses it with an `error` line naming the key. It used to fall back to
  name order "so a stale sort key never refuses to list a directory", and that reasoning was wrong
  twice: `list` sorts by name itself, so a refused `sort` leaves the listing exactly as it was, and
  the fallback answered `kind` and `mode` as name order in silence, so a header mark reading Kind
  described an order that was not on screen and a descending sort was undone under it.
- `sort.rs`: the name sort is stable and stays stable, and `metasort.rs` holds `SortBy::Size` and
  `SortBy::Mtime` to the same rule. **Neither may reach for `sort_unstable_by`**: those keys are
  not total, because two files can share a size and two can share an mtime, so an unstable sort
  would order them arbitrarily and two runs could differ. `metasort.rs` breaks the tie on the name
  for that reason, and "Why the listing is an arena" says why the unstable sort loses on time here
  anyway.
- `run.rs`: a request line that is not valid JSON, or whose `"c"` is not one of
  `list`/`window`/`sort`/`quit`, becomes `Request::Unknown` and is answered with
  silence, deliberately: no error line, no crash, the loop just reads the next line.
  `tests/protocol.sh` asserts this ("junk produces no output and no crash").
- `run.rs`: `sort` with a `by` this wire never defined, `"kind"` and `"mode"` among them, answers
  `{"t":"error","where":"sort",...}` rather than silently sorting by name. An honest error beats a
  silently wrong order.
- `prewarm.rs`: see Predictable path writes above; the pid in the temp file name and
  the unlink-both-on-failure contract both live there rather than being repeated here.
- `scan.rs`: a non-UTF8 filename goes lossy in phase 1 and then cannot be stat'd, so
  it reports zeroes. The fix is a byte arena and belongs to a later plan. **The thumbnail
  path inherits it**: two names in one directory that differ only in their invalid bytes
  collapse to the same lossy string, so `State::asked` sees one path where the client named
  two rows, the dedupe swallows the second and that row is answered only when the listing is
  replaced or the process drains. Not fixed here, because the fix is the same byte arena and
  it is a whole plan's worth of change through `Listing`, `stat_range` and the wire.
- `main.rs`: `std::env::args()` panics with exit 101 on a non-UTF8 argv byte, so `flea --open`
  inherits it and answers none of 0, 2 or 3, but only a shell or a `.desktop` file can produce
  it and the window cannot, because `scan.rs` goes lossy in phase 1 and `Pane`'s `join` only
  ever sees that lossy name; same family as the entry above, pre-dating every mode, and the
  fix is `args_os` across all four modes.
- `open.rs`: between `canonicalize`, `is_dir` and `spawn` a path that vanishes or is swapped
  for a directory changes the answer, which a concurrent `rm` in the user's own session on
  this single-user machine can produce and which is not exploitable, since no shell and no
  privilege boundary is involved.
- `thumbcache.rs` and `ui/Row.qml`: the cache READ path is not sandboxed, and Plan 5 shipped
  the decoder that reads it. `lookup` reads a PNG out of `~/.cache/thumbnails/large` in the
  backend process and parses its `tEXt` chunks there; that parser is bounds checked and total.
  `Row.iconSource` then hands the same path to a QML `Image`, so Qt's PNG decoder runs
  unconfined in the `qs` process, on bytes this process did not write and that any application
  on the box may have written. **Generation is sandboxed and display is not.** The mitigations
  that exist are real and partial: the path always comes from the backend's own `lookup` and
  never from a filename or from anything a client names, the read is a plain `Image` with no
  scripting surface, and generation is still confined under `bwrap` with `prlimit`. What is
  left is Qt's decoder against a cache the whole desktop shares, and the residual risk is a
  decoder bug in a PNG some other application wrote. **This plan did not close it and closing
  it belongs to a later plan**: the candidates are decoding outside the shell process and
  validating the bytes before one sees them, and choosing between those is a design pass
  rather than a step in an implementation plan.
- `scan.rs`: `d` describes the link, not its target, so a symlink to a directory
  reports false. The UI still does not guess the target: only `d` navigates directly, and
  every other row goes to `flea --open`, which canonicalises and answers 3 for a directory,
  so the pane navigates on that answer rather than on a guess of its own. The ICON does
  follow the target, see "Icons in the row".
- `run.rs`: an unreadable stdin line stops the loop after an error line, because the
  stream's framing cannot be trusted past it.
- `tools/flea-bench`: refuses to run against tmpfs, because `drop_caches` cannot evict
  pages that are already the filesystem.
- `tools/flea-bench`: the directory-exists check it runs before a warm pass reads the
  directory and warms the cache, so a cold run must skip that check entirely; that is
  what `COLD=1` is for.

### Icons in the row

`icon_for` answers a name for every row, and two of those names were wrong before Plan 5
put a renderer in front of them.

A symlink to a directory keeps `d:false`, because `d` is the link's own type and
`Pane.openCursor` navigates on it. Only the icon follows the target, through
`Meta::target_is_dir`, which `stat_range` fills with a second `metadata()` call made for
symlink rows alone. `stat_range` is already scoped to the window a client asked for, so
this cannot become a directory sweep. `t` stays true for every symlink whatever its
target, which is a documented gap the renderer guards against rather than a narrowing
this made.

That second call follows the link, and following a link is a blocking syscall with no
timeout on the one thread that answers requests. A symlink pointing into a mount that has
stopped answering therefore stalls the backend itself, not just the row that named it.
This box has no NFS, no CIFS and no sshfs, but it does mount `fuse.gvfsd-fuse` at
`/run/user/1000/gvfs` and `fuse.portal` at `/run/user/1000/doc`, so a symlink into a live
gvfs mount whose server goes away is a reachable case here rather than a hypothetical.
The exposure is bounded by the window: `stat_range` only ever runs over the rows a client
asked for, so a 100,000 row listing with 350 rows held makes at most 350 of these calls,
and only for the symlinks among those 350. It is not defended against in code on purpose.
There is no way to put a timeout on a synchronous `stat` without giving each row its own
thread, which is an absurd price for an icon, and the precedent is already shipped:
`thumb_rows` in `backend/run.rs` calls `std::fs::metadata` on a client-named path, also
following the link, also on this loop, and Plan 4 shipped that deliberately. A real fix is
one asynchronous stat path for both call sites and belongs to whichever plan takes on
non-blocking IO, not to an icon change.

190 of the 613 distinct `application/*` types in `globs2` have no `generic-icons` entry on
this box and fall through to the class arm. Both counts move with the installed applications,
because `update-mime-database` merges `/usr/share/mime/packages/`: a root built from
`freedesktop.org.xml` alone gives 138 of 552, which is why no test asserts a row of these
tables, see `docs/protocol.md` and issue 1. That arm used to answer `application-x-executable` for
all 190, so a `.pem` drew as a program. The row's own execute bit now decides, the same
three bits `Row.nameColor` reads: set gives `application-x-executable`, clear gives
`application-x-generic`. Of the 190, three are usually executable and reachable by a glob:
`application/x-sharedlib` (`*.so`), `application/x-object` (`*.o`, `*.mod`) and
`application/x-ms-dos-executable` (`*.com`). They still draw as programs when the mode
says so. `application/x-shellscript` and `application/x-pie-executable` have no `globs2`
entry at all, so `mime::Db` can never produce them; `*.sh` is `text/x-shellscript`, which
`generic-icons` lists as `text-x-script`, so a shell script never reaches this arm.

`Row.iconSource` turns that name into a source. `Quickshell.iconPath(name, true)` does not
answer a filesystem path: it answers the image provider URL `image://icon/<name>`, and the
empty string for a name the theme lacks. Measured against `Yaru-wartybrown-dark`, both
`application-x-generic` and a made-up name answer empty, which is why the call is the OEM's
own two-step from `AppLibrary.qml`, name then `text-x-generic`. Cutting the second half
leaves every `application/*` row with no source at all.

That lookup needs a Qt platform theme in the process environment. Hyprland's `envs.lua`
exports `QT_QPA_PLATFORMTHEME=gtk3` and the session republishes it, so a user launch
resolves icons; a bare ssh inherits neither, and `omarchy-drive env` does not emit it, so
every name answers empty and every row draws nothing. `tests/ui.sh` imports it from
`systemctl --user show-environment` rather than hardcoding it, and so does
`tools/flea-field-bench`. The harness did not until Plan 5 Task 3, so every Flea row in
`docs/baseline-2026-08-29.md` is a Flea with no icon theme. **Both corrections below come from
Task 3's own A/B at `32a3e68`, three cold runs an arm, differing in nothing but whether Qt
loaded the gtk3 platform theme. Neither is a subtraction across the two documents**, which
would cross a unit boundary; what licenses applying the A/B to that document's rows is that its
theme-off arm reproduces them to within 0.02 MB, 45485 kB against a printed 45.5 and 44805
against a printed 44.8. **PSS: the theme is worth 3285 kB on the scale fixture and 3824 kB on
the media fixture**, 45485 against 48770 and 44805 against 48629, that is 3.21 and 3.73 MiB, so
the understatement is a pair and not the one number it was written as. **CPU: up to 0.08 s, and
that is the scale figure alone**, medians 0.22 s against 0.30; media moved 0.26 to 0.29. **That document's Flea rows are not a valid before for
anything in this plan**, and `docs/baseline-2026-08-30b.md` supersedes them. Of the rest
of the field only `dolphin` is Qt and can move at all; the GTK entrants and the terminal ones
cannot see the variable. The same trap catches a launch by hand: `~/bench/flea-relaunch.sh`
does not publish the variable either, so a window started through it draws blank icon slots
with thumbnails still working, which reads as a renderer bug and is not one. Export it from
`systemctl --user show-environment` for any by-hand launch.

The slot is `Theme.iconSize`, which is `rowHeight` less twice `spacing.rowPaddingY`, which is
by construction the row's own text line box, `Math.round(font.bodySmall * lineBoxRatio)`. It is
23 px at this box's text size, from a `rowHeight` of 37 and a `rowPaddingY` of 7, and it moves
with `omarchy display text size` because every term in it does. No pixel constant appears
anywhere on the path, which is why `tests/ui.sh icons` asserts the rendered pitch off `itemRect`
against `Theme.rowHeight` rather than against a number.

Qt keys the pixmap cache by source URL, so the column is bounded by the number of distinct
names a directory produces and not by the row count. Measured at 256 px against a window
holding no images: 335 `Image`s sharing one source cost 3.2 MB, ten distinct sources over
the same 335 cost 14.3 MB, and decoding per `Image` would have cost about 84 MB. The column
is not memoised. `Quickshell.iconPath` costs 0.6 to 0.9 microseconds per call once warm,
because Quickshell caches it already, so memoising the name would save about 0.3 ms across
a whole 335 row scroll. What the column does cost, 2.3 MB and 0.02 CPU s on both fixtures,
is the decode, the texture upload and the per-delegate `Image`, none of which a lookup memo
touches.

**That same cache is also how a stale frame reaches the screen, and the row carries an mtime query
because of it.** A freedesktop thumbnail path is the md5 of the file URI, so a regenerated thumbnail
lands at the SAME path, `Image.source` never changes, and Qt hands back the pixmap it already has.
Every correctness check on this branch compared URLs and stayed green while the screen was wrong.
Reproduced by pixel on 2026-08-30: a flat red JPEG replaced by a flat blue one, the pane left and
re-entered so `Pane.open` clears the thumbnail map and the row is asked again, the PNG on disk going
from RGB 220,20,20 to 21,20,220, and the icon slot still reading 217,20,20. `iconSource` now appends
`?m=` and the row's own `m`, which is exactly the key the backend validates the cache entry against,
so the URL moves when and only when the entry is no longer valid. `QUrl::toLocalFile` reads the path
alone, so the query changes Qt's key and not the file that opens.

**`cache: false` was the other candidate and it was measured, not argued about.** Five interleaved
rounds, both fixtures, both a settled listing and one after ten scroll bursts, every arm launched
through `flea --gui` so the launcher's `PR_SET_THP_DISABLE` is in the picture. `cache: false` cost
PSS in the settled column of both fixtures in 5 of 5 pairs, median +418 kB on scale and +292 kB on
media, because it stops the whole icon column sharing one pixmap per name. The mtime query keyed only
the thumbnail branch and came out inside the noise, medians -79 and -6 kB with the sign flipping
across rounds. Memory is the one GUI column Flea does not win on the media fixture, so the arm that
costs nothing is the arm that ships.

**What the mtime query does NOT cover.** The row's `m` comes from the listing while the backend
validates against a fresh `metadata()` call at thumb time, so a row re-asked without a re-list keeps
its old key. The only path to that is the 240-entry `thumbCap` evicting a row, and scrolling far
enough to evict 240 entries crosses `refetchMargin` and refetches the window's rows too, which
refreshes `m`. `cache: false` has no such hole; it was still the wrong trade.

The IPC seam reports `icon.source` through the `iconUrl` alias and not `iconSource()`,
because the two can disagree: cutting the `source` binding while leaving the function
intact drew no icons at all and left the case passing. `sourceSize` caps the themed PNG,
which is the only half that applies here, since Yaru ships mimetypes and places as PNG
alone at 16, 24, 32, 48 and 256, so a 23 px slot resamples the 24 px art by 23/24.

### The row mark: Shape, not a generated font

Plan 5 Task 5 registered a decision rule before running any measurement, writing the rule
down before looking. The contest, spec section 5aa: a lucide mark drawn as `Shape` with
`preferredRendererType: Shape.CurveRenderer`, the same pattern this operator's other Omarchy
plugins already use for `AdGuardIcon.qml` and `SlackIcon.qml`, against the same mark shipped
as a generated subset font rendered through `OpticalGlyph`. `Shape` wins unless it costs more
than 2 MiB of PSS or more than 15 percent of scroll CPU against the font arm across nine
interleaved pairs on the media fixture. It wins ties: no toolchain, no binary blob, no build
step, and two prior good results in this project. The 2 MiB threshold is set by the budget:
media PSS is fourth place and 2.80 MiB from third, a column this plan may not make worse.

The font arm never reached that measurement. `python3 -c "import fontTools"` on the dev box
raises `ModuleNotFoundError: No module named 'fontTools'`. `pacman -Ss python-fonttools`
answers `extra/python-fonttools 4.63.0-1`: the package exists upstream, nothing on this box
has ever pulled it in, and installing it needs a `pacman -S` this task was told not to spend,
even though the sudo chain to run one works headless. A gate meant to be decidable cheaply
does not buy a win with a box mutation, so the check stopped there rather than installing a
toolchain to measure the thing the toolchain requirement was already an argument against. An
arm that needs a toolchain this box does not carry, a checked-in binary blob no diff can
review, and a name-to-codepoint map loses on the toolchain axis alone, which the registered
rule already treats as decisive by itself. The nine interleaved pairs were never run; there
was no font glyph built to interleave against.

`Shape` wins by the registered rule, on the toolchain axis, without needing the PSS or CPU
numbers. `ui/Glyph.qml` is the one mark this task ships: the `file` outline at lucide's
native 24 unit grid, `strokeWidth` fixed at 2, and a `Scale` transform doing the resize so the
stroke never doubles with it. It is registered in `ui/qmldir` per "Every component needs a
qmldir line" above. Task 6 wires it into `ui/Row.qml` in place of the themed `Image` and
supplies the rest of the set; this task decided only the renderer.

### Lucide path data

Lucide-static 1.38.0, ISC licence, fetched from `https://unpkg.com/lucide-static@1.38.0/icons/
<name>.svg` (unpkg resolves `@latest` to that version as of 2026-08-31); source repository
`https://github.com/lucide-icons/lucide`. `ui/js/Icons.js` carries fifteen marks this way:
`file`, `folder`, `file-text`, `code`, `image`, `film`, `music`, `archive`, `type`, `terminal`,
`table`, `presentation` for the row map, plus `house`, `download`, `folder-git-2` for the
sidebar. `Icons.js` stayed one file at 79 lines against its 300 hard cap, so the split into a
separate `IconPaths.js` this task's brief anticipated was not needed.

The brief expected some marks to carry "a second optional path"; the real set needed as many as
eight shapes in one icon (`film`: one rounded rect plus seven strokes), so `Glyph.qml` instead
takes one `PathSvg` per mark whose `d` string is every source `<path>`, `<rect>` and `<circle>`
joined into one multi-subpath SVG path, which `PathSvg` already draws as a single shape. This
is simpler than a second-path property and needed no change to `Glyph.qml` beyond reading
`Icons.pathFor(name)`, one line.

A `<rect>` or `<circle>` element has no `d` attribute, so each was converted with the standard
rounded-rect-to-path and circle-to-two-arcs formulas rather than typed by hand. Concatenating
several independently authored `<path>` elements into one string surfaced a real bug in the
first pass: a leading lowercase `m` is only equivalent to an absolute `M` when it is the very
first command in the whole string, and several lucide marks (`code`, `terminal`, `download`,
`presentation`, the diagonal in `image`) lead with `m` followed by a bare-number implicit
lineto tail, whose relativity is inherited from that leading letter. Naively forcing every
leading `m` to `M` reset that tail to absolute too and drew a diagonal line to the wrong point;
the fix re-issues an explicit lowercase `l` before the tail so its relativity survives the
letter change. Every one of the fifteen conversions was checked against this: `rsvg-convert`
rendered the original multi-element SVG and a synthetic single-path SVG carrying the converted
`d` at 240x240, and `magick compare -metric AE` came back under 4 differing pixels out of
57,600 for all fifteen (antialiasing only) once the fix landed, against thousands of differing
pixels for five of them before it.

**The Omarchy cut, 2026-08-31.** The set was recut for the brand pass on `oem-look`: every
rounded corner lucide bakes into a `d` as an `a2 2` arc became a hard corner, keeping the
source glyph's bounding extents (a first pass shrank `file`, `file-text`, `folder` and
`download`; review round one restored them to lucide's box), genuine curves stayed (circles,
the `folder-git-2` pipe), and `Glyph.qml` switched RoundCap/RoundJoin to SquareCap/MiterJoin,
the edge of `omarchy-logo.svg`'s square spiral. The mechanical rule for recutting any stock
lucide glyph is therefore: square the corner arcs, keep the extents. One deliberate geometry
change beyond corners: `music` note heads are squares, not circles, the set's brand tell
(`file-text`'s three rules are lucide's own, restored with the body). The AE compare
above no longer applies to recut marks (they differ from source on purpose); the gate is the
montage eyeball plus a live `qs -p ui` look on the box (bench numbers still come from the
launcher path, hard rule 7). `rsvg-convert` only
proves a `d` parses; librsvg renders through a bad tail and exits 0, so its exit status is
not a gate.

**The row-name distribution, re-derived 2026-08-31** (the task brief's own figures, checked
against the artefact and not just quoted): `cut -d: -f2 /usr/share/mime/generic-icons | sort |
uniq -c | sort -rn | head -30` gives `x-office-document` 106, `package-x-generic` 89,
`application-x-executable` 57, `text-x-generic` 46, `image-x-generic` 40,
`x-office-spreadsheet` 38, `text-x-script` 36, `font-x-generic` 23, `x-office-presentation` 21,
`video-x-generic` 15, `text-html` 11, then a tail of five or fewer each, exactly matching the
brief.

### The Network row marks: TailscaleMark and DropboxMark

**Both third-party marks are reproduced from the official artwork, not recut**, and neither appears
in `ui/js/Icons.js`. GM ruled the reproduction after both recuts were rendered beside their originals
at 19, 24, 32, 48 and 96 px, which brackets the sizes these actually draw at: a 2 unit stroke on a 24
unit grid spends most of a small mark on outline, and a one-weight language cannot carry the
Tailscale mark's own emphasis, because varying size to signal weight destroys the uniform grid that
is the mark's strongest feature. **The recut ruling is superseded and must not be restored.**

`ui/TailscaleMark.qml` is nine circles of radius 18 at centres 30.5, 84.5 and 138.5 on both axes of
the official `0 0 169 169` box, four at full opacity and five at 0.4. The two opacities are
load-bearing: the brand toolkit says the gray dots adapt in opacity and says to avoid single-colour
reproduction, so one palette role at two opacities is the correct reading. `ui/DropboxMark.qml` is
five tiles on a box `iconSize * 1.18` by `iconSize`, centres at 0.25/0.75/0.25/0.75/0.50 across and
0.188/0.188/0.564/0.564/0.812 down, each half the box wide and 0.376 of it tall. The wide box is the
AdGuard lesson: scale to the drawn box, not to a square slot, or the mark renders smaller than its
neighbours.

Both sit outside the 24 grid and outside `Glyph.qml` sizing, where the brand marks sit, and both keep
a `ui/qmldir` line. Monochrome and palette-tinted, no brand colour anywhere.

**The geometry replacement retired the Font Awesome Free 5.4.1 Brands "dropbox" derivation** that the
previous `DropboxMark` carried, and with it the CC BY 4.0 attribution the repo would otherwise have
taken into a public release. Nothing in the tree derives from Font Awesome now.

SMB and WebDAV rows use lucide's `server` glyph instead of a hand-drawn mark, added to
`ui/js/Icons.js`'s `PATHS` the same way as the fifteen marks above: `lucide-static@1.38.0`'s
`server.svg` is two rounded `<rect>`s (converted with the same formula as `image`'s outer
rounded rect) and two near-zero-length `<line>` elements, which is lucide's own technique for
the rack unit's two LED dots since `Glyph.qml`'s `RoundCap` turns a zero-length stroked segment
into a filled circle. `rsvg-convert` at 240x240 plus `magick compare -metric AE` against the
original four-element SVG came back 0 differing pixels, an exact match, because a path made of
straight lines and circular arcs has no curve-fitting error to introduce.

### A second ContextMenu instance breaks the whole window's keyboard focus

Task 15's first cut of the Network group's unmount action reused `ui/ContextMenu.qml` a second
time, one instance owned by `ui/Sidebar.qml` for "Unmount" alongside `ui/Pane.qml`'s own instance
for "Open", reparented to Pane's FocusScope on open so its focus-restore-on-close logic could find
"list" as a real sibling. Live on the dev box this broke every keyboard key in the whole app, not just
near the menu: `j`, `Tab`, arrows, all silently did nothing, from the moment the window opened,
whether or not the second menu was ever actually shown. Bisected by swapping files against the
committed HEAD one at a time (`git stash`, then `git show HEAD:<file> > /tmp/x.qml` and diffing
combinations): the original `Pane.qml` plus original `Sidebar.qml` plus the new `shell.qml` alone
kept working, and the fault isolated to the second `ContextMenu` instance's mere presence in the
tree, not to reparenting (removing only `parent: root.parent` did not fix it) and not to
`NetworkMounts.qml`'s Timer or Process activity (stubbing those out did not fix it either).
`ContextMenu.qml`'s inner `action` Item declares a bare `focus: true`, unconditional on `opened`;
with two such instances live in the same FocusScope, Pane's own `list` (also `focus: true`) loses
the scope's active-focus slot at startup, apparently regardless of either menu's visibility. The
fix was not to explain the exact Qt Quick internals further but to drop the second instance. The
finding is about a SECOND instance, not about the rail: the rail raises `ui/Pane.qml`'s own single
`ContextMenu`, which is the shape below. Reintroducing a second focus-claiming popup anywhere in
this window should re-run this exact bisection before shipping it.

### The rail raises the pane's one menu, and the two-right-click arm is gone

For a while a right click on a mounted rail row armed it and a second right click within 4000 ms
fired the release. That worked and nobody could find it: no menu, no button, no hint, and the
operator reported the eject as missing. It was never an accelerator either, because it cost the
same two deliberate acts a menu costs and named neither of them.

So `ui/Sidebar.qml` `openRailMenu` now hands `ui/Pane.qml`'s single `ContextMenu` the rows to draw,
through `openForRail(key, entries, scenePoint)`, and `ui/js/Mounts.js` `railMenu` decides what a row
offers: Eject on a mounted removable volume, Unmount on a mounted network share, nothing anywhere
else, and a row that offers nothing opens no menu at all. Both rows draw the `eject` mark, which is
what the icon language's shelf lists it for. A chosen row comes back as `railChosen(action, key)`
and the key, not the index, is what resolves it: the rail rebuilds on a five second poll, so the
position the menu opened over can name a different row by the time a row inside it is chosen (see
`ui/js/Mounts.js` `rowByKey`). Right click never activates a rail row any more, so it cannot mount
and open a stick somebody only meant to ask about.

### m raises the listing's menu too, under the cursor row

`m` is one action, `menu`, and `ui/js/Focus.js` routes it by focus view. In the rail `raiseMenu`
asks `Mounts.railMenu` and says `<label> has nothing to eject or unmount.` over a row with no
release. In the listing `act`'s `menu` case calls `ui/Pane.qml` `openCursorMenu`, which scrolls
the cursor row into view (a wheel scroll in the grid can leave it off screen), opens the one
`ContextMenu` at that delegate's bottom-left through `openAt`, and answers whether a delegate was
there at all; an empty directory, a listing still loading and a filter that hid every row all
answer false, and the status line says `No row under the cursor to open a menu on.` rather than
nothing. Right click and `m` reach the same menu, so a selection is honoured the same way by both.
`m` stopped being a type-ahead letter for this. The menu's own `keyCatcher` reads
`Keymap.lookup`, so `j` and `k` step it exactly as Down and Up do, and Space chooses like Enter.

### A FileView write can race a reload fired the moment setText() is called

`ui/NetworkDialog.qml`'s bookmark write and `ui/NetworkMounts.qml`'s bare-root share expansion
(below) both write `~/.config/gtk-3.0/bookmarks` through their own `FileView.setText()`, then
signal the caller to `reload()` a DIFFERENT FileView instance (`ui/Sidebar.qml`'s own read-only
`bookmarksFile`) pointed at the same path. `setText()` queues its write rather than blocking, so a
`reload()` fired in the very next line can start its read before that write has actually landed,
seeing stale (pre-write) content; the file's own watcher can self-heal this later, but not
reliably inside a five-second test window, and a real user would see the rail miss its own most
recent edit just as often. Measured live: two of four consecutive runs of `tests/ui.sh network`
passed and two failed with an empty `networkEntries`, and adding an unrelated debug delay before
the check happened to make it pass again, which is what pointed at a race rather than a hard
failure. The fix is `bookmarksWrite.waitForJob()` (blocks until the current async operation
finishes) between `setText()` and the signal that triggers the reload, in both files. Four
consecutive clean runs after the fix, two of four before it, on the same box.

### A server root with no share segment mounts, but GVFS gives it no FUSE path

The operator's own real NAS bookmark (`smb://192.168.1.10/`, no share name) is exactly the "browse
the whole server" shape nautilus itself writes when you connect to a server without picking a
share first. `gio mount` on it succeeds (or reports "already mounted", itself a live GVFS quirk
for this shape: see "The real gio mount -l sample" above), but `gio info` on it prints no
"local path:" line at all, because GVFS only exposes a FUSE mount point for a mount that names a specific
share, never for a bare server-root browse. Activating this kind of entry used to end at "This
network location has no browsable folder," a dead end for the one bookmark shape the operator
actually has. `ui/NetworkMounts.qml`'s `isBareRoot()` recognises the shape (`scheme://host/` or
`scheme://host`, nothing after) and, only when `gio info` comes back with no local path for it,
runs `gio list` on the same URI, which DOES enumerate the server's shares over the same connection.
Each discovered share becomes its own `smb://host/share/` bookmark line (deduped against what the
file already carries, so a second activation reports nothing new instead of growing the file every
time), and the message names how many were added. One click on the operator's real "NAS" bookmark
now turns it into five separate, individually mountable rows: appdata, backups, data, isos,
timemachine, each of which opens a real GVFS FUSE path once mounted, verified live against the
real NAS both through gio directly and through Flea's own rail.

**Superseded, fix round 2:** the operator overruled the bookmark-expansion design above ("clicking
the NAS in the sidebar should allow me to view all folders and go into them, that's all"). The
detection this section describes (`isBareRoot()`, `gio info` with no local path, then `gio list`)
still stands, but the terminal action changed: `gio list`'s own names are no longer written into
the bookmarks file at all. See "The share browser overlay" below for what replaced it.

### The rotating empty-state caption, exact OEM values

Read directly off `/usr/share/omarchy/shell/Ui/PanelHero.qml`'s `meta` text (the component both
`tailscale/Panel.qml` and `io.github.thisisgm.omapods/Panel.qml` drive with an identical
`phraseTimer`/`activePhrases`/`phraseSwap` triple): the rendered text is `root.meta.toUpperCase()`
even though every source `activePhrases` array is authored in ordinary sentence case ("Encrypting
connections", "All gone Pete Tong"); the colour is `root.dim`, `Qt.darker(foreground, 1.4)`, not
the row's own muted role; the font is `Style.font.caption`, `font.bold: true`,
`font.letterSpacing: 1.2`. The rotation itself is `Timer { interval: 2800 }` driving a
`SequentialAnimation` of two `PropertyAnimation`s on opacity, 180 ms out (`Easing.OutQuad`) then
260 ms in (`Easing.InQuad`), with the phrase index advanced by a `ScriptAction` in between, not a
plain text swap. `ui/EmptyState.qml` now matches all four exactly; the file's own eight-phrase
list stayed sentence case in its source, matching `activePhrases`' own authoring convention, with
`.toUpperCase()` applied only at render time.

**Correction, fix round 2:** this entry originally recorded the darken factor as `1.55`, read off
an identically-named `dim` property in `tailscale/Panel.qml` governing its own rows rather than
`PanelHero.qml` itself. `PanelHero.qml:22` reads `readonly property color dim: Qt.darker(foreground,
1.4)`; `ui/EmptyState.qml` and the value above are both corrected to `1.4`.

### Capitalization, derived from the shipped plugins and not from a stale doc comment

Grepped every `text:` literal across `tailscale`, `dropbox`, `agents`, `weather`, `clock`,
`bluetooth` and `network`'s `Panel.qml` files under `/usr/share/omarchy/shell/plugins/`. Three
clear, internally consistent registers, by what the string IS rather than which component renders
it:

- **Group and section headings** (`PanelSectionHeader`, and the compact stat tags like weather's
  "FEELS"/"WIND"): **ALL CAPS** in every real usage found ("CONNECTIONS", "MACHINES", "BALANCE",
  "DNS PROVIDER", "RECENT FILES", "SCANNING WI-FI…"), with no exception. `PanelSectionHeader.qml`'s
  own doc comment gives lowercase examples ("DNS provider", "Wi-Fi networks") that no shipped
  plugin actually follows; `network/Panel.qml` itself writes "DNS PROVIDER" in caps. The doc
  comment is stale against its own callers, so the rule here is read off the plugins, not the
  comment.
- **Buttons and clickable action labels**: sentence case, first word (and proper nouns) capitalised,
  nothing else: "Back to today", "Authorize Tailscale operator", "Start weeks on …", "Confirm"/
  "Cancel" (`ConfirmDialog.qml`'s own defaults).
- **Status, error and empty-state sentences**: ordinary prose, sentence case, a full stop for a
  complete sentence: "Tailscale CLI is not installed or not on PATH.", "No machines found on this
  tailnet.", "Fetching forecast…".

Applied this round: `ui/NetworkDialog.qml`'s two `PanelSectionHeader` texts, "Add network location"
and "Dropbox", became "ADD NETWORK LOCATION" and "DROPBOX" (heading rule); `ui/EmptyState.qml`'s
eight-phrase array was re-cased to sentence case in its source with `.toUpperCase()` at render
(rotating-caption rule, above). Every button label, status sentence and error sentence already in
Flea (`ui/js/Errors.js`, `ui/Pane.qml`'s inline messages, `ui/ContextMenu.qml`'s "Open", the
dialog's "Add"/"Cancel"/"Install Dropbox") already matched their registers and needed no change.
`ui/Row.qml`'s column headers ("Name", "Mode", "Size", "Date Modified") are Title Case, a fourth
register with no shipped-panel counterpart to check against since no OEM panel renders a data
table; left alone as a different genre, not swept.

### The row-swap PSS gate: measured, ranked, shipped anyway

The registered rule for this task was that wiring the row through `Glyph` must not regress
media PSS, predicted instead to save memory by retiring `Quickshell.iconPath`'s URL-keyed
pixmap cache. Nine interleaved pairs per fixture, alternated old (this tree at `cabcc0e`,
`Image`-only) against new (this tree with the row swap), launched through the real product
path `flea --gui <fixture>` with `FLEA_UI`/`FLEA_BIN` pointed at each arm so
`PR_SET_THP_DISABLE` stays in the picture, settled PSS read off `smaps_rollup` once three
consecutive 300 ms samples sat within 256 kB. **Warm cache, not cold**: the 1Password
`claude-skills` Environment mount that carries `OMARCHY_SUDO_PASS` answered empty this
session (the desktop app was not reachable to serve it), so `drop_caches` was never run; the
comparison is still relative and same-process, which is what this gate asks.

Media fixture (`/home/flea-sandbox/flea-media-btrfs`), 9 pairs: old median 67,847 kB, new median 71,058 kB,
every pair higher for new, spread inside each arm under 350 kB. Scale fixture
(`/home/flea-sandbox/flea-bench-btrfs`), 9 pairs: old median 63,774 kB, new median 67,347 kB, same
direction, same magnitude. **The regression is a fixed roughly 3.2 to 3.6 MiB on both
fixtures**, not proportional to row or file count, which points at `QtQuick.Shapes` and
`Shape.CurveRenderer` module load rather than a per-row cost. A five-pair isolation swapping
only `preferredRendererType: Shape.CurveRenderer` for the Qt default renderer on the media
fixture attributes about 1.7 to 1.9 MiB of the total to `CurveRenderer` specifically; the
remaining roughly 1.4 MiB is the cost of having any `Shape`-based `Glyph` wired into the row
at all, even on the default renderer. Task 5's registered rule for `Shape` versus a generated
font never measured either arm's PSS, deciding on the toolchain axis alone; this is the first
real measurement of what `Shape` costs against the `Image` path it replaced, and on this box,
for this row set, it costs rather than saves.

**The plan's predicted saving is falsified.** The spec argued the swap would save memory,
"an icon on every row costs 3.2 MB instead of 84 kB" against the `Image` path's URL-keyed
pixmap cache; the measurement above says the opposite, 9 of 9 pairs on both fixtures, and
that prediction is recorded here as falsified rather than quietly dropped.

**The rank check**, run against `docs/baseline-2026-08-30b.md` before ruling on the numbers
above. Media PSS ladder, MiB: thunar 37.94, pcmanfm 38.74, nemo 53.41, flea 56.48, dolphin
96.08, nautilus 134.47. Flea plus the measured 3.2 MiB is about 59.7, **still fourth**; the
gap to third (nemo) widens from 3.07 to about 6.3 MiB, still short of dolphin's 96.08 by a
wide margin. Scale fixture: flea 53.75 against dolphin 194.84; plus the measured 3.5 MiB is
57.25, **first place holds by 3.4x**. No rank moves on either column.

**Ruling: ship the swap as implemented, `Shape.CurveRenderer` included**, not the cheaper
default-renderer arm. The plan this task serves put aesthetics first ("linux does not have to
be ugly"); these marks are the single biggest visual change in the design, and the 1.8 MiB
`CurveRenderer` premium buys curve quality at exactly the 23 px size where the default
geometry renderer's tessellation aliases most. `AdGuardIcon.qml` and `SlackIcon.qml`, the two
prior marks built this way in this operator's other Omarchy plugins, made the same choice for
the same reason. The budget line ("media PSS must not get worse") does not block this: it was
authored on the saving prediction this measurement falsifies, and the rank check above shows
the budget's real purpose, protecting the field-bench ranks, is intact on both columns. The
ruling is reversible in one commit either way: swap `Shape.CurveRenderer` for the Qt default
renderer to recover about 1.8 MiB, or swap `Glyph` back for `Image` to recover the rest.

**One follow-on recorded, not built**: a per-glyph rasterised-texture cache (render each of
the fifteen marks to a small `Image`-backed pixmap once, keyed by name and colour, instead of
one live `Shape` per row) could recover most of the `QtQuick.Shapes` module and per-instance
cost without leaving the native-mark design, if this line item ever needs to be reclaimed.
Not attempted here: the ruling above ships without it, and it is a real project on its own.

### The thumbnail decode arm

`sourceSize` is set on the thumbnail `Image`, to the row's icon slot, and Plan 5 Task 6
measured both arms rather than inheriting the KB's ruling. The KB measured `sourceSize` on
4K PNGs, where it makes decoding slower, because PNG has no scaled decode path so Qt decodes
the whole image and then pays a smooth scale on top. A freedesktop thumbnail is a different
regime, and that difference is the thing Plan 6 needs from this section: it is at most 256 px
on its longest side, so the common 16:9 case decodes to 256 by 144 rather than 256 by 256,
and the slot it lands in is 23 px, which is `Theme.iconSize` here, `rowHeight` 37 less twice
`rowPaddingY` 7. Nothing below transfers to a full size image.

The harness was two `qs` configs under `~/bench/thumbarm`, identical but for the two
`sourceSize` lines, each drawing 35 of the operator's own cache entries as distinct sources,
read and never written, over ten interleaved pairs. Wall time from the file list landing to
the last `Image` reporting `Ready` came out in the high forties to the mid sixties of
milliseconds for both arms, with medians a few percent apart and the sign of the difference
flipping between the two five-pair batches, so the arms are indistinguishable on time at this
scale. PSS separated cleanly and repeatably: the sized arm sat near the low 170s of MB and
the unsized arm near the high 170s to low 180s, a gap of roughly 6 to 7 MB for one screen of
35 rows, which tracks the 5.55 MB of RGBA those particular 35 files decode to. At 94 rows the
gap grew to about 17 MB against 14.5 MB of decoded pixels, so it scales with pixel count and
not with row count.

The rule was fixed before the numbers: take the `sourceSize` arm unless it costs more than
twice the other arm's time to fill a screen, because memory is the one GUI column Flea does
not win on the media fixture, while a screen of thumbnails is not on the first paint path at
all. The worst pair of the twenty put the sized arm at 1.25x and the pooled medians put it
under 1x, so the threshold was never approached and the sized arm wins on both columns. Treat
every figure here as a magnitude and not a constant: two batches of this same harness
disagreed on the sign of the time difference, which is the same instability the field
benchmark shows.

**The swap happens in the icon slot and nothing else moves.** `Row.iconSource` answers a
`file://` URL when the pane holds a path for this row and the themed icon lookup otherwise, so
one `Image` draws both and the row's geometry never depends on which of them it is drawing.
There is no animation on the change and no spinner, by construction rather than by a rule: a
`source` change is one binding re-evaluation. The URL is built with `encodeURI` and then `#`
and `?` are replaced by hand, because `encodeURI` leaves both literal and Qt then reads them as
a fragment and a query rather than as path bytes. A cache root holding either byte otherwise
produces a URL a test can read and Qt cannot open, which is why `tests/ui.sh hashcache` asserts
`Image.status` reached `Ready` and not only the string.

**`QSG_TRANSIENT_IMAGES=1` was measured against this tree and DROPPED, so do not re-measure it.**
The variable is real and undocumented, it lives in `libQt6Quick.so.6` on this box's Qt 6.11.2, and
it drops the CPU-side `QImage` once the texture is uploaded. Plan 5 Task 5a measured it on the
media fixture through the shipped product path, nine interleaved pairs: peak PSS 70,091 KiB
without it against 69,989 KiB with, `pss_kb` over 1024, a 0.10 MiB median difference against a
0.56 MiB spread inside the base arm, and the sign flipped 5 to 4. That is nothing. **The reason is
that `sourceSize` above already collected the whole prize**, and that was proven rather than
argued: a third arm with `sourceSize.width: 0` and `sourceSize.height: 0`, five interleaved pairs,
went from 76,775 KiB without the variable to 72,412 KiB with it, 5 of 5, a real 4.26 MiB. So the
variable works, and the CPU-side image it would free in the shipped tree is a 23 px icon slot,
roughly a kilobyte a row. The invalidation battery the candidate would have needed, scene graph
loss, a theme change and a scroll out past the buffer and back, was therefore never run: a lever
worth 0.1 MiB does not earn that risk. If a later plan ever removes `sourceSize`, for a preview
pane or a larger slot, this becomes a 4 MiB lever again and the battery becomes mandatory before
it can ship.

Two things the harness settled in passing. `sourceSize.width: 0` really is Qt's unset value,
so one file can measure both arms: a third config binding it to 0 landed in the unsized band
and the same file binding it to the slot landed in the sized band. And `FileView.onLoaded`
fires after the root's `Component.onCompleted` even with `blockLoading: true`, so a clock
started in `Component.onCompleted` reads an empty list, while `text()` called directly there
does return the content.

### The share browser overlay: fix round 2 replaces the sidebar bookmark expansion

The operator ruled round 1's fix ("A server root with no share segment mounts..." above) out:
turning a bare `smb://host/` bookmark into five permanent sidebar rows was not what "clicking the
NAS should let me view its folders" meant. Fix round 2 keeps round 1's detection (`isBareRoot()`,
the `gio info` no-local-path check, `gio list` to name the shares) but changes what happens with
the names: `ui/NetworkMounts.qml`'s `listShares()` fires a `sharesListed(baseUri, baseLabel,
names)` signal instead of writing bookmark lines, and nothing in this file touches
`~/.config/gtk-3.0/bookmarks` for this path any more (`addShareBookmarks` and `expandBareRoot` are
gone). `ui/ShareBrowser.qml` is a new overlay, the same instantiated-from-shell pattern as
`ui/EmptyState.qml` and `ui/Preview.qml`: `ui/shell.qml` sizes it over `pane.listArea`, exactly
like the empty state, and `ui/Pane.qml` gained a `shareBrowser` property wired the same way
`preview` already was, so its own `Keys.onPressed` can route j/k/Enter/Escape to
`Focus.shareBrowserAct` while it is active, ahead of the normal list/rail routing.

Each share row is a real `Flea.Row` fed a synthetic row object
(`{ n: name, d: true, p: 0, s: 0, m: null, i: "folder" }`) rather than a hand-rolled second row
visual: `Icons.glyphFor("folder")` already maps to the folder mark, `p: 0` renders as the mode
column's own all-dashes string with no special-casing needed, and `d: true` already blanks the
size column (`Row.qml`'s own `root.row.d ? "" : Format.size(...)`). Only the date column needed a
real change: `root.row.m === null` now renders `"--"` instead of calling `Format.date(null, ...)`,
which would have silently printed the 1970 epoch as if it were a real date. This is the only line
`ui/Row.qml` gained; every other column already degraded correctly for a directory it had never
listed a stat for.

Enter on a row calls the exact same `openShare(uri, false, label)` a bookmarked share's own
`activate()` already calls, so a share picked from the overlay is never a second code path: mount,
`gio info` for the local path, `pane.open()`. `ui/shell.qml` closes the overlay on `pane`'s own
`opened` signal (fired once a listing actually lands), not on the mount succeeding, so a share that
mounts but is slow to enumerate over the network still shows the overlay until there is something
real to show instead of it.

### "Already mounted" is not just a bare-root quirk

Proving the overlay's own Enter action live against the real NAS surfaced a second bug in code
round 1 shipped, not round 2's own new code: `gio mount` on a URI GVFS already considers mounted
answers a nonzero exit and the stderr line `"<uri>: Location is already mounted"` for ANY uri
shape, not only a bare server root. Reproduced directly: `gio mount smb://192.168.1.10/isos/`
against an already-mounted `isos` share exits 2 with exactly that stderr, identical in shape to
the bare-root case. `mountProcess.onExited`'s old check, `exitCode !== 0 && !isBareRoot(uri)`,
treated this as a hard failure for any non-bare-root uri, which is wrong: the location IS mounted,
that is what the message says. This is reachable through the overlay in one ordinary sequence: pick
a share, mount it, browse back to the bare root, pick the SAME share again (or a different overlay
instance discovers a share Flea already mounted through some other route). The fix reads the
process's own stderr instead of guessing from the uri's shape: `mountProcess` gained a
`StdioCollector` on `stderr`, and `isAlreadyMountedQuirk(text)` (`/already mounted/i`) replaced the
`isBareRoot()` check at that one call site. `isBareRoot()` itself stays, still used by
`infoProcess.onExited` to choose `listShares()` over the dead-end message, a genuinely
shape-dependent decision. `tests/ui.sh case_sharebrowser` reproduces this with a stubbed `gio`
that answers exit 2 plus that exact stderr for its second fixture share, asserting the mount still
resolves to an open rather than the "could not be mounted" message.

### The Network group's plus mark: a lucide glyph, not a font character

The operator's own words: the `Text "+"` next to the NETWORK heading "looks more like a christian
cross than a proper plus sign." Fetched `lucide-static@1.38.0`'s `plus.svg`
(`https://unpkg.com/lucide-static@1.38.0/icons/plus.svg`, ISC licence): two straight strokes,
`M5 12h14` and `M12 5v14`, joined the same way `Icons.js`'s other multi-subpath marks are (one
string, each subpath its own leading absolute `M`). Straight lines have no curve-fitting error to
introduce, so the `rsvg-convert` plus `magick compare -metric AE` check this project runs for every
lucide conversion came back an exact 0 differing pixels at 240x240, the same result the `server`
glyph's own all-straight-lines conversion got. `ui/Sidebar.qml`'s `addMark` is now a `Glyph` sized
off `Theme.font.caption`, the same token the NETWORK heading beside it renders at, in place of the
`Text` element; the click handler and keyboard path (`a` from the rail) are unchanged, since neither
ever depended on the mark being a `Text` item.

### The preview strip's play/pause marks

Task 22's transport strip needed two more lucide marks `Icons.js` did not carry yet.
`lucide-static@1.38.0`'s `play.svg` (`https://unpkg.com/lucide-static@1.38.0/icons/play.svg`) is
a single path, `M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5
19z`, copied across unchanged since it has no `<rect>` or `<circle>` to convert. `pause.svg` is
two rounded `<rect>`s (`x="14" y="3" width="5" height="18" rx="1"` and `x="5" y="3" width="5"
height="18" rx="1"`), run through the same rounded-rect-to-path formula the `archive` and `server`
marks above already use. `rsvg-convert` at 240x240 plus `magick compare -metric AE` against each
source SVG came back 0 differing pixels for both, the same exact-match result straight lines and
circular arcs give every time there is no curve to fit.

### URI normalization: one canonical form, so a trailing slash cannot fork a row in two

The dialog wrote whatever the user typed verbatim, and `gio mount -l` always reports a share URI
with its own trailing slash; nothing forced the two to agree, so `smb://h/data` (typed) and
`smb://h/data/` (gio's own report of the same share once mounted) compared unequal everywhere a
uri was used as a dedup key, and could show as two rows for one share. `ui/js/Mounts.js` gained
`normalize(uri)`: every trailing slash is stripped, except a bare server root (nothing left but
`scheme://host`), which keeps exactly one, the shape `gio` itself needs to parse it back. Applied
in two places, per the ruling ("apply it... on write and on compare"): `ui/NetworkDialog.qml`'s
`appendBookmark()` writes the normalized form rather than the raw typed string, and
`ui/NetworkMounts.qml`'s `rebuild()` dedups live mounts against bookmarks using normalized keys
rather than the raw `uri` fields, so a live mount (gio's trailing-slash form) and a bookmark (now
also normalized, but an old hand-edited line might not be) still collapse to one row. `parseMounts()`
and `nonFileBookmarks()` themselves are left returning whatever form their own source actually
carries; only the write path and the dedup comparison were ever the places two spellings needed to
agree. `tests/js/mounts.js` proves `normalize("smb://h/data") === normalize("smb://h/data/")` and
that a bare root normalizes to its one-trailing-slash form regardless of how it was typed.

### The rail menu, tested with a stubbed gio and a stubbed lsblk

`tests/ui.sh case_unmount` and `case_eject` drive the rail menu through the real window.
`ui/shell.qml`'s `railRowCentre(i)` is the same `itemRect`-based reader `rowCentre` uses for list
rows, through `ui/Sidebar.qml`'s `railItemFor(index)` (the rail has no `ListView` and every
Repeater keeps every row instantiated, so this just indexes into whichever group carries it).

`case_unmount` stubs `gio mount -l` to report one fake share as mounted and `gio mount -u` to log
its own call, and proves that a right click opens one Unmount row drawing the `eject` mark without
unmounting anything, that Escape closes it with nothing run, that choosing the row unmounts and
messages "Unmounted \<label\>.", that a favourite opens no menu at all, and that the list still
takes keys afterwards, which is the focus regression the second-instance bisection above found.

`case_eject` stubs `lsblk --json` as well, so a removable volume exists at zero privilege with no
real device anywhere near it. Its negative control is the whole point: the `gio` stub always exits
0, and while the stubbed listing still shows the volume mounted the status bar must refuse rather
than say "safe to unplug", because the verdict is read off the listing and never off the exit code
(`ui/js/Eject.js`). Only when the stub actually drops the mountpoint does the safe sentence appear.
It also asserts the eject went through `gio mount -e <mount point>` and never `mount -d`, which
glib dispatches before it reads `--eject`, and that no `-f` ever reached gio.

A status bar transient clears after 4000 ms and an eject verdict lands on the rail's own 5000 ms
poll, so both cases catch a sentence with `wait_message` as it lands rather than sleeping past it.

### Places.relabel: normalized matching, every duplicate rewritten, control characters stripped

Task 19's network rename writes through `ui/js/Places.js`'s `relabel(body, path, name)`, the one
function in this tree that edits the GTK bookmarks file's own bytes. Matching is normalized the
same way `ui/js/Mounts.js`'s `normalize()` already dedups the rail, so a live `gio`-reported uri
(trailing slash) still finds a bookmark line written without one (`ui/NetworkDialog.qml`'s own
`appendBookmark()` strips it). Fix round 1 found two gaps: the function used to stop at the first
matching line, so a hand-edited file carrying two bookmark lines for the same normalized uri
(never produced by Flea's own writers, but not excluded either) left the second stale after a
rename; `relabel` now rewrites every matching line in one pass and appends only when none match at
all. Second, `name` reached the join with no defence against an embedded `\n` or `\r`; today's
`qs.Ui` `TextField` cannot produce one, but the function is a trust boundary in its own right and
must not depend on its one caller staying that way, so `name` is stripped of both before it is
trimmed. `tests/js/places.js` proves both: a two-line-duplicate fixture ends with both lines
rewritten and every other line byte-identical, and a name carrying an embedded newline still
produces a file with the same line count it started with, the newline gone rather than splitting
one bookmark into two.

### Flea ends when its last window closes, and a wedged listing no longer freezes the rail

Three fixes on 2026-09-02, one on the way out and two on the rail. None of them had a line here.

**The exit, `5fc2668`, `ui/shell.qml`.** A Quickshell shell outlives its windows by design, and
0.3.1 exposes no exit API: `Qt.quit()` and `Qt.exit()` are no-ops, which the engine says out loud
as "no receivers connected", so signalling its own pid is the only lever left. The window carries
`Connections { target: Quickshell; function onLastWindowClosed() { ... } }` and the body is
`Quickshell.execDetached(["kill", String(Quickshell.processId)])`. Closing the window is what a
user does to quit, so without it the process stayed resident with nothing on screen.

**The NETWORK rail, `8cf5418`, `ui/NetworkMounts.qml`.** `gio mount -l` against a share whose
server has stopped answering never returns, and nothing bounded it, so `pollMounts` refused every
later poll and no new mount appeared until the app was restarted. A `listTimeoutMs` of 10000 now
ends it. `_listTimedOut` gates both the collector's `onStreamFinished` and `onExited`, because a
listing ended that way collected nothing and reading that as "no shares" would empty the rail and
take Unmount with it exactly when a server is misbehaving; it is cleared when the next listing
starts and never in `onExited`, so it still reads true while the ended listing's stream drains. A
re-read asked for mid-listing sets `_pollAgain` and runs when that listing ends, instead of being
dropped and leaving a just-mounted share to wait out the five second poll.

**The DEVICES rail, `b28e992`, `ui/DeviceMounts.qml`.** `lsblk` on the same five second poll was
unbounded too, and one that stopped answering left `poll()` refusing every later listing for the
life of the process. It takes the same 10000 ms bound and the same `_listTimedOut` rule, so a
listing ended that way holds its rows rather than emptying the rail, which is what keeps Eject
reachable while a device is misbehaving. The `_streamPending` interlock is released in `onExited`
rather than in the timer, because a collector whose stream was cut off may never finish it and
`poll()` refuses on that flag alone.

**Every reusable Process starts with an empty fallback.** `StdioCollector.text` and the parallel
`_...Output` value exist because `Process.onExited` can race the collector property update, but the
fallback is ordinary component state and survives the next command. Before this rule, a quiet
second run could reuse the first run's bytes: an empty `gio mount -l` retained a disconnected
share, empty `gio info` reopened the previous share's local path, an empty server listing repeated
another server's names, a failed mount with no stderr inherited "already mounted", empty `lsblk`
retained removed devices, and empty Tailscale status retained old Taildrop targets. Each start
function now clears only its own fallback after refusing any overlapping run and before setting
`Process.running`. `tests/process-output.sh` poisons all six values, starts all six real Process
objects, and requires every reset synchronously; removing any one reset makes the suite red.

### Closing the window quits the backend, and a copy in flight is cancelled

`ui/shell.qml` answers `Quickshell.onLastWindowClosed` with `backend.quit()` and signals its own pid
only once `Backend.onQuitReady` reports the child gone. Before this it killed the pid immediately and
left the backend to notice EOF by itself.

**The old path was right here by a margin of milliseconds that nothing in this tree owned.** Measured
on this box over three closes at 5 percent of a 4 GiB copy, driven with Omarchy's own SUPER+W bind:
`qs` exited 64 to 82 ms after the key, and the backend exited 18 ms before it once and 1 ms after it
twice. Every run removed the partial, because Quickshell closes the child's stdin during teardown and
`drain()` cancels inside that window. Which of the two dies first is not stable between runs, and
`src/backend/copyfile.rs` creates the destination with `create_new(true)` under its FINAL name, so a
drain that lost the race would leave the file in the destination at partial size with nothing having
said so. Sending `quit` makes the drain happen while `qs` is fully alive instead of racing its
teardown. After the change, three closes at the same point put the backend at 66 to 222 ms and `qs`
at 106 to 261 ms, the backend consistently about 40 ms ahead of it; on an idle close with nothing to
drain the two land within 1 ms, which is this harness's own resolution and not an ordering claim.

**What a close does to a running transfer: it cancels it.** That is the transfer card's own Cancel
control reached by a different gesture, and it is the only answer that holds the invariant without
inventing a surface. Finishing the copy instead would run it for minutes inside a process with no
window, no progress and no way to stop it, which is the resident-with-nothing-on-screen state the
exit exists to prevent. A confirmation dialog would be the only one in the product: `Delete` trashes
with no prompt, as `tools/flea-live-ops` asserts, and a cancelled copy destroys nothing, so the
dialog would guard the least destructive action of the set. A cancelled move keeps its source too,
because `move_any` copies before it removes and the cancel path removes the partial destination.

**The two edges `Backend.quit()` answers.** A quit before `onStarted` would only join the pending
queue and never be written, so `queueing` short-circuits straight to `quitReady`; nothing can be in
flight that early anyway. And a child that never answers at all would leave a shell resident with no
window, the exact defect the exit fixes, so `quitDeadline` bounds the wait at 30 s, longer than the
backend's own 25 s `DRAIN_LIMIT`.

**Measuring a close needs the real bind, and the wrong one reports the window it never touched.**
`hyprctl dispatch closewindow address:0x...` is not valid on this box: Hyprland takes Lua here and
answers `')' expected near 'address'`, then names the shape it wanted, `hl.dsp.window.close()`. A
harness discarding that stderr reads a syntax error as a close and then reports whatever the
untouched window was still doing, which is how a copy still running at 30 percent was first written
down here as a surviving partial. SUPER+W is Omarchy's "Close window" bind, and only a real uinput
key press fires a compositor bind, so `omarchy-drive hotkey --global super w` is the gesture. It acts
on the ACTIVE window, so assert the active window is the one the run launched before sending it, and
never call `hl.dsp.window.close()` with no argument: it returns `ok` rather than printing a signature
and acts on whatever is active, which may be a window you did not open.
