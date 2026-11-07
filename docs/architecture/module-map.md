## Module map

- `main.rs` dispatches on argv, and this is every flag it matches: `--backend` runs the command
  loop, `--prewarm <path> <first> <dest>` writes the prewarm file, `--open <path>` hands one file
  to the desktop's handler, `--terminal <dir>` opens the configured terminal there,
  `--run <path> <dev> <ino> <size> <mtime_s> <mtime_ns>` and `--run-terminal` with the same identity
  start the card's program outside or inside a terminal, answering `0`, `2`, `6` for a file changed
  since the ask, `7` for a noexec volume or `8` for a spawn failure, `--open-text <path>` opens
  one file in the desktop's text editor through the resolved desktop file rather than its bare id,
  `--open-handler <path> <desktop id>` with the same identity hands an AppImage to the handler the
  card named,
  `--update [check]` asks the installing source for a newer Flea or opens Omarchy's updater,
  `--default [off]` claims or releases the OS-level default, the "Show in folder" registration and
  the chooser routing together,
  `--youleftmeforstrata` is the undocumented second spelling of `--default off`, `--picker [off]`
  claims or releases the desktop's file chooser alone, `--pick <reply>` opens one chooser window
  for `tools/flea-portal`, `--ui-state [<patch>]` reads or merges the shared view state,
  `--unlock <device>` opens the unlock dialog over the window for one device, see "Modes",
  `--launch-warm <list> <gvfs-path> <gvfs-dest>` runs both launch jobs under one fork, "-" skipping
  one, which `gui.rs` starts and `tui/warm.rs` starts with "-" for the list, see "The first window",
  `--figure-helper` runs the sandboxed quickjs-ng figure renderer on stdio, see "Markdown figures",
  `--version` prints the version, `--print-target` resolves `--select`'s pair for the tests, and
  anything else opens the window, on `--select`'s parent directory when one is given, unless
  explicit `--tui` requests the terminal interface, `--gui` being the explicit spelling of the
  window a bare invocation already means, see "Modes".
- `paths.rs` resolves the retained QML data directory and whether a display is available; the GPUI window launch checks the display and resolves its executable separately.
- `gui.rs` re-exports the GPUI launcher in `gui/launch.rs`, which execs `flea-gui`.
- `crates/flea-gui/src/window/root.rs` is the GPUI window root. It holds the shell as one cached view and paints
  each pane's loading mark over it in an overlay, inside the content mask the mark's slot recorded, so a narrow pane
  cuts the mark at its own edge. A surface that covers a pane (Quick Look, menus, sheets, dialogs) paints in a later
  layer than that overlay, a GPUI `deferred`, so it is never drawn under a mark.
- `thp.rs` the `prctl(PR_SET_THP_DISABLE)` declaration behind `disable()` and `enable()`.
- `icdexclude.rs` filters known Vulkan drivers for absent hardware before `flea-gui` starts; see rule 5.
- `icdmanifests.rs` supplies that filter's bounded manifest inventory and driver library classification.
- `gui/gpui_renderer.rs` clears Qt renderer settings and an inherited legacy display pin, then applies the absent-vendor filter without adding a display pin or probe cache.
- `open.rs` hands one file to `gio open` and waits for it, see "Opening a file".
- `terminal.rs` hands one directory to `xdg-terminal-exec --dir=` and does not wait, see "Opening a file";
  its `detach` is the set of guards every program Flea starts and does not wait for carries.
- `tearoff.rs` names the three tab tear-off variables and removes them from a child's environment, for `detach` and `open.rs`.
- `unlock.rs` `flea --unlock <device>`: validates the device and display, then calls `gui::exec_gpui_unlock` to open the GPUI window with that one unlock request.
- `figurehelper.rs` `flea --figure-helper`: maths and diagrams through quickjs-ng under the
  thumbnail jail's flags and caps, see "Markdown figures".
