## The preview swap

CSV and TSV in the Columns preview expose the table body through the same viewport contract as code: wheel and F/B page the body, the sticky header stays fixed, and returning restores the file position before the preview hold releases. Restore stays pending until the visible records are held and reapplies against the final ListView origin, so fractional row layout cannot move the returning file. The body model counts every data record while requests cover only the visible records; Quick Look keeps its existing two-axis grid. CSV and TSV suffixes route both surfaces to text even when the MIME icon is a spreadsheet; binary spreadsheets retain their unsupported state. Both table and code views mark destruction before queued window requests can run, preserving their models until their children are destroyed. The text IPC reader reports the live table viewport and null for its absent wrap field. The merged startup pin is 169 units, with 1,628,263 bytes from the `wc -c` sum over `tests/startup-units.txt`; `CsvTable.qml` stays outside the first frame. `tests/quicklook-code.qml` has a 452-line ceiling and `tests/js/facts.js` a 361-line ceiling by `wc -l` for the CSV and TSV IPC and routing cases. The `CodeLines.qml` and `PreviewScroll.qml` allowances in both unqualified-read profiles use the merged source lines; the available profile is measured by `tests/staticgates.py`.

A cursor move drew the next preview half-built: v0.3.4 shows 145 to 151 mid frames over 18 column
moves and 13 to 15 over 16 Quick Look moves, measured headless through `tests/preview-swap.sh`.
`ui/PreviewSwap.qml` owns the hold and `ui/js/PreviewSwap.js` decides. The host's children go into a
content Item; a Loader creates a ShaderEffectSource on it only while a hold runs, `live: false`,
`hideSource`, `visible`. The queued changes run under the picture once the frame that first syncs the
layer has synced (`afterSynchronizing`, armed in that frame's `afterAnimating`): the picture is taken in
that frame's render from the tree as synced, and the layer's first sync queues its own repaint, so the
changes land in that repaint's frame rather than one later. That holds only where the signal arrives
queued (the threaded loop, Flea's own): the basic loop emits it inside the frame before the grab, and a
change there left the grab drawing a Shape whose window was gone (a crash in `columnsfolder`), so the
handler runs only once the event loop has turned since the arming frame (`syncQueued`), and otherwise
`scheduledUpdateCompleted`, after the grab, runs them as before. The layer exists only while a hold does, so a settled preview
draws directly and costs no texture. A `ground` Rectangle in the backdrop colour sits under the panes
while holding, inset by the border width with the matching corner radius on Quick Look, because text
in the layer blends against transparent and draws heavier without it. A capture guard gives up after
100 ms (`CAPTURE_MS`) if the window is not drawing, and the change goes ahead unheld.

Three traps are load bearing. Set `holding = true` before `capturing = false`: the other order
re-creates the layer and captures the half-built state. On release, set the layer's `sourceItem` to
null before the Loader lets it go: that unhides the source and drops its effect reference in the release
frame, where the layer's own destructor would do both only at the deferred delete, one frame later (a
layer that hides its source with no layer drawn is a blank frame). Clicks, hover and the wheel are
blocked while held by a MouseArea over the picture. `tests/preview-holdframes.sh` counts the frames
one column move draws: the change in the first frame after the key frame, nothing after the release.

The column holds with `burstEnds: true` on the key `path + "\n" + cursorIndex`. The hold starts on the
move, an idle move loads at once through `PreviewSettle.plan` and only a move inside the 120 ms settle trails on the timer, and `clear`
runs after the capture so `pending` makes `Facts.state(null, loading)` return LOADING. `armSettle` remains only the folder-to-file arm in `ColumnsArea.moveThird`, overridden by `settleFor` when idle; a held key's first repeat after the repeat delay counts as idle too. On an idle press `SelectionPreview.askCursorThumb` sends the cursor row's thumbnail ask before the capture, so the backend round trip overlaps it, and `load()` then finds the row asked through the same `PreviewAsk.workFor` gate and sends nothing twice; a walk step and a manual-hold class ask nothing early. A folder shows
through the deferred `shownIsDir` and `shownChildPath` and never holds a picture: a move onto a folder
whose peek is outstanding keeps the old column by data, and the landed peek shows it with its rows in
the same pass; a folder whose peek already answered lands at once with no hold. Only a move onto a folder
whose peek is outstanding restarts a single-shot fallback at `Swap.HOLD_MS`, which shows the pending
(empty-held) state when the peek is late; any other move stops it, and a move onto an unanswered folder
during a live hold hands that picture to the folder's wait, with no new hold and no new capture: the
landing or the fallback releases it with the folder in one pass, and a stale queued show never draws
an unanswered folder, only the fallback's pending show passing force.
A different key during a column hold is held j: it gives the picture up, the
loading state stays live with `heldOff` until `loadSelection`, which holds again with `atWork`. The
picture cap is `Swap.CAP_MS` counted from the start of the preview's own work in `load()`, a PDF's
included: its `DOCUMENT_SETTLE_MS` (120) runs under the cap, so an idle press opens on the next frame
and the last step of a walk falls back while it settles. Past the cap `fallBack()` sets `fellBack`,
which draws the loading state in the content, and calls `scheduleUpdate()` on the layer, so the held
picture becomes that loading state in the next frame; it stands `Swap.MIN_SHOW_MS` from then
(`shownAt`, carried across a burst and a new hold until a whole preview lands), and `check()` asks
`PreviewSwap.releaseIn` before it lets a whole preview go. At the end of the minimum the `minShow` timer
lands a whole preview, or lets a preview still building go live under the same loading state so its mark
can crawl. `describe()` reports how long the last loading state stood as `shownMs`. The folder fallback
is a separate single-shot at `Swap.HOLD_MS`, 150 ms, restarted only by a move onto an unanswered folder:
like `ui/LoadingState.qml` outside the pane, it draws no loading state that stands `Swap.MIN_SHOW_MS`, so it keeps the longer hold-off until one does.
A PDF page render does: `renderCap` in `ui/PreviewPdf.qml` falls back at `Swap.CAP_MS`, and the loading state stands `Swap.MIN_SHOW_MS` from `fellBackAt`, through `Swap.showLeft`, before the page that landed, or the failed render's state, shows. Quick Look holds in `follow()` with no apply, `load()`
mutates under the picture and then calls `start(isPdf)`, held j joins the hold with the old preview
staying, and `close()` calls `cancel()` so nothing queued runs. Space's own open takes no picture: its card and
dim wait for the content's first frame (`lookReady`, which sets `landed`), and `revealed` is `landed || revealedEarly`.
The hold ends early over the loading state, shown at once (`heldOff`) for every kind, when the `revealCap` single-shot
fires at `Swap.CAP_MS` (66 ms) counted from the key for a kind already built, armed in `load()` before `show()` builds the Loader items (the cap
once started after `show()` returned, so the key turn added to the 66 ms and the mark drew a frame late), and from after `show()` for a kind's first build, so a cold PDF compile is not inside it (`builtKinds` tells them apart, `close()` clears `revealCapStart`, and so does an open that arms nothing while no cap runs), or when a press or the wheel lands anywhere on the overlay: the top `holdShield` takes it, through
its own release, so no click reaches a card that never drew. That early loading state stands `Swap.MIN_SHOW_MS` from
`loadingSince`: `revealIfReady` asks `Swap.showLeft`, and content ready sooner lands when the `minShow` timer ends, while
content ready before the reveal lands at once. A move during the hold takes no picture and keeps the open's cap;
an idle one loads at once and a burst still trails on `followSettle`, which `revealIfReady` waits out, so whatever
reveals is the cursor's own file and a held key asks nothing per row. The card and dim read opacity 0 for the whole hold
and share one `fade`, so a reopen inside the close fade cuts it. `lookEmptyFrames` in `previewSwapState`'s `lookOpen`
counts every frame the card or the dim draws while the open has neither landed content nor the early loading state, read
from `active`, `landed` and `loadingEarly` rather than `unrevealed`, so it does not restate the opacity binding
(`tests/ui.sh lookopen` requires none, a Space, Escape, Space reopen included, no cap reveal for its small PDF, image
and text, and exactly one for an 8000x8000 PNG with thumbnails off; `tests/quicklook-hold.sh` drives the press, the
wheel, a move that must keep the cap's running Timer, the reopen, a stalled `show()` after which the cap must be
running, and a press on text and a PDF, headless; a Markdown open's first screen lands in Space's own turn, so it has no
hold to press in).

**Quick Look on a Markdown file shows its content in the first frame (038-qlopen, 2026-10-04).** Space's own open
reveals its card once the content can draw, so the document's first blocks must land in one event-loop turn or the
reveal waits on them.
Three parts, each measured headless (offscreen, software, warm QML disk cache, medians of n=10 processes, key to the
first frame that holds block 0; the clock is `Date.now()`, 1 ms). (a) `Preview.show()` names the file the open is for in `inlineMarkdownPath`, only when `Prepared.readsInline(row, class, known)`
holds: the folder's class has landed (`storageKnown`, reset to false by `ui/js/Nav.js` on every new path, so a hop from a local folder to a share never spends the old class), a regular file (`st_mode` in the row's `p`, so a FIFO, socket or device named `*.md` never), listed size 1 to
64 KiB, on a class with no `ExtThumbs` entry (cifs, nfs and listed FUSE shares classify `network` in `extclass.rs`).
`ui/PreviewMarkdown.qml` takes it as `blockPath`, and `pointFile()` points the `FileView` in one step: blocking only for
a path equal to `blockPath`, and never reading a path other than `blockPath` while one is set (a pane built at kind-set
still holds the last file's path). A size not yet known never blocks: the decision comes from the listing row, not from
the pane's `size`, which binds after `path`. A blocking read starts from path `""`, because a view holding another file
takes a new path asynchronously. Both orders of a file change are pinned (the `order` leg). (a2) The unseen Source
`Text` lays out nothing (`text` is the raw text only when `shownView` is Source, never `view`: a too-deep document shows its source under Rendered, and `markdown-html` 15 goes red on `view`): it held the UI thread 100 ms over a 1 MiB document.
`MarkdownRemote` draws its dashed border as one `Shape` (18 objects against 322), which brought doc 08 under the poll.
(b) `QuickLookWarm.markdownUnit` holds `Qt.createComponent("MarkdownPane.qml", Component.Asynchronous)` one second
after the first listing is ready, never an instance: the first Space of a session loads no compilation unit (first open
12 ms against 14 ms, pane created at 4 ms against 6 ms) and the window PSS did not move (medians 28438 against 28465 KB,
n=6). (c) The column's parse is not reusable: it is parsed against the column's chrome (`Theme.color.surface`) and Quick
Look's against `Theme.color.background`, and the chrome is baked into every inline code span, so a shared parse would draw
the wrong code ground. Instead `ui/QuickLookPrepare.qml`, for a cursor that rested `restMs` (120, restart on every move), reads the file with
`head -c 65537` (the bytes actually read are capped, whatever the file grew to; a longer answer is dropped), parses it
in the Markdown `WorkerScript` (a retired `Loader`, so no UI-thread parse), and stores the one entry of
`ui/js/MarkdownPrepared.js` when the worker answers for the request still waiting and a cursor that has not moved.
Quick Look's own parse is stored there too, so a reopen also hits. A hit needs the same path, folder, chrome, ink and
text, which the Space read supplies, so an edited file never shows a stale parse. Known limits: a Space on a row whose
size is stale reads the whole file blocking (`FileView` cannot cap), a symlink to a Markdown file is read ahead never,
a FUSE type outside the `extclass.rs` list classifies local, and a Space before the folder's class lands takes the
asynchronous path, so that card can draw before its first block.
`tests/quicklook-firstframe.sh` pins it, and the worker's prepared blocks equal a fresh parse of the same text on the merged parser, canonically (the fixture carries a badge row, a lone picture, an empty heading and an empty quote; red with the worker dropping its last block): the key's function returns with the card active and the blocks in the model
(before the fix: card=true and 0 blocks), frame 1 holds block 0, no frame draws the card without it, the parse was taken
from the entry, and a held key (twenty moves at 30 ms across event-loop turns, a 300 ms rest) reads no file, red with `rest.start()` for `rest.restart()`. Legs: reduced, motion, order, class-network, class-phone, class-usb, unknown (the class reply held back: no read at rest or inside the key), pipe (a FIFO listed at 900 bytes, refused only by its file type; a `head` child left on it fails the suite), capped.

**Quick Look's first open compiles nothing and draws its small pictures (038-mdq1, 2026-10-05).** The native h2h had 01 to 03
(lists, tables, strike) settling 40 ms after 04 to 06 and 07 and 17 (a local picture) 85 ms late. A headless trace (offscreen, software, a fresh process
per run, n=10, `Date.now()` marks; the working hypothesis was that the first list or table compiles its component in the first frame) says
otherwise: every `Markdown*.qml` is compiled before the key (the held `QuickLookWarm.markdownUnit` pulls the whole stack in as dependencies), and the only source
compiles between the key and the first frame are `QuickLookSwap.qml` and `PreviewSwap.qml`, which the first open's lazy `swapLoader` built. `QuickLookWarm.swapUnit`
holds that wrapper's compiled unit beside `markdownUnit`, one second after the window settles and never instantiated (window PSS +74 KB at the median of 16
interleaved runs against 16 without, noise 3 MB, so the claim is a bound, not a digit). Medians of n=20 interleaved runs, key to settled frame in ms: 02 29 to 24, 03 23 to 18.5, 04 19.5 to 14.5,
and 01 31 to 30.5, which is inside its noise (range 18 to 108). A list or table costs 1 to 4 ms more than a paragraph on first build and again on the second: block count, not compilation.
The picture is the first image decode of the process: `QImageReader` asks every image plugin before the built-in PNG one, and the reader thread starts, so a
64 px logo went from created to ready in 47 ms and a second decode of any file in 3 ms. `QuickLookPrepare` now also sizes the prepared document's top-level local
pictures with one `stat` child (`Prepared.pictureUrls`, at most 8, `Prepared.smallPictures`, at most 256 KiB each, so a larger or missing one keeps the open's own load) and holds an `Image` per
picture, built as the block builds its own (`asynchronous`, `autoTransform`, `PreserveAspectFit`: the pixmap cache key holds the transform and the fit, and a held
Stretch image is never found), until the cursor moves. The block's `Image` then finds the pixel data in the cache and is Ready when it is built, inside the key.
A picture the document gives both a width and a height draws Stretch and misses the cache, so it keeps today's load, with the plugins already warm. Settled frame, n=20 interleaved: 07 68.5 to 17.5 ms
(picture ready 63 to 9), 17 69.5 to 18 (62.5 to 7). Known limit: a picture that decodes far past its 256 KiB file (Qt refuses past 256 MB) is now decoded when the cursor rests on its document, not at Space.
The `rawText` binding loop (`Binding loop detected for property "rawText"`, twice in the native `cap_markdown` log) came from `readonly property string rawText:
file.text()` under a blocking first read: `text()` loads inside the binding, its own `textChanged` and `onLoaded` re-read the binding, and the parse reads
`rawText` through `MarkdownPane`. `loadedText` is now set by `onLoaded` and taken once by `pointFile` for the blocking path only; a path read asynchronously keeps the last text until its load lands, as the binding did, since clearing it parsed the old file under the new path (preview-hunt `parse-quick`, `parse-column`, `parse-worker` went red). A failed load and a path of none (inactive, too large) do clear it, as `file.text()` answered "" there (`parse-clear`).
The picture prefetch takes only pictures under the document's own folder (`Prepared.insideDir`, no `..` segment, a stray percent skipped), so its `stat` never reaches another mount, runs as one sizer at a time (`QuickLookPrepare.sizer`, released on a move and on a new rest, dropped after `PICTURE_SIZE_DEADLINE_MS` so a hung mount holds nothing past it); preview-hunt `pictures` pins it. A picture reached by a symlink inside the folder is still stat'd by its target, a known limit. `tests/quicklook-firstframe.sh` pins all three on every leg: no `from disk cache` line (a source compile, from `qt.qml.diskcache.debug`) between `QLFF KEY 1` and `QLFF STEP 1`
(red before: `QuickLookSwap.qml PreviewSwap.qml`), no `Binding loop detected` (red before: 10, 10 and 14 on reduced, motion and order), and the card's pictures Ready when the key returns, in the
`a-notes` fixture (a badge row and a lone picture) and in the 07-shaped `h-html` leg (red before: `built 3 of 3 picture(s) still loading when the key returned`). The `shape-f-table`, `shape-g-tasks` and
`shape-h-html` legs are the 02, 03 and 07 shapes each as a first open. The harness waits for the idle-held units before its first key, as a user's first Space comes after the one second. Pure parts: `tests/js/mdprepared.js`.

**The first Space keeps every rest-time gain though Quick Look is built late (038-qlwarm, 2026-10-05).** `ui/Preview.qml` is built by a window's first Space (`WindowBody.previewLoader`, `Pane.quickLook()`), so the rest prepare, the picture hold and the idle unit warm no longer live in it: `ui/QuickLookWarm.qml` holds `QuickLookPrepare` and the `previewUnit`, `markdownUnit` and `swapUnit` (the compiled `Preview.qml`, `MarkdownPane.qml` and `QuickLookSwap.qml`, one second after the window settles, never instantiated: with Preview lazy its own unit compiled inside the first key). `WindowBody` builds it with `Qt.createComponent(...).createObject(view)` on the cursor's first landing on a Markdown file (`Prepared.isMarkdownRow`) and keeps it, so a window that never rests on one creates no object for it (`startup-objects` stays 618) and the first Space takes the same prepared entry, held pictures and compiled units as every later one. A rest on a non-Markdown file builds no card, and `pane.preview` stays null there until Space (038-qlfirst below changed the Markdown rest, 0310-qlfirst every kind). Measured headless (offscreen, software, a fresh process per leg, `Date.now()`), key to the first content frame, ms: first open 10, a later open 7 (the other shapes 8 to 7, 7 to 6). `tests/quicklook-firstframe.sh` pins it: the window's first Space is a true first open (no Quick Look before the cursor rests, `reusedParses` 1, pictures ready as the key returns, no compile inside the key, red before: "no prepare or unit warm is built before the first Space", and with the unit hold dropped "compiled inside the first open: Preview.qml PreviewSettle.js PreviewText.qml MediaStrip.qml PreviewArchive.qml"), and a `noopen` leg rests on a Markdown file with Quick Look never opened (the one card built and closed, prepared entry and compiled units made; red on f54f077f with 0 cards), then moves off it and requires the prepared entry gone at once. Known limit: a first Space on a file that is not Markdown compiles the swap and Preview units in the key, as memfix left it. The remaining first-open overhead (key 8 ms against 5 later) is not a build and its cause was not isolated, and a card laid out ahead needs `active` (an invisible item has no node, an opacity of 0 draws none), so qlwarm did not build it ahead (038-qlfirst below does, at the Markdown rest).

**The first Space only opens a card the rest already built (038-qlfirst, 2026-10-05).** The native h2h had the first change after Space at 19 to 23 ms on 7df6dab7 and 59 to 63 ms on fc937947, one poll later, because qlwarm moved the card's build into the first Space (`previewLoader.active` flipped inside the key: 73 objects, `startup-objects` `STARTUP_OBJECTS QUICKLOOK CARD 73`, about 2 to 3 ms headless on the key). `QuickLookWarm.cardDue` (every compiled unit Ready, `QuickLookPrepare.rested`; 0310-qlfirst below widened it from a Markdown row to any file whose kind has a unit) turns the loader on, so the card exists hidden and inactive when the first Space comes and a window that never rests on such a file still builds none (`startup-objects` stays 618, the card is built by `pane.quickLook()` after the census). Headless first-Space key and key-to-frame medians over the Markdown legs, ms: 7df6dab7 6 and 7, 08077a23 5 and 8, f54f077f 10 and 11, after 8 and 9. `tests/quicklook-firstframe.sh` pins it: KEY 1 finds a card built, closed and invisible, and no Space builds a card (`cards=` from the loader's `loaded`; red on f54f077f: "a rest on a-notes.md left no Quick Look card built before the first Space" and "step 1 built 1 Quick Look card(s) inside the key, want 0" on every leg).

