## Markdown figures

Maths (MathJax 4.1.3) and diagrams (Pele 1.6.0) render in a sandboxed quickjs-ng
helper, `flea --figure-helper`, because neither Qt engine runs them. Qt's WorkerScript engine
could not run MathJax or the first diagram bundle (no lookbehind, no Unicode property escapes, no `new Worker` for ELK,
bare-`this` errors), and the GUI process went from 80-108 MB PSS to about 357 MB after one
formula. QtWebEngine is not an option: on minipc a Quickshell 0.3.1 config with a `WebEngineView`
crashes at start (`QEventLoop: Cannot be used without QCoreApplication`).

`ui/vendor/figure-helper.mjs` reads newline-delimited JSON off stdin and sets no globals (until
pele 2026-10-18 it named `global`, `setTimeout` and `clearTimeout` for ELK). It imports
`math.mjs` on the first maths request and `mermaid.mjs` on the first diagram through dynamic
`import()`, so a maths-only document never loads the diagram bytes. `window` and `self` stay
undefined, since with either defined a bundle takes a browser branch that reads `navigator` or
`Worker`. quickjs-ng names its modules `qjs:std`
and `qjs:os`, not `std` and `os`. `ui/js/FigureWorker.mjs` is a plain ES module both node and qjs
import: the Qt SVG fix-ups, `checkSafe`, the hostile href, url and click refusals and the size
limits, with everything that only existed for the Qt worker engine deleted.

The protocol is one JSON line per request and one line back. A request carries `{id, kind,
source, display, theme}` and an answer carries `{id, svg}` or `{id, error}`; a line that is not
JSON answers `{id: 0, error}`. One bad request never ends the loop, and EOF ends the process at
0. `flea --figure-helper` (`src/figurehelper.rs`) resolves `/usr/bin/qjs`, or `FLEA_QJS` when it
is an absolute path (a test hook, documented as one), resolves the UI tree's `vendor/` directory
the same way the GUI's own root is resolved, and execs the helper under
`sandbox::wrap_readonly_extra`: the thumbnail jail's flags and prlimit caps (2 GiB address space,
30 s CPU), read-only binds for the vendor directory, for `ui/js/FigureWorker.mjs` and for the qjs
binary when it lives outside `/usr`, nothing writable, no network. With no bwrap or prlimit, or
no qjs, it prints one line on stderr and exits 127, without running anything unsandboxed.

`ui/FigureService.qml` owns one `Process` with stdin and a `SplitParser` on stdout, started
lazily on the first request and stopped after `idleExitMs` (30000) with nothing waiting, so the
memory returns to the system. Only the oldest written and unanswered ticket has a `renderMs`
(2000) deadline starting when it becomes the head; expiry kills the helper and fails that ticket
as "render timed out", while every other written ticket is resent once to the next helper with a
fresh turn and the poison ticket never returns. An unexpected helper exit requeues every
written ticket and charges only the head one strike; its second strike fails that ticket,
so a crash cannot loop and one cumulative CPU-cap exit costs no figure. The service's own
kill charges no strike, and exit 127 still refuses the session. A 64-entry LRU keyed by
`FigureService.cacheKeyOf` answers a revisit or a theme flip back without re-rendering, and that
key is pinned against a reference copy in `tests/figure-cache.mjs` by a node check, since the
helper never computes it. A 127 exit or a spawn that never starts latches `available: false` for the session:
one log line, and every figure shows its fenced source exactly as a render failure does, with no
retry storm. `ui/MarkdownFigure.qml` is unchanged by the move: the ask/done ticket API is the
same, so md3 wires figures into the Markdown view later without touching this unit.