- `update.rs` `flea --update [check]`: the install kind, the source's answer, and Omarchy's updater, see "Updates".
- `defaults.rs` claims or releases the OS-level default: the desktop-entry install check,
  the `inode/directory` MIME default via `xdg-mime`, and reporting each half, see "Modes".
- `hyprkeys.rs` adds or removes the additive, markered block in Omarchy's
  `~/.config/hypr/bindings.lua` that binds the two file-manager chords to Flea, see "Modes".
- `userfile.rs` resolves `$HOME`, `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME`, walks the XDG data
  ladder for an installed file, and rewrites a per-user file through an exclusive temp plus
  rename, see "Predictable path writes".
- `error.rs` the one error type, naming the failing operation and input.
- `json.rs` the wire's JSON: read one named field out of one line, escape one string into one.
- `jsondoc.rs` one whole JSON document in and out, which the one-line scanner above deliberately is not.
- `jsonstring.rs` one JSON string in and one Rust `String` out: the escapes and the surrogate pairs.
- `jsonmembers.rs` one member of a JSON object found as the Vulkan loader's cJSON finds it: any case, every repeat seen.
- `uischema.rs` the shipped `ui.json` shape and the rule each key is measured against.
- `uistate.rs` the `ui.json` merges: a file onto the defaults, one caller patch, and 0.1.3's `view.json`.
- `uimigrate.rs` the one-time changes a stored `ui.json` is owed, applied on read and recorded by its
  `stateVersion` stamp, see "The state file".
- `uistatecli.rs` is `flea --ui-state`, the window's path to read or patch `ui.json` (the TUI writes
  through its own `uistore::Store`), and the shelf switch that follows a patch.
- `uistore.rs` where `ui.json` lives and the one locked, atomic way it is rewritten, see "The state file".
- `backend/mod.rs` is module declarations and nothing else, the `#[cfg(test)]` ones included. It
  declares more modules than the list below names, which is the load-bearing ones and not a census.
- `backend/listing.rs` the arena-backed `Listing`.
- `backend/aliases.rs` resolves a MIME alias to its canonical name, see "MIME aliases".
- `backend/scan.rs` phase 1: readdir plus `fstatat` for typeless entries only.
- `backend/sort.rs` parses the sort key, holds the name order, and holds the one sentence a key it does not know answers with.
- `backend/meta.rs` phase 2: stat a row range for a `window`, or every row for a size or date sort.
- `backend/metasort.rs` the size and date orders: sorts an index over the metadata pass, then
  gathers the spans behind it, see "Two-phase listing".
- `backend/owner.rs` resolves a uid to its login name from `/etc/passwd` alone, for the meta reply.
- `backend/mime.rs` resolves a file name to a MIME type from the shared globs2 database.
- `backend/icons.rs` resolves a MIME type to a freedesktop icon name from generic-icons.
- `backend/md5.rs` computes the thumbnail cache's filename digest, see "Thumbnail cache".
- `backend/thumbspec.rs` parses the `.thumbnailer` files on the search path into a table of
  validated specs, see "Thumbnailer specs".
- `backend/thumbargv.rs` turns one spec plus one input into the exact argv a child is exec'd
  with, see "Thumbnailer specs".
- `backend/thumbcache.rs` names and reads entries in the shared thumbnail cache, see
  "Thumbnail cache".
- `backend/thumbwrite.rs` creates the temp, stamps the PNG and writes the fail marker.
- `backend/sandbox.rs` wraps a thumbnailer's argv in bwrap and prlimit, and the extract's argv in
  that same jail without the CPU cap, see "Thumbnail sandbox".
- `backend/child.rs` runs one argv under a deadline and says whether it succeeded, failed or
  never started, which is the whole of what decides a `fail/` marker, see "Thumbnail pool".
