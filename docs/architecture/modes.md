## Modes

`main.rs` dispatches on argv before anything else runs, but only `--backend` is fully insulated
from the flag parsing below: it is matched anywhere in argv and always wins. `--prewarm`,
`--open` and `--terminal` are matched only in their exact well-formed shape, `args.len() == 5`
for the first and `args.len() == 3` with the flag in argv[1] for the other two, so a MALFORMED one
is not caught here at all. It
meets its flag's own usage error just below (`--prewarm takes a path, a first index and a destination`,
`--open takes one path`, `--terminal takes one directory`) and exits 2; `flea --open` with no path and `flea --open a b` are both that case.
`--open` takes exactly one path and exits `0` on a successful handoff, `2` on anything that could not be opened, carrying one elided
sentence, and `3` where the resolved target is a directory, carrying no output at all; the window's path adds `4` and `5` below. A
directory is refused rather than handed on because the desktop default for `inode/directory` is a
file manager either way, `org.gnome.Nautilus.desktop` on an unclaimed box and
`com.thisisgm.flea.desktop` once `--default` has claimed it, so handing one on opens a file manager
from inside a file manager; the caller navigates instead. On the `main.rs` `--open` path two more
answers ride beside that contract: `4` asks the run card, with one JSON line on stdout carrying the
kind, the size, an AppImage's handler id and name, the R3 identity with dev and ino as strings, and the
resolved target, and `5` asks Open with where no application claims the file; in-process callers (the TUI and
the shelf) stay on `0`/`2`/`3` and refuse an executable program with a sentence instead of asking. On
every path an ELF or AppImage missing its run bit is refused with `2`, and a script missing its run bit
opens with its handler the way it did before 0.3.9. See "Opening a file".

`--terminal` takes exactly one directory and has a two-value contract: `0` is a successful handoff
to `xdg-terminal-exec`, and `2` is everything else, carrying one elided sentence on stderr. A path
that does not resolve and a path that resolves to something other than a directory both answer
`that directory could not be opened in a terminal, check that it still exists`; a handler that
could not be run at all answers `nothing on this system could be asked to open a terminal there`,
so the pair tells a refusal from an unimplemented mode the way `--open`'s does. There is no third
status: a terminal has no `IS_DIRECTORY` case to report. See "Opening a file".

`--update` and `--update check` are matched in their exact shapes (`args.len() == 2`, and
`args.len() == 3` with `args[2] == "check"`), and a third check catches any other argv whose `args[1]`
is `--update` with the usage error `--update takes nothing, or check`. `--update check` prints one line
and has a three-value contract: `0` is an update the installing source can deliver, `3` is nothing to
install (up to date, or a build no release describes), and `2` is a check that could not be made, which
still prints its line and adds one sentence on stderr. `--update` alone has `--terminal`'s two values:
`0` is a handoff to Omarchy's updater and `2` is a presenter that could not be run. See "Updates".

`--ui-state` is matched on `args[1]` alone and handles its own shapes: none reads the state file,
one merges that JSON object through the shared update path, and anything more is a usage error. See
"The state file".

`--unlock <device>` is matched in its exact shape (`args.len() == 3`) and opens the GPUI window with `FLEA_UNLOCK` naming the device; ordinary launches remove an inherited unlock request. Invalid argv, a path outside /dev, a missing display or `FLEA_FRONTEND=qml` exits `2` with one `flea:` sentence; a GPUI executable that cannot be started exits `1`.

