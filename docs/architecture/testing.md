## Testing

The cohesion capture port moves two ceilings, each re-derived with `wc -l`: `ui/Ipc.qml` 985 to 992 and `ui/PickerWindow.qml` 812 to 819, for the `cohesionControls` call each window makes into `ui/js/ControlReader.js`, the one reader the cohesion and button look captures take every control's box and state from; the reader stays off the first frame: `ui/IpcControls.qml` imports it and each window's `cohesionControls` builds that file on its first call (`Qt.createComponent(...).createObject(null)`, collected after the call), so neither `ui/Ipc.qml` nor `ui/PickerWindow.qml` imports the unit and `tests/startup-units.txt` does not list it. The pinned bytes are 1628899, as `startup-objects` measures them with the reader off the first frame (169 units); a mutant that imports `js/ControlReader.js` at the top of `ui/Ipc.qml` again turns `startup-objects` red.

The port re-derives both `tests/staticgates-unqualified.tsv` profiles with Qt 6.11.2: 1,469 allowances with Quickshell present and 2,219 with it absent. The absent-module census uses `qmllint --bare` with the real Qt modules and the Quickshell stubs, excluding the installed Quickshell module; it pins 532 rows absent from the inherited pin, removes stale reads and names the current declared ids in Glass and MediaSession and the unresolved Hyprland singleton. Every allowance names its current read with an applicable reason and the compiler's duplicate count, and the present profile keeps its exact row set after the picker's three lines per profile move from 712 to 713, 713 to 714 and 714 to 715. The `startup-objects` census proves the 169-unit, 1,628,899-byte list, and an eager `ControlReader.js` import in a marked source copy fails the explicit cold-unit assertion.

The capture port keeps the stage's later suites and native cases. The sweep's PDF column capture follows the continuous stack from `c5196e133`: it wheels to the second decoded page and requires a positive scroll offset, instead of asking the removed `columnChevronCentre` reader. `tests/cohesion-reader.sh` reads both windows offscreen, including the stage's `TabBar`, `PathEntry` and `PickerFilter` types in place of the stale `TabButton`, `PathField` and `Framed` names. `ui/js/ControlReader.js` also reads a control's own `name`, which is how the filter calls itself "Filter". The filter draws hover and press from its existing pointer handlers rather than exported flags, so the reader observes those handlers when a control has no direct flag; the picker reader test requires both capabilities. Native `ui:cap_b72` and `ui:cap_cohesion` include their picker captures, `ui:cap_columns14` takes the current Columns view at text size 14, `ui:cap_archives_format_live` and `ui:cap_archives_format_kanagawa` take the archive Format cards, and `ui:cap_unlock` records unlock failure and success for the controller to inspect.

