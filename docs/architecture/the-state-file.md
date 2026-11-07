## The state file

`~/.local/state/flea/ui.json`, or `$XDG_STATE_HOME/flea/ui.json` when that is set and not empty, is
the one file Flea writes for itself. `src/uischema.rs` holds the shipped shape and every default,
copied from the 0.1.4 build handoff; `src/uistate.rs` holds the merges; `src/uistore.rs` holds the
paths, the lock and the write. Two defaults have moved since that handoff. Row density is `compact` from
0.3.2, a deliberate change. Every write stores the whole document, so a state file any earlier Flea wrote keeps
the `normal` it recorded, and only state that never stored a density takes the new default.
`tests/ui.sh` case `icons` launches with no `ui.json` at all and pins those rows below the board's.
Settings > Places > "Show unmounted drives", `places.showUnmounted`, is on from 0.3.3, also deliberate,
and that one reaches the files earlier Flea wrote as well, once, through the `stateVersion` migration below.

**One lock, and every write goes through it.** `flea --ui-state` prints
the merged document and writes nothing. `flea --ui-state '<json object>'` merges that patch through
the lock and prints the result. The window reaches it from `ui/ViewState.qml` through a `Process`;
the terminal interface (`flea --tui`, `src/tui`) holds a `uistore::Store` of its own and writes the same
file under the same lock, patching its own keys only, because menus and places are the window's.
Scale is on neither list: `src/uischema.rs` has no `scale` key at all, it stores an Omarchy text-size
stop under `display.textSize.mode`, and the multiplier `ui/js/Scale.js` applies is a session value.
**The streams and the status are the contract**, because
that `Process` reads the status alone: either shape prints the whole document on stdout and exits 0,
and every refusal, a patch that is not JSON, a key or value this Flea does not take, a state file it
could not write, or more than one argument, prints one `flea: ` sentence on stderr, prints nothing
on stdout at all, and exits 2. Pinned in `tests/uistate.sh`, both streams for each of the four.

**A patch names what that window changed, and nothing else.** `ui/ViewState.qml` holds a second
document beside the one it draws from, `unsaved`, built by the same two rebuilds and starting empty;
`patch()` is that document. It used to render a snapshot of all four keys the window owns, and the
lock cannot save that: the merge in `src/uistate.rs` protects a key the caller leaves OUT, and a
snapshot names every key explicitly, so a window that changed only the text size wrote its startup
read of `keys` back over a change another window made after that read. The lock was working
correctly and the data was still lost. Running the rebuild over both documents is what keeps them
honest in the other direction too: `changeLeaf` merges a leaf into the group the window DRAWS, so a
sub-key a newer Flea left in `display` or `menu` stays on screen, and owes the leaf alone, because
this Flea has no rule for that sub-key and `check()` refuses a whole patch that names one. The owed
document is a union rather than the newest change alone, so a change made while a writer runs
coalesces with whatever is queued behind it; a refused write keeps it owed so the next patch carries
it again. **A write that LANDS takes its own settings back out, leaf by leaf, the moment it exits**,
and never when the queue happens to drain: waiting for the drain left a setting that was already in
the file still owed, so it rode along inside the patch queued behind it and overwrote whatever
another window or the CLI had put there in the meantime, which is the same lost update one step
later in time. The queued writer is therefore launched with what is owed when it starts rather than
with the bytes that were waiting, and a setting whose value changed again while the writer that
carried the old one ran is NOT taken out, because the file does not have the new one. A setter that
lands the value already held owes nothing at all. `tests/uiwriter.sh` drives two windows over one
state file both ways round and holds a queued writer at the door while the CLI writes under it, and
`tests/js/uistate.js` pins the patch bytes.

**The three map-valued keys merge per entry.** A `folderSorts` or `folderViews` patch
entry names one folder and a `columnWidths` patch entry names one column edge, so one
window's write keeps what another wrote since its read. A null `folderSorts` or
`folderViews` entry forgets that path instead, and only a patch may send one. A re-sort
or re-view moves its folder to the newest end of its map. Past either folder map's 500 cap
the oldest entries go and the newest stay.

With per-folder sorting enabled, Downloads, Pictures and Videos open in Modified order, newest first, unless their folderSorts entry chooses another order. These paths use user-dirs.dirs under XDG_CONFIG_HOME (or ~/.config), with the conventional XDG names for missing roles; a role pointing at Home is excluded. Browser listings, column peeks and the file chooser resolve the same defaults without storing an entry or using a slot in the 500-folder cap. Explicit Name ascending remains a stored choice. Turning per-folder sorting off uses the global order instead. Other folders and the terminal interface retain their existing sort behavior.

