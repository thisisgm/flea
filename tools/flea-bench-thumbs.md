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

Preview logs and screenshots alone do not prove successful 800 px decode and
render, so `preview_800_ms` stays empty. Inherited `perf task-clock:u` wraps
the private session bus, including activated helper descendants and the bus
itself. This is diagnostic user CPU, not certified whole-cgroup user CPU or
the unpublished source's run boundary. `user_cpu_s` stays empty until the full
workload and whole-cgroup accounting are proven. A missing counter never
becomes zero. The candidate remains unlaunched while thumbnail handles are
unavailable. These limitations prevent a completed benchmark or release win
claim from this recovered harness alone.

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