- `tests/menu-settle.sh` has a second leg, `tests/menu-open-cost.qml`, that opens the real pane's row menu by the `m` key and by a click point, in two processes so the key and the click path each take the cold first open, and pins `MenuFit.reads` (ui/js/MenuFit.js, what the card's fit reads) to one read per rows build. The card's width binding reads every row's `wantedWidth`, so it re-ran once per row built and cost 209 reads cold and 114 warm for 3 to 4 builds; `ContextMenu.setEntries` now holds it while the rows build (`rowsBuilding`) and it reads once when they stand (4 and 3 reads). Headless, that was about 2 ms of a 13 ms cold open and 1 to 2 of a 10 ms warm one.
- `tests/picker-040.sh` is 0.3.10's Picker040 acceptance suite for path entry and collision review, expected red until then and named in run-all's not-run list.
- `tests/xwstate.sh` is a headless run-all suite: it drives two shipped windows offscreen through `tests/xwstate-control.qml`, a test-only IPC seam copied into a sandbox. It proves the received tab keeps its cursor, settings and Favorites reach the idle window, a cut in one window stays there with one "Copied in this window only" notice and never reaches the other, a window pasting its own cut moves the file and empties its clipboard, and undo crosses windows. Offscreen there is no compositor, so sharing a clipboard between two windows is the native `clipboard` case of `tests/ui.sh`, not this suite's.
- `tests/rename-scroll.sh` is a headless run-all suite for the rename editor under real scrolling: `tests/rename-scroll.qml` and `tests/rename-scroll-origin.qml` instantiate the shipped `ui/List.qml`, `ui/GridArea.qml` and their Row and GridTile editors over `tests/rename-scroll-pane.qml`'s stub pane in a real offscreen Window (list, grid and the shifted list origin, 38 checks), through `qs -p` in a `mktemp -d` root, and ends with `RENAMESCROLL DONE checks=<n> failed=0`. It fails on a warning, a binding loop or a load error, and when `ui/Pane.qml` drops the two `showRow` branches the stub copies; `tests/js/watch.js` holds the handler half (a pending rename refuses a second one, a dead backend leaves the listing in error).
- `tests/preview-040.sh` is 0.3.10's TallPreviews acceptance wrapper: it exits 0 only on its one known per-file scroll restoration failure and exits 1 on any other result, unexpected green included; that expected-failure contract keeps it out of the default battery.
- `tests/sheet-query.sh` is a headless run-all suite for the keymap sheet's query (CommandPalette, "After typing") in two halves. The stub half, `tests/sheet-query.qml`, loads the real `ui/KeymapSheet.qml` over a stub pane: one cap column at rest and under a query, the board's `?` prompt in the cap column and the query in a hairline accent field on the label column (decided 2026-10-03: focus is the accent frame, never the board's ring), an action that is both a key and a menu row listed once as its key (`SheetQuery.unique`) and found by either wording (a word only the menu's wording holds lists the key row in that wording), a Compress leaf listed as its own row "Compress to .zip" (`sheetLabel` from `Archive.formatEntries`, the flyout keeps ".zip") with no muted "in Compress" and no Compress parent row beside it (`SheetQuery.withoutListedParents` drops a flyout parent whose leaves are listed, for every flyout), the card's top and title where the rest card put them in every state with the card growing from its bottom only (`restCardHeight`, in a window short and tall), the cursor's lift edge to edge across the card's inner width, the preview's mute key saying "in Preview" in the muted ink (`SheetQuery.CONTEXT_NAMES`), every other flyout leaf keeping its label and "in <flyout>", a place's or recent file's muted suffix inline after its name and keeping its width while a long name elides, Backspace through the sheet's own `Keys` handler, "esc closes" in every state, and the Permissions row run or refused from the menu's own availability (`ui/ContextMenu.qml`'s `listingContext` for the pane's real cursor, live because `ui/Pane.qml` keeps `menuValuesLive` true while the sheet is open, `sheetReadsMenu`). The pane half, `tests/sheet-query-pane.qml`, loads the real `WindowBody`, `ui/Pane.qml` and backend and drives the sheet with real key events, opening no menu for those runs: Enter on Permissions, Move to and a Compress leaf must each reach the menu's own surface, and Delete permanently, which has a key row in every preset, is run through `SheetQuery.runMenu` itself. That is what `Pane.sheetMenuAction` exists for: it stands the menu on the cursor row (`hasRow`, `forHeader`, `openedIdentity`) before it snapshots, because `ContextMenu.validateChoice` otherwise compares the selection with the identity of a menu that was never opened and refuses with "Selected items changed; reopen the menu.". The pane half's last stage then opens the pane's own menu on the cursor row: that menu must still omit and refuse Permissions, which the shipped profile hides, and a pick of Permissions through `PaneMenuActions` with no sheet request must be refused when the backend replies. The tab rows read the chord alone ("ctrl-pagedown", "ctrl-pageup", the board's strings) because `[` and `]` carry `sheet = false` in keys.toml: they stay bound and hinted. The sheet's shots, the fl query over a favourite and a recent file, the delete card on Cancel and then Delete, and the Permissions dialog are the native `cap_sheet` case in `tests/ui-captures.sh`; `tests/ui-captures-sheet.sh` runs that case's own shell against a stub ipc with one control scenario per assertion, so it proves the case, never the pane.
- `tests/gridmark.sh` is a headless run-all suite for the Grid's match mark: `tests/gridmark.qml` builds the shipped `ui/GridArea.qml` over a pane stub and reads each drawn wash back as a box in caption coordinates, measured against plain Texts for the run and Qt's own laid-out width for each centred line, at rest, on the cursor, hovered, selected, cut and on a drop target, across a wrapped label and in search; the wash lands in the turn the narrowing does, and a key after the strip opens builds nothing.