mdfast 2026-10-04 makes the first figure show sooner, in three kept changes under `$XDG_CACHE_HOME/flea/figures` (`FLEA_FIGURE_CACHE=off` turns every
cache off, `=svg` only the bytecode, both test hooks). Bytecode: `ui/vendor/figure-compile.mjs` compiles `math.mjs` and `mermaid.mjs` to quickjs-ng
bytecode (`ui/vendor/figure-bytecode.mjs`, stripped source, the final export clause rewritten to a global because compiled modules are unnamed), and
the helper loads `<key>/math.bc` and `mermaid.bc` through `--bytecode=DIR`. The key is a 128-bit digest (`src/figurecache.rs`) over the engine binary,
its `libqjs*` and six sources, so a changed engine or source misses the old directory. Bytecode is trusted input, so `figurecache::verified` checks each
blob's size and digest against the manifest before the launcher names the directory, refuses a link or a foreign manifest, and any refusal runs the
helper from source. The first start runs from source and spawns `flea --figure-compile` detached (`src/figurebuild.rs`: lock, `.failed` mark retried after
an hour, scratch `.tmp-PID` swept when stale, the swap one rename); the build runs in a jail whose only writable path is its scratch directory
(`sandbox::wrap_compile`), proves the bytecode answers what the source answers on two requests, and sweeps other keys. The render jail never gains a
writable bind: the verified directory is one more read-only bind. Warm start: `PreviewMarkdown` calls `FigureService.warm` when a rendered document
with figures rests, which asks the store whether every figure is already on disk and, when not, starts the helper with `--warm=KINDS` so it loads those
bundles and renders a throwaway figure before the first ask (about 26 MB resident with both kinds against 12 MB idle, gone at the 30 s idle exit; a moving
cursor, Source view and a document with no figures start nothing). Remembered figures: `flea --figure-store` (`src/figurestore.rs`, `ui/FigureStore.qml`)
keeps one file per drawn figure in `figures/svg`, named by the source digest and the digest of the whole in-memory cache key (theme, advances, display
mode, kind, source), holding the key itself so a name that matches while the key does not is a miss; a get is also an access, the oldest access goes first
past 512 entries or 64 MiB, a figure over 4 MiB is not kept, writes are exclusive scratch plus one rename at mode 0600 in a 0700 directory, and a link,
a truncated or foreign file is a miss that the next put replaces. A hit answers before any helper starts; every store failure is a miss and latches the
store off for the session. Not changed: `ui/vendor/mermaid.mjs`, `tools/vendor-js` and `tests/js-runtime.mjs`. The 50 ms `askSettleMs` in
`ui/MarkdownFigure.qml` is now the largest term left on a cold first figure; it is a placement race, so it waits for a native capture before it moves.

mdfast-r2 2026-10-04 closes the first review round. The helper takes `--report` (the launcher adds it under `FLEA_FIGURE_REPORT=1`, a test hook) and prints
`{"id":0,"bundle":"math","from":"bytecode"}` once per bundle it loads, because a failed bytecode load falls back to source and answers the same bytes: the
build installs a cache only when its bytecode run reports bytecode for both bundles and its source run reports source, and `tests/markdown-figures.sh` makes
the same assertion on the corpus runs and proves it refuses a helper that answered from source. The build's lock holds an owner token, so the final unlock
removes only this builder's lock, and `LOCK_STALE` is derived from `BUILD_DEADLINE` times `BUILD_RUNS` plus a margin so it always exceeds the longest build.
`figurecache::verified` opens the key directory with `O_NOFOLLOW | O_DIRECTORY` (a link at the key directory is refused, which `O_NOFOLLOW` on the manifest
alone never did) and the SVG store refuses a linked `svg` directory the same way, both leaving the same-user swap-after-check race to the 0700 root. The store's
`known` query takes full cache keys (kind, theme, display mode, advances, source) and asks for each key's own entry, where it matched any theme for a kind and
source; `PreviewMarkdown` builds those keys from an unplaced `MarkdownFigure` so a figure drawn under another accent still warms the helper. The idle stop of
the store (`FigureStore.stop`) is refused while a reply is owed or a line is queued, and otherwise closes its stdin and lets it drain to EOF, answering every reply sent and committing every put before it exits, so an idle stop loses neither; the service asks again at each idle while the store is still active (refused, or draining and perhaps restarted for a queued put), so a store never runs on for the session. A line's reply clock (`replyMs`) starts when it is written to the process, never when it is queued. Each concern has its own bound: a get or known queued behind a drain or a start waits at most `drainMs` (`replyMs`, checked by the 250 ms reply poll) and is then a miss for the helper, dropped from the queue, with the draining store left alone, and a queued put waits for the next store; a drain still running at `drainHangMs` (60 s, which commits 15 puts of the 4 MiB entry cap at a 1 MiB/s slow-disk write rate) is a hung store and is killed, uncommitted puts in flight being the only loss, without latching the cache off, while only a store that stops answering within `replyMs` latches it (`hangKills` counts the kills for the suite); `owed` is a notifying count, because the reply timer once bound to an in-place change of a `var` and
never ran, so a hung store was never failed. The compile jail's escape probes all write inside the test's own root, and a run with no jail turns each red.

