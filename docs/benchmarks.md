# Benchmark method

This document describes how Flea's field benchmark works: what it measures, how its fixtures
are built, how a run is set up and executed, and how to reproduce one. It carries no result
tables. Results belong beside the claim they support (the README's comparison table); a number
copied into a method document outlives the run that produced it and stops being citable the
moment the tree, the field, or the box changes under it.

Every timing figure a run produces is a magnitude, taken from repeated runs on one machine, not
a citable constant. Two batches of the same harness on the same box have disagreed by 2x to
nearly 4x run to run. PSS is the one column that reproduces tightly, typically to within a few
tenths of a MiB. Read every number this method produces the same way: as three runs and a
median, never as a single figure worth memorizing.

## The two fixtures

The harness runs against two fixtures, built once and reused across runs. Both fixtures are
built by scripts in `tools/`, live under a sandbox root outside `$HOME` (`tools/flea-sandbox-guard`
enforces this; see "Where fixtures live" below), and carry a marker file so a rebuild can never
delete the wrong directory.

**The scale fixture** is 100,000 empty `.txt` files in one directory, built by `tools/flea-bench`
(`N=100000` by default). It exercises the listing path alone: readdir, sort, and a windowed stat.
Every file is the same size and holds no bytes, so nothing about content, thumbnailing, or MIME
sniffing enters this fixture at all.

**The media fixture** is 2,000 files built by `tools/flea-media-fixture` (`COUNT=2000` by
default), mixed across formats so a viewport of a few dozen rows meets several of them: jpg, png,
webp, heic, mp4, mkv, webm, and plain text notes as the long tail. The builder encodes one seed
file per format, then copies each seed into place rather than re-encoding per file, so building
2,000 files costs seven encodes and roughly 2,000 plain copies (`cp --reflink=never`, deliberately
not a reflink, so a cold read on btrfs cannot serve two thousand names from one shared physical
copy). The mix repeats every 20 files across 10 slots, with mkv and webm sharing one slot so both
containers still appear at a 2,000-file count. Seeds are removed once the build succeeds; a build
in progress can be told apart from a finished one by their presence.

Both builders refuse to overwrite a directory that exists but carries no marker, verify the
built count against the requested count before finishing (`ls -U | wc -l`, which is what the
field bench also checks before a run starts), and refuse to build on `tmpfs` (page cache drops
cannot evict a tmpfs page, so a cold run against one measures nothing).

## Where fixtures live, and why that is checked rather than assumed

Fixtures live under a sandbox root (`tools/flea-sandbox-guard`, default `/home/flea-sandbox`,
overridable with `FLEA_FIXTURE_ROOT`), never under `$HOME`. Every script that builds or deletes a
fixture sources that guard first, and every delete goes through it: the target path is resolved
to its canonical form, checked as an absolute path at least two components deep, checked against
a list of forbidden roots (`$HOME` and the standard system trees), and checked for the fixture's
own marker file before anything is removed. A directory that exists but is not marked is refused,
not deleted. This exists because a prior destructive test run against real paths under `$HOME`
deleted an entire home directory with no way to restore it; every fixture-owning script in this
tree now goes through one guard rather than repeating that judgment call inline.

## What "cold cache" means here

Two independent caches are cleared, and each is cleared for a different reason.

**The page cache** is dropped before every single run, not once per entrant: `sync` followed by
writing `3` to `/proc/sys/vm/drop_caches`. This needs root, so the harness reads a sudo password
once, as the first line of its own stdin, and never puts it in argv or an environment variable.

**The shared thumbnail cache** (`~/.cache/thumbnails`) is the operator's own directory and is
never treated as disposable. On a media run the harness moves it aside once, at the start, and
moves it back once, at the end or on any exit path (`trap ... EXIT INT TERM`), rather than
deleting anything the operator owns. A scratch cache is created in its place and its
subdirectories are asserted empty before the first entrant runs, because a crashed prior run
leaving a populated scratch cache behind would let the next run report cache hits as generation.
A GUI run that never asks for thumbnails (the scale fixture) does not touch this cache at all.

## What is measured

- **`mapped`**: milliseconds from launch until the window manager lists a client of the entrant's
  window class. TUI entrants run inside a terminal, so their `mapped` figure is the terminal's own
  startup cost and is not comparable to a GUI entrant's; only `settled`, memory, and CPU are
  compared for a TUI row.
- **`settled`**: milliseconds from launch until neither the entrant's own CPU ticks
  (`utime + stime + cutime + cstime` from `/proc/<pid>/stat`) nor the summed ticks of its live
  descendant tree have moved for a continuous 500&nbsp;ms. The descendant walk is breadth first
  over `/proc/<pid>/task/*/children` and runs only on a poll where the watched set itself earned
  no tick, because Flea's thumbnail decode happens two process levels below the window (the
  backend forks a sandboxing wrapper, which execs the thumbnailer), and a settle rule that watched
  only the top process would call the run finished while a child was still decoding. An entrant
  that declares a second, out-of-process worker (Flea's Rust backend, matched by command name and
  a `--backend` argument token) has that worker added to its watched set from launch.