- `marquee_press` and `marquee_to` stop within 4 px of their target (`marquee_glide`'s default tolerance), so a native case never aims them within that distance of a boundary between two press targets (a row's right edge is the scroll bar's left edge). A band is started through `marquee_begin_below` in `tests/ui-marquee.sh`, which glides onto the bare ground below the last row and checks where the pointer landed; `case_clickedge` starts its band there after End with an exact glide, because the List tail is 3 px, and requires the band live, the selection exact and `viewContentY` unchanged across the release.
- `tests/run-all.sh` counts a suite that exits 0 while its output holds a line matching `(^|[: ])FAIL[: ]` as failed and quotes that line, the same contract flea-ci enforces, so an expected failure a suite shows is reworded (`expected: ... failed ...`) and never echoed raw. `tests/runall-rule.sh` pins that rule on a scratch copy of run-all.sh with stub suites, a quiet one, one past 64 KB of output and the non-matching controls included, so a rule that loses its match on a loud suite goes red.
- **The warning gate covers `#[cfg(test)]`, which is the point of it.** `cargo build` cannot see
  anything inside a test module, so a build-only gate hides every unused import and dead helper
  there; two lived here for three review rounds for exactly that reason. Keep debug and release
  coverage and keep zero warnings, but **build incrementally**: the `cargo clean` before each of the
  four invocations was ceremony, not coverage, and cargo decides for itself what needs rebuilding.
  Do not separately rerun what `./tests/run-all.sh` already covers. `cargo build` cannot see anything inside `#[cfg(test)]`,
  so a two-invocation gate hides every unused import and every dead helper in a test module. Two
  of them lived here for three review rounds, one of them predating the whole thumbnail plan,
  because the gate said "build" and nobody ran the other half.
- `cargo test` runs the unit tests inside every module.
- `case_network` (and `case_editplace`) run Flea against their own `gio` double, whose `mount -li` lists nothing, so the operator's gio mounts never reach the rail and the case never refuses on them; cases `gvfs` and `networklive` use the real gio by design and check only their own rows. The double does not cover `/proc/self/mountinfo`, which Flea also lists kernel NFS and CIFS mounts from: such a mount outside home still shows, and `case_network` then fails showing that row rather than refusing. It waits on `networkBuilt` (`ui/Ipc.qml`), true only once the rail rebuilt from a listing and a mountinfo read, because an empty group is also the unbuilt state, then asserts the group is empty; a failed ipc read fails by name.
- **Each `tests/ui.sh` case opens with a paragraph rather than a one-line comment**, and that is
  deliberate. A case is a fixture, a stub and a sequence, and the part a reader cannot recover from
  the assertions is what the stub is standing in for and what the case controls for;
  `case_sharebrowser`'s preamble is fifteen lines and `case_hangshare`'s is six. The comments inside
  a case body stay short. Three review rounds have now raised the preambles against the one-line
  comment rule, so what the file does is recorded here instead of being re-litigated per round.
- **`./tests/run-all.sh` is the one command, and it exists because nothing executed any suite
  at all.** Before `96186ff` this tree carried twelve suites, no runner and no CI: every
  cross-reference to a suite, here and in `README.md` and in tool and source comments, was
  prose naming it rather than a line running it, and `PKGBUILD`'s `check()` ran
  `cargo test --release --locked` alone; it now also runs `tests/js.sh` and `tests/keymap-gen.sh`,
  the only two that need no built binary and locate themselves under makepkg's moved
  `CARGO_TARGET_DIR`. **`96186ff`'s own message is wrong about this and cannot be rewritten,
  because the branch is shared:** its subject says nine suites were uninvoked and its body says
  seven, and the derived answer is zero of the twelve there were
  then. **The runner builds both cargo profiles unconditionally, and it lost that once.** It
  built them from `324f321`, and `8eec5fc` replaced an `[ ! -x <path> ]` pair with an
  unconditional one on measured evidence: the guards were satisfied by a stale binary from an older
  commit, and the debug and release hashes were unchanged across a whole run-all over edited source.
  A conflict resolution over `tests/run-all.sh` then kept the union of two `headless=` lists and
  dropped the `cargo build` pair BOTH sides carried, so it was absent at `a485200` and through the
  0.1.4 lanes until it was restored. **A guard answers "is there a binary", never "is it this
  commit's binary"**, which is the defect `39e1737` and `8eec5fc` were both written to close, so the
  unconditional build is the contract and the per-suite `-x` guards exist only for a suite invoked
  directly. The suites that drive the debug binary use `target/debug/flea` and `thumbs.sh` the release one. It runs every suite that
  needs no display, session or hardware, and reads each suite's OWN exit code, never a pipeline's.
  Those suites need a shell, python3, the Qt and Quickshell runtimes, and quickjs-ng for the figure
  suites; the package build and check dependency closure must supply those tools. A missing required
  tool must make its suite refuse loudly, naming the executable or paths it looked for.
  Its own `headless=` list is the inventory of those and its own `not_run` list is the inventory of
  the rest with what each needs, so this paragraph carries neither a count nor a membership for
  either list to outgrow. A suite in neither list fails the runner's own audit, so one cannot go
  uninvoked a second time.
- **`./tests/drag.sh` is the internal drag's characterisation suite**, whose own last line prints its tally, and it has to
  be run by hand: no runner invokes it. It was written against the drag's behaviour BEFORE the
  platform-drag rewrite, so it is the net that catches what the rewrite changes, and it earned
  that immediately: the first cut of the rewrite failed R3, `expected [kept] got [GONE]`,
  because `Drag.active = true` runs a nested event loop in which **the window receives no key
  events at all**, so the `Keys.onPressed` handler carrying ctrl never fired and every
  ctrl-copy silently became a move. The file still arrived, so nothing looked wrong. R7 stops the
  suite's own backend to prove a drop on the current tab refuses while its listing is out,
  stays refused after the listing lands, and a second drag then copies across devices.
  A row's or tab's screen centre is awaited through `centre_into` and `tab_centre_into`, never read from a
  bare substitution: a listing mid-swap answers no centre, and the helper ends the suite on a
  `DRAG_CENTRE_FAIL` evidence line and a `die` naming the row, which `dragwire.sh` proves against doubles.
- **Copy versus move versus link is one function, and the modifiers are read at the lift.**
  Same device moves, another device copies, Ctrl forces a copy, Shift forces a move, Ctrl with
  Shift links. A link needs no device and no deletable source. A device
  that could not be read copies, and a source that cannot be deleted copies, unless Shift is
  held. A drag that started in another process follows that same device rule. A plain lift's
  `Drag.supportedActions` offers `Qt.CopyAction` alone: Qt hands that straight
  to Chromium as `effectAllowed`, and a move offer is what made Chromium report `dropEffect: move`,
  which Google's uploader refused in 0.1.4. Ctrl at the lift offers copy
  alone, Shift offers move alone, and Ctrl with Shift offers a link alone, because a receiver
  takes whatever is offered and does not have to notice the key Flea read, so it still takes
  the lift's verb. Files accepts a move and then
  moves the uri-list itself, after `dragFinished`, so Flea does not delete on that acceptance:
  doing so trashes the files before Files has read them and the destination never gets them. A
  drop this window's own transfer handles accepts copy instead, and the transfer does the move
  or the copy. A directory this user cannot write is marked not deletable on the listed line's
  `w` field, and a drag from it copies unless Shift is held. Qt's platform drag runs a nested
  loop in which the window receives no keys, so Ctrl
  or Shift pressed after the drag starts cannot change what a foreign receiver is offered. A drop
  onto one of Flea's own folders reads the keys held at the drop (#259, `keys` in `ui/js/DragOut.js`),
  which is why the status line says `Ctrl copies, Shift moves`, naming both keys,
  and a Ctrl with Shift lift links, its Link line naming the verb outright. `drag.sh` proves a same-device move and a Ctrl copy but needs the display and a
  real pointer, so `dragwire.sh` carries the offer into the headless battery: a leaving drag
  must offer copy alone and `text/uri-list`, a link lift must offer `Qt.LinkAction`,
  and the shelf drag stays copy only.
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
  it launches the tree it measures; `~/bench/` is where it runs here. A working NOPASSWD rule for
  `drop_caches` needs no stdin; otherwise the sudo password is read from stdin. It launches Flea through `$FLEA_BIN --gui`, never `qs` directly:
  the GPUI launcher applies its environment policy and calls `thp::disable()` before execing
  `flea-gui`, so a direct GUI launch bypasses those steps.
- `tools/flea-field-nav measure` with the `ql` column journeys Quick Look on a 40-row-table-first Markdown document
  (`ql_md_table_first`) against a tableless one of the same 4096 bytes (`ql_md_plain`) and a byte-identical twin as
  the negative control (`ql_md_plain_b`), each in a fresh launch, key to `settled_ms`, and `report` prints INCOMPLETE
  with no twin rows, INVALID when the twin control reports INSTRUMENT-NOISE, else SHIP when p95(table) <= p95(plain) +
  the control's own gap |p95(plain) - p95(twin)|, else HOLD.
- `tools/flea-localsend-battery --flea BIN --cli LOCALSEND_CLI` is the LocalSend battery. It needs no root: it
  re-executes inside a new user and network namespace, puts four nodes on private bridges and runs real
  `localsend-cli` peers that accept with Y (the declined case presses N, the unanswered and cancel-at-prompt cases
  press nothing), so discovery and port 53317 never reach the host's LAN. Node a runs
  `flea --backend` with that CLI first on its PATH. Each case runs `--runs` times (default 20; `--cases name:runs,...`
  narrows it, `--list` names them): discovery, a late answer, small, Unicode-named, 1000-file, 2 GiB and folder sends,
  a second interface, cancel during the transfer and at the accept prompt, an unanswered and a declined send, two
  sends and two discoveries at once, discovery during a send, a peer appearing, leaving mid-transfer and sleeping
  (SIGSTOP), a changed address, and the reverse direction. Peer B's link is shaped to `--rate` (default 300mbit, so
  the 2 GiB send takes about a minute). Every failure is printed and appended to `--log` with its case, run and
  reason, the summary gives one pass count per case (`send-small 20/20 ok`), and the exit status is 1 if any run
  failed. It needs iproute2 with `tc`, util-linux and Python 3; a full pass takes about an hour.
