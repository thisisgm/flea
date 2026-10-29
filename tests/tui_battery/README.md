# Native terminal battery

`tools/flea-tui-battery` captures the approved terminal states in independent alacritty,
foot, ghostty and kitty windows, and in a private tmux server inside foot. It uses a
controller-owned Hyprland headless output and compositor-delivered shortcuts. Text
setup replay is labelled separately in the PTY transcript.

The tool uses Python's standard library. Native runs also require the selected
terminals, `hyprctl`, `grim`, `bwrap`, fontconfig and the candidate's optional preview
tools. No package installation or persistent configuration changes are performed.

```bash
run=$(mktemp -d "$PWD/.superpowers/tui-battery.XXXXXXXX")
printf '%s\n' 'owned terminal battery' > "$run/.flea-test-sandbox"
tools/flea-tui-battery --binary /absolute/path/to/flea \
  --boards /absolute/path/to/design-hig --fixtures /absolute/path/to/fixture-bundle \
  --output "$run/capture" --terminals alacritty,foot,ghostty,kitty,tmux
```

The output must be new and beneath an existing, nonempty `.flea-test-sandbox`
marker. Evidence is retained, including failures. Each case's `/fx` tree is built
in its own marked root on tmpfs (`/dev/shm`), the filesystem the boards' folder totals
come from: a tmpfs folder reports 40 bytes plus 20 per entry, and Flea's folder sizes
sum those. Once the terminal closes, the tree is copied into the case's `fx` directory,
where it is retained, and only the tmpfs copy is removed.
The candidate sees only the private writable `/fx` tree, private temporary files and
read-only system libraries. `/fx/v/work` is a namespace path, never a host mount.
The candidate has no host network, clipboard socket or CUPS socket. Two host resources
a user's terminal provides are bound read-only: the system font cache
(`/var/cache/fontconfig`), so Flea's 150 ms font probe answers as it does outside the
sandbox, and under tmux the server socket that `TMUX` names, so Flea can ask its tmux
client for RGB.

`--states tui-list-text,tui-empty` and `--terminals foot` select reruns. `--list`
prints the complete request inventory and exclusions. The original 61 states retain
305 primary terminal/state pairs and 315 captures, including both small-window sizes
and both long-list phases. The approved `tui-convert` and `tui-convert-pdf` references
add ten pairs/captures, for 63 states and 325 captures. Missing Convert integration
blocks those scenarios; unrelated filters and fake diagnostics remain runnable.
The `convert-variants` control also requires every image/PDF format, metadata behavior,
PDF page ranges, multiple-image combined/separate PDFs, office conversion, collision
naming and actual outputs. Missing prerequisites return `BLOCKED`, never a passing skip.

For display-free diagnostics, use the same invocation with `--dry-run` and no binary
or fixture arguments. Each adapter launches a fake terminal around a real controlling
PTY and the real relay. Diagnostics check fragmented query-response forwarding, input,
exit status and termios restoration. They also run negative predicate, separator and
capability controls. No diagnostic image or fake terminal result counts as native proof.

## Display ownership, preflight and smoke

`--preflight` (with `--boards`) answers in about three seconds. It refuses a locked
session, a `FLEA-` output or a `flea.battery.` window left by another run, a mapped
screensaver, and any pointer, focus, workspace or window change during its quiet
window. A native run repeats the same check before it creates its output.

Each terminal opens on the owned output's shown workspace, never a silent one, and
must be shown there before calibration. Every capture checks that the owned output
still shows the owned workspace and that the session is not locked. When either check
fails, the run stops and blocks every remaining state with the same reason. Before
`grim` runs, each capture parks the pointer on the owned output's bottom-right pixel,
outside every window, and reads its position back. The
window class is a valid GTK application ID (`flea.battery.t<hex>`), so ghostty keeps
it. A terminal that opens at another cell size is resized from its own reported cell
metrics; one that reports no pixel metrics is blocked. Each state quits through the
keymap's quit key, with an Escape retry, and the relay only signals a candidate that
did not exit.

`--smoke` runs one state (`tui-list-text`) per terminal. Its status is `SMOKE-PASS`
when every machine check passes and only the controller's visual review is pending.
`SMOKE-PASS` proves the native path, never acceptance.

## Offline CI suite

`tests/tui-battery.sh` is registered in `tests/run-all.sh`. It discovers every
`test_*.py` module, then runs the display-free diagnostics through all five fake
adapters. It ends with `PASS tui-battery: <count> regressions, 5 fake adapters` only when
all discovered tests and all diagnostics pass. The count follows future test additions.
Failures, skipped tests, incomplete diagnostics and missing reference inputs fail the suite.