- `backend/thumbs.rs` the bounded, cancellable thumbnail pool, see "Thumbnail pool".
- `backend/proto.rs` the wire types, the request dispatch and the one-line responses.
- `backend/rows.rs` serialises one window of rows and its per-response Kind dictionary.
- `backend/rowguard.rs` stamps each rows line with its listing's numbering and refuses a trash,
  transfer, paths, collisions or menu snapshot that names an older one, see "The listing swap".
- `backend/thumbreq.rs` the thumbnail request policy: cache lookup, queueing, cancel and
  result reporting, see "Thumbnail requests".
- `backend/run.rs` the command loop, see "Thumbnail requests". stdin is read on its own
  thread and the pool answers on its own channel, and a forwarder thread joins the two, so
  client requests and worker results arrive on one `recv`, because `std` has no `select`.
- `backend/dirsizeworker.rs` runs one cancellable size job at a time, with generation-checked
  replies and an explicit 16 MiB stack for the recursive walker. Navigation never joins it.
- `backend/events.rs` the `Event` those sources arrive as, and the forwarding threads that join
  them onto the loop's one channel. It came out of `run.rs` at 398 of the 400 hard cap, and it is one
  job: how work reaches the loop, as against what the loop does with it.
- `backend/watch.rs` the one inotify watch on the directory the current listing came from, its
  reader thread and the `changed` line it answers with, see "The open directory is watched".
- `backend/copyfile.rs` the copy primitives every transfer is built from: streaming, preserving symlinks and refusing to overwrite.
- `backend/copyread.rs` the copy source reader: at most four FUSE reads use workers, with Cancel polled every 25 ms while a source read is stalled.
  Local sources read directly, as do FUSE sources when the worker cap is full or a spawn fails.
  A cancel still unlinks the partial destination synchronously, so a destination on a device that stops answering waits for it, as in 0.3.10.
- `backend/copystage.rs` where a copy's bytes wait until whole: a new folder under one hidden sibling renamed once, a file as an `O_TMPFILE` inode named by `linkat` or, where that is refused, under a hidden sibling.
- `backend/copysweep.rs` the sweep of dead copies' hidden files and folders before a transfer lands in a folder.
- `backend/undoshare.rs` the one undo history every backend in the session shares, under
  `$XDG_RUNTIME_DIR/flea/` with an flocked lock file and predictable-path writes; a take is one
  locked read-modify-write, so two windows pressing Ctrl+Z undo two different operations.
- `backend/undocodec.rs` that journal's versioned wire form and its read-time checks, both directions.
- `backend/undostage.rs` that journal's file text, in and out: `read` parses the file once (a document, a newer
  writer's file, or garbage) and `Staged` renders each entry once, so trimming to the cap sums piece sizes and a write
  is a join. `backend/undorebase.rs` is `RebaseMap`, one walk of the document for a whole batch of moves.
- `heap.rs` pins glibc's mmap threshold for the backend, keeps the TUI to one arena and gives the TUI's freed heap pages
  back after a picture is sent (`give_back`), see "The listing arena returns to the OS".
- `launcher/mod.rs` re-exports `prewarm`, nothing else.
- `launcher/prewarm.rs` writes the listing and first screenful before the UI starts.
- `ui/boot/shell.qml` is the retained QML reference entry, with its window, geometry readers and
  scene-graph error path; the GPUI launcher does not load it, see "The first window".
- `ui/WindowBody.qml` owns everything inside that QML reference window: the view, the startup path, the quit
  handshake and the read-only IPC seam. It is loaded by file: URL, so it has no qmldir line.
- `ui/RendererRetry.qml` is the QML reference OpenGL retry helper, loaded by file: URL only once
  that reference scene graph has failed.
- `ui/Backend.qml` is the only QML component that talks to the Rust child, and carries
  `thumb` and `thumbcancel` out and `thumbed` in alongside `list`, `window` and `sort`.