- **The tested Flea starts from its own caches, as an installed one does after its first launch.** Its
  QML units go to `target/qmlcache` of the tree under test (`QML_DISK_CACHE_PATH`), and
  `tools/flea-field-bench` parks the operator's `~/.cache/flea/prefetch` for the run and restores it
  at exit (gated by `tests/field-prefetch-park.sh`). On TERM, INT or another exit, teardown first
  stops the background warm shell and its process group with bounded TERM and KILL waits, then
  sweeps the warm Flea and its control group before restoring the operator's caches. The suite
  checks the shell and group both before Flea starts and while it polls, including a setup child
  that ignores TERM; dropping the warm-job stop makes these checks red. One untimed warm launch compiles the units and
  records the first list; each timed launch then reads the list the launch before it recorded, so
  every list it reads comes from the build under test. A list recorded by another build names that
  build's binary, QML and units, so its prefetch reads the wrong files and the right ones are read cold.
- **Nothing an entrant starts outlives its run.** Every launch exports `FLEA_FIELD_BENCH` with a token
  unique to the invocation, and every `clean_bench` ends any process that still carries it, names it
  as a `LEFTOVER` line, and fails the run if one outlives SIGKILL (gated by
  `tests/field-entrant-sweep.sh`). The kill list and the control group cannot do this alone: nnn's
  preview-tui starts a terminal of its own whose children kitty moves into scopes outside the run's
  group, and on the media fixture each one left an `mpv` decoding at about one core under every later
  entrant. A start finds a token from an earlier invocation the same way and refuses.