The wrapper owns a marked `mktemp` root, starts Python with `env -i`, `setsid -w`
and stdin from `/dev/null`, and sets only private HOME/XDG/temp/test/reference paths.
Every regression respects `FLEA_BATTERY_TEST_ROOT`; the wrapper removes only its own
marked root on exit. The CI broker supplies the private PID namespace and disabled
network. No native terminal, display, installed chafa or external design directory
is needed by this offline suite.

`fixtures/references.tar.xz` contains byte-for-byte approved TUI HTML and PNG inputs;
`fixtures/references.json` records their provenance, sizes and SHA-256 identities.
The offline loader checks the archive, every member and the complete inventory
before unpacking into its marked root. Reference changes require an explicit snapshot
refresh from the approved design set. This frozen offline snapshot does not authorize
native visual acceptance; native runs continue to use the controller's `--boards`.

## Fixture bundle

The read-only bundle's `manifest.json` has schema:

```json
{"families":{"work":{"root":"original-work","features":[],"entries":[
  {"path":"A","kind":"directory","mode":493,"mtime_ns":1750000000000000000},
  {"path":"notes.txt","kind":"file","mode":420,"mtime_ns":1750000000000000000,
   "size":99,"sha256":"SHA-256 of the exact source bytes"},
  {"path":"Media/link.png","kind":"symlink","mode":511,
   "mtime_ns":1750000000000000000,"target":"photo.png"}
]}}}
```

`tools/flea-tui-fixture-bundle OUTPUT [--seed BENCH_WORK_DIR]` writes this bundle, read-only, beneath a marked
`mktemp` root and prints its `bundle_sha256`. `--seed` copies the bench fixture's Media encoder outputs byte for
byte; missing ones come from the bench recipe's magick and ffmpeg commands, and `report.pdf` takes the recipe's raw
pixels in the bundle's own PDF writer, with fixed dates and no `/ID`. In `test_fixture_bundle.py`,
`test_two_builds_without_a_seed_in_different_seconds_match` checks that two `--without-rasters` builds on one host,
started in different seconds, print the same digest, and `test_recipe_report_pdf_ignores_the_writer_clock` checks
that a clock-stamping writer cannot change `report.pdf`. No test covers the reference rasters a build without
`--without-rasters` adds: they are `pdftoppm` and `ffmpeg -fflags +bitexact` PNGs, and the LibreOffice PDF export
they rasterize stays in the build's scratch directory.
With `--fixtures`, `--list` and `--dry-run` verify every family the selected states name and report each family's
`requires_not_supplied`; a missing or altered family exits 2.

List every directory and file, not only the illustrative entries above. Families
are `work`, `office`, `prints`, `ja`, `ar` and `trash`. Each contains its own work
snapshot. Original Docs must have three entries; office Docs must have exactly the
eight reference files and their exact document sizes. Directory modes are applied
after copying, so source Locked can remain readable while its private copy is locked.
All source bytes, copied metadata and actual uid/gid values are recorded.

Optional `auxiliary` entries seed private tools, config, data, state or home. Each
entry has `path`, `source` relative to the bundle, `size`, `sha256` and `mode`.
For example, private provider helpers and Trash metadata belong here. Auxiliary
entries cannot replace existing files or escape the private tree. Family `features`
declare which fixture integrations they supply; observable behavior still has to pass.

`--seams FILE` accepts a map from state IDs to `features` and `environment` objects.
Only explicit candidate `FLEA_TUI_` test variables are accepted. This is for forced
font states, never evidence that kitty or ghostty lack their builtin glyphs. Graphics
negative states are implemented by the relay itself: it drops only recognized positive
graphics replies and logs every dropped response. It never invents a positive reply.

`--steps FILE` replaces setup steps for named states when the integrated keymap or
fixture operation needs different setup. Steps support `action`, `key`, `text`,
`select`, `menu` and `wait`. Actions resolve through the candidate's `keys.toml` with
preset precedence. Predicates and comparisons cannot be disabled through this file.
Progress requires a real controlled I/O fixture and its setup steps; its default
unresolved hook is a blocker. Provider, plugin, CUPS, language and font seams also
require their actual integrations. An asserted feature does not synthesize the UI.

Permanent-delete cases always send compositor Shift+Delete and require actual incoming
`CSI 3;2~` bytes in every terminal, including tmux. Synthetic decoder input cannot
satisfy this proof. `X` is Carry. Required enhanced/plain pairs
are Ctrl+Enter/O, Ctrl+Shift+Z/Ctrl+Y and Ctrl+Shift+N/N; Ctrl+Tab retains Ctrl+PageDown.

## Geometry, comparison and review