- **`pss_kb` / `pss_anon_kb` / `uss_kb`**: read from `/proc/<pid>/smaps_rollup` (`Pss`,
  `Pss_Anon`, and `Private_Clean + Private_Dirty`), sampled on every poll and kept at its peak
  rather than its value at the settle point, because sampling only after the loop ends usually
  finds the process already exited.
- **`drm_kb`**: GPU-resident system memory from `/proc/<pid>/fdinfo/*` (`drm-resident-system0`),
  de-duplicated by DRM client id since several file descriptors can share one client. This is
  memory PSS cannot see at all, so it is read once after settle rather than polled.
  NVIDIA's proprietary driver publishes no `drm-resident` key, so a process holding an NVIDIA GPU
  node (`/dev/nvidiaN`) also adds the driver's own per-process `used_memory` from
  `nvidia-smi --query-compute-apps`, which is video memory rather than system memory. Without
  that, a GPUI entrant rendering on NVIDIA reads as the `0` of the idle iGPU client it opened while
  enumerating adapters. When the driver does not list the process, the column is `unmeasurable`,
  never a total that leaves the NVIDIA device out.
  **A quickshell process resident on the bench box shares its Qt libraries with any Qt entrant
  launched afterward**, so a Qt entrant's PSS is charged roughly half of what those libraries
  would cost in isolation, while a GTK entrant's libraries are shared with nothing and charged in
  full. This is a property of the box's own state at run time, not of the entrant, and is worth
  stating beside any PSS comparison that includes both toolkits.