**The window's read is the settled file, and not a raw one.** `main()` calls `Store::settle` before
it hands off to `qs`: an empty patch through the same lock and the same per-key validation, so
whenever that settle succeeded on a document it could read, a value a hand edit left in a key this
Flea knows has already fallen back to its own default by the time `ui/ViewState.qml`'s `FileView`
reads it. A key this Flea does not know is not one of those: `merge` keeps it verbatim, so a newer
Flea's settings survive the settle rather than falling back to anything. Without the step the two
front ends answer one file two ways, because the window's read is a `JSON.parse` and applies no rule
of its own. The settle can fail, on an unwritable state directory or a `ui.json` that is a link, and
then `main()` prints one line and opens the window anyway on a file it did not validate. **The
migration is `read()`'s and not `settle`'s**: `read()` falls back to `view.json` exactly while
`ui.json` is absent, so every path that reads reaches it, `flea --ui-state` included. Measured, a
first `flea --ui-state '{"view":"grid"}'` on a box with `view.json` and no `ui.json` writes the
migrated columns, the same ones the settle writes, and leaves `view.json` byte for byte. `settle` is
only where the migration is first written down on a launch that never patches anything.

**A launch never spends the file to settle it.** A launch with neither file writes nothing, the way
a first run always has. Neither does one whose `ui.json` already reads back as exactly what a
rewrite would render, because that rewrite would change nothing: the lock, the temp, the `sync_all`
and the rename measured 6.7 to 18.6 ms a launch on this box, against 1.3 to 1.7 ms for a launch that
only reads, which is real money against a 77 ms startup. Both are per-launch times of `flea --gui`
driven to the missing shell, 100 launches a pass over eight passes of two harnesses that differed
only in how `ui.json` was reseeded between launches; the read figure has the separately timed reseed
subtracted, and the write figure's own eight passes spanned that whole 6.7 to 18.6, which is why
each is a range here and never a number to cite. And neither does a launch whose `ui.json` cannot be
read as a JSON object at all: that file is the only copy of whatever the operator wrote, both front
ends already read it as the full default shape, and rewriting it would spend the operator's
settings to close nothing, so it is left byte for byte and `ui/PaneWire.qml` says once that what is
on disk was not used. **That is the settle and not the file.** `read()` answers the full default
shape for such a document, so the first patch the window sends merges onto those defaults and
renames them over the operator's only copy, every hand-written line included. Deliberate, because
the sentence has already been posted and a save has to land somewhere, and pinned in
`tests/uistate.sh` so the next change to `update()` is visible rather than silent.

**A `ui.json` whose bytes cannot be read at all is refused, and that is a different case.** The
paragraph above is a document this can read and cannot parse. A regular `ui.json` whose bytes
`read()` cannot get at all is the third instance of the data loss this release closed twice
already: `read()` answers the shipped defaults for it too, and `update()` used to rename those
defaults over the only copy of what the operator wrote. `chmod 000` is one way in and one
`sudo flea` is the other, because the rename needs write on the directory and never read on the
file. `read()` still never fails, because a front end reads it before the first paint; the
distinction lives in `refuse_a_bad_target`, which `write()` calls before it creates the temp, so a
link, a device and a file this cannot read are all refused with one sentence naming the path and
nothing is renamed over any of them. `tests/uistate.sh` pins both ways in, the exit status, the
sentence and the file byte for byte after, with a first run beside them to prove the guard did not
close the write that legitimately creates the file; `tests/ui.sh` `settings_read_refused` drives
the same file through the window, where the operator is told twice, once by `ui/PaneWire.qml` that
what is on disk was not used and once by the status bar that the setting was not saved.

**A refused write reaches the operator.** `ui/ViewState.qml` records a patch as stored only when
`flea --ui-state` exits 0. `ui/js/UiState.js` holds that bookkeeping, and `ui/PaneWire.qml` turns
the failure into the status bar's one transient sentence through `ui/js/Errors.js`, the same slot
every other write operation reports in. The status alone is read and not the child's stderr,
because a `Process`'s own `onExited` can race its `StdioCollector`'s text. A patch that was refused
is attempted again the next time the same toggle is made, rather than short-circuited by a book
that already believed it. **A writer that never starts is that same refusal.** Measured on
Quickshell 0.3.1: a `Process` whose program cannot be run emits no `exited` at all, only `running`
going false, so `ui/ViewState.qml` reads that transition as a status of 2. Without it the patch
stays in flight for the life of the window, every later change queues behind it unwritten, and the
one failure this module exists to report is the one it cannot report. A real exit clears the book
before its own `running` goes false, and a restart inside `onExited` emits no false transition at
all, so the handler cannot fire for a writer that ran. `tests/uiwriter.sh` drives both.