- **A TUI row's `mapped_ms` is the terminal's, and `first_rows_ms` is the app's.** kitty maps its window before its
  child draws anything, so `mapped_ms` for a TUI entrant measures kitty starting. `first_rows_ms` is when the child's
  output, as kitty parsed it, first names a fixture entry, taken from a rows probe: before each measured TUI launch,
  `run_guarded` launches the same entrant once with kitty's `--dump-bytes` into the run's config directory (tmpfs),
  stats that file every 5 ms from the bench's own process, and greps it against the first screen of an ascending name
  sort (byte and natural order, names of four characters or more). The probe ends once its rows and map are known, and
  the measured launch runs kitty without the dump, because `--dump-bytes` switches kitty to `parse_worker_dump`, which
  calls into Python on kitty's main thread for every drawn character and every command, CPU that would land in the
  run's own control group. A probe's listing before its own window maps is stamped at that map, a probe whose window
  never mapped has none, and the measured row writes the probe's value unchanged (gated by `tests/field-tui-frame.sh`).
  The entrant lines carry `\$TUI_DUMP_FLAG` escaped, so each launch's `eval` reads the probe's setting. GUI rows write `-`.
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
- **A gvfs fixture lists outside every entrant's tree, so the settle clock watches the daemons and the
  gap outlasts the server.** On a fixture under `/run/user/*/gvfs/` the readdir runs in `gvfsd-fuse`
  and the share's `gvfsd-smb` backend, so an entrant blocked in readdir earned no tick and read as
  settled (nnn "settled" in 0.55 s on the NAS while the same stat work takes 19 s on USB): those pids,
  resolved once, join `TREE_TICKS` and never the entrant's own `cpu_s`, and the log and the manifest
  record which ones. Even watched, nothing on the box ticks for about 0.9 s while the server enumerates
  a 10k folder, so a 500 ms quiet gap still settles early: `SETTLE_GAP_MS` is an override defaulting to
  500 and to 2000 on a gvfs fixture unless the caller set one, and `settled` stays the last tick so the
  gap never inflates it. Both move `settled_ms` on NAS only; `FIXTURE_KIND=nas|usb` skips the btrfs and
  marker checks while still asserting `EXPECT_FILES`, `flea-tui` adds TUI `preview_ms` rows, and the
  per-run receive bytes on the NAS interface land on stderr beside each row, moving no column.
- **`cpu_tree_s` was added at the END of each row**, so an old parser still works. Until 0.3.2 it
  carried the watched set's tree total; `cpu_s` then reported the entrant process alone, a figure now
  in `cpu_main_s` while `cpu_s` equals `cpu_tree_s`. On the media
  fixture they diverged hard then: pcmanfm read 38.83 against 90.00 and dolphin 3.29 against 68.59.
