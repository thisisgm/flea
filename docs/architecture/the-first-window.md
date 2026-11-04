## The first window

`flea --gui` execs `flea-gui`, which opens the GPUI window.
The launcher settles shared view state, configures the child environment and disables transparent
huge pages before the exec; GPUI creates the window through `window::open`.
Qt renderer settings, the old display ICD pin and the Vulkan probe cache do not select this window's renderer;
see [GUI renderer fallback](../gui-renderer.md).

### QML startup reference

The QML entry and components below remain in the tree as reference files; the production GPUI launcher does not load them.

**The context menu waits for the first frame (2026-10-13).** Each pane builds `ui/MenuHost.qml`, which holds the menu's inputs, its closed state and reply validation (`ui/js/MenuContext.js`), and loads `ui/ContextMenu.qml` asynchronously one idle second after the first frame swaps and the listing settles; a menu asked for before then loads it synchronously, so the first right-click or menu key never waits on that timer. The merged startup gate pins 587 objects by the census after Drag to Trash (583 before), 172 first-frame units and 1,639,271 bytes by the `wc -c` sum over `tests/startup-units.txt`: `tests/startup-objects.sh` checks that the nine units only the menu uses and the ContextMenu root are absent at its cut, and `tests/menu-settle.sh` that a warmed menu keeps one root across reopen. The merged ceilings are `ui/Pane.qml` 1234, `ui/PaneMenuActions.qml` 527, `ui/ContextMenu.qml` 737 and `tests/js/menu.js` 669, each re-derived with `wc -l`; both unqualified-read profiles follow the merged source lines, with the available profile measured by `tests/staticgates.py`.

Both land235b unqualified-read profiles are re-derived from compiler JSON: 1,436 reads with Quickshell present and 2,185 with its types missing. The missing profile uses a bare Qt import tree and unresolved Process and StdioCollector controls; its previously stale source positions are replaced with the measured positions, retaining the existing reasons, plus the imported Hyprland singleton allowance.

The land235b census compares the actual merged `af9db27ab` UI with the first-use cue loaders: 597 to 587 objects, 171 to 169 first-frame units, and 1,634,373 to 1,630,079 bytes, each unit weighed with `wc -c`. `ui/Row.qml` loads `ui/RowDropTarget.qml` by URL only while its cue or clipboard mark is drawn; `ui/RailTrashRow.qml` loads `ui/TrashDropHandler.qml` synchronously on its first matching drag, preserving a live pane binding and the existing feedback. `ui/DropTarget.qml` and `ui/js/TrashDrop.js` therefore leave the first-frame trace. `tests/startup-objects.qml` drives the handler after its census cut and requires the correct pane, a retained handler and the existing foreign-offer refusal.

The land235c review fixes retain 587 startup objects and the same 169 first-frame units. The `tests/startup-units.txt` byte sum moves from 1,630,079 to 1,630,044: the row's completion guard adds bytes, while the one-line rail comments remove more. Both unqualified-read profiles retain their allowance counts and reasons at the shifted source positions.

The land235r4 Trash backend guard and rail drag hold retain the first-frame unit list. The `wc -c` sum over `tests/startup-units.txt` moves from 1,630,044 to 1,630,730 bytes; `tests/startup-objects.sh` checks the census on both images.

The land235r5 catcher name retains 587 startup objects and the same 169 first-frame units. The `wc -c` sum over `tests/startup-units.txt` moves from 1,630,730 to 1,630,763 bytes. Both unqualified-read profiles retain their allowance counts and reasons at the source positions shifted by the name.

The 0.3.11 stage join of the rail highlight fix and Drag to Trash keeps both first-frame additions, the shared XDG reader and the Trash row, in one inventory of 172 units; the `wc -c` sum over `tests/startup-units.txt` is 1,639,271 bytes, re-derived from the merged files. The rail Trash row carries the same `highlighted` binding as every other rail row.

