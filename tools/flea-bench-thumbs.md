`flea-field-bench --thumbs` restores the 16-file thumbnail worker comparison.
The oracle is section 6 of `BENCH-GATE-46.txt` and the original B7 brief. Its
four columns are all thumbnails done, per-thumbnail worker time, successful
800 px preview time, and user CPU for the whole application run.

The source method is the [benchmark comment on PR 1660](https://github.com/lgse/strata/pull/1660#issuecomment-6172896025).
It used private Xvfb, release builds with LTO disabled, four browser workers,
one warm-up and six alternating measured rounds. This harness uses native
Hyprland/Wayland, a disclosed transport change. A private HOME, XDG directories,
process namespace, session bus and thumbnail cache live under each marked run
root. The operator's cache is never read or cleared. Run real collections only
on the controller's serialized native benchmark lane.

```bash
tools/flea-field-bench --thumbs --help
root=$(mktemp -d /tmp/flea-b7.XXXXXXXX)
touch "$root/.flea-test-sandbox"
tools/flea-field-bench --thumbs fixtures --root "$root"
tools/flea-field-bench --thumbs verify --root "$root"
tools/flea-field-bench --thumbs roster > "$root/roster.json"
# Fill the roster from the controller's current latest-release inventory.
tools/flea-field-bench --thumbs run --root "$root" --roster "$root/roster.json"
```

The fixtures have 16 separately named, reproducible seeded RGB rasters per
family: 6000x4000 JPEG near 2.9 MB, 2560x1440 PNG near 5.9 MB, and 1200x800
JPEG near 100 KB, within 10 percent of the requested size. Unique metadata
prevents identity reuse. The manifest records hashes, sizes, dimensions and
encoder versions. The published source pixels and exact CPU boundary were
not supplied, so comparison with the published After values stays provisional.

The roster includes Flea 0.4.0, Flea 0.3.12, Strata stable and nightly,
Krusader, Dolphin, Nautilus, Thunar, PCManFM and Nemo. Every binary needs an
absolute path, SHA-256, exact version and an inventory timestamp less than
24 hours old. Both Flea builds need their build SHAs; rivals need release
evidence URLs. Stable and nightly require separate archive identities.
The controller owns the latest-release check. The collector checks hashes
and versions again before samples. Package versions use display-free package
ownership and `pacman -Q`, preserving epoch and pkgrel; archive versions use
`--version` and verified B5 receipts, including embedded source commits.
Packaged roster rows include `package_name` and the exact `package_version`.
The candidate also requires `gui_sha256`, the checksum of the `flea-gui`
sibling that the inventoried `flea` launcher executes. Both binaries must
come from the same release build receipt.

`installed-roster --root ROOT --b5-root INVENTORY` inventories nine installed
arms from a marked B5 root with `rival-versions.json`. `FLEA_0312_BIN`,
`STRATA_STABLE_BIN` and `STRATA_NIGHTLY_BIN` select explicit binaries; both
Strata paths are mandatory. `run --installed` runs those arms without claiming
they are latest. `--only` limits a focused run. `--passes 1-7` includes the
warm-up; fewer than seven passes cannot rank. `doctor` reports prerequisites.

Thumbnail completion requires exactly 16 verified fixture outputs at the
same 256 px edge. A cache entry must have the fixture URI's MD5 filename, be
a decodable PNG, and carry the matching `Thumb::URI` when present. Strata's
completion log supplies each worker interval. Flea 0.3.12 uses 16 distinct
`FLEA_THUMB_TRACE` rows through an executable-quoted `FLEA_BIN` passthrough.
The trace has no wall clock, so its batch boundary is estimated from the
10 ms observation poll and each row's total duration. Per-thumbnail time is
worker duration, never the batch duration divided by 16.

Rivals without worker clocks leave those columns empty. A rival's 128 px
normal-flavor cache is retained in `cache_written` but cannot complete the
256 px comparison. `all_thumbnails_settle_ms` is retained only as a diagnostic
window-map-to-cache interval, outside the four ranked columns. A default policy
decline, including Nemo's 1 MiB limit, remains listed and incomplete; it cannot
remove a rival from first-place eligibility.

GPUI Flea uses the existing `FLEA_B7_BENCH` sample token to enable a receipt
producer. With the token absent, it creates no observer or job clocks. For
each thumbnail it records the real viewport request and the backend's
`thumbed.ms` worker duration. A receipt requires a successful decoded-image
paint, visible paint bounds, the exact submitted draw and its compositor
`Presented` terminal with `CLOCK_MONOTONIC` timestamp. Neither cache writes,
pixel differences nor diagnostics can produce this receipt. Sixteen distinct
fixture paths and sixteen independently decoded 256 px cache entries are
required. The native run retains the receipt and a capture immediately after
the reader observes the final presented thumbnail.

The thumbnail batch boundary is first thumbnail enqueue to last successful
output, never process launch or window mapping. Flea records enqueue at the
viewport's actual request and ends at native presentation of the last decoded
thumbnail; Strata and 0.3.12 retain their existing worker-log and trace ends.
Flea therefore includes delivery, upload and compositor latency after worker
completion, which those rival logs do not expose. Per-thumbnail time remains
the median worker interval excluding queue time, read from `thumbed.ms`, not
the batch interval divided by sixteen. Rival methods are unchanged.

After thumbnails complete, Flea follows the same Home, Space and fifteen
Right steps as the Strata arms. In marked B7 runs Quick Look requests a real
800 px longest-edge raster from the original through its normal decode path,
independent of monitor size. `preview_800_ms` is the median of sixteen
request-to-presented-output intervals. A successful paint of a smaller interim
or a failed raster cannot complete it. Every preview has a capture and a
receipt with decoded dimensions and native time. Rivals' unverified preview
logs remain diagnostic and their ranked preview cells remain empty.

Flea runs the private application command in a fresh systemd user scope.
`user_cpu_s` is the difference of cgroup v2 `cpu.stat`'s cumulative `user_usec`
immediately before executing the private session command and after graceful quit
and the last descendant's exit. This spans listing, all sixteen thumbnails,
all sixteen previews and shutdown, the same launch-to-graceful-exit boundary
that wraps the rivals. Backend workers, decode helpers and the private session
bus remain in the scope; system CPU is excluded. The collector retains the
before/after counters, monotonic boundaries, cgroup path and empty final process
set. Forced cleanup, remaining helpers, missing counters or missing preview
outputs leave CPU empty. Rival inherited `perf task-clock:u` remains diagnostic;
those arms still lack certified whole-cgroup CPU. No absent counter becomes zero.
The start marker executes inside the prepared sandbox, directly before
`dbus-run-session`, matching the rivals' perf boundary and excluding sandbox
setup and the collector's Python imports from the counter difference. The
marker stays the session command's parent, because bwrap's exit kills whatever
remains in its pid namespace before the collector can see it. After the command
exits, the marker waits up to 5 s for the rest of the namespace to exit and
records any process still there, which leaves CPU empty. The marker sleeps in
`wait` while the session runs; its drain check, a `/proc` scan every 10 ms for at
most 5 s, and its exit fall inside the difference.

`FLEA_B7_DELAY_MS=1000` is a bench-only negative control: from the first
receipt-eligible paint, normally a thumbnail, it withholds receipt staging for
every job for 1000 ms while the app continues painting. Each job still open then
stages at its next paint, so the thumbnail batch ends at least 1000 ms after the
first thumbnail paint, while previews requested after the window keep their own
time. The token gates this control, the value is bounded at 5000 ms, and mutant
samples are excluded from rankings. Run this control in a separate private
sample and retain both captures and receipts.
Different rival output coverage and the unpublished source corpus still
prevent a completed benchmark or release win claim from the recovered harness.

The display lock serializes the native lane. Ownership requires both descent
from the launched process and the sample marker in `/proc/PID/environ`.
Hyprland addresses may be reused, so PID and marker are rechecked before
focus, input, screenshots and close. Lua dispatch requires the exact `ok`
reply. `wtype` input is checked before and after delivery; focus can still
move during delivery, which fails the sample. XWayland targets are refused.
Quit uses the entrant's documented chord or its owned compositor close
request, then the other action after five seconds. Failed or forced cleanup
cannot produce thumbnail completion metrics. KIO entrants start with an empty
private thumbnail directory so their normal cache policy can save previews.
KIO's cache decision comes from `filepreviewjob.cpp:497-542,667-670` and
`previewjob.cpp:83,251`: a missing thumbnail root has device 0, and the private
session has no Solid backend to authorize it. The sample's TMPDIR is private;
`/tmp` stays read-only so fixtures and executables below it remain visible.

The run retains `manifest.json`, incremental `runs.jsonl`, sample logs,
identity commands, cache entries, perf records and owned-window screenshots.
`summary.json` contains only the four original columns, with medians and
ranges over exactly six measured rounds. Missing rounds, output, rivals,
nonpositive values, simulated rows and ties cannot become first place.
Incomplete collection exits nonzero. `verify-raw` delegates the original
RAW corpus checks to `tools/flea-raw-corpus` when that owner's paths exist.

`tests/bench-thumbs.sh` runs fixture and offline contract tests. The file named
`bench-thumbs-native.py` drives the real collector with stub native commands;
it supplies no native timing or visual proof. `bench-thumbs-stub.py` is its
command fixture, not a standalone suite. Background tests exercise the listed
preflight module and CLI, refusing animated backgrounds and busy renderers;
the excluded controller shell wrappers are not part of this bundle.