`--profiles FILE` supplies per-terminal `font` and `font_size`. Initial values are
launch seeds. Actual PTY integer cell metrics, compositor client geometry and the
observed row-2 separator calibrate the grid before input is sent. The rule's full-width run
fixes the grid's left edge. Its thinnest column gives its thickness, since a junction's stroke
continues below it. The grid's top follows from where alacritty, foot, ghostty and kitty draw a
light rule, `(cell height - thickness) // 2` pixels below the top of its cell. The first plain
filled cell on screen then confirms that top, because a cell background covers exactly its cell.
An absent or ambiguous separator, or a fill that disagrees, blocks with the coordinates it compared. Native images are never stretched to the browser's 8.4 by 22 cells.
The two small-window reference origins are `(4,9)` and `(4,175)`.

Board HTML supplies text, wide cells, frames, fills and ink roles at exact cell
positions. Comparison checks blank cells too. Only verified filesystem/free-size
facts may change after a separate real candidate backend reports exactly those facts.
Expected text is rebuilt from that reply and the approved localized labels. Following
`boards_tui.py:status`, the tail ends at column `COLS - 1`; its start follows its current
cell width. The hint suffix stays fixed. Wider and narrower values, units and filesystem
names retain this right anchor. Every resulting character, width, label and space is
compared normally. Foreground, background and attributes remain checked on every field
cell, including spaces and padding vacated by a narrower tail. Counts, hints and owner metadata remain checked without substitution.
English, Japanese, Arabic and Russian disk fields all require the backend assertion;
localized decimal separators are normalized only for verification. Character positions
use terminal columns, including wide Japanese glyphs. Generic rectangle exemptions
are rejected by the comparator.
Every captured grid gets an unscaled difference and 50 percent overlay. If dimensions
differ, artifacts are padded and the geometry constraint is explicit.
Reverse video is resolved to displayed foreground/background; bold, dim and underline
must match the reference attributes. Properties Kind uses the MIME kind word,
including `Plain text document`, and remains an exact cell comparison.

Primary kitty/ghostty image states require actual Kitty replies and placements;
foot requires DA1 sixel support and sixel traffic. Both require nonempty native
picture pixels. Kitty storage (`a=t`) alone is insufficient: a completed `a=T`
display or payload-free `a=p` placement of its fully transmitted image is required.
Every image must begin after a candidate-received positive reply, using the raw PTY
timestamps even when its control is fragmented. Sixel obeys the same ordering.
Replies must start after their matching query completes, including when fragmented.
Alacritty and passthrough-off tmux use primary half-blocks; forced
fallback scenarios are labelled separately. Half-block picture regions use the corresponding
image or model fallback reference. PDF, office, video, Quick Look, 3D Quick Look,
print-sent and Arabic PDF states use independently prepared source rasters and the
actual chafa executable. Combine uses ordered numbered boxes. Full raw native captures,
exact cell expectations and chafa command/version/exit receipts remain available.

For those eight states the fixture family's `representations` maps each state ID to
`{"sources":[{"input":"v/work/Docs/garden-report.docx",
"raster":"data/reference-pages/garden-report-1.png",
"derivation":"First page independently exported and rasterized from the source"}]}`.
Each input must identify that state's actual cursor fixture, in listing order for
Combine. Original inputs must match the copied fixture hashes; prepared PNG rasters
must match the independently imported auxiliary hashes. A PNG input can itself be its
raster. A prepared raster must include its derivation for the controller to inspect;
it must come from an independent source render, never the candidate capture.
The controller's representation decision approves the exact input/raster hash pairs
and their content correspondence as well as the displayed mode. This review is
required even when the automatic cell comparison passes.

The driver verifies source dimensions and aspect against the approved picture bounds,
then independently invokes chafa with explicit half-block, full-color, no-dither,
no-probing options, the calibrated font ratio and the bounded picture size. It parses
that output into exact glyph, width and rendition expectations. Partial edge cells
and all padding remain compared, along with all unchanged cells outside the picture.
Missing tools, wrong source hashes/aspects, unsupported output and overflow block proof.
No picture rectangle is skipped. Combine's two source identities remain checked; its
numbered boxes retain card ground, muted digits in listing order, a muted frame and
an accent focus frame on the first image in this initial state.