- `ui/ViewState.qml` reads `ui.json` once at startup with a blocking `FileView` and writes nothing
  itself: every change, the header menu's columns and all three settings sections alike, goes back
  out through `flea --ui-state` as a patch naming that change alone, see "The state file". It also
  watches the file and applies another window's settled preferences live through
  `UiState.applyExternal`, owning the apply/prune settler whose exit and collected text join the
  landed/whole way before anything is parsed.
- `ui/js/UiState.js` is `ViewState`'s writer bookkeeping and the two pure rebuilds every writer goes
  through: the newest patch a writer landed, what the running writer carries and what waits behind
  it, and the key and group rebuilds `ViewState` runs over both the state it draws and the patch it
  owes. It also owns the refusal prune (`invalidKeys`, `dropInvalid`,
  `revertedState`) and the settle join and order (`landed`, `whole`, `pruneAsk`, `settleNext`).
  It imports no QML, so
  `tests/js/uistate.js` can redden on a mutation of the rule that only a zero exit proves a save.
- `ui/Theme.qml` owns the singleton palette, type and spacing tokens from the Omarchy
  theme plus the user override.
- `ui/Pane.qml` owns one directory view, its integer model, held window, actions, the
  settle timer and the row-to-thumbnail map, see "Thumbnail requests in the GUI".
- `ui/PaneWire.qml` is where every reply from outside the window lands: the backend's, and
  those of the three foreign programs the pane runs (`ui/Opener.qml`, `ui/ShareLink.qml`,
  `ui/Taildrop.qml`); it writes through the pane handed in and owns only the state a reply needs
  before the pane has a row for it: the folder a `made` line will open an editor on, and the
  watched re-read's debt, timer and cursor anchor, see "The open directory is watched".
- `ui/PaneSwap.qml` is where a listing's `listed` and `rows` replies land, and holds the old rows
  on screen until they do, see "The listing swap"; `ui/js/Swap.js` makes its decisions.
- `ui/Header.qml` renders the column header band and its rule, and owns nothing else: it
  was lifted out of `Pane.qml` at the 400-line hard cap and has no behaviour.
- `ui/Row.qml` renders one row delegate: the icon slot, which a thumbnail replaces in
  place, the PlainText name and the semantic colours.
- `ui/Opener.qml` is the only component that runs Flea's own opening modes, by running
  `flea --open`, `flea --terminal` and a one-line `sh` that pipes into `wl-copy`, which is why
  `wl-clipboard` is a `depends` entry, see "Opening a file".