`--clip` and `--clip-own` are matched in their exact shapes. A malformed `--clip` leaves by the usage
error `--clip takes get, set copy|cut, or clear TOKEN` and `--clip-own` with anything after it by
`--clip-own takes nothing`, both with exit `2`. `--clip` is the terminal and test seam over the system
clipboard the backend's `clipSet`, `clipGet` and `clipClear` requests use (docs/protocol.md has their
shapes). `--clip get` prints the `clip` reply line on stdout and exits `0`; a failure prints the same
line with `ok:false` and an `error` sentence and exits `2`. `--clip set copy|cut` reads NUL-separated
absolute paths on stdin (at most 64 MiB), prints the 32-hex-char token and exits `0`; a refusal prints
one `flea: <sentence>` line on stderr, nothing on stdout, and exits `2`. `--clip clear TOKEN` has
`--clip get`'s shape with the `clear` reply line (`cleared` says whether anything was cleared), and an
empty TOKEN is a `flea:` sentence on stderr with exit `2`. There is no `--clip watch`: the watcher is
the backend's `clipWatch` request alone. `--clip-own` is the detached owner behind one copy and not for
direct use: it reads `<op> NUL <token> NUL <path> NUL ...` on stdin, offers every clipboard type,
prints `ready`, then points its stdio at /dev/null so the copy outlives the pipes of the process that
started it (`setsid` gives it a session of its own). It exits `0` when a newer copy cancels it, and
`2` for a payload it refuses (one `flea:` sentence on stderr, before `ready`) or a compositor
connection that is lost.

`--default` and `--default off` are matched the same way, in their own exact shape
(`args.len() == 2`, and `args.len() == 3` with `args[2] == "off"`), dispatching to `main.rs`'s own
`claim_both()` and `release_both()` rather than straight into `defaults`: a box updating from 0.1.3
carries a Flea with no chooser routing at all, so one command finishes the job. Like `--prewarm`
and `--open`, a malformed `--default` does not fall through to the unknown-flag branch: a third
check catches any argv with `args[1] == "--default"` that matched neither shape and names its own
usage error, `--default takes nothing, or off`, before exiting 2. `defaults::claim()` refuses and
writes nothing when Flea's own desktop entry, `com.thisisgm.flea.desktop` (`defaults::DESKTOP_ID`),
is not installed under `$XDG_DATA_HOME` (or `~/.local/share`) or any of `$XDG_DATA_DIRS` (default
`/usr/local/share:/usr/share`): the packaged entry is the proof the pacman package landed, and
pointing `xdg-mime` or Hyprland's bindings at an uninstalled binary would be a claim on nothing.
Past that check `defaults::claim()` runs three independent halves, each one line of output through
`defaults::report()`, which takes exactly those three and prints every one whatever the others did:
the `inode/directory` MIME default via `xdg-mime`, the user-level D-Bus registration described in
"Show in folder", and the additive, markered block `hyprkeys::claim()` adds to Omarchy's
`~/.config/hypr/bindings.lua` for the two file-manager chords. The first and third rewrite existing
per-user files through `userfile::replace_file` (see "Predictable path writes"); the second creates
its own file with `userfile::create_file`, so a symlink planted at that path is refused rather than
followed.

**The registration half refuses rather than skipping.** `claim_service()` reads its `Exec` out of
the packaged `com.thisisgm.flea.FileManager1.service` found on the same XDG ladder
`installed_entry()` reads, and with none installed it returns an `Err` naming what is missing rather
than writing a path of its own. That `Err` makes `defaults::claim()` non-zero, so `claim_both()`
stops before the chooser step. The box that reaches it is the same one the chooser step skips: a
`cargo build` binary run against a package that predates 0.1.4.

**The chooser step is the one that skips rather than fails, and no other step does.** Past the handler half,
`claim_both()` asks `chooser::backend_installed()`, and with no `flea.portal` in any portal
directory it says `no portal backend is installed, so the file chooser step was skipped` on stderr
and counts that as no failure: that is what a source build gets, because only the pacman package
installs that file. With one installed it runs `chooser::claim()` too, which writes its one key into
the portal file the session reads, `~/.config/xdg-desktop-portal/portals.conf` or the desktop's own
`hyprland-portals.conf` when one exists, and a second markered block in the same
`~/.config/hypr/bindings.lua`. So a full `--default` touches four files, not two:
`~/.config/mimeapps.list`, `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service`,
`~/.config/hypr/bindings.lua` and that portal file.
**A refused handler claim stops the command there**, so
the chooser half never writes behind a step that wrote nothing. `release_both()` is
unconditional and runs every step back, each half a no-op when it was never claimed; no half's
failure blocks another, see `defaults::report` and `chooser::report`. **Running a step back is not
always restoring it.** The key block and the picker's window rule are marked blocks, the chooser
routing is one key, and the registration is a whole file of Flea's own, so cutting them leaves what
was there before; the handler half is
`release_mime()`, which only calls `drop_default()` to delete Flea's line and then reports what
`xdg-mime query default` answers next. Nothing persists the `was <id>` that `claim_mime()` printed,
so a handler the operator had pinned is never written back, and on a box that had one
`flea --default off` leaves a `[Default Applications]` section that no longer names it.
The `undo both with:` line
belongs to the invocation and not to a step, so `main.rs` prints it once, after the steps it ran,
and `--default off` prints none at all.