**Qt never caches a Quickshell config.** Quickshell serves its config through its own `qs:@/qs/`
URL scheme (`src/core/rootwrapper.cpp`, intercepted in `src/core/qsintercept.cpp`) and Qt's QML
disk cache accepts local files only. Run any launch with
`QT_LOGGING_RULES="qt.qml.diskcache*=true"` and every Flea file prints
`File has to be a local file.` Anything loaded by a `file:` URL is cached, which is why the OEM
bar's plugins have `.qmlc` files and Flea had none. So the entry stays small and hands off:
`ui/boot/shell.qml` imports `Quickshell` and `QtQuick` and nothing else, and `ui/WindowBody.qml`
arrives by `file:` URL.

**The entry lives in its own directory because of a Qt rule.** A document implicitly imports its
own directory, and Qt loads every composite singleton that directory's `qmldir` names. An entry in
`ui/` therefore compiles `Theme`, `ViewState`, `Favourites`, `MediaSound`, `Scripts` and
`ShelfPins`, plus the JavaScript they import, through `qs:` before the window can map, and then
never instantiates any of them, because the `file:` side loads its own cached copies. `ui/boot/`
has no `qmldir`. It needs `Commons` and `Ui` symlinks of its own, the same targets as the two in
`ui/`, because `import qs.Commons` resolves against the config root and `ui/boot` is the config
root now. The package ships all four.

**The window waits for its body.** The entry is the window, and its `Component.onCompleted` sets
the `Loader`'s source before the window exists, so the first buffer the compositor gets already
holds the chrome and the rail, and the rows land one or two frames later when the backend answers.
0.3.2 loaded the body on the window's first `frameSwapped` instead: the window mapped about 160 ms
earlier warm and held nothing but its background colour for the 280 ms the body and the listing
took, 1.2 to 2.9 s on the first launch after an update, and that bare window is what people
noticed. Measured 2026-09-22 on the studio folder, warm, five launches each, exec to the first
buffer that holds rows, read off `WAYLAND_DEBUG`: 0.3.1 624 to 655 ms, 0.3.2 401 to 421, this 305
to 312. The map itself moves from 120 to 138 ms to 281 to 287 warm, and on `tools/flea-field-bench`
on the scale fixture from 271 to 299 to 516 to 578, against pcmanfm's 444 to 458 in the 0.3.2 field
run and about the same as thunar and strata, while settled went 1105 to 1255 to 1018 to 1081 and CPU 0.50 to
0.65 s to 0.45 to 0.51. Two other orderings were measured and rejected the same day: a zero `Timer`
after the window exists mapped 15 ms earlier and put the rows 50 ms later, because the build and
the first frame contend; and a `FloatingWindow` created with `visible: false` never maps when it is
set true on Quickshell 0.3.1, so hiding the window until the rows land is not available.
`Loader.Error` logs and quits: an empty window that stays empty is the failure that would otherwise
say nothing at all.

**Launch helpers and prefetch state.** `flea --launch-warm` still runs the page-cache and GVFS
jobs through `src/gui.rs`, and the TUI starts the GVFS warm through `src/tui/warm.rs`.
`configure_child` removes inherited page-cache and GVFS prefetch variables. The main-window
launch supplies its own warming and recording identity through `exec_window`, as described below.

**The GPUI window prefetches the same way (2026-11-02).** `exec_window` in `src/gui/launch.rs` starts the
same `flea --launch-warm` helper before it execs `flea-gui`, and hands the window `FLEA_PREFETCH` and
`FLEA_PREFETCH_SHELL`, its own pid, which exec makes the window's, so the window's backend records the
list at its first rows as it did for the shell. A picker or unlock launch neither warms nor records, and
`configure_child` still strips both from every other child. On the 100,000-file btrfs fixture with the page
cache dropped, launches that had a list mapped at 290 to 349 ms where the same build without one mapped
at 340 to 388, three-launch field batches on minipc.

