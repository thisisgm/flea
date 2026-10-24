# Flea

A GUI file manager for Linux, keyboard first, native to Omarchy.
P0 is the local browser, and remotes, search, disk operations and Flea's own terminal
interface (`flea --tui`, `src/tui`) have since landed on top of it. An encrypted drive
unlocks and locks from its rail row through `src/unlock.rs`.

## Reading the architecture reference

Read the seven load-bearing rules below before changing the implementation. Then
read the sections relevant to the task through the section routes at the end of
this file or the [architecture index](docs/architecture/README.md). The index also
links directly to every subsection.

Use the module map to locate implementation owners, Testing for verification,
and File budget for file limits. Read the listing, preview, thumbnail or write
sections when changing those paths. A reference to an original section title,
including one formerly described as above or below, resolves through this index.
Source paths in the preserved reference text are relative to the repository root.
Markdown navigation links are relative to the document containing them.

## The seven load-bearing rules

1. **Every per-file operation stays scoped to the viewport.** Icons, MIME sniffing and
   thumbnails all stay inside the visible rows. Most of the measured difference on a large
   directory comes from this: the other file managers measured stat every entry in the
   directory and Flea stats only the ones a screen can hold. Measured: the compared apps stat
   100,000 entries, Flea's first window stats 350. `ui/Pane.qml` owns the viewport-sized held window and calls
   `ui/Backend.qml`'s `window(start, count)` request only when that window drifts.

2. **The QML model is an integer count, never a list.** `ui/Pane.qml` binds its
   `ListView` to `model: root.total`, so it instantiates and recycles only the delegates
   the viewport needs. A `Repeater` over a fetched window was tried first and dropped
   220 of 480 frames, because swapping the window destroys every delegate it held.
   The count reaches each view through `ui/StableModel.qml`, a `DelegateModel` over a
   fixed integer, so a count change moves a group's tail instead of resetting the model
   and keeps the delegates that stay drawn (see "The list model").

3. **`list` answers with the first screenful, unasked.** The window is rows from 0, unless a `select` names a row
   at or past `first`: `Listing::first_start` in `src/backend/listing.rs` then starts it at that row. `ui/Backend.qml` sends one
   `list` request and emits its `listed` and initial `rows` replies; `ui/PaneSwap.qml`
   consumes those replies as a pair before accepting the new held window. Making the
   client ask for it separately costs a full round trip that lands mid first-paint.
   Measured: 64 ms if asked for afterward, 4 ms riding along with the listing.

4. **Prewarm stays disabled until it is both safe and faster.** The `flea --prewarm`
   producer exists, but the proposed UI reader measured a 485 ms median against 439 ms
   for the live path. After its removal, ignored experimental generation measured
   469 ms against 437 ms live. Production `ui/WindowBody.qml` intentionally calls only
   `pane.open(start)`, and `tools/flea-first-paint` preserves the comparison. The reader
   was rejected as slower and stale-capable; it remains disabled until the prewarm file
   can prove it is still fresh and a new measurement proves a real win.