`--youleftmeforstrata` is a second, undocumented spelling of `--default off`, matched in its own
exact shape and dispatching to the same `release_both()`. It is deliberately kept out of `usage()`,
`README.md` and `docs/install.md`, and it is written down here because this file is the internal
contract: it is state-changing, so anything auditing the mode list has to know it exists.

`--picker` and `--picker off` are matched in the same two exact shapes as `--default`, with the
same third check naming `--picker takes nothing, or off`, and dispatch to `main.rs`'s
`claim_picker()` and to `chooser::release()`. `claim_picker()` is `chooser::claim()` plus the
`undo both with: flea --picker off` line, printed only when there was a backend to claim, and it
lives there rather than in `chooser` so that `--default` cannot print a second undo line naming a
command the operator did not run.
`--pick <reply>` is matched in its own exact shape and is not for people: it is how
`tools/flea-portal` opens one chooser window. See "The file chooser portal".

For a browsing launch, `src/launchargs.rs` parses `--tui`, `--gui`, the starting path and
`--select`. `src/main.rs` then dispatches to the terminal interface or the window.

`--tui` and `--gui` are mutually exclusive; giving both is a usage error naming the conflict.
With neither flag the window is the default, including when stdin and stdout are terminals.
`--tui` enters the implemented terminal interface through `src/tui/mod.rs`'s `tui::run`.
Before that call, `src/main.rs` requires both stdin and stdout to be terminals; otherwise it
prints `flea: the terminal interface needs a terminal on stdin and stdout` and exits `2`.
For example, `flea --tui | head` refuses because stdout is a pipe. The TUI branch runs before
the graphical-session check, so it does not require `WAYLAND_DISPLAY` or `DISPLAY`.
A window launch without a non-empty value for either variable refuses with exit `2`.

Flea opens the GPUI frontend by default. A bare `flea`, `flea --gui <path>`, a positional directory, and `flea --select <path-or-file-URI>` open the GPUI window. The file chooser portal and FileManager1 use the same launcher and retain their existing command-line and reply protocols. `--tui` selects the terminal interface and requires a terminal on stdin and stdout; `--gui` and `--tui` remain mutually exclusive.

Flea finds `flea-gui` beside its own executable first, then on `PATH`. The legacy QML GUI route is removed. Setting `FLEA_FRONTEND=qml` refuses the launch with: `flea: the QML frontend is no longer available; unset FLEA_FRONTEND to open the GPUI window`.

HTML previews use a separate jailed, headless qml6 and QtWebEngine process to render PNGs. These Qt dependencies remain in 0.4.0; the replacement HTML renderer is deferred to 0.4.1.

`./tests/modes.sh` checks the window default with redirected handles and with a real pty
provided by `script`. It also checks conflicting flags and refusal of `--tui` without a tty.

`paths::ui_dir()` still resolves the retained QML data: `FLEA_UI` first, then
`/usr/share/flea/ui`, then the dev tree's `ui/`, accepting a candidate only when it holds
`boot/shell.qml` and treating an empty `FLEA_UI` as absent.
The GPUI window launch does not use that lookup; it resolves the `flea-gui` executable.

`gui::exec_gpui` calls `Command::exec`, replacing the launcher process with `flea-gui`,
so signals sent to that pid reach the GUI directly.
It calls `thp::disable()` immediately before the exec; see "Transparent huge pages" for
the inherited setting and its restoration for programs Flea opens.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