**The lock is the sibling `ui.json.lock`.** `File::lock()` is `flock(2)` here: advisory, exclusive
and cross-process, held across the re-read, the per-key validation, the caller-key merge, the temp
write and the rename, so a second Flea cannot land between this one's read and its write. Measured
on the box with the lock taken out, twelve concurrent writers landed 1 of their 12 keys; with it in,
12 of 12. `update()` makes the state directory and takes the lock before `patched()` validates, so a
patch this Flea refuses leaves both of those behind and only never writes the state file itself. The
cost is an empty `~/.local/state/flea/` and a lock the next accepted save uses anyway, which is why
the check says what it leaves rather than the code being moved to leave nothing.

**Failure is per key, and per file only when the document does not parse at all.** That one case
is per file by construction: a document that does not parse reads as the full default shape rather
than throwing, and is left on disk exactly as it was rather than rewritten into that shape, so every
key in it is lost to the read at once. Every other failure costs one key: a value a key cannot take
costs that key alone, and every other key in the file stands. A key this Flea does not know is kept
and rewritten as it was read, at the top level and inside a nested object, so an older Flea cannot
eat a newer one's settings. A patch is the
other way round: it is checked whole before any of it lands, and one bad key refuses the whole patch
with a sentence naming it, because a patch comes from Flea and not from a text editor. **A number is
measured against JSON's own grammar and not Rust's**, because `Json::Num` keeps the literal it was
read with and writes it straight back: `1.`, `1.e5`, `0192`, `-0192`, `01`, `00` and `-.5` all parse
as an `f64` and none of them is a JSON number, and accepting one would have let a settle rewrite
`ui.json` into a document the window's own `JSON.parse` refuses, after which every setting reads as
default. The shape check is `is_json_number` in `src/jsondoc.rs`, run before the `f64` parse and not
instead of it: the parse is what bounds the magnitude, since `1e400` is a JSON number and no finite
`f64`. **A `\u` escape is four hex digits and nothing else**, each digit checked on its own, because
`u32::from_str_radix` takes a leading `+` for an unsigned type as well and `\u+041` decoded to `A`:
the same divergence as the number grammar, one escape further in. **Every parse error names a
byte**, the four that ran off an end included, so a hand edit is answered with a position. It is the
byte the parse stopped at, except for the six messages that name where the offending token STARTED
instead, because on those that is the end a reader can act on: a string that runs to the end of the
document names its opening quote, a number that is not JSON or not UTF-8 names its first character,
a byte that is not UTF-8 names the first byte of the sequence, and a `\u` escape that is short or
not hex names the first of its four digits.
`malformed_input_is_an_error_naming_the_byte_it_is_about` asserts the whole sentence rather than
`is_err()`, which is all it could see before, and it pins a nonzero offset for each of the two
messages this branch added, because byte 0 is what a hardcoded format string would print too.

**`columns` names what the list row SHOWS, and it is a set that always holds `name`.** Every entry
is one of the five column keys, no key appears twice, and `name` is among them, because
`src/uischema.rs`'s own row says name is never optional and `ui/js/Columns.js` draws it whatever the
file holds. A duplicate is not harmless: measured through the real `ui/ViewState.qml` singleton, a
stored `["name","size","size","date"]` left one header-menu "Hide Size" click still drawing
`name,size,date`, because `toggleColumn` splices the first match and the second still shows the
column. The header menu offers only the four optional keys onto an array that already carries
`name`, so no click this window can produce is refused by the rule.

**`wrapAtEnds` is read by the window and by nothing else.** `ui/Pane.qml` exposes it off the
document `ui/ViewState.qml` already holds, and `ui/js/Focus.js` `step` is its only reader: with the
key off a cursor step past an end clamps, and with it on a step taken from an end comes round. The
key exists because issue 27 asked for the wrap, and it ships off because a second user reported
that same jump past the top as a bug.

**`pickerView` is the file chooser's own last-used view and nothing else.** It is `"list"` or
`"grid"`, default `"list"`, deliberately separate from the main window's `view`, so a grid
picker never flips the browser and a list browser never flips the picker. The picker reads it
once at open and writes it when the user switches view, never per cursor move; a stored word
it cannot draw costs the key its default, like every other key.