**A rest on any kind builds the card closed (0310-qlfirst, 2026-10-12).** The instant census had the first Quick Look of a kind slower than a repeat (`open.ql.pdf.first` 108.8 ms against 65.6). A headless census in `tests/quicklook-firstkind.sh` counts what a first Space builds and maps against a reopen of the same file: before, the first Space built 95 more objects than the reopen on every kind (text 95 against 0, image 112 against 17, PDF 203 against 108, clip 115 against 20), the card (73) and the swap wrapper that `show()` builds lazily, because `cardDue` built the card for a Markdown rest only; a text, picture and PDF Space mapped no new file in the key, and the clip's backend load is the media warm's (gm/0310-cql). `QuickLookWarm.cardDue` now takes any rested file whose kind has a unit (`restUnit` Ready beside `previewUnit` and `swapUnit`) and also calls `ensureSwap()`, so the first Space of every kind builds the same objects as the second (text 0, image 17, PDF 108, clip 20; `startup-objects` and `lazy-objects` unchanged: a launch and a rest on a folder, an archive or an unsupported file build none). What a reopen still builds is the per-open item (the PDF viewer, 108 objects) and is not a first-open cost. `tests/quicklook-firstkind.sh` pins it on each leg: a closed card with its swap built after the rest, no card at a rest on a file with no unit before the leg's first unit file (the loader keeps a built card, so a no-unit file after a unit one shows the kept card and is not checked), a first Space that builds no more objects than the reopen (red with `cardDue` limited to Markdown on all four kinds), and no new mapped file in a text, picture or PDF first Space. A clip that plays never settles in the census (`open.ql.video` repeats settle at 5021.1 ms against 5080.5 first): `PreviewMedia` autoplays by design, so the watched region keeps changing until the harness cap; it is the clip playing and not a hold in Flea.

