## Make Flea the default, in Settings > About

`flea --default` and `flea --default off` (see "Modes") have a switch in Settings > About, the check
row "Make Flea the default" in the This box group, directly under the File manager fact, which stays
as it was. The row is the label and the box, with no caption and no mark, and a one-line note under
it. It is an action with a live state, not a setting: it writes no `ui.json` key and has no rule in
`src/uischema.rs`, so `src/uistate.rs` would refuse a `makeDefault` patch whole. `ui/SettingsPanel.qml` `activate()`
routes the id `makeDefault` to `DefaultClaim.toggle()` before the generic branch that hands every
other check to `ViewState.changeSetting`, so Enter, Space and a click all reach the run and none
reaches the writer.

**The box is the truth, not the last click.** It is ticked exactly when `xdg-mime query default
inode/directory` answers `com.thisisgm.flea.desktop`, the same answer the File manager fact states.
`ui/DefaultClaim.qml`, a singleton so one run is in flight per process whatever window pressed it,
reads that answer when About first opens, the read `ui/AboutFacts.qml` used to make itself, and again
after every run; a run that started counts as in flight until that re-read lands, and after a switch
until the portal restart below has answered too, so the box never shows the answer from before it.
Only the run's own re-read ends a run that started; one that never started ends at once (below). Reads are numbered as they start and `finished()` records the number
the next one will carry, so `settled()` ignores a read another window's About began while flea was
still running, or one already in flight when it exited, whose answer may predate what flea wrote.
Quickshell does not restart a `Process` whose `running` is already true, so a run that exits during
such a read asks for one more behind it. Each process's answer is taken once its exit and its
`StdioCollector`'s text have both landed, in either order (`landed()` and `whole()`), because
`onExited` can race the collector's text (see "The state file"); measured on Quickshell 0.3.1 the
text lands first, which `tests/mount-listing.qml` records, so this is the tree's guard and not a
reproduced race. **A program that never starts still ends the run**, by `ui/ViewState.qml`'s writer
rule: Quickshell raises no `exited` for it, only `running` going false, and a real exit lands its
status before that false, so a false with no status in (`neverRan()`) is a program that never ran.
An xdg-mime that cannot start is a read answered `NEVER_RAN`, which states no handler, so File
manager reads "Not reported" and the run it was the re-read of settles. A flea that cannot start
fails at once with "<command> could not start" in the error role (`unstarted()`), owing no re-read
and no restart. A systemctl that cannot start is caught by the same rule with `claim.restarting` as
the book and gives the restart-failed note. A press runs `[FLEA_BIN, "--default"]` from an unticked box and
`[FLEA_BIN, "--default", "off"]` from a ticked one, the running binary the way `ui/UpdateCheck.qml`
runs `flea --update`, as a Quickshell `Process`, which never blocks the window. It is not
`startDetached()`, because the exit status and stderr are what the note is made of. A press while a
run is in flight does nothing.

**The switch restarts the portal, and so does the command at a terminal.** xdg-desktop-portal reads its routing once,
at startup, so after a run that exits 0, a claim, a partly claim or a release, `ui/DefaultClaim.qml`
runs `systemctl --user try-restart xdg-desktop-portal.service` as its own `Process`, and file dialogs
follow at once. `try-restart` does nothing when the portal is not running. A failed or refused run
restarts nothing. The run stays in flight until both the handler re-read and the restart have
answered. A restart that exits non-zero does not fail the switch: the box keeps the re-read answer and
the note says "File dialogs follow after xdg-desktop-portal restarts." in the foreground, except after
a partly claim, whose own note stands because its file dialogs never follow. The switch restarts the
portal so file dialogs follow at once (decided 2026-09-22), and the command then restarted
nothing. The install line is `omarchy pkg aur add flea-bin && flea --default` (decided 2026-09-23)
with no restart of its own, so `chooser::report()` now makes the same `try-restart` for `flea --default`,
`--picker` and both `off` forms when stdout is a terminal and the routing half went through. With
stdout piped or redirected, as the switch runs it, the command restarts nothing and prints the restart
hint, which keeps the switch at exactly one restart; a refused or failed routing prints neither.
`tests/modes.sh` pins each path with a stub `systemctl` and script(1) for the terminal.