**The code a launch runs sits together (2026-11-02).** One launch of `flea-gui` runs about 4,300 of its
34,000 functions, 7.1 MB of its 31 MB of code, and the default layout spread them over 362 of the 64 KB
windows the kernel reads and maps around a fault, so about 23 MB of code was resident three seconds after
map. `crates/flea-gui/build.rs` writes a linker script from `crates/flea-gui/startup-order.txt` that puts
those functions first, in the order the launch first called them, in `.text.flea.startup` ahead of `.text`.
On the same fixture the window's PSS three seconds after map was 39.1 to 39.5 MB, against 53.7 to
54.0 MB without the script. Rust's v0 symbol names
carry a hash of each crate's compiler and dependency versions, and the release builds on current stable,
so the list writes every crate hash and back reference as `*`, and a new toolchain still matches; a cold
function sits in `.text.unlikely`, so each line names both sections. Only release builds for x86_64 Linux
take the script, and only after a one-line program links with it, so a linker without `SECTIONS` support
keeps the default layout. `tools/flea-startup-order record` rewrites the list from an Intel PT trace of one
launch, and `tools/flea-startup-order check` fails a build whose section is missing or under 4 MiB, which is
what a list that stopped matching looks like.

**Nothing the first frame does not show is built for it.** `ui/js/Keymap.js` builds its sheet in
`sheetFor`, never in `setPreset`, which a launch runs twice and which spent about 10 ms of a 127 ms body
build on tables no first frame reads. A menu hint is built per action at that action's first `hintFor`,
from that action's own rows: 0.3.5's status strip asks for the key of `deletePermanently` while the body
builds, and a first ask that built the whole table ran about 9,700 JavaScript calls (86 `lookupFor` walks
of the table) inside the first window: 4 to 7 ms of map on the studio folder warm, medians of 20 interleaved
launches per arm against two v0.3.4 arms.

**What has to stay with the window.** `itemRect` is the window's, so `centreOf`, `rectOf` and
`boxOf` stay in the entry and `ui/Ipc.qml` reads them through its own `fleaWindow`. `sceneGraphError`
arrives on the first frame, and the entry is the one object that always exists, so the handler and
PR119's one OpenGL retry are in the entry too; the argv rule itself is `ui/RendererRetry.qml`, loaded by `file:` URL
only once the error has arrived, because the boot directory cannot import `ui/js/Renderer.js`
through `qs:` and the startup path must not compile it. The body takes the window as `host` rather
than `fleaWindow`: a root property of that name binds to itself through `Ipc` and reads null, which
is how an earlier experiment's `rowRect` came back empty.
The IPC seam (`ui/Ipc.qml`, `ui/IpcMenuState.qml`, `ui/IpcPreviewState.qml`, 77,651 bytes the product never reads) is built after
the first rows: `ui/WindowBody.qml`'s asynchronous `ipcLoader` activates once when the primary pane leaves `loading` (a listing that
never answers ends in `waiting` after 10 s, which builds it too), so the first window compiles 166 of 169 units (165 of 168 before `ui/Glass.qml`) and the target `flea`
exists a few milliseconds after the rows. A client that drives or times a launch through IPC waits for `ready` (`tests/ui.sh assert_theme`)
and never assumes the target at map, so a native case holding the first listing reads no `loading` over IPC: `ui:cap_heldcolumns` takes that proof from a live `ipc ready` answer with no seam ("Target not found.") on each side of its reading shot, shooting every Columns limit from 2 to 5, and `tests/heldcolumns.sh` pins loading, waiting and the landing offscreen once at the shipped limit 3, since held rows read no column count; the Loader is the one object this adds to the startup gate, which is why `startupObjectLimit` was 619 then; it is 628 on today's tree, the total the startup-objects suite measures, and "The startup object gate" records the objects that joined since (background-only opacity, the spinner's still second, the list's cursor mark, the rail's lit place, the Quick Look warm loader, the hidden rail's dwell, the New tab mark's frame, the held button's mark and the flasher's result `Connections`) as history, not as a sum. The move takes the `ui/WindowBody.qml` ceiling from 651 to 657, re-derived with `wc -l`.