**A rested file's maths is drawn in Quick Look's first frame (038-qlmath, 2026-10-05).** Traced offscreen, the first frame of a document with an
inline formula came about 25 ms after a plain one because the process's first picture decode (the 1x1 `Maths.CLEAR` PNG of
`ui/MarkdownMathsText.qml`'s calibration Text) ran inside the key, and the formula then waited `askSettleMs` (50) before asking. On the same rest
`QuickLookPrepare` holds one decoded `Maths.CLEAR` (`warmDecoder`, for any document with a formula, since a typeset figure decodes nothing at rest, released by `moved()`) and builds one
unseen `MarkdownFigure` per formula (`typeset`, at most `Prepared.FORMULA_LIMIT` 8, from `Prepared.formulasIn`), so the figure service holds each
drawing under the key the open asks with; `MarkdownFigure.takeRemembered()` applies a held drawing on the change itself and drops any older request in
flight (`ticket` 0, `askTimer` stopped). `recount` settles nothing while the stat child is out (`sizerDeadline` still releases a child that never
answers). `tests/quicklook-firstframe` holds the `i-maths` leg (figures drawn at the key, decoder warm only for a maths document, `decoderWarms`
counted across the held-key sweep) and the paired sizer probe; `tests/figure-component.qml` holds the held-drawing and late-reply checks. On the same rest a document holding a table (a quote's or a list item's included) sends the warm table's own Markdown to the same worker, then builds one unseen `MarkdownTable` from its answer once per prepare and destroys it at once (`tableWarmed`, counted in `tableWarms` only once the table built and measured, never under a held key or for a parse, the warm's included, answered after Space), a bold cell, a code span and a link at Quick Look's body size on a stand-in preview, because no card holds a Markdown pane at rest: the process's first table build, its bold and code faces and its first rich import (2.1 to 2.9 ms in a headless probe) are then paid before Space, and `tests/quicklook-tablewarm.sh` pins it.