- **`cpu_tree_s` is read from a control group since 0.3.2, because the tick total was blind to every
  sandboxed decode.** Measured on minipc: ten `ffmpegthumbnailer` decodes of the media fixture add
  0.95 s to the caller's `cutime` run bare and 0.01 s run under Flea's own `prlimit` plus `bwrap`
  argv, so a decode under `bwrap`'s PID namespace never reaches the process that launched it, and
  the old column counted none of Flea's thumbnails, nor any other entrant's sandboxed ones. The
  bench now re-enters itself under `systemd-run --user --scope -p Delegate=yes`, makes one leaf
  cgroup per entrant run, and has the launching shell join that leaf before it forks the entrant,
  so the entrant and everything it starts, reaped, reparented or still alive, is in the leaf's
  `cpu.stat` `usage_usec`. The same ten sandboxed decodes read 1.02 s there. A declared helper
  outside the leaf, which is `tumblerd` since it is D-Bus activated, is added from its ticks; Flea's
  backend is inside the leaf and is not added twice. The old figure stays as `cpu_reaped_s` ahead of
  `cpu_sys_s`, so a table from before this change is compared against like, and a leaf, or a
  TUI's kitty scope below, still holding a process after the kill list ran names it as `LEFTOVER`
  and ends it by `cgroup.kill`. **A TUI row's leaf holds its terminal too**: the run launches kitty
  and kitty starts the TUI, so that group's column includes the terminal's own drawing, the same
  kitty for every TUI entrant, where the old column, rooted at the TUI process, never counted it.
  kitty 0.48 then moves the TUI into a scope of its own, `kitty-<kitty pid>-<n>.scope`
  (`/usr/lib/kitty/kitty/child.py`), so the row adds that scope's `usage_usec`, read at settle, once
  the scope's kitty pid is proved to be in the run's leaf. `tests/bench.sh` drives `settle_ticks`
  with a helper outside the leaf, and `entrant_cg` and `release_run_cg` against a real kitty-named
  scope; the settle-time sum itself runs only inside a field run.
- **`cpu_sys_s` adds the mount daemons to `cpu_tree_s`.** It is `cpu_tree_s` plus each gvfs mount
  daemon's own CPU between launch and settle, so NAS rows rank that work too. It reads `unmeasurable`
  when a daemon was not readable for the whole run, and it equals `cpu_tree_s` on local and USB,
  where no daemon is watched.
- **`thumbs_by_format` was the newest column at the END of each row when it landed**, and it is why a count can be
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
- **`ranked` is the column a report must read first.** `yes` is a
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
  other thirteen. `n/a: tui in <terminal>` is the whole TUI group: a
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
  entrant's own processes. A selected entrant that is not installed refuses the whole run at start,
  before any launch; run order still writes an absent row with place `-`, and the launch loop refuses
  it again if it is reached. `tests/field-launch-order.sh` drives the start check and that loop with
  missing GUI and TUI commands and a missing Strata binary, and checks that ONLY excludes an absent
  entrant it does not name.
  Strata provenance parses a Cargo.toml `version = "1.0.0"` line and the binary's `strata 1.0.0`
  output, with the exact formats documented beside its parser. A re-measured row is a row from a different day and the manifest says so
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
  the harness refuses to rank, including a group-wide refusal when the ranked rows differ in
  thumbnail work by ten times or more. On the media fixture the denominator is **1800**: 2000 visible
  entries less 200 `.txt`. It is not 2008; that count came from a seeded fixture an interrupted run
  left behind, and a rebuild produces 2001 with the marker.
- **The TUI group runs as-intended, in kitty, on the bench's own configuration.** foot is sixel
  only while every entrant here that previews at all prefers the Kitty protocol, so the old
  group's `thumbs_n 0` across all eight was the harness's terminal choice and not a result about
  the entrants. `tools/flea-bench-tui/` is the `XDG_CONFIG_HOME` handed to every TUI entrant, copied
  into a `mktemp -d` per run so the operator's own `~/.config` is neither read nor written and the
  repo never collects the state files entrants leave. Provenance per entrant is in that directory's
  README; the manifest records the materialised files verbatim, because lf reads `previewer` as a
  path and does not expand a variable in it, measured, so the run substitutes the real path in.
  **Verified live, one screenshot each: yazi, ranger, lf, nnn and superfile all draw an image.**
  mc, broot and xplr have no image preview feature at all, which is a capability finding and not a
  configuration gap.