5. **Vulkan where the loader can deliver it, lazy multimedia later.** `src/gui.rs` sets
   `QSG_RHI_BACKEND=vulkan` when the user did not choose a renderer and `src/vulkan.rs` has
   created a throwaway instance and seen a device, which costs 2.4x less memory than the OpenGL
   default and initialises 35 ms faster, with identical frame timing. The same probe now reads
   each device's PCI id and matches it against `/sys/class/drm` connectors whose `status` is
   `connected`. Flea sets `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` to the ICD whose `library_path`
   belongs to the display GPU when Vulkan lists a display GPU beside one that owns no connector and
   that other GPU's PCI vendor differs. The vendor condition is load bearing: an ICD list is vendor
   granular, so two cards from one vendor, an AMD APU beside a Radeon or an Intel iGPU beside an Arc,
   cannot be separated by one, and a pin there would claim a restriction it did not make.
   That is the hybrid-GPU blank window: Hyprland compositing on NVIDIA while Qt
   opens Intel, which presents a mapped window with no buffer. An index pin (`QT_VK_PHYSICAL_DEVICE_INDEX`)
   cannot be used: this box enumerates NVIDIA then Intel to `vkEnumeratePhysicalDevices` and
   Intel then NVIDIA to QRhi, so the index Flea would hand Qt is the other GPU. Restricting the
   ICD list does not care about order. Forcing the NVIDIA ICD on every NVIDIA laptop would blank
   classic Optimus, where the compositor sits on Intel and the connected connector is Intel's.
   An explicit `QSG_RHI_BACKEND=vulkan` now runs the full probe too, where before this merge an
   explicit renderer skipped it entirely: measured on a quiet box, 25 interleaved pairs of the whole
   launch to a stub `qs`, the explicit Vulkan arm takes 10.0 to 10.6 ms against 1.9 to 2.0 ms for the
   explicit OpenGL arm, so it pays the same roughly 8 ms the automatic arm already paid.
   An explicit `VK_DRIVER_FILES` or `VK_ICD_FILENAMES` is the operator's, and an exported-but-empty
   one is absent, the same rule `QSG_RHI_BACKEND` follows. The pin is said once on stderr and names
   the chosen GPU's PCI pair and the ICD path, because a silent device change is the defect the
   OpenGL downgrade already refused to hide, and a hybrid box whose display GPU matches no ICD file
   says that instead and pins nothing. A connected connector is only a proxy for the compositor's
   render device, so a lid-closed box whose one connected panel hangs off the other GPU is a shape
   this heuristic can get wrong; the operator's own `VK_DRIVER_FILES` is the escape, and this box is
   single-GPU so neither the heuristic nor its limit can be validated here. The pin belongs to Qt and
   to nothing else Flea starts, so it carries a marker, `FLEA_VK_PIN`, and `src/open.rs` and
   `src/terminal.rs` drop the pair and the marker together when it is set. Without the marker they
   drop nothing, because an operator who exported a list of their own must keep it in the program
   they opened. Another GPU must also own a DRM card: a software ICD such as lavapipe is a Vulkan
   device owning no connector, and without that condition it would make a plain single-GPU box pin.
   A box whose
   every Vulkan device owns a connected connector, or that has no DRM connectors at all, leaves
   the loader's default and says nothing.
   **A driver for a GPU vendor the box does not hold is not loaded where the loader can be told so,
   and that is not the pin.** On an AMD-only box (an RX 7900 XTX beside a Granite Ridge iGPU) with
   `nvidia-utils` 610.57.04 installed, the NVIDIA ICD made `vkCreateInstance` take 0.63 to 1.1 s and
   found no device: it runs `nvidia-modprobe` twice, and each run took 365 to 370 ms there. Qt then
   paid the same again, so Super+Shift+F mapped the window 1.56 to 1.62 s after the key. In a clean
   Omarchy 4.0.4 lab guest with the same package and a virtio GPU, `nvidia-modprobe` returns in 2 ms
   and the NVIDIA ICD costs the launch about 20 ms, 17 to 31 ms against 2.9 to 6.2 ms over 12
   interleaved pairs of launches to a stub `qs`, so the size of the win is the box's and not the
   driver's. The pin cannot help, because every device the probe lists is AMD. `src/icdexclude.rs`
   runs before the probe and before `qs` in every renderer arm, because the OpenGL retry inherits the
   shell's filter and zink draws GL through Vulkan. It sets `VK_LOADER_DRIVERS_DISABLE` to the
   manifest file names whose `ICD.library_path` file name is, whole, one of six packaged hardware
   driver libraries (`libGLX_nvidia.so.0`, `libvulkan_nouveau.so`, `libvulkan_radeon.so`,
   `amdvlk64.so`, `amdvlk32.so`, `libvulkan_intel.so`) whose vendor no PCI display controller (base
   class 0x03) and no PCI-backed DRM card carries; a vendor word inside any other name names nothing,
   because only these are known to drive one vendor alone. Venus, `libvulkan_virtio.so`, is kept:
   its vtest transport serves Vulkan from whatever GPU or software renderer backs it, with no virtio
   device on the box. Vendors are read from
   sysfs, never from Vulkan, which is the call being avoided, so a dGPU whose `nvidia-drm` is not
   loaded keeps its driver. Nothing is excluded when the operator set `VK_DRIVER_FILES`,
   `VK_ICD_FILENAMES`, `VK_ADD_DRIVER_FILES`, `VK_LOADER_DRIVERS_SELECT` or
   `VK_LOADER_DRIVERS_DISABLE`, when `vulkan/loader_settings.d/vk_loader_settings.json` exists under
   any root searched, when a display controller's or DRM card's vendor or device id cannot be read,
   when any DRM card's device is not on the PCI bus (a SoC GPU, a virtio-mmio GPU, an evdi dock),
   when neither a display controller nor a DRM card names a vendor, or when the search lists more
   than 256 manifests, counted before any is opened. A driver it cannot name is kept: lavapipe,
   swiftshader, any other library, and any manifest that is not a regular file, runs past 4 KiB
   (packaged ones are a few hundred bytes, and jsondoc's object parse is quadratic in its keys) or
   does not parse. `libvulkan_intel_hasvk.so` is excluded when no present display device is
   hasvk-class (Gen7 and Gen8 Intel), and kept beside one, where the probe's `hasvk_only` and the
   display pin decide; loading it costs a cold launch 12.7 ms, range 6.9 to 25.3, on minipc, and
   finds no device. Manifests are opened through `backend/regfile`, non-blocking, so a fifo swapped
   in after the check cannot hold Flea's read; a fifo the loader itself opens still holds its
   probe, as it did before this. The loader's filter names files and not paths, compares them
   case-insensitively and reads only its first 16 entries, so a name any kept manifest on the search
   path shares is kept, a name its glob would read as more than itself is never written, and NVIDIA's
   names go first in a list that stops at 16. The search is a superset of the loader's, read as raw
   bytes: config home, `XDG_CONFIG_DIRS`, `/etc/xdg`, `/etc`, data home, `XDG_DATA_DIRS`,
   `/usr/local/share` and `/usr/share`. `vulkan::icd_library` reads `ICD.library_path` through
   `jsondoc`, for the pin as well, because a first-match text scan read a decoy key placed before it.
   `src/jsonmembers.rs` finds both keys the way the loader's cJSON does, without regard to case, and
   sees an exact repeat, which `jsondoc::parse` folds into the last while cJSON takes the first, so a
   repeat of either key names no library. cJSON hands names and the path to the loader as C strings,
   so a name is compared only up to its first NUL, which makes `ICD\u0000x` a repeat of `ICD`, and a
   path holding a NUL names no library. A member anywhere else, or a string holding the words,
   changes nothing, which keeps the pin working for manifests with metadata. Measured on the AMD box,
   `vkCreateInstance` then takes 6 to 8 ms and the window maps in
   154 to 185 ms over two warm launches against 1622 to 1950 ms over three for the installed 0.3.7;
   deciding costs well under a millisecond: over 25 interleaved pairs of the launch to a stub `qs`,
   the same build given the operator's own filter, which skips the decision, differs by a paired
   median of 0.06 ms both on the AMD box (deciding slower in 14 of 25 pairs, inside the noise) and in
   the 4.0.4 guest (19 of 25), and the explicit OpenGL arm measured above now pays it too. A loader
   older than 1.3.234 ignores the variable and is as slow as before. Nothing is said on stderr,
   because no device changes. The marker `FLEA_VK_DISABLE` holds the exact filter written: a launch
   inheriting a filter its marker names, a new window or the OpenGL retry, decides again, and
   `src/open.rs` and `src/terminal.rs` drop that pair; a filter its marker does not name is the
   operator's, so only the stale marker goes. Helpers QML starts directly inherit the filter, as they
   inherit the pin; it names only drivers for absent vendors, so an eGPU of such a vendor plugged in
   while Flea runs is the one shape it hides from them. A new window inheriting Flea's own pin adds no
   filter, because the pin's `VK_DRIVER_FILES` already loads only the display GPU's ICD.
   A loader that cannot deliver a
   Vulkan device is given `opengl` before `qs` starts at all, because Quickshell hands
   `QRhi::create` a `QVulkanInstance` it never created and SIGSEGVs there rather than raising
   the scene-graph error the QML arm listens for, issue #14 on a QEMU Virtio GPU. That downgrade
   is not silent: `usable()` answers with the call or library that refused, and with the
   extensions in the two arms that asked for them, and `src/gui.rs` prints that as one sentence
   on stderr, because a 2.4x memory regression the operator cannot see is the defect and not the
   report of it. The probe is `dlopen` plus `vkCreateInstance` plus `vkEnumeratePhysicalDevices`,
   and it costs an implicit launch roughly 8 ms on this box, so about a quarter of that init
   advantage is what buys the crash out; dropping the device count would save only 2 of those
   milliseconds and would stop catching a loader that creates an instance and then lists nothing.
   A fingerprinted cache in front of it (`src/vkprobecache.rs`) skips the probe when boot, GPUs, manifests, loader, environment and binary all match.
   A scene-graph failure after launch still relaunches once with OpenGL and drains the failed
   backend; an explicit `QSG_RHI_BACKEND` is never replaced, and
   an exported-but-empty one is absent rather than a choice, the same rule `paths::has_display()`
   applies. What the probe proves is exactly what it asks the loader for: an instance carrying
   `VK_KHR_surface` and the session's own surface extension, `VK_KHR_wayland_surface` here and
   `VK_KHR_xcb_surface` where only `DISPLAY` is set, plus a physical device. Those three names are
   Qt's own: `libQt6Gui.so.6`, `libQt6XcbQpa.so.6` and `libQt6WaylandClient.so.6` here carry exactly
   those and no other surface name. The request is not satisfiable without a driver either: with
   every ICD hidden this loader enumerates 5 instance extensions and no surface one, and
   `vkCreateInstance` answers `VK_ERROR_INCOMPATIBLE_DRIVER`. What it does not prove is that a
   device can present, which an instance-level request cannot ask and this probe never does. The
   cost is measured against a probe-free build of the same tree, 25 interleaved pairs of the whole
   implicit launch to a stub `qs`, timed in microseconds: 10.6 to 11.6 ms with the probe against
   2.6 to 3.2 ms without it, medians about 8 ms apart, so the probe is most of that launch rather
   than a component hidden inside it. An earlier note here read 11 to 12 ms in both arms, which is
   wrong: only the probe arm is near that, and a 1 ms instrument resolves this gap eight times over.
   The retry arm is covered in two halves, because no whole of it can be driven here.
   `ui/js/Renderer.js` is the decision and the argv, driven by `tests/js/renderer.js`; the signal
   reaching that decision is driven by `tests/ui.sh renderer`, which starts Qt's GL backend with
   no EGL vendor file to load. That is the one scene-graph failure found to be raisable on this
   box: a broken Vulkan loader cannot stand in for it, because the shell dies first, which is the
   whole of issue #14. Measured here with every ICD hidden, `qs` warns `No QVulkanInstance set for
   QQuickWindow` and exits 255 without raising anything; the SIGSEGV is issue #14's own report on a
   QEMU Virtio GPU. `bodyLoader.Window.window` is null while `ui/boot/shell.qml` loads and holds
   the `QQuickWindow` once it exists, and that same `Connections` was measured receiving
   `sceneGraphInitialized`, the signal Qt raises at the phase `sceneGraphError` replaces. What no
   test reaches is the positive arm, a Vulkan failure leaving `QVulkanInstance` valid and failing
   at `QRhi::create`, which no environment variable here was able to produce.
   **`ui/boot/shell.qml` no longer carries the `//@ pragma DefaultEnv QSG_RHI_BACKEND=vulkan`
   line**, so `src/gui.rs` is the only thing that chooses a renderer for a launch, the one OpenGL
   relaunch in `ui/boot/shell.qml` aside, and a direct `qs -p ui/boot` launch bypasses it entirely:
   `tools/flea-metrics-gate`, `tests/ui.sh`, `tests/drag.sh` and the
   `README.md` dev loop each state `QSG_RHI_BACKEND` for themselves, so their numbers stay on the
   Vulkan baseline they were recorded against. A direct launch now bypasses two more things the
   launcher does, `FLEA_FIRST_PAINT` and the gtk3 trade of "The first window", so anything that
   TIMES a launch goes through `$FLEA_BIN --gui` instead: `tools/flea-field-bench` always did, and
   `tools/flea-first-paint` was moved onto it when this landed. `tools/flea-metrics-gate` starts
   nothing and times nothing, it reads the token seam over IPC from a Flea the operator started, and
   the four direct `qs` launches in `tests/ui.sh` assert a cursor, a selection, a scene-graph error
   and the retry helper's argv rather than a duration, so each is still the right launch for its job.
   Preview and QtMultimedia are in the tree and instantiated lazily: `ui/PreviewMedia.qml` is the
   only file that imports QtMultimedia, and `ui/PreviewPdf.qml` the only one that imports
   QtQuick.Pdf, each reached through a `Loader` built by the first press of play or the first
   PDF open. Compiling is a separate step: `ui/QuickLookWarm.qml` calls `Qt.createComponent`
   for the unit of the cursor's file kind (Markdown, PDF, clip or audio, image, text) after a
   1 s rest on such a file, so a rest on a clip or audio file resolves `import QtMultimedia` and
   a rest on a PDF resolves `QtQuick.Pdf` with Quick Look never opened, which buys a first Space
   that compiles nothing, and a window that never rests on one of those loads neither library.
   Headless offscreen `qs` (`tests/quicklook-firstkind.sh`, five runs, a fresh process each),
   PSS from `/proc/self/smaps_rollup` after the rest: a text file median 121,816 kB (range
   89,788 to 127,020), a clip median 129,359 kB (range 68,826 to 147,303), the per-run clip
   minus text difference median +5,309 kB (-20,962 to +25,487, three of five runs within 5,000
   to 6,100). The spread is wide because the run shares the box, so this is a headless
   estimate and not a field number.
   Issue #160: when every Vulkan GPU is a Gen7 or Gen8 Intel GPU (Ivy Bridge to Broadwell and Cherryview, served only by
   Mesa's hasvk), the launcher starts the shell on OpenGL instead and says so once on stderr naming
   hasvk and the fallback, with no retry marker; a CPU-type device such as lavapipe is not a GPU and does
   not count, a hasvk device beside another Vulkan GPU keeps Vulkan,
   and an explicit `QSG_RHI_BACKEND` is never touched. The environment is the only setter either way:
   `ui/boot/shell.qml` carries no `DefaultEnv` pragma for it, so what `src/gui.rs` puts in the `qs`
   environment is what Qt reads.

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

7. **The media comparison measured `strata` fully on 2026-09-03, and two of Flea's figures moved.**
   Measuring `strata`'s work properly moved it from unranked into the table, and Flea's relative
   figures moved with it: Flea's memory went from fifth of six to **sixth of seven** at 112.6 MiB
   against strata's **70.3 MiB**, and Flea's CPU time, which was 3.61x lower than `nemo`'s, is now
   **1.28x lower than strata's**, 1.42 s against 1.82 s. Flea still has the lowest settled listing time
   of six and the lowest CPU time of seven, and the 100,000 file group did not move at all. This
   was accepted with an optimization pass to follow, so it is a deferred ticket and not a defect.
   Item 6 above is the first place to look: its `Loader` gating was MEASURED to recover 14.7 MB at
   100k and is not in the tree, and memory is the figure whose position changed. Read its
   caveat first, because that figure is a list-view number rather than a whole-product one, and
   the split it needs broke `tests/ui.sh` twice. The CPU figures in this rule predate 0.3.2 and are the
   reaped-tick column, which counted no sandboxed decode for Flea or strata; see "Testing" for the
   control-group column that replaced it.

## Architecture section routes

Each link names the destination of the original top-level section. The
[architecture index](docs/architecture/README.md) includes the subsections.

<a id="two-phase-listing"></a>

- [Two-phase listing](docs/architecture/two-phase-listing.md)

<a id="the-open-directory-is-watched"></a>

- [The open directory is watched](docs/architecture/the-open-directory-is-watched.md)

<a id="mount-workers"></a>

- [Mount workers](docs/architecture/mount-workers.md)

<a id="the-listing-swap"></a>

- [The listing swap](docs/architecture/the-listing-swap.md)

<a id="the-preview-swap"></a>

- [The preview swap](docs/architecture/the-preview-swap.md)

<a id="why-the-listing-is-an-arena"></a>

- [Why the listing is an arena](docs/architecture/why-the-listing-is-an-arena.md)

<a id="the-listing-arena-returns-to-the-os"></a>

- [The listing arena returns to the OS](docs/architecture/the-listing-arena-returns-to-the-os.md)

<a id="prewarm"></a>

- [Prewarm](docs/architecture/prewarm.md)

<a id="the-first-window"></a>

- [The first window](docs/architecture/the-first-window.md)

<a id="predictable-path-writes"></a>

- [Predictable path writes](docs/architecture/predictable-path-writes.md)

<a id="the-state-file"></a>

- [The state file](docs/architecture/the-state-file.md)

<a id="modes"></a>

- [Modes](docs/architecture/modes.md)

<a id="the-file-chooser-portal"></a>

- [The file chooser portal](docs/architecture/the-file-chooser-portal.md)

<a id="show-in-folder"></a>

- [Show in folder](docs/architecture/show-in-folder.md)

<a id="updates"></a>

- [Updates](docs/architecture/updates.md)

<a id="make-flea-the-default-in-settings--about"></a>

- [Make Flea the default, in Settings > About](docs/architecture/make-flea-the-default-in-settings--about.md)

<a id="module-map"></a>

- [Module map](docs/architecture/module-map.md)

<a id="where-the-backend-binary-comes-from"></a>

- [Where the backend binary comes from](docs/architecture/where-the-backend-binary-comes-from.md)

<a id="every-component-needs-a-qmldir-line"></a>

- [Every component needs a qmldir line](docs/architecture/every-component-needs-a-qmldir-line.md)

<a id="where-the-ui-patterns-come-from"></a>

- [Where the UI patterns come from](docs/architecture/where-the-ui-patterns-come-from.md)

<a id="how-the-list-renders"></a>

- [How the list renders](docs/architecture/how-the-list-renders.md)

<a id="the-list-model"></a>

- [The list model](docs/architecture/the-list-model.md)

<a id="file-budget"></a>

- [File budget](docs/architecture/file-budget.md)

<a id="the-key-table-is-generated"></a>

- [The key table is generated](docs/architecture/the-key-table-is-generated.md)

<a id="testing"></a>

- [Testing](docs/architecture/testing.md)

<a id="fixtures"></a>

- [Fixtures](docs/architecture/fixtures.md)

<a id="mime-aliases"></a>

- [MIME aliases](docs/architecture/mime-aliases.md)

<a id="thumbnail-cache"></a>

- [Thumbnail cache](docs/architecture/thumbnail-cache.md)

<a id="thumbnailer-specs"></a>

- [Thumbnailer specs](docs/architecture/thumbnailer-specs.md)

<a id="thumbnail-sandbox"></a>

- [Thumbnail sandbox](docs/architecture/thumbnail-sandbox.md)

<a id="thumbnail-worker"></a>

- [Thumbnail worker](docs/architecture/thumbnail-worker.md)

<a id="markdown-figures"></a>

- [Markdown figures](docs/architecture/markdown-figures.md)

<a id="thumbnail-pool"></a>

- [Thumbnail pool](docs/architecture/thumbnail-pool.md)

<a id="thumbnail-trace"></a>

- [Thumbnail trace](docs/architecture/thumbnail-trace.md)

<a id="thumbnail-requests"></a>

- [Thumbnail requests](docs/architecture/thumbnail-requests.md)

<a id="a-drag-does-not-survive-a-workspace-switch"></a>

- [A drag does not survive a workspace switch](docs/architecture/a-drag-does-not-survive-a-workspace-switch.md)

<a id="drag-to-trash"></a>

- [Drag to Trash](docs/architecture/drag-to-trash.md)

<a id="backend-memory-levers-that-were-measured-and-dropped"></a>

- [Backend memory levers that were measured and dropped](docs/architecture/backend-memory-levers-that-were-measured-and-dropped.md)

<a id="write-operations-and-the-undo-journal"></a>

- [Write operations and the undo journal](docs/architecture/write-operations-and-the-undo-journal.md)

<a id="deliberate-corners"></a>

- [Deliberate corners](docs/architecture/deliberate-corners.md)

<a id="the-image-flasher"></a>

- [The image flasher](docs/architecture/the-image-flasher.md)