- `ui/ContextMenu.qml` is the one pane-owned right-click popup and its single Open action.
- `ui/StatusBar.qml` renders the path, row counts and transient messages.
- `ui/TabBar.qml` is the window's tab strip, hidden with no height until a second tab exists.
- `ui/ChromeBar.qml` renders the top chrome, and owns the path bar: the same strip typed into
  rather than drawn, opened by `:`, `Ctrl+L` or a double click on the path.
  Decided 2026-10-03, comparing one focused dialog field drawn three ways (0.3.6, 0.3.7's ring, the 0.3.8 stage's cut ring):
  0.3.6's drawing is the correct one. **Fields, an accent
  hairline frame as 0.3.6 drew it; buttons, their own frame.** Every text field's focus is its own one hairline frame
  in `Theme.color.accent` (the error role in `ui/RenameField.qml` while the pane holds an error), the frame is
  `Theme.color.muted` while it does not hold the caret (`ui/DialogField.qml`, the dialogs' own fields, the
  picker's save name), and no field draws a ring. This replaces DESIGN-040's "A focused field keeps its muted
  frame and a foreground caret". The 2 px foreground frame (`Buttons.RING`) is a button's own
  frame drawn inward where the hairline cannot show focus, and nothing that takes text draws it. The path field
  keeps the strip geometry 0.3.6 and 0.3.7 drew (two hairlines above, one below, the flush Jump dropdown), a list row keeps
  `Theme.fileRowHeight` while its editor is open, and the rename editor's muted extension patch lies a hairline
  inside the frame so all four sides are whole. The rename editor is one field in every view (decided 2026-10-03):
  `ui/RenameField.qml` owns its height (`lineBox`, the row's text line box, or the typed line and its two hairlines
  where a small stop is shorter), and a host only places it, centred on whole pixels, never sets another height and
  never covers a neighbouring cell wherever its row can hold it, as `tests/rename-frame.sh` pins for the list, dual,
  columns, grid and rail: in every host whose row or cell is at least the frame's height, the mapped frame stays inside
  it and overlaps no other row, cell or column drawn beside it. At a stop where the row is shorter than the frame, the
  frame overhangs the row by the same amount top and bottom, and so reaches that far into the rows beside it.
  The stem selection is one box filling the frame's interior edge to edge (`RenameField.selectionBox`, the field's own
  is transparent), the grid centres the field on its caption's first line, and in the columns the renaming row grows by
  the error line as the list's does (`ColumnRow.errorGrowth`), the line running from the label column to the column's
  right inset. A button's focus frame is never cut: `CardScroll` carries `bleed`,
  room inside its clip that the caller's margins give back, on every dialog that holds a `Flea.DialogButton`
  or `Flea.CheckBox` (all of them: Collide, Trash, menu action, open with, convert, permissions, network and
  the save picker). `CardScroll.reveal()` keeps `bleedY` of clearance on the side an item is revealed from, so
  the first control returns the view to the top and the last to the end with its frame whole (a menu sets no
  bleed and lands flush as before). `tests/ring-bounds.sh` pins all of it from the real window at every text
  stop: the content positions on whole pixels after the card snap, in `tests/ring-bounds.js` (every key compared once, every other
  dialog listed unpinned with its reason), no ring on any button and every button's own frame inside each clip,
  and a scrolled probe card revealed last, first, last. Every button's focus frame draws inside its control,
  but a focused filled check box draws its keyline two hairlines outside its box, so `bleed` and
  `Theme.focusClearance` stay: dropping both together would move no control (the pins hold) but would cut that
  keyline at a body's edge and shrink each card's clip and reveal room. Only the rename editor has an error frame, so only it is driven in the error role.
  `tests/ui.sh` `case_click` reads the open path field's accent frame, its four sides and the two pixels
  outside each side from the native chrome capture, counting a pixel as an ink within `chrome_ink_tolerance`
  of its theme hex (the window composites over a translucent ground) after asserting accent, foreground and
  the ground are further apart than twice that.
- A dialog card lands on whole device pixels, because a hairline drawn at a half pixel renders at half strength.
  Every card that centres itself (`ui/MenuActionDialog.qml`, `ConvertDialog`, `NetworkDialog`, `OpenWithDialog`,
  `TrashConfirm`, `CollideConfirm`, `KeymapSheet`, `SettingsPanel`, `PermissionsDialog` and the `Preview` surface) takes
  its size from `Theme.cardSpan(want, room)` (the want rounded up, inside the room) and its origin from
  `Theme.cardOrigin(room, span)`, set as `x` and `y` and never `anchors.centerIn`; NetworkDialog and Preview add their
  `rise` to `y`. `tests/permissions-adv.sh` reads the helpers in odd and even rooms and the Permissions card's mapped
  rectangle at every text stop in an odd window, `tests/ring-bounds.sh` reads every opened card's mapped rectangle in
  its even 1000 x 650 window (a subject with no `cardItem` fails unless it lists why it is no card), and
  `tests/js/permissions.js` pins that each card's own bindings, children excluded, use the helpers and never `centerIn`.
  The Permissions card is the board's 480 at base size 14 (`round(480 * bodySmall / 13)`), its strip sits inside the
  card's border like the board's title, its bit columns are whole thirds, and its note is a 1.5 x caption line box
  with the glyphs centred in it (`lineHeightMode: Text.FixedHeight` plus a top padding, as CSS line-height centres).
  The mixed check box draws the board's bar as a rectangle (8 x 2 at an 18 px box, whole pixels, the check's cut-out
  ink), not the `minus` glyph at stroke 3.