The first capture can run before review. It saves `media.json.review_requirements`
and stays `BLOCKED` while still producing cell comparisons. After inspecting the
source/render correspondence and full capture, copy that exact object to the case's
`representation` in `--reviews`, adding `"approved":true`. It binds mode, cell/pixel
bounds, source/raster hashes and dimensions, derivation, chafa binary hash, arguments
and calibrated cell size (or Combine's focused image). Reconciliation accepts this
review only when automatic content, geometry and color checks passed and retained
source bytes, capture configuration and all original artifact hashes still agree.

`--reviews FILE` maps `terminal/state/variant` to `capture_sha256`, `board_sha256`
and `approved`. A fractional-cell constraint additionally needs `geometry_constraint`
with the exact `native_size`, `reference_size` and a nonempty measured `reason`.
A visual review cannot waive missing source/chafa proof or excuse wrong text,
geometry, whole-cell colors, focus, missing graphics, changed artifacts or unsafe shutdown.

After reviewing retained PNGs, run with `--reconcile /absolute/path/to/capture`,
the same binary and boards, `--reviews`, and a fresh `--output`. It verifies the
original source/binary/reference identities and every artifact hash without launching
windows again. Changed identities require affected captures to be repeated.
Reconciliation must request the captured `--preset`, `--profiles`, size variant,
logical setup and seams. It verifies the hashed terminal profile/config files,
fixture identity, launch/relay specifications, original and final candidate settings,
dynamic facts and raw graphics evidence as well as image artifacts. Captures lacking
these original hashes remain blocked. A changed preset, font, size or setup cannot
reuse an old approval, even when the screenshot itself is unchanged.
Both live execution and reconciliation require zero terminal and candidate exit codes
and restored candidate/relay termios. Missing or signal exits fail acceptance.

The Python regressions include native-boundary doubles for the real acceptance paths.
`settings_reader/Cargo.toml` is a dependency-free probe of Flea's actual Rust settings
reader: run it through the build broker, passing preset/JSON pairs emitted by those
regressions. It verifies nondefault root-level `keys` without modifying product code.

## Supplemental controls

`controls.json` enumerates required native controls and their individual assertions.
`calendar.json` retains the original 17 calendar labels and original driver identity.
`surfaces.json` registers close matches, Ctrl+G contents search, the Jobs status row
and the open-refused sentence. These cases remain explicit blockers until their native
fixtures and cell mappings land. Surface receipts must prove all listed assertions,
including the close-match counts, two-row contents results, `· 3 jobs`, and a refusal
in the error role with no application launches.
These remain blocked until the controller supplies `--supplement BUNDLE`; board
captures alone never satisfy them. Existing wake and playback-quit suites can contribute
receipts, with their real execution environment recorded.

The supplemental bundle contains `manifest.json` with the capture `identity` and
`records`. Each record has `terminal`, `kind` (`control`, `calendar` or `surface`), `id`, a relative
`directory`, artifact `sha256` values and `verdict_sha256`. Each directory contains
the standard capture artifacts plus `profile.json` and `verdict.json`.
The verdict requires separate `behavior`, `cell_geometry`, `color` and `native_visual`
values of `PASS`, `native_capture_sha256`, and every named `assertions` value true.
Calendar verdicts also require `original_driver_sha256` and exact-date, selected-identity,
view-predicate and clean-quit assertions. Missing variants, stale identities, empty
transcripts, synthetic captures and incomplete grids are blockers.

Exit codes: 0 means the requested native records and supplemental controls passed,
or the explicitly labelled dry-run diagnostics passed; 1 means a failed comparison or
shutdown; 2 means blocked prerequisites or pending review. Neither a scoped pass nor
a dry-run pass establishes completion of the entire native release matrix.

## Candidate composition

`python3 -B -m tui_battery.candidate --boards H --list` (with `PYTHONPATH=tests`)
prints the complete derived candidate inventory. The composition CLI accepts the existing
binary/boards/fixtures/profile/step/seam/review inputs, `--identity-inputs`, and
`--calendar-bundle`, `--controls-bundle`, `--surface-bundle`. Use `--reconcile` to consume
matching primary captures without opening windows again. It invokes the accepted
calendar/control verification and the existing battery driver. It never promotes
proof rows. Missing routes, fixtures, outputs, lifecycle records or review remain blocked.

The final matrix requires exact identities and inventories, including all 325 primary
variant captures, 85 calendar, 135 control and 20 surface records. The small windows,
long-list phases, Convert formats/options and original date labels remain mandatory.
`--identity-inputs` names actual external catalog/roster/font/keyboard receipt files;
the tool also binds all shipped i18n JSON and the keymap. Capture and reconcile require
the same identities. Every present root JSON hook and all four lifecycle receipts are
sealed, and control supplements run `control_native.validate_record` before acceptance.

For `tui-3d-quicklook`, the driver observes two actual Right inputs and changed presented
pictures, retaining the full board/source comparison and independent 30-degree raster.
No visible angle label is required. A seam's `tool_policy: no-chafa` creates a private
resolver that excludes chafa and has no system PATH fallback; the facts-only controls
still need their actual assertions. Operation receipts require real retained source/output
bytes and private HTTP/IPP traffic, not feature names. `candidate.catalog_case` reuses
the caller's display lease and private locale preparation for I50-27. Final capture and
all-language acceptance remain controller/I50 work after their prerequisite gates.