**The seven states and their notes** are `ui/js/MakeDefault.js`, pure, so `tests/js/settingsabout.js`
drives each one. Off has no note. On says "Folders, Show in folder and file dialogs open Flea." in the
foreground. Working greys the row (`inert`, which `ui/SettingsRow.qml` draws at the disabled opacity
without its handlers, the cursor still resting on it) and says "Making Flea the default" or "Handing
folders back" in the muted role. Partly is a claim that exited 0 having printed `claim_both()`'s
`no portal backend is installed, so the file chooser step was skipped`: the box is ticked and the note
says "File dialogs need the flea package's portal files." Portal is a switch whose restart failed, as
above. Failed is a non-zero exit that is not the refusal below: the note is the first stderr line
that starts `flea: `, prefix removed, in the error role, because xdg-mime's own complaints reach the
same stream; with no such line it is the first non-empty one, and with none at all the command and
its status. A refusal is told from a failure by its sentence and never by its status, because both
exit 1: a non-zero run whose stderr carries `defaults::claim()`'s `is not installed in any
applications directory` is Unpackaged, and `defaults::claim()` prints it having written nothing.
Unpackaged greys the row and says "Install a Flea package to make it
the default.": it is known up front from a probe that walks `src/userfile.rs` `data_file()`'s ladder
(`$XDG_DATA_HOME` or `~/.local/share`, then `$XDG_DATA_DIRS` or `/usr/local/share:/usr/share`, an
empty variable read as unset) for `applications/com.thisisgm.flea.desktop` with `sh -c`'s `[ -f ]`,
each path its own argument, and from `defaults::claim()`'s refusal if the probe and the binary ever
disagree. Every note is one caption line, 11 px at the base size, that elides rather than wraps
(`elide: "right"` on the hint), so a long failure never pushes the rows under it down.

**Tests.** `tests/js/settingsabout.js` holds the rows, the seven states, the notes and their roles,
inertness, what a press runs, the stderr reading, which handler read may settle a run, the exit and
text join in either order, each of the three programs never starting, when the portal restart is asked for and its argv, and the probe's paths
and its argv with the script pinned word for word. The refusal and the partly line it matches are
not copied: it reads `src/defaults.rs` and `src/main.rs` the way `tests/js/themes.js` reads
`colors.toml` and takes `claim()`'s and `claim_both()`'s one `flea: ` sentence and `DESKTOP_ID`
from them, so a reworded sentence reddens the suite instead of turning a refusal into Failed.
`tests/ui-makedefault.sh`, sourced by `tests/ui.sh` as the `makedefault` case, launches with a
stub `flea` on `FLEA_BIN` that answers only the two exact `--default` argv shapes, refuses any other
argv of the modes `src/main.rs` routes to `claim_both()`, `release_both()`, `claim_picker()` or
`chooser::release()`, and execs the real binary for everything else; a stub `xdg-mime` that answers
only the handler query, from a fixture file; and a stub `systemctl` that logs every call and answers
only the exact portal restart. A refused call exits non-zero into `refused.log`, which the case
asserts empty, so a drifted argv reddens the case and never reaches the real `xdg-mime`, the
operator's `mimeapps.list` or their portal. The refusal and skipped-chooser sentences the stub prints
are read from the same Rust source. A fixture desktop entry is prepended to `XDG_DATA_DIRS`. It
drives Space, a click and Enter, a second Space during a run, the partly, failed and refused runs, a
failing restart, and a press on the inert row, then relaunches with the first handler read held
past a claim's exit and asserts the held answer from before the claim settles nothing and the read
behind it ticks the box, and relaunches once more on a PATH with no xdg-mime anywhere on it, the
stubs and a link to the first of every other program on the suite's PATH, and asserts the claim
still leaves working with File manager reading "Not reported". It asserts exactly one `--user try-restart xdg-desktop-portal.service` after
each run that went through and none after a failed or refused one, and that neither the session
state nor `ui.json` carries the id at any depth, reading each as an object first so an IPC or parse
failure fails the check rather than passing it.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