- `ui/PathJump.qml` is the path bar's folder jump, the Jump board: a name typed into the bar lists
  matching folders from Flea's favourites, zoxide's ranking and `recently-used.xbel` in one ranked
  dropdown under the field, and Enter opens the cursor row. `ui/JumpPath.qml` draws one row's path.
  `ui/js/Jump.js` ranks the rows in this order: a match on the folder's own name, then the
  more contiguous match, then zoxide's frecency, a favourite winning a tie, five rows a source.
  `ui/js/Fuzzy.js` ports `backend/fuzzy.rs`, whose exact scores `tests/js/jump.js` and the Rust test
  `the_exact_scores_the_jump_port_mirrors` share, value for value. The history is read once and kept,
  and a `FileView` that never loads it re-reads it only after the file changes.
  `backend/jump.rs` answers the three sources once per open of the bar, see docs/protocol.md "jump".
  A recent file stands for its folder only after that folder resolved, checked through `existing()` on
  the shared budget under its own mount key, so a wedged recent file costs its row and the recent rows
  after it, never the answer or the other two sources.
  An open behind a slow zoxide, its run still in flight or past its limit, draws the last ranking that
  answered in time, empty on a first-ever open.
  A line with a slash in it, starting with `~`, or `.` or `..` alone, is typed as a path exactly as
  before the jump, and so is every line once Tab has completed it, so "Wo", Tab, Enter still opens
  `./Work`; so is a name that matches nothing. A name's Enter before the backend has answered is held
  for the answer, the keys after it are not typed, and each open's answer carries its own id, so the
  same keys open the same folder however fast they come. `tests/jump-ui.sh` drives all of
  this through the real bar, offscreen.
- `keys.toml` is the one key table, and `tools/flea-keymap-gen` turns it into `Keymap.js`.
- `ui/js/Keymap.js` is the generated key-to-action lookup and imports no QML.
- `ui/js/Format.js` is the pure size, date, permission and name-elision formatter.
- `ui/js/Errors.js` turns a backend failure into the one sentence the status bar shows, and
  imports no QML; it was lifted out of `Pane.qml` at the hard cap and has no behaviour of
  its own.
- `ui/js/Thumbs.js` is the pure row-to-path map and the visible-row request plan, and
  imports no QML so `./tests/js.sh` can redden on a mutation.
- `ui/js/Tabs.js` is the directory-tab snapshots and the t/w/1-9 policy, and imports no QML. A
  snapshot carries a cursor and a selection only across a switch that re-lists nothing, because an
  index names a row and a re-read can put a different file behind the same number.
- `ui/js/Trash.js` is the dd pair's arm-and-fire policy, split out of `Focus.js` at its cap.
- `ui/js/Collide.js` is the paste-collision card's pure decisions: its words, where h, l and Tab move
  its focus, and the question and transfer it shapes. `ui/CollideHost.qml` holds the transfer waiting
  on the answer and `ui/CollideConfirm.qml` draws the card; `backend/collide.rs` answers the question
  and applies the choice, see "Write operations and the undo journal".
- `ui/js/TextSize.js` is the Display section's text size: the seven stops the SettingsScale board
  documents, 9, 10, 11, 12, 14, 16 and 20 px, the two modes it names, and the sentence a chord
  announces. `ui/ViewState.qml` stores `display.textSize` as `{"mode":"system"}` or `{"mode":N}`,
  which is `src/uischema.rs`'s own rule for that key and the only stored shape, and owns
  the writers, `ui/Theme.qml` derives its whole token ladder from the size in force, and `keys.toml`
  aliases `textSizeUp`, `textSizeDown` and `textSizeReset` onto the same three writers, so no
  surface reads the chord itself and a chord and a row cannot hold two different sizes. The monitor
  scale is the compositor's: `ui/Theme.qml` reads it once from `hyprctl monitors -j` and the panel
  shows it read-only, because the board rules that Flea does not step or cycle it.