- **`preview_ms` is the TUI group's number, and a thumbnail count is not.** A TUI draws the cursor
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
- **The measured group, end to end on the media fixture:** yazi 745 ms, lf 733, nnn 702,
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
- **`KILL_TARGETS` includes nautilus's indexer, `localsearch-3` and its extractor, for the same reason.**
  nautilus activates it over D-Bus on its first launch, and on this box it indexes all of `$HOME`
  unthrottled. After the 2026-09-24 reboot it started at 00:54:52 in the 0.3.5 battery's warm-up pass
  and read 14.5 GB over the next two hours, under every later entrant's cold launch: Flea's scale first
  window read 505 ms against 409 in 0.3.3, and almost every other entrant slowed by 48 to 119 ms. With it idle
  the same trees read 402 to 430 (medians of 12 runs), and a full run with this entry read 407 against
  pcmanfm's 475. A full run refuses to start while it is alive, so stop it first
  (`systemctl --user stop localsearch-3`); the kill list ends it after every nautilus run.
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
- **`cpu_main_s` is the entrant process's own `utime + stime`**, fields 14 and 15 of `/proc/<pid>/stat`
  and not `cutime + cstime`, so no child's CPU is in it. For Flea that excludes the Rust backend and
  every thumbnailer child, so that column compares front ends and not total work; `cpu_s` is the
  whole control group, the same value as `cpu_tree_s`.
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
- The transparent huge page check inside `./tests/modes.sh` launches a stub `flea-gui` that
  prints `THP_enabled` out of its own `/proc/self/status`, so what it asserts is the kernel's
  view of the process `gui::exec_gpui` launched and not the return value of a function. It reddens if
  the `prctl` is removed and it reddens if the call fails; a second check asserts the stub
  reported at all, so a broken stub cannot be mistaken for a passing gate. See "Transparent
  huge pages" under "Deliberate corners".
- The `--open` checks inside `./tests/modes.sh` put a stub on `PATH` that reports
  its own pid, argv count, argv and `THP_enabled`, which is what pins symlink resolution, the
  handoff, a name starting with a dash and a name with a newline in it arriving as one absolute
  argument. That stub's last write is what the read waits for, so it is exiting whatever `--open`
  did and the reaped-launcher check on it cannot go red; a third stub in `lingerbin/` writes its pid
  and then sleeps half a second, which is the arm that can, and it is the shape of the real
  `DBusActivatable` wait in miniature. A second stub in
  `failbin/` exits `3`, which is the check that `--open` reports a refusal instead of the `0` a
  detached spawn reported. All three are named from `src/open.rs`'s own `Command::new` target, and the
  `--terminal` stub from `src/terminal.rs`'s, rather than spelled in the test: when the target moved
  from `xdg-open` to `gio` the hand-written name did not follow, so every `--open` in the block
  resolved the real `/usr/bin/xdg-open` instead, and a stub named from the product cannot drift.
  Issue 41 has its own case beside them, driving the real `gio` against an
  isolated `XDG_DATA_HOME` and `XDG_CONFIG_HOME` holding one `Terminal=true` entry and a stub
  `xdg-terminal-exec`, so the operator's own MIME state is never read or written. The huge page half needs a second stub, because a bare `flea --open` runs in a
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
- The three `--open` error paths each need a check on the sentence, not just the status, because
  the unknown-flag branch also exits 2 and also prints no errno: against the pre-Task-4 binary
  the status and errno checks all pass unchanged, and only `could not be opened`,
  `gio open refused that file` and `nothing on this system could be asked` tell an implemented
  `--open` from an absent one.
- `./tests/budget.sh` asserts `tools/flea-file-budget` itself rejects an oversized
  file and passes a clean tree.
- `./tests/hyprdispatch.sh` keeps compositor command construction in `tests/lib/hypr-dispatch.sh`.
  Its seven typed entry points validate hexadecimal addresses, integer coordinates, nonnegative resize
  dimensions and exact `on|off`, then require the compositor reply to be exactly `ok`. Window calls
  always use `address:ADDR`; move and cursor coordinates may be negative for monitor origins.
  `hypr_window_resize_absolute` and `hypr_window_move_absolute` send `relative = false`, the form the
  xw6 harness always used, where `hypr_window_resize` sends `exact = true` and `hypr_window_move` no
  relative key; every caller of the two absolute forms is in `tests/ui.sh`, the drop captures it
  sources (`tests/ui-captures-dropmods.sh`) and the layer-drop probe.
  The helper's program form serves Python callers and prints `ok` only after the same checks pass.
  The static gate scans shell, Python, QML and JavaScript under `tests/`, removes only full-line `#`/`//`
  comments after whitespace, and outside the exempt files refuses `hl.dsp.` (including a backslash-split
  prefix) and applies rule A first to any physical line containing both `hyprctl` and `dispatch`/`--batch`.
  Rule B requires a file naming `hyprctl` as a whole word to name no whole raw word (`dispatch`, `--batch`)
  anywhere in the comment-stripped file, and reports every `hyprctl` line once, keeping rule A's first report
  for a line.
  The exempt files are `tests/lib/hypr-dispatch.sh` (which keeps its call count),
  `tests/hypr-dispatch-proof.py` and `tests/hyprdispatch.py`.
  A raw word built at run time or taken from another file is outside this static contract.
  The proof runs the real helper against a fake compositor, checks exact text and refusal paths,
  and executes each shell harness's own helper source statement from its directory. It verifies
  the resolved source path of every entry point; missing and foreign helper copies must fail.
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
  table to feed it. It launches no entrant, only two named copies of `sleep`, one in a systemd scope
  named the way kitty names its own, and builds its fixture in a `mktemp -d`. It exists because
  the field bench had no gate at all, and its first check is the defect that found: an entrant row
  arriving indented missed its own case arm, so Flea reported itself as `quickshell 0.3.1`, which
  is what `pacman -Qo` says about its launcher.