`columnReady` waits for LOADING never, an image for its cache file, or with none coming the original
`frameThumb` decodes itself, drawn or refused (Ready or Error), a video for its poster or none coming,
a PDF for `shownPage >= 0` or failure, text and code for `PreviewLines.loading` false, an archive for
its meta, and symlink, audio, unsupported and multi for the facts alone; a folder waits for
`answered(shownChildPath)`. `lookReady` waits for `status` not loading, or for the interim cache
thumbnail shown whole at the final rect (`interimShown`, from PreviewImage's `interimReady`), and a PDF also for
`shownPage >= 0` or failure. `PreviewImage` requests the final only once the interim has settled (`interimSettled`, armed one turn after the build because `Preview.qml` binds the path before the thumbnail): the pixmap reader decodes one request at a time and the newest first, so a final queued beside the interim held it back for the whole decode, 6 of 6 loaded runs of the held phase. A close and reopen while the final still decodes lands on the Ready interim in the same turn and reveals without the cap, because the interim decodes on the GUI thread and never queues behind the reader. `tests/js/previewswap.js` drives the moves, the cap, the frame kinds and
every ready rule; mutating `columnReady` reddens it. `tests/preview-swap.sh` grabs the swap item
headless over the file kinds and asserts 0 mid frames on those holds; its one folder step is a
simulated wait, and the real folder data hold is driven by `tests/columnsfolder.sh` through the
real `ColumnsArea`. `tests/ui-noblank.sh` reads `previewSwapState` (holds, fallbacks, midFrames) live.

**// corner: the move to the PNG keeps 2 mid frames by design.** At full-resolution grabs the sharper
original replacing the cache file draws twice between the settled frames. It is the decode finishing,
not a half-built preview, and the count is pinned rather than fixed.

**The geometry gate: a picture drawn at the wrong size never ships again.** A small clip's poster
was seen drawing at its own pixel size in a Columns frame no suite measured, so the rule table is now
measured on every commit: images draw at min(own pixel size, aspect-fit), never enlarged, in the
Columns column and Quick Look; a video poster and any playing video fill the frame on the limiting
side like the player, enlarged when the clip is small; an office thumbnail fills only when its
embedded picture reaches cache size and otherwise draws own-size; a PDF page contains. The headless
`tests/preview-geometry.sh` (37 cells over both surfaces, both frame widths, every source-size class,
one `GEOMETRY` line each plus a `GEOMETRY_SHEET` contact sheet) and the native `previewmatrix` case
in `tests/ui-captures.sh` (real files, column poster and player content rects, Quick Look picture
rect, `PREVIEWMATRIX_SHEET` contact sheet) assert every cell within 1 px headless and 2 px native,
centred. A change to any preview surface runs both and its contact sheet is looked at before the
change is called done. Quick Look PDF has no geometry cell yet: its page item is unexposed, so its
contain rule is stated from `ui/PreviewPdf.qml` and not measured.

Decided 2026-10-03: no Rendered or Source control in the Markdown Quick Look; it renders automatically. r still flips the open Quick Look.
The bar is the language mark, the name, the line count and close. Every Markdown preview opens rendered, Quick Look and column alike,
and nothing stores a view: `preview.markdownView` is not a state leaf. `ui/Preview.qml`'s `markdownSource` holds the flip for the
open Quick Look only. A move to another Markdown file keeps it, `close()` forgets it, and the column never draws Source. A state
file that still carries the key loads without a message, because an unknown key is kept as read; a patch naming it is refused.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