- `ui/js/Settings.js` is the settings panel's whole model: the three sections, the context-menu
  action inventory and its groups, the tri-state master over the six basic actions, which
  `masterState` derives from `menu.hidden` rather than storing beside it, the Shortcuts group's
  single `keyHints` row, which is the one check that is not a menu action and so is never in
  `MENU_GROUPS`, and the row list each section draws. Pure, so `tests/js/settings.js` drives every control without a window.
  `ui/SettingsPanel.qml` paints what `rows()` returns and owns the panel's two-sided keyboard,
  `ui/SettingsRail.qml` the section rail and `ui/SettingsRow.qml` one row of the pane.
  `ui/SettingsPane.qml` builds View and the open section's rows at the first open and the rest one section per 150 ms
  tick (`idleBuild`), so the cold open builds about 2.3k objects, not 11.9k; a section shown first builds when shown and
  none is torn down. `tests/settings-tail.qml` pins the first-open build, a show of a section the held walk has not reached, and the Columns subpage keeping every section column.
- `ui/js/SettingsAbout.js` is the About section's rows, the Updates group among them; `ui/js/Settings.js`
  routes `rows("about")` to it.
- `ui/UpdateCheck.qml` is the updater singleton: the check and launch processes and the six hour poll,
  see "Updates". `ui/js/Update.js` is its state and every word the row, the note and the footer use.
- `ui/DefaultClaim.qml` is the Make Flea the default singleton: the handler read, the entry probe,
  the `flea --default [off]` run and the portal restart after it. `ui/js/MakeDefault.js` is its state, the row, the note and what a press
  runs; see "Make Flea the default, in Settings > About".
- `ui/js/MenuRefresh.js` is where the menu's keyboard cursor lands when a provider refresh rebuilds the
  rows under it, and `ui/MenuEdgeFade.qml` is one edge of a menu that scrolls; both came out of
  `ui/js/Menu.js` and `ui/ContextMenu.qml` whole, see "File budget".
- `ui/js/Menu.js` `applyHidden` is the only consumer of the stored hidden set, and it runs at the
  end of `listingEntries`, so a switched-off action leaves every menu that carried it at once.
  `open` and `toggleHidden` are refused there as well as drawn locked in the panel, so a
  hand-edited state file cannot empty the menu.
- `ui/js/PreviewKeys.js` is what the preview overlay does with a key, and the 5 s seek step only it
  reads, split out of `Focus.js` at its cap the second time it reached one.
- `ui/js/RailKeys.js` is what the rail does with a key, split out of `Focus.js` at its cap the third
  time it reached one; `Focus.handleKey` calls it directly, as it already called `PreviewKeys`.
  Enter or `l` on a row (#181) arms `Sidebar.focusOnOpen` and moves focus into the folder only when
  its open lands through `openFrom`, so a mount, or a share whose `gio info` is slow or never answers,
  leaves focus on the rail rather than in the folder it came from.
- `ui/js/Menu.js` is what either menu holds and where its frame sits: the submenu test, the
  edge clamp, the listing and header row lists, and `openAtCursor`, the keyboard's own entrance,
  which came out of `ui/Pane.qml` when PR 34's `openTerminal` would have pushed it past its cap.
- `ui/js/Scale.js` is the interface scale's step, clamp and sentence; `ui/ViewState.qml` holds it
  for the window and `ui/Theme.qml` multiplies its own tokens by it, so no surface reads the chord
  itself. It is a session value: the state file stores an Omarchy text-size stop and no multiplier.
- `ui/js/PathBar.js` is what a typed path line means: the tilde, the relative name, the
  `file://` URI, the interior `.` and `..`, and what Tab makes of one directory's names. Pure,
  so `tests/js/pathbar.js` drives all of it; the field itself is `ui/ChromeBar.qml`'s.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