**The GPUI launcher does not trade the Qt platform theme.** Its child configuration removes
the Qt renderer controls, but does not replace `QT_QPA_PLATFORMTHEME` with `QS_ICON_THEME`.
`src/gui.rs` retains `restore_platform_theme` for programs opened from an older window that
passed `FLEA_QT_THEME`; only a nonempty marker restores the saved theme and removes the marker
and `QS_ICON_THEME` from that child.

**The window's own colour is never seen.** The body's Rectangle covers it on the first frame, so
the entry carries `ui/Theme.qml`'s literal fallback and nothing more. A glass launch makes it
transparent instead, before the window exists; see "Theme roles and sources". 0.3.2 passed the theme's
background in as `FLEA_FIRST_PAINT` for the frames the window spent empty, and that went with them.

**One cost is accepted by design.** The first launch after each update is slow once, while Qt
writes its 122 cache files with an `fdatasync` each: measured 2026-09-22 with only that cache
cleared, three launches, the window arrived 1.2 to 2.9 s after exec with its first screen in it.
Nothing else is in that number. Flea never writes the Qt pipeline cache, the `qqpc_vulkan` under
`~/.cache/quickshell` is the bar's, and clearing the Mesa shader cache costs about 50 ms once per
box, not per update. Every later launch is fast, across reboots. Arch's `qt6-declarative` ships no
`qmlcachegen`, so the package cannot ship the cache ahead of time.

**The retry helper drags `ui/qmldir` with it, which is another reason it is loaded late.**
`ui/RendererRetry.qml` implicitly imports its own directory, so compiling it compiles every
singleton `ui/qmldir` names, and those import `qs.Commons`, which resolves against the CONFIG ROOT.
Production's root is `ui/boot` and carries the two symlinks, so it resolves; a probe launched from
any other root answers `Type Favourites unavailable`, which is how this was found.

**The chooser's own failure path cost three attempts, and the working one is a late look.** A
missing `ui/PickerWindow.qml` is not a Quickshell config error: the entry loads, the window never
arrives, and the process sits there while `tools/flea-portal`'s caller waits out its 600 s, which is
driven. `Component.onCompleted` on `ShellRoot` is `Non-existent attached object`; on a `QtObject`
child it fires before `LazyLoader` has an item, so it killed a healthy chooser; `onLoadingChanged`
never gave a usable edge in either arm. What works, with both controls driven, is one late `Timer`
that looks once, well past the slowest honest load.

**The chooser keeps its window whole.** `ui/boot/picker.qml` is a thin entry whose `LazyLoader`
takes `ui/PickerWindow.qml` by `file:` URL, so the chooser gets the cache and the boot directory,
but not the body-after-the-first-frame split. Its title and size are read from `ui/js/Picker.js`
and the Theme, so splitting them off the first map would resize a portal dialog in front of the
operator, and the chooser is not on the measured path.