- `./tests/keymap-gen.sh` asserts the committed `ui/js/Keymap.js` is byte-for-byte what
  `tools/flea-keymap-gen` produces from `keys.toml`; see "The key table is generated" above.
- `./tests/touchpad-tool.sh` runs `tools/flea-touchpad`'s `create()` and `swipe()` with recorder
  stubs for `fcntl.ioctl` and `os.open`/`os.write`, so the uinput ioctl shapes are gated headless
  with no `/dev/uinput`.
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
- `./tests/js.sh` runs the pure JavaScript suites through `qml6`: every suite under `tests/js/` (the fixture files there are not suites; run the script for the current list). The thumbs suite is 66 lines against a
  77-line helper and pins the four properties the request policy rests on: `plan` names only
  visible rows carrying `t`, it never re-asks a row the map already holds in any of its three
  states, it drops only rows that are still waiting when they leave the viewport, and
  `remember` evicts at the cap in insertion order. It reddens on a mutation because
  `ui/js/Thumbs.js` imports no QML.
- `./tests/uiwriter.sh` drives `ui/ViewState.qml`'s writer under a real headless Quickshell, which
  is the half `tests/js/uistate.js` cannot reach: the book alone never sees a `Process`. It copies
  the singleton and `ui/js/UiState.js` into its sandbox with a two-line `qmldir` rather than
  importing `ui/` whole, because a directory import makes Quickshell scan every file in it and warn
  about the two OEM symlinks a headless run has no session for. It makes two column changes in one
  turn, so one run drives the start, the queue and the drain, and it drives that twice: once with
  `FLEA_BIN` at a path that does not exist, where both patches must end refused and the book must
  drain, and once against `target/debug/flea`, where both must reach `ui.json` and nothing must be
  reported. Without `ViewState`'s `onRunningChanged` arm the first three checks go red and the
  second run stays green, which is the exact shape of the defect it was written for.
- `./tests/ui.sh` drives the real window through `omarchy-drive` and takes a case name to run
  one of the `case_*` functions, defined in the file or in a sourced `tests/ui-*.sh` library
  (`renamelife`, for one, lives in `tests/ui-rename-design.sh`); the file's own usage line lists
  the entry points. With no argument it runs its `wanted` list and
  then three whole-run checks, a backend drain, a log grep and a cache count, so a clean run
  prints `0 of N checks failed`, with N three more than the length of that list. Neither the list nor either
  number is written out here: both went stale the first time a case was added.
  - `open` replaced the old `symlink` case. It puts a stub opener on `PATH`, named from
    `src/open.rs`'s own `Command::new` target the way `tests/modes.sh` names its own, which
    intercepts the `open` subcommand and execs the real `gio` for every other one, and asserts
    all three Enter answers on one listing: a symlink to a file hands the resolved target to
    the handler and does not leave the directory, a symlink to a directory navigates and is
    never handed to the handler, and a broken symlink says one sentence, moves nothing and
    leaves the listing standing.
  - `icons` catches removing the icon slot from `ui/Row.qml` or its theme fallback. Every row
    must answer a non-empty source, a directory and a symlink to a directory must both draw a
    folder, a `.jpg` must draw the image icon, a `.pem` must not draw as an executable, and
    the rendered row pitch read off `itemRect` must still equal `Theme.fileRowHeight`, the row
    the stored density asks for, so a slot that grows its row reddens.
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
    `import` 12 (qmllint fails `qs.Commons`, 2 warnings, for each importer it reaches in `ui/*.qml`
    glob order before `Backend.qml`, today only `ui/ArchiveCompressDialog.qml`; 10 before it), `unused-imports` 0, `signal-handler-parameters` 0, `unresolved-type` 6, and
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

- The UI has no accessibility tree, so the one `IpcHandler` in `ui/WindowBody.qml` is the seam
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
- The `pane-function-value` gate in `tests/staticgates.py` rejects a depth-1 Pane function read that stands beside `===`, `!==`, `==` or `!=` without its call parentheses, while call, callback-argument and `typeof` guard reads pass.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