- **`cpu_s` / `cpu_tree_s` / `cpu_main_s`**: CPU seconds at the settle point. `cpu_s` equals
  `cpu_tree_s`, and `cpu_main_s` is `utime + stime` of the entrant process alone. Rows written
  before `cpu_main_s` existed carry that process-only figure under `cpu_s`, so an older `cpu_s` is
  never compared with a newer one, and `tools/flea_bench_product.py eligible` refuses such a row.
  `cpu_tree_s` is `usage_usec` of a control group the run made for this
  entrant, which holds everything it started, sandboxed or reparented, plus a declared helper that
  lives outside it (thunar's `tumblerd`). A TUI row's control group holds its terminal as well, so
  that column includes kitty's own drawing, plus the scope kitty moves the TUI itself into. Before 0.3.2 it summed reaped ticks, which a decode under
  `bwrap` never reaches; that older figure is kept as `cpu_reaped_s`, the last column.
- **`thumbs_n`**: on a media run, the count of files in the shared cache's `large/` bucket at the
  settle point, evidence that generation actually happened rather than a warm cache being read, and
  `n/a` rather than `0` on a run that never dropped the cache and so never took the count. A
  `fail/` marker (a thumbnailer that ran and declined) is counted and reported separately, never
  folded into the total, because "refused" and "never attempted" are different facts a bare zero
  would conflate.
- **`thumbs_by_format`**: which formats an entrant actually produced, derived by hashing the
  fixture's own file names into the freedesktop cache-key form (`md5` of the `file://` URI) once
  per fixture into a lookup map, then classifying whichever keys landed in the cache against that
  map. This is what lets the harness report "0 png" as a real capability fact instead of a
  structural artifact of counting a directory that happens to hold none.
- **TUI preview time**: a second, separate launch per TUI entrant, after every other number for
  that entrant has already been taken, so a screen capture never lands inside the timed pass. The
  harness screenshots a fixed band of the screen (the region every previewing entrant was observed
  to draw into, clear of status lines) and counts unique colors in it; a frame is accepted as a
  preview once two consecutive polls both clear a minimum unique-color threshold, which is what
  tells a photograph apart from a status line or a text listing. An entrant with no image-preview
  feature at all is expected to time out and is reported as such, not as a failure.

## Fixtures are payload-checked before a run, not assumed

Before any run starts, the field bench re-verifies the fixture it was pointed at rather than
trusting a path: the visible file count must match what the run expects, every dotfile present
must be either a known fixture marker or one of the media builder's encoder seeds (an unexpected
dotfile fails the run by name), and the fixture's filesystem must be `btrfs`. This exists because
a fixture whose payload silently drifted (an interrupted build, a stray file added by hand) would
otherwise still produce a run, and produce numbers nobody could trust.

## An idle box is a run condition

The harness refuses to start a run above a one-minute load average of 0.50, and instead waits
(up to 15 minutes by default) for the box to settle, printing what it is waiting for. This exists
because a run once started against a load average of 1.10 and produced a timing column
measurably worse than the same run at idle, for reasons that had nothing to do with the change
under test. Before starting, the harness also checks that none of the processes it is about to
kill and relaunch are already alive under another owner; if any are, it refuses to start rather
than kill something it did not launch.

## Every process the run starts, the run ends