mermaid-r1 2026-10-04: beautiful-mermaid is patched, `tools/vendor-js/patches/beautiful-mermaid+1.1.3.patch` applied by `build.sh` after
`npm ci` (hard failure on a reject, fuzz or offset), so layout matches mermaid.js: dagre's cycle breaking (node positions in DFS
reverse-postorder with ELK's INTERACTIVE breaker; DEPTH_FIRST where subgraphs exist), labels measured from the drawn font, the mirrored
bottom actor row and lifeline spacing grown to fit each message. Rebuild: edit the patch, run `bash tools/vendor-js/build.sh`, copy the bundle over `ui/vendor/mermaid.mjs`.

mermaid-r2 2026-10-04: labels are measured per character, not by one mean: `ui/MarkdownFigure.qml` sends `advances` and `boldAdvances`
(printable ASCII 32 to 126 in thousandths of an em, from `FontMetrics.advanceWidth`), the library sums them (wide and combining rules as before,
other non-ASCII at the table's mean), and the cache key (`FigureService.cacheKeyOf`) carries an FNV-1a digest of each table (`tableDigestOf`). Node ids stop at a link (`A-->B`, `my-node`), a
bare mention of a known node joins the open subgraph and the first block to close with a node keeps it (mermaid's flowDb `makeUniq`), every
link form parses (`-- text -->`, `-. text .->`, `== text ==>`, any length, `<-->`, `o--o`, `x--x`, `~~~`) and a link of length n spans n ranks through n - 1
empty ELK nodes whose pieces are joined into one edge, notes over several lifelines span the first to the last, a subgraph is at least as wide
as its title, and bundling never routes an edge through a node. The figure tests share `tests/js-runtime.mjs`, so `tests/markdown-figures.sh` runs
them under quickjs-ng (the CI image has no node) and again under node where it exists.

mermaid-r3 2026-10-04: every end mark draws under Qt. A flowchart start marker was a mirrored polygon under `orient="auto-start-reverse"`, which reversed it twice and left it under the node;
it is now oriented `auto`, a class composition or aggregation diamond is referenced at its tip (`refX` 12), and a plain class `--` link draws no arrowhead.
`tests/markdown-figures-render-ends.qml` samples accent ink at both ends of the first edge of 12 link cases, from a real helper figure. A subgraph with an edge leaving it is laid
out with the whole graph, as mermaid.js's dagre wrapper does (its docs example draws one, two and three in one row); a subgraph with none stays an ELK compound node. The
flattened frames hug their members, the edges between them and their child frames, and the layout falls back to the nested one when a frame would take in a foreign node or cross a
sibling (LR docs example, nested clusters with outside edges). A later bare mention keeps a node's first shape through the library's own `registerNode` isNew guard, unpatched (pinned in tests/mermaid-syntax.mjs).

pele 2026-10-18 replaces beautiful-mermaid 1.1.3, its Flea patch and ELK with Pele 1.6.0 (npm `pele`, MIT, obsidianmd/pele tag 1.6.0 at commit
659c3593ce43794dce7ec7ab45d424ad2de182a5, `ui/vendor/LICENSES/MIT-pele.txt`), which draws all 33 Mermaid 12.1.0 diagram types where the old bundle drew 6
(flowchart, sequence, class, state, ER and XY chart); the mermaid-r1 to r3 notes above describe the old bundle. `tools/vendor-js/mermaid-entry.mjs` hands Pele a measurer built from the request's `advances` and
`boldAdvances` (wide and zero-width rules as before, other non-ASCII at the table's mean), turns off links, images, responsive sizing and automatic direction,
draws the Gantt today marker at noon of the day the request's theme names as `today` (`FigureService.datedTheme` sets it for a Gantt source, and
`cacheKeyOf` joins it to the key, so a stored chart is that day's drawing and every other figure keeps the key it had), and resolves every `--pele-*`
variable in an attribute value, never in a label's words, to a concrete value of a Flea theme role. The mapping is `bg`, `fg` and `accent` as named, line and border in `muted`, surface in `surface`, surface-alt on the pane ground `bg`, critical and
error in the accent, both fonts in `theme.font`, and the eight chart series from the four colour roles, each pure and then mixed toward another. The entry
also folds the one `color-mix` Pele writes (the halo a link gets where it crosses a nested subgraph title) into one colour, so neither the post-pass nor
either renderer sees a `var()` or a `color-mix`. `postMermaid` turns Pele's style attributes into presentation attributes (Qt SVG reads no `rx` and no
`var()` from a style, and no font family is kept since every label is measured in the theme font the root names), turns each nested icon `<svg>` into a translated and
scaled group drawn in the foreground, rebuilds
a multi-line label as one text per line because Qt SVG draws no tspan placed by its own x and y, gives an unstyled link's marks the accent as Flea drew them
before (a link a user style colours keeps that colour), unwraps links and drops images, grows the canvas when a user style's border is wider than Pele's
one-unit pad, and then runs `checkSafe`. `checkSafe` is an allowlist and refuses by default: it reads every tag whole and passes only the elements,
attributes, data attribute names and style properties that the diagrams and formulas of the figure corpus draw, plus the four presentation attributes
`STYLE_ATTRIBUTES` writes from a style (`letter-spacing`, `stroke-dashoffset`, `stroke-opacity`, `word-spacing`), which
`tests/figure-worker.mjs` holds to the list (13 elements, 44 attributes, 22 data names, `vertical-align` and `min-width` as lengths), the SVG namespace as the only `xmlns`, and no other markup (a comment, a document type, an
instruction or character data refuses). Each attribute value is read in either quote style with its character references decoded, and an `href` that is
not a same-document fragment, a `url()` that is not one in any case, remote text and a leftover `var()` or `color-mix` refuse; a data attribute holds
words, a formula's TeX among them, so only its name is checked, and the literal checks for `http:`, `https:`, `@import` and a non-local `url(` read the
whole document but its namespace name. `FigureService.cached` dates a Gantt theme as `ask` does, so a revisited chart draws in the first frame.
`currentColor` is replaced in attribute values only, like the entry's colours. `src/figurestore.rs` moves its entry format to `flea-figure-svg 2`,
because a stored figure is keyed by its request and not by the engine, and its `known` query reads each entry's format line, so a figure the old bundle
stored warms the helper; the bytecode key moves on its own with the bundle digest. `tests/mermaid-types.mjs` renders one docs example per type from
`tests/fixtures/markdown-mermaid/` and fails by name when a type stops rendering, loses its own mark class or a label, or has no canvas, and
`ui:cap_markdown_pele` shoots each type in Quick Look and in the preview column at text size 14 as `pele.<type>.<engine>.quicklook` and `.column`.
Measured 2026-10-19 in the suite lane, both bundles on one binary and helper. A first diagram (spawn, bytecode load and render) took
40 to 48 ms median against 100 to 106 ms, a warm render 1.0 ms against 13 to 17 ms, a qjs 7.8 MB at its peak against 16.9 MB, `mermaid.bc` is
1037608 bytes against 2585187, and of a 147-source corpus (every Mermaid fence in `tests/` plus one docs example per type) Pele draws 146 and the
old bundle 115.
sequence-defaults 2026-10-25: Pele drew a sequence's participants 64 px wide and 40 apart over solid lifelines, where v0.3.12 and the Quick Look
figures board draw them 80 by 40, 60 apart, over lifelines dashed `6 4` at 0.75. For a sequence source the entry writes those three numbers as
the first `init` directive, after any front matter and leaving out a key its `sequence` section names, because Pele merges directives key by key but a
`render` config whole; every key a source sets still wins. It dashes the `pele-lifelines` group, and the store moves to `flea-figure-svg 3` so no
format 2 sequence is served again.

pele-geom 2026-10-25: Pele laid a diagram out at the body size, with round flowchart boxes at least 48 wide (44 high at body 13) and a sequence's first message 46 below
its participants; v0.3.12 and both HIG figure boards draw square 60 by 36.9 boxes (41.95 wider than a long label, 16.9 a line plus 20, a 0.75
stroke), the first message 20 below with its label 6.15 above it, messages 40 apart and the closing row 40 under the last, all laid out at body
13 and scaled. The entry now lays every diagram out at 13 and scales the root's width and height by the body size over 13 (`FigureWorker.mjs`
sets the root font at 13 to match). A sequence takes `messageMargin` 42 among its defaults, and when its first row is a straight message and nothing reshapes the rows above it (a note, frame, rect or
activation above it, a created or destroyed participant, no closing row, or the source's own messageMargin), the content rises 26, its labels 2.5 less, and the closing row 6. A flowchart's plain boxes are redrawn at v0.3.12's size; each rank closes or
opens along the flow by what its furthest reaching shape along the flow gained or lost, edge ends, markers, labels and subgraphs move with it, ranks that overlap (side
by side subgraphs) move as one band at their own spacing, and a drawing the pass cannot place exactly (a relative path or an unknown group) keeps
Pele's own. The store moves to `flea-figure-svg 4`.

mathsize 2026-10-04: a display formula's ex equals the body font's x-height, so maths and prose letters stand level.
`ui/MarkdownFigure.qml` measures `FontMetrics.xHeight` on its own family at `bodyPx` and sends it in the theme as `exPx`;
`postMath` in `ui/js/FigureWorker.mjs` converts a formula's ex units at that height, display or inline (the one scale, as
a browser's 1ex does for Obsidian and GitHub), and the em rule (`bodyPx` times 0.442) stays only for a figure that reports no height. `exPx` is in both cache
keys, so a text size or font change re-renders instead of serving the old size. `tests/markdown-mathsize.qml` pins it.

mddraw r2 2026-10-04: every block kind nests. `blockPass` (`ui/js/MdBlocks.js`) re-parses an item's or a quote's lines (`nested` skips front
matter) through the same writer (`partsOf` in `ui/js/MdDocument.js`, sharing the document's defs, tokens and cited footnotes), so a fence,
indented code, table, heading, thematic break, display maths, an inline-maths run, a quote in an item or a list in a quote becomes a part
drawn with its top-level recipe. Prose alone stays one `text`; otherwise a quote carries `parts` and a list `parts[k]` beside `items[k] = ""`.
`NEST_LIMIT` (8) keeps a deep chain linear: deeper lines stay Qt Markdown. `ui/MarkdownBlocks.qml` draws a parts array and `MarkdownBlockView.qml`
one block; `MarkdownList.qml` and `MarkdownQuote.qml` reach `MarkdownBlocks.qml` through a `Loader` by file name because QML rejects a static type cycle; the Loader `source` stays empty until it is
needed, since `markdown-security` reads a Loader with a source and a Null status as an unsettled image. An empty heading or quote has no height.
`MdChunks.js` cuts every array a list carries at the same places and gives each chunk the runs open at its edges (`carry`, `tail`, the widest
marker per run) so a sublist at a seam keeps its text column, and a later chunk keeps the gap above its first entry. A run with inline maths sizes
its line box and picture padding from `bodyPx` like a plain run. The spec exceptions `qt-nested` and `qt-empty-fence` are gone: `markdown-spec`
passes 633 of 697. `markdown-nest` (`tests/markdown-nest.*`, `tests/js/mdnest.js`) pins the geometry and recipe of each nested kind and the
zero height of an empty heading or quote; `markdown-blockcost` now draws its formulas through a stand-in figure helper and counts the drawn state
(maths 31 objects for two distinct formulas, list 26, quote 17, each with a Loader). `ui/PreviewMarkdown.qml` is 489 lines and `tests/markdown-linearity.qml` 572 (nested-block cases), their budget rows.

Measured on this Debian box through direct qjs, whose bwrap cannot run Arch's jail: cold
spawn-to-answer 51 to 55 ms for a formula and 167 to 176 ms for a diagram, five samples each;
warm 0 to 2.5 ms per formula and 0.9 ms per diagram; the helper peaks at 41380 to 41524 kB RSS
with both bundles loaded. The GUI side is taken offscreen on minipc, five samples each, every
number a range: PSS before any figure, after the ten formulas and after the six diagrams, and
PSS 5 s after the idle exit with the helper gone. The gate judges formula and diagram `Rss`
growth against the same 10240 kB limit from one stamped `/proc/self/smaps_rollup` read, because
RSS includes this process's resident file-backed pages without PSS's changing shared-page
divisor, and prints PSS and Anonymous as evidence.
Five native runs of the suite on minipc at `9b661b2e` (2026-10-03), every read a fresh `/proc`
read: GUI PSS 52838 to 52974 kB before, 52387 to 53232 kB after the formulas, 52415 to 53260 kB
after the diagrams and 52452 to 53300 kB 5 s after the idle exit; idle minus before is -394 to
+338 kB within a run, and the helper peaks at 41132 to 41200 kB RSS there.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