**`menu.hidden` stores what is hidden**, and its rule is deliberately open, an action id rather than
a closed list, because a closed list would make this Flea drop an id a newer one hid. It is the
Menus section's whole visibility state: the panel's master row over the six basic actions is derived
from the set every time it is drawn, so there is no second value for a hand edit to leave it
disagreeing with. **An id here is the action id `ui/js/Menu.js` gives the row and never a second
name for it.** The shipped set named Open in terminal `terminal` while the menu built the row as
`openTerminal`, so `applyHidden` matched nothing, the row drew in every menu whatever the setting
said, and the panel carried no switch to say otherwise; both sides now read `openTerminal`.

**`keyHints` is the Menus section's one row that is not an action.** It governs presentation across
two surfaces: `ui/MenuRow.qml`'s key column, whose width goes with its text so a menu with hints off
reads exactly as it did before that slot existed, and the tip `ui/WindowBody.qml` draws under an empty
directory. It ships off. A hint is only ever `ui/js/Keymap.js` `hintFor`, which is generated from
`keys.toml`, so no surface can advertise a key nothing is bound to, and no chord depends on the
setting: the keymap is read by `Focus.handleKey` and this value is read by nobody in that path.

**A dead menu row dims as a whole, key hint included.** `ui/MenuRow.qml` draws an unavailable row's key hint
in the label's muted ink at the label's 0.55 opacity; only a reason sentence (`hintWrap`, Extract's "bsdtar is not
installed") stays at the foreground, full opacity, because the sentence is the row's content. An available row's hint
keeps the label ink it has had since 0.3.7. `tests/menu-card-sink.qml` pins the three. **The Copy as switch governs two
different rows.** The file menu shows the Copy as flyout; a Places or Favorites row's menu and the Locked tile show the
flat Copy path of 0.3.7 (one click, that row's own path), because MenuAdditions040 replaces the file menu's row and
draws no place menu. `ui/js/Menu.js` `availableEntry` turns the `copyAs` spec into `copypath` for kind P, and
`src/uimigrate.rs` already carries a user's old `copypath` choice to the one switch.

**`display.textSize.mode` is `"system"` or one Omarchy stop**, one of 9, 10, 11, 12, 14, 16 and 20.
It is one key and not two, so there is nowhere for a free number to be stored.

**The write is a temp plus a rename, never a truncation**, created at mode 0600 in the `open()` call
inside a directory created at 0700, and a symbolic link at either the file or the lock is refused
rather than written through. **The temp carries the same guarantee and by the same mechanism as
`src/userfile.rs`**: `write_new` opens `ui.json.<pid>.tmp` with `create_new`, which is
`O_CREAT | O_EXCL`, so a link or a file planted at that path is an error rather than a redirect, and
the pid in the name keeps two concurrent writers off each other's temp. The direct proof is
`the_write_replaces_the_file_rather_than_truncating_it`, which holds an fd open across an update,
reads the old bytes back through it and compares inodes. The
sweep beside it fires a `SIGKILL` 1 to 9 ms into each of 120 rounds and lands on a live process in 88
to 120 of them, measured across nine sweeps on this box and four more with other lanes live on it,
the second set ranging 92 to 120; no round of the 120 has ever left the file as
anything but the old document or the new one. The timed sweep's kill count is printed as a diagnostic
and never asserted, because a faster box finishes more writes inside its 1 to 9 ms budget. The proof
that a kill never leaves a partial `ui.json` is the three deterministic stage kills in
`tests/uistate-deterministic.py`: before the temp, inside the publication, and after the rename.
Each stage witnesses a live writer, requires SIGKILL termination and compares `ui.json` with the
complete before or after document.
A killed round leaves litter: `write()` unlinks only `ui.json.<own pid>.tmp`, so a process killed
between `write_new` and the rename leaves that temp for good, and tens of the 120 rounds do, 47 to
90 across the four runs that added the check, which is a magnitude and not a number to cite. Nothing
reaps them and nothing may: the concurrency block above holds twelve live temps at once, so no
process can tell a peer's from a corpse, and deleting one in flight is worse than the litter. The
suite lists the directory with `ls -A` and asserts that every leftover is a killed writer's own temp
and that there are never more of them than there were kills.

**0.1.3 did store something, and it is migrated.** `ui/ViewState.qml` wrote `hiddenCols` and
`uiScale` to `$XDG_CONFIG_HOME/flea/view.json`, so the handoff's "0.1.3 stored nothing" is wrong.
`hiddenCols` named what was HIDDEN and `columns` names what is SHOWN, so the migration inverts it;
`uiScale` is dropped deliberately, because 0.1.4 stores an Omarchy stop and never a free
multiplier. The migration rides `read()` above, so the first paint after an upgrade already reads
the migrated columns, whether the settle or a `flea --ui-state` patch was what wrote them down. A
`ui.json` that exists means `view.json` is never read again, whether or not this Flea can read that
`ui.json`'s bytes, and `view.json` is never written again.

**`stateVersion` is the record of every one-time migration, and it lives in `ui.json` itself.** A
changed default alone reaches only a fresh install: every write stores the whole merged document, so
nearly every 0.3.2 file holds `"showUnmounted": false` whether or not its operator ever touched the
switch. The switch is turned on for everyone once and respected after that, so a user who turned
it off on purpose in 0.3.2 gets it back one time, an accepted cost. `src/uimigrate.rs` holds `STEPS`, each
change paired with the version it arrived in, and today there are two: version 1 sets
`places.showUnmounted` to `true`, and version 2 hides the MenuAdditions040 rows once for a file
stamped below 2, replacing a hidden `copypath` with `copyAs` at the same position (a file that had
switched Copy path on keeps Copy as visible) and appending `pasteAs` and `invertSelection` when
absent, leaving every other entry and its order alone. `migrated` runs inside `uistate::from_file` and reads the stamp off
the document as FOUND, never off the merged one, because the merge fills a missing stamp from the
shipped default and would call every 0.3.2 file migrated. A file whose stamp is absent, not a number,
or below a step's version gets that step and that step's stamp; a file at or above it gets nothing.
The shipped document is already stamped 2, so a fresh install's first write is stamped too and its own
switch-off is never undone. **It happens on read.** `from_file` is under every reader, `Store::read`,
`flea --ui-state` with or without a patch, the TUI and the base every patch merges onto, so a read
answers the switch on before anything is written, and a bare `flea --ui-state` still writes nothing.
**It is written down by the settle**, the write-on-migrate place the launch already had:
`left_as_it_is` compares the file with its own `from_file` rendering, an unstamped file no longer
matches it, so the first 0.3.3 launch of either front end writes the migration once, and every later
launch reads a stamped file that renders to itself and writes nothing. The window's `FileView` reads
that settled file, which is how the rail shows the drives on the first 0.3.3 launch. A launch whose
settle fails opens on the raw file, which still says off, until the next write lands the stamp, the
same limit every other rule here has when the settle fails. **Concurrent writers cannot lose it.**
Each one re-reads under the lock, so whichever of two windows, the TUI or the CLI writes first migrates
and stamps, and every writer after it reads a stamped file; a patch names only what that front end
changed, so the operator's own switch-off lands on top of the migration whatever the order.
`Rule::Version` keeps any whole number, so a newer Flea's higher stamp survives this one's rewrite, and
`check()` refuses a patch that names `stateVersion` at all, so no front end can rewind a migration a
file has had. A 0.3.2 binary keeps the stamp as an unknown key, so a downgrade and an upgrade again do
not run it twice. **The window reads an absent `showUnmounted` as on**: `ui/Sidebar.qml` and the rail
rows of `ui/js/Settings.js` read `!== false`, because the window applies no schema of its own and a
first launch has no file to read. A later migration appends one `(version, step)` pair to `STEPS`,
raises `"stateVersion"` in `src/uischema.rs` DEFAULTS to that version, and changes only the leaves it
names. **0.4.1's step has its own stamp instead, `menuVersion`.** 0.4.0 refuses a file whose
`stateVersion` is above 4: it reads every key as its default and writes nothing, so a raised stamp would
cost a downgraded user their settings. `menuVersion` 1 shows Properties and Permissions and hides
Show Hidden Files and Hide Code Icons from the file menu once, when the stored `menu.hidden` equals a
list an untouched file holds after steps 2 to 4; the background menu keeps both view rows whatever the
list says. 0.4.0 keeps the key as an unknown one, so a downgrade and an upgrade again do not run the
step twice. 0.4.0 reads a listed Hide Code Icons as hidden from both its menus, so after a downgrade
that row is back only through its Settings > Menus switch.
`src/uimigrate.rs`'s own tests drive a 0.3.2 file through a read, a write and two settles, a
stamped file switched off, a fresh install switched off, unknown keys, a patch onto an old file, and
six threads writing one old file under the lock; `tests/uistate.sh` drives the same through the binary,
`tests/ui-rail.sh` case `unmounted` opens a raw 0.3.2 file in the window and reads the rail and the
stamp the launch wrote, and `tests/ui.sh` case `eject` seeds the switch on, so the mounted volume's menu is
Open, Unmount, Eject and the case proves Ctrl+E and the menu both release it.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