Before and after every entrant, the harness kills a fixed list of process names covering every
entrant, every previewer an entrant can spawn as a side process (a nautilus thumbnailer, a
terminal's own image-rendering helper), and anything D-Bus-activated that persists past the
entrant that triggered it and writes into the shared thumbnail cache. A single-instance
application that survives a run would otherwise hand its next launch off to the surviving
process and report timings for a launch that never happened; this is why the kill pass runs both
before and after every single entrant, not once per fixture.

## Groups, and how a run is judged

A GUI entrant is judged only against other GUI entrants, and a TUI entrant only against other
TUI entrants; the two never share a ranking. Within the GUI group, an entrant that finishes a
media run having produced zero thumbnails is reported but marked unranked rather than fast,
because a settle time with no generation behind it answers a different question than the one the
column claims to answer. Where the ranked rows in a group differ in thumbnail work by ten times
or more, the settle-time comparison for that group is refused outright, with the ratio that
triggered the refusal printed alongside the refusal. The TUI group's thumbnail column reads
`n/a` with the terminal it ran in named beside it, never `0`: a TUI previews one file at a time
under the terminal's own image protocol, and a `0` there would read as a slow thumbnailer rather
than what it is.

The media fixture's file names sort by format (clips before images before notes before photos),
so an entrant that settles partway through a media run never reaches names later in that order.
A zero, or a low count, in a format column from a field run means "did not get there before it
stopped," not "cannot." Whether an entrant can produce a format at all is answered by a separate
capability pass, not by the field run.

## Capability is measured separately, and is never timed

`tools/flea-bench-capability` answers "which formats can this entrant produce at all," a question
the field run cannot answer because the field run's format numbers are gated by how far each
entrant got through one directory in one pass. It copies one sample file per format out of the
media fixture into a private, sandboxed directory, gives every entrant the same generous fixed
timeout per file format (45 seconds by default, chosen to be far longer than generation could
plausibly need, so a slow entrant is not mistaken for an incapable one), and classifies whatever
landed in the shared thumbnail cache the same way the field run does. Nothing here is ranked and
no time is reported as a result; the only output is which formats each entrant produced and which
it did not.

## The manifest: what makes a number citable after the box moves on

Every field run writes a manifest alongside its results (`tools/flea-bench-manifest`), opened
before the run starts and closed after it ends, recording:

- the kernel, the fixture path, its payload count and filesystem, and whether that count matched
  what the run expected;
- a full histogram of the fixture by file extension, counted from the fixture itself rather than
  from what the builder was asked to produce;
- every entrant's version, read off the installed artifact rather than typed by hand: a package
  manager query for a packaged entrant, a `git describe` and build timestamp for one built from
  source, and the exact source commit, working-tree cleanliness, and binary build time for Flea
  itself. A number attached to a stale or dirty build is not a number about the tree it claims to
  measure;
- how long since the box's last full package upgrade, with a warning if a run is old enough
  relative to that upgrade that the packaged entrants might not be current;
- the load average at the start of the run and how long the harness waited for the box to go
  idle before starting;
- for the TUI group, the terminal used and the exact, materialized configuration each TUI
  entrant read (a copy of the committed template with its placeholder path substituted in, which
  is what actually ran, not the template itself);
- at the end of the run, which formats every entrant produced, refused, or never attempted, and
  which rows the harness declined to rank and why.

## Reproducing a run

From a clean checkout with a release build:

```sh
cargo build --release

# Build the scale fixture (100,000 files) if it does not already exist.
tools/flea-bench

# Build the media fixture (2,000 files, mixed formats).
tools/flea-media-fixture

# Every selected entrant must be installed, Strata as two rows: STRATA_BIN (stable) and
# STRATA_NIGHTLY_BIN (nightly) name both binaries, and a run without them refuses to start.
export STRATA_BIN=/path/to/strata-stable STRATA_NIGHTLY_BIN=/path/to/strata-nightly

# Run the field bench against the scale fixture: three cold runs per entrant, no thumbnail arm.
printf '%s\n' "<sudo password>" | tools/flea-field-bench ~/bench/scale.csv

# Run the field bench against the media fixture: three cold runs per entrant, thumbnail
# generation exercised and the shared cache dropped before each.
printf '%s\n' "<sudo password>" \
  | FIXTURE=/home/flea-sandbox/flea-media-btrfs EXPECT_FILES=2000 DROP_THUMBS=yes \
    tools/flea-field-bench ~/bench/media.csv

# Capability pass: what each entrant can thumbnail at all, unranked and untimed.
tools/flea-bench-capability

# One entrant, re-measured. ONLY scopes the kill list to it as well, so a run on a box in use
# cannot close a window it did not start. This is how strata's rows in media-rc-2044.csv were
# taken, a day after the rest of the field.
printf '%s\n' "<sudo password>" \
  | ONLY="strata-stable strata-nightly" FIXTURE=/home/flea-sandbox/flea-media-btrfs EXPECT_FILES=2000 DROP_THUMBS=yes \
    tools/flea-field-bench ~/bench/strata.csv
```

`tools/flea-field-bench` launches the tree it measures, so it is meant to be run from a copy of
the checkout kept outside the tree under active development (the whole `tools/` directory, since the
bench sources `flea-sandbox-guard` and runs `flea-bench-keys`, `flea-bench-manifest` and its other
sibling tools from its own directory); measuring a tree while also editing
it invites benching a build that does not match the source beside it. The script also checks this
directly and refuses to run if the release binary is older than the sources that would produce
it.

Every run above needs a display: it launches real windows under the desktop session, so only one
such run may be in flight on a given box at a time.
