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

5. **The GPUI launcher leaves adapter selection to GPUI.** `src/main.rs` sends a window
   launch to `gui::exec_gpui`, which finds `flea-gui` beside the launcher first and then on
   `PATH`, settles the shared view state and replaces the launcher with that executable.
   `FLEA_FRONTEND=qml` is refused; `ui/boot/shell.qml` is no longer a launch route.
   `src/gui/gpui_renderer.rs` removes `QSG_RHI_BACKEND`, `FLEA_RENDERER_AUTOMATIC` and
   `QT_VK_PHYSICAL_DEVICE_INDEX` from the child environment, drops an inherited
   `FLEA_VK_PIN` and its ICD list, and adds no display GPU pin or Vulkan probe cache.
   GPUI uses compositor feedback and checks adapters against the window surface.

   **The ICD filter remains.** Before the exec, `src/icdexclude.rs` can set
   `VK_LOADER_DRIVERS_DISABLE` for known driver manifests whose hardware is absent or idle, with
   `FLEA_VK_DISABLE` holding the filter Flea wrote. `src/icdidle.rs` adds NVIDIA's
   manifests while an NVIDIA GPU drives no display beside one that does and has its own
   driver, so the dGPU stays suspended; `FLEA_VK_KEEP_NVIDIA=1` or a PRIME offload variable keeps them.
   An operator's nonempty driver override or a loader settings file prevents that automatic
   exclusion; an inherited filter still matching Flea's marker is reconsidered.
   `src/open.rs` and `src/terminal.rs` remove Flea's marked filter and legacy display pin
   from programs they launch, preserving an operator's unmarked choices.
   The remaining hasvk classification in `src/vulkan.rs` serves the absent-driver filter,
   rather than selecting an OpenGL launch for an older Intel GPU.

6. **A hidden view is not a free view.** `visible: false` does NOT stop a QML view doing
   model work: it keeps its geometry, stays bound to the listing, and pays per row. All
   three views were instantiated unconditionally and hidden with `visible:` alone, and
   the cost was measured at **110 bytes a row with one view instantiated, 232 with two
   and 262 with three**, on the 100,000 file fixture. Gating the two non-default views
   behind `Loader`s was MEASURED to recover 14.7 MB at 100k and take the listing from 264
   bytes a row (that run's own three-view baseline) to 147, and was then **REVERTED and is not
   in the tree**. The repair needs `Pane.qml` split first, because two `Loader`s inline exceed
   the 400-line hard cap, and
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

<a id="a-drag-survives-a-workspace-switch"></a>

- [A drag survives a workspace switch](docs/architecture/a-drag-survives-a-workspace-switch.md)

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