**The startup object gate.** `tests/startup-objects.qml` counts the objects one window has built
when its first listing settles and fails above `startupObjectLimit`, which is 587, the total the suite prints on W after Drag to Trash adds two DropAreas and the Trash row's two first-use Loaders (583 after the context menu waited for the first frame, 628 before). What follows is a history: each object is named as it joined the first window, with the count of its day, and later work that removed or merged objects never re-summed it, so its steps do not add up to the current total and the measured total is the only number that holds. 619 once the IPC seam moved into its Loader (`ipcLoader`), down from 714 in 0.3.8, and two more for background-only opacity, `ui/Pane.qml`'s glass ground and the second half of `ui/TabBar.qml`'s strip ground, two plain Rectangles drawn only on glass. Neither can wait for first use for less, because the Loader that would hold either is itself one object, and off glass neither is visible, so neither adds a scene-graph node. One more is `ui/Spinner.qml`'s still `Timer`, which holds a spinner still for its first second on screen: the pane starts life in `loading`, so `ui/LoadingState.qml`'s spinner and its Timer are built with the first pane. It cannot wait for first use for less either, because a Loader holding it is itself one object, and it runs only while its spinner is visible. One more is `ui/List.qml`'s `CursorMark`, the one QtObject that files each delegate by listing row so a cursor move relights only the row it leaves and the row it reaches; it is built with the first list, and it replaces the cursor binding every live delegate re-ran on each key. The release-line join of the rail's lit place and the Quick Look warm loader adds these to the count: `ui/Sidebar.qml`'s three `Connections` relight the place that holds the current folder when its listing lands, `ui/SidebarRow.qml` draws the keyboard cursor as its own frame, and `ui/WindowBody.qml`'s `warmLoader` stays inactive until the cursor first lands on a file. `ui/PaneRail.qml`'s `dwell` Timer, built with the rail, shows a hidden rail's edge only after the pointer rests there; the Dual divider's Loader in `ui/WindowBody.qml` takes the place of the Rectangle it replaced, so it adds none. The Q9 and Q10 contrast join adds none: the menu cursor is one fill (RULINGS 141), so `ui/MenuRow.qml` builds no frame Rectangle beside its `menuWash`. `ui/TabBar.qml`'s `addFrame` Rectangle, the keyboard frame of the New tab mark, is built with the strip because the mark is there from the first window (a single tab shows no tabs, so a tab's own `tabFrame` is built only with the second tab), and a Loader holding it would be one object too, so deferring it removes none. The press-keep fix (0130e7b35) adds `ui/PaneStates.qml`'s passive `PointHandler`, the mark of a held button, which is built with the first pane, and a Loader holding it would be one object too, so deferring it removes none. The stage5 dry join adds none: no Loader is built per list row, and a Loader in front of `ui/RowDrag.qml`'s DropArea would add one per list row and three here (631 on that day's tree, before the context menu waited for the first frame), so the limit does not rise for it. The
cross-window tab drag had raised it from 702 to 714, twelve more by the suite's own count, all built in every
window. `ui/WindowBody.qml` holds the window's DropArea and the `tabAck` Loader; `ui/TabBar.qml`
holds the strip's DropArea, the `ackTimer` and `tabDropDeadline` Timers, the `takenAck` Process,
the `dragOrigin` Item and one `TabDragGeometry` with its `query` Process; `ui/boot/fleatab.qml`
holds the catcher's IpcHandler and its two Connections. Seven of the twelve cannot wait: the two
DropAreas, the `tabAck` Loader and the catcher it holds must be live before any lift, because a
drop from another window arrives unannounced, and `dragOrigin` is the strip's declared
`Drag.source`. The other five (the two Timers, `takenAck`, and the geometry with its `query`)
could be built on the first lift. Deleting those five was measured on 2026-10-03 (offscreen, THP
off, `Pss_Anon`, medians of ten against a control of the same batch) and returned 62 kB of 44 MB
against a run-to-run sigma of 240 kB, so they stay plain objects rather than gain a loader that
buys nothing measurable. The window as a whole costs about 0.6 MB more there than at 759fd8eb
(43,536 to 44,298 kB, +631 paired, and +486 and +696 in two more batches), 0.37 MB of it at the
tab drag's merge, and no deletion variant located that in the instances, the JS libraries or a
per-row handler. A change that raises the count names each new object here, and for each one either says
why it cannot wait for first use or gives the measurement that shows waiting returns nothing. The flasher's zip probe adds one more: `ui/Pane.qml`'s backend `Connections` carrying `onFlashImageResult`, which cannot wait for first use because the cursor and listing signals it answers fire from the first listing, so a lazily built probe would miss the first zip under the cursor. The menu rows are drawn as v0.3.9 again (RULINGS 147): the cursor and the picked row are the one `menuWash` fill in `ui/MenuRow.qml`, with no frame Rectangle, so the rows add none. Before menu deferral, the suite printed `total=628` against `startupObjectLimit` 628.

**The first-frame unit gate (UNION5, 2026-10-10).** Every QML and JS unit loaded before the first rows costs every launch (the entry's `qs:` units compile each time, and the `file:` units from `ui/WindowBody.qml` on compile on a cold cache and load from it after), and the pair of the union1 head `3601db54` against int `c7e23485` changed `rows.10k.warm.content` (+8.5 ms) and `mem.listleg.cpu` (+0.02 s). The census (`startup-objects` in the flea-ci image, three runs per arm, `qt.qml.diskcache` lines) found int at 178 units and 1,564,070 bytes, the union at 179 and 1,602,504: the one new unit was `ui/DropKeys.qml`, the #259 key store, compiled because `RowDrag`, `DropInto`, `FileDrag` and the pane's key handlers named the singleton, though only a drag uses it. So the record is now the `keys` object in `ui/js/DragOut.js`, a library every one of those files already imported (`ui/Pane.qml` gains the import), and the singleton and its `qmldir` line are gone; the collision host is built by the pane's first `asks()` (`Qt.createComponent`, as `FlashHost` is), owned by the pane so a closed tab takes its card, and refusing loudly with the load error if the component cannot load, so `pane.collide` is null until a transfer asks, its readers check it, and a merge's terminal line goes through the host while every other line stays `Ops.transferDone`; the archive cards' titles, sentences and leaves moved to `ui/js/ArchiveCard.js`, which only the cards import. `tests/startup-objects.sh` now pins Flea's own units the probe loads by `file:` URL before its rows settle (its cache is empty, so each logs a miss; the `qs:` ones are the probe's scaffold and Omarchy's `Commons` and `Ui`), by name and summed bytes, against `tests/startup-units.txt`: a unit added or dropped fails with the list, and bytes over the `bytes` line fail; the line is raised only with a reason here. Red on `3601db54`, green on this head at 177 units and 1,606,443 bytes (union2's tags, cursor and bulk rename included). `startup-objects` counts 620 against its 623 limit, one Loader fewer. Ceilings, each from `wc -l`: `ui/Pane.qml` 1111 to 1116 for the `DragOut` import and the five-line `asks()` that checks the component, `tests/js/focus.js` 745 to 746 for the fake pane's `asks`. The press-keep fix raises the `bytes` line from 1652484 to 1653139: the `pointerHeld` property in `ui/Pane.qml`, the `PointHandler` and its comments in `ui/PaneStates.qml`, the `liftEnded` clear in `ui/FileDrag.qml` and the read in `ui/js/Anchor.js`, all first-frame units.

**Compare stage settle (2026-10-13).** The marked indices, held rows, own kinds, snapshot kinds and Compare activation live in `ui/js/Ops.js`, which already owns target indices and paths and is imported by both `ui/Pane.qml` and `ui/js/Focus.js`; those callers no longer import `ui/js/Compare.js`. The overlay's sentences, titles and diff parsing stay in `ui/js/Compare.js`, imported by the lazy `ui/CompareView.qml` and its tests. Before the capture readers were added, the Compare branch kept 175 first-frame units in `tests/startup-units.txt` with no `js/Compare.js` entry, and its bytes line was 1,683,012, re-derived with the `wc -c` sum in `tests/startup-objects.sh`, down from 176 units and 1,686,115 bytes before the split; before menu deferral, the measured object total and `startupObjectLimit` were 628. The moved helpers take the `ui/js/Ops.js` ceiling from 539 to 577 lines, re-derived with `wc -l`. Before menu deferral, both ContextMenu allowance profiles retained the earlier rows shifted by the same two-line delta, with the quickshell-present profile measured by the gate. The Compare capture case reads the overlay through `compareState`, while the design CI code-wrap and preview-column paging cases read the code viewports through `previewTextState`; after the Compare capture merge, these read-only readers leave `ui/Ipc.qml` with a current count and ceiling of 985 lines, re-derived with `wc -l` (983 before the merge).

On the stage, the committed bytes line in `tests/startup-units.txt` rose from 1,678,085 to 1,678,950 for the merged eject rules, then to 1,683,012 when the merged Compare helpers moved into the already-loaded `ui/js/Ops.js`; the preview paging keys then added 537 bytes to `ui/js/Keymap.js` and 167 to `ui/js/Focus.js` (1,683,716), the execute marker in `ui/Row.qml` another 24 (1,683,740), and the Markdown Source merge 152 more in `ui/js/Focus.js` for its Source wrap guard (1,683,892). On the Compare branch the capture readers raised the same pin from 1,683,012 to 1,685,828 (+2,816), with 198 added bytes in `ui/Ipc.qml` and 2,618 in `ui/IpcPreviewState.qml`; the Compare capture merge adds those 2,816 bytes to the stage's 1,683,892, so the bytes line after that merge is 1,686,708 across the same 175 units, each step re-derived with the `wc -c` sum in `tests/startup-objects.sh`. The Quick Look code reader fix, live Compare kinds and the menu diagnostics add 531 bytes (`ui/IpcPreviewState.qml` -5, `ui/IpcMenuState.qml` +463 and `ui/ContextMenu.qml` +73), taking the pre-deferral line to 1,687,239 over the same 175 units, re-derived with the same sum. The two ceilings in `tools/flea-file-budget` move by `wc -l`: `ui/ContextMenu.qml` 783 to 784 before menu deferral and `tests/quicklook-code.qml` 424 to 437.

The HTML preview keeps the same 175 first-frame units and the object limit of 628. Its lazy bodies and QtWebEngine helper are absent from that unit list. The first-frame byte sum in `tests/startup-units.txt` is re-derived from 1,686,708 to 1,688,583: `ui/Backend.qml` adds 346 bytes for the signal and request, including Quick Look viewport height, `ui/Ipc.qml` 98 and `ui/IpcPreviewState.qml` 1,049 for the observer, `ui/js/Focus.js` 25 for the HTML key guards, `ui/js/Icons.js` 80 for the HTML mark, `ui/js/Kinds.js` 70 for HTML recognition, `ui/js/Messages.js` 72 for reply dispatch, and `ui/js/PreviewKeys.js` 135 for rendered-page scroll routing. `ui/ViewState.qml` adds no document-view reader. The sum is the same `wc -c` calculation the startup gate uses. Markdown and HTML Quick Look bars draw the language mark, filename, line count and close, with no view control. Both open rendered; r flips only the open Quick Look, follows file moves while it remains open and is forgotten on close. The preview column always requests rendered documents. Neither document view is a stored preference.

The HTML helper loads `file:///page/<the original filename>`: the sandbox binds the confined input over that file inside the read-only page folder, so relative URLs and the document's own base keep their resolution. The policy limits base URLs and local resources to that folder and injects no base element. A leading UTF-8 BOM, whitespace and complete comments stay before the doctype; its first `>` ends it even inside a quoted identifier, as WHATWG 13.2.5.58/59/64/65 specifies. A cached PNG requires the HTML key and every engine-recorded local dependency to match. The bounded sidecar holds at most 256 resource entries and 64 KiB, with page-relative paths, sizes, nanosecond mtimes, inodes and devices; missing, unreadable, malformed or changed records force another render. Hit validation and refresh share one process-wide lock with eviction's final recheck and unlink. Quick Look requests a viewport height scaled to the render width, and that height is part of the cache key, so a short page covers the frame to its bottom edge. Column requests retain the helper's 600 px default. The height field adds two lines and 86 bytes to `ui/Backend.qml`, re-derived with `wc -l` and `wc -c`; the first-frame unit list and object ceiling are unchanged.

**Unlock, after its fidelity critic (2026-10-10).** The board keeps one name for an encrypted drive: the unlocked row, the folder's footer and the card all say "Backup". An unlocked crypt now takes its container's label when the owner named the container (`Devices.containerName`), so the row reads as its locked row did and as the footer says; an unnamed container's volume keeps its own filesystem label, as before. `flea --unlock` opens before the rail's first `lsblk`, so the card shows the device's leaf until a locked row names it: the rail's row first, else the host's one-shot `DeviceMounts` poller while the name is pending (`Unlock.namingRow`), stopped after its first listing and never started once the card is touched, taken only while the card is still untouched at its first typing state. The board draws the masked dots through Unlocking and a wrong passphrase, but the field still empties on submit: `tests/unlock-stdin.qml` sweeps every property for the passphrase after a submit, a refusal and a failed mount, and a field that kept its text failed six of its checks, so the field empties on submit and keeps only the submitted length as `field.heldMask`, and those two states draw that many of the input's own mask bullets until the retry's first edit clears them. Tab walks field, Cancel, Unlock; submitting moves the keyboard to Cancel, the one stop while unlocking, where Unlock drops out with the field and the ring is Cancel's own focus. A refusal returns the keyboard to the field, and closing returns it to the window's pane. `ui:cap_unlock` asserts the Tab walk, the empty field after a submit and the dropped keyboard natively, the last on the right passphrase submitted from the Unlock button, since a refusal can end inside one poll; `tests/js/unlock.js` reads the Unlocking stop list from the source. The unit gate now reads only the log lines before the probe's `STARTUP_OBJECTS TOTAL`, because the probe opens Quick Look and starts the power-off chain after it, so eight units it pinned were never the first frame's (`Preview.qml`, `PreviewArchive.qml`, `PreviewText.qml`, `MediaStrip.qml`, `PowerSectorsReader.qml`, `js/Facts.js`, `js/PreviewSettle.js`, `js/PreviewSwap.js`); the pin is 169 units and 1,543,154 bytes, `Devices.containerName` and the Ipc reader's fields included. The probe builds the IPC seam (`Ipc.qml`, `IpcMenuState.qml`, `IpcPreviewState.qml`) before that line, so it stays in the list though the product builds it only for its first IPC call. Ceilings, each from `wc -l`: `ui/js/Devices.js` 395 to 401, `ui/Ipc.qml` 927 to 931, and `ui/UnlockDialog.qml` 420 to 439, re-derived with `wc -l`, for the submit focus move to Cancel and the retry edit clearing the error frame and note plus the held mask count kept on submit and cleared on open, close and success plus the five-line capture-harness alias block and the three-line capture state in the frame JSON. The one-shot name poller keeps its first listing after its Loader ends and polls again when the timeout ends a listing unread, which raises the first-frame `bytes` line to the measured 1,543,443.

**Union2 clean-up (2026-10-10).** The parked Back's wait is `ui/ParkedWait.qml`, built by `Qt.createComponent` on the first park as `HoverTimers.qml` is, in place of a Timer parsed from a QML string, with its own `qmldir` line, and a load error throws with the component's error as the collision host's does. The backend answers `tagstates` on `tagStatesResult` and `settagged` on `settagged`, one signal per message as every other route in `ui/js/Messages.js`, and both sample lines sit together in `ui/Backend.qml`'s block of sample inputs, which takes it from 506 to 508 lines. `tests/js/tags.js` routes both messages through `Messages.route`, so a swapped branch fails there. The first-frame `bytes` line is the measured 1,543,216, `ui/PaneSwap.qml`'s load check included.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
