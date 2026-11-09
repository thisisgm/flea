## Deliberate corners

- The name arena uses u32 offsets, so it tops out at 4 GiB of names. No directory
  reaches that, but a later search phase reusing this arena could.
- The context menu's backdrop consumes the press that dismisses it, so a left click on a row
  while the menu is open closes the menu without selecting that row; `tests/ui.sh menu`
  covers this and it is behaviour, not a bug.
- `ui/Preview.qml` decodes an untrusted file inside the GUI process on Space, where thumbnail
  decoding runs argv-direct under `bwrap` instead. The difference is consent and blast radius: a
  thumbnail sweep decodes files the user never chose, a preview decodes the one file the user
  pressed Space on, the same trust the user already extends by opening it in any other viewer.
  The no-sweep rule is untouched: a preview reads exactly one path per open and, Quick Look's one
  cache-only next-row ask and its one shown-row decoding ask aside, issues no thumbnail requests, which `tests/ui.sh nosweep` still covers unchanged.
- `ui/Preview.qml`'s markdown kind is a name-suffix check (`.md`/`.markdown`/`.mkd`), not an icon
  check: `text/markdown`'s own `/usr/share/mime/generic-icons` entry is `x-office-document`, not
  `text-x-generic`, so the row's icon name alone cannot carry it. It is a display-mode choice
  over an already-text-shaped file, not client-side content sniffing.

### Size formatting matches real GLib, not Finder, and not the design spec's assumed rounding

`ui/js/Format.js`'s `size()` is GLib's SI, one-decimal ladder (`g_format_size`), because storage
capacity has no power-of-two basis and hard rule 2 picks the desktop's own convention over
Flea's old IEC one. Finder renders 26,950,000,000 bytes as "26.95 GB". The design spec that
named this fixture quotes that string as a value that "reproduces exactly" and assumed GLib
rounds it up to "27.0 GB"; measured live on this box (glib 2.88.3, `python3 -c "from
gi.repository import GLib; print(GLib.format_size(26950000000))"`, and cross-checked with
`numfmt --to=si 26950000000`) real GLib prints "26.9 GB", not "27.0 GB". The nearest IEEE-754
double to the decimal 26.95 is 26.949999999999999289, strictly below the true midpoint between
26.9 and 27.0, so any correctly-rounded one-decimal formatter, GLib's included, rounds it down.
`tests/js/format.js` pins "26.9 GB" to match the measurement, not the design spec's assumption.
A later reader comparing this test against that spec should trust the live measurement here,
not the spec's quoted string.

### Transparent huge pages

The GPUI launcher calls `thp::disable()` immediately before `Command::exec` replaces it with
`flea-gui`; the chooser path does the same.
Programs Flea opens regain huge pages through `thp::enable()` in the opener and terminal paths.

The Qt-specific measurements below belong to the retained QML reference, which the GPUI launcher does not load.

`prctl` is declared in `src/thp.rs` as an `extern "C"` symbol rather than pulled in with a crate, because
`std` already links the system libc. It is declared
variadic, `fn prctl(option: i32, ...) -> i32`, the shape glibc gives it (C's own `int prctl(int, ...)` prototype), and each call passes its
extra arguments as `c_ulong`; the `extern-variadic` gate in `tests/staticgates.py` refuses a
fixed-arity declaration of `prctl`, `fcntl` or the other variadic libc calls it names, the shape
rustc's `invalid_runtime_symbol_definitions` lint denied for `open`. The earlier fixed-arity form happened to work rather than being
correct. Three backend files declare
their own copy in the same shape for what they set in a child process: `src/backend/menu_registry.rs`
(`PR_SET_THP_DISABLE` cleared for an application a menu launches), `src/backend/localsend.rs`
(`PR_SET_PDEATHSIG`) and `src/backend/thumbworker.rs` (`PR_SET_NO_NEW_PRIVS`, `PR_SET_DUMPABLE`,
`PR_SET_SECCOMP`). `objdump` on glibc 2.44 here
shows `prctl` is a raw syscall stub, `endbr64; mov %rcx,%r10; mov $0x9d,%eax; syscall`, so the five
register arguments reach the kernel as passed. The kernel rejects a non-zero `arg4` and
`arg5` for this option, so passing all five is required rather than merely harmless. **`arg3` is a
flags argument and not a zero-only field**, corrected on 2026-08-30 after this section had asserted
the opposite: `/usr/include/linux/prctl.h` lines 181 to 190 here define
`PR_THP_DISABLE_EXCEPT_ADVISED` as `1 << 1` and say outright that flags apply when disabling, and a
direct probe on this 7.1.9 kernel returns 0 for `arg3 = 0` and for `arg3 = 2` while returning
EINVAL for `arg3 = 1`, for `arg4 = 1` and for `arg5 = 1`. The shipped call passes `arg3 = 0`, the
plain disable, which is what every figure in this section measures. The call is
deliberately not fatal: a non-zero return prints one sentence and the launch continues, since a
larger process is not a reason to refuse to open a window.

`PR_THP_DISABLE_EXCEPT_ADVISED`, `arg3 = 2` on this 7.1 kernel, was built and measured and buys
nothing: 42.08 MB against the shipped call's 42.12 MB, because nothing in the Qt or Mesa stack
advises huge pages. It also leaves `THP_enabled` reading 1, so the shipped test would not pin the
mechanism under it. The simpler call is the right one.

**// corner: the setting is inherited by every descendant, and that is fine only while Flea launches
nothing foreign.** The descendants that keep it are the backend and the helpers it runs, such as the
`bwrap` thumbnailer children and the LocalSend CLI; the thumbnailer was measured to pay nothing for
it: `ffmpegthumbnailer` on a fixture clip ran 88 to 94 ms with huge pages on against 89 to 92 ms off over five interleaved pairs, output byte-identical. A foreign program
CAN now be launched from the window, because Enter on most files runs `flea --open`, and the undo the
corner asked for ships with it: `open::open` calls `thp::enable()`, which is
`prctl(PR_SET_THP_DISABLE, 0, ...)`, in the `flea --open` process before it spawns `gio open`, so the
opened program inherits huge pages back on; an application a menu launches gets the same call in
`src/backend/menu_registry.rs`'s `pre_exec`. The `flea --open` call lives in `src/open.rs`, after the
`canonicalize` and after the directory refusal and immediately before the spawn, so a path that
never reaches a handler never changes the setting, and `src/thp.rs` holds the `extern "C"`
declaration used by the opener and by the GPUI launcher. Without it Flea would silently change a
system-wide performance setting for every application launched from it, for that
application's whole life. `tests/modes.sh` pins it through a
stub `flea-gui` that execs `flea --open` against a stub handler reporting its own `THP_enabled`, which is
the only shape of check that can redden when the `thp::enable()` call is deleted. See "Opening a
file" below.

`--backend` never reaches this branch, so a backend started from a shell keeps huge pages on and
its stdout is byte-identical either way. The backend `qs` spawns does inherit the setting, and
that was measured to cost it nothing: over nine interleaved pairs on the 100,000-file fixture the
listing read stayed in the same 22 to 25 ms band in both arms with the per-pair sign split five
to four, and the sort in the same 2.1 to 2.3 ms band.

The price was paid in CPU and not in latency. Over eighteen interleaved pairs at two workload
sizes, the same scripted scroll workload cost `qs` a few percent more CPU with huge pages off,
the same fraction at both sizes rather than a growing one, while the input-to-rows latency of a
jump to the far end of the listing did not move: equal medians at both sizes and an even per-pair
sign split. The extra minor faults are in the low thousands, far too few to account for the CPU,
so what is being paid for is page-walk work rather than allocation.

### Opening a file

`flea --open <path>` is the one route that opens a file with the desktop's own handler, and both
front ends use it rather than launching anything themselves. It exists in Rust because the huge page
setting above is inherited by every descendant, so a program launched from QML would run its whole
life with transparent huge pages disabled without ever asking; `open::open` hands the setting back
before it spawns.

It runs `gio open <path>` and waits for that one process, which is not the same as waiting for the
application. `gio open` asks the desktop database for the handler, launches it and returns: measured
on this box in the low tens of milliseconds against a stub handler that then ran for five seconds
of its own, with the handler still running long after `gio` had been reaped. Ten runs against an isolated
`XDG_DATA_HOME` holding one entry, whose `XDG_DATA_DIRS` was still the box's own 107 system
entries, and ten more with the operator's 17 user entries on the search path as well, were the
same 10 to 14 ms, so the size of the database is not what that number is made of. **That whole
paragraph measures the `Exec=` path and is not a bound on the other one**: a `DBusActivatable` entry
makes `gio open` wait on the `org.freedesktop.Application.Open` reply instead, which on this box's
archive default is 0.32 to 0.75 s, and the `ui/Opener.qml` paragraph below carries those numbers.
Waiting is what makes the exit status mean
anything, and the detached `xdg-open` spawn it replaced answered `0` for a handoff that had not
happened. It is still a spawn and never an `exec`, because an `exec` would leave a Flea-descended
process alive for the application's whole life, as a child of the `qs` process, which Quickshell may
kill when Flea quits. `Command::status` is argv-direct exec and never a shell, which is the binding
requirement. The child gets `process_group(0)` so that nothing which later kills Flea's process group
reaches the opened application.

The child also gets `/dev/null` on all three descriptors. Quickshell hands `flea --open` pipes and
closes them the moment it exits, so a handler that inherited those pipes dies with `SIGPIPE` on its
first write. Measured on this box: `imv` was spawned and gone inside 250 ms without ever mapping a
window, and the same spawn with `Stdio::null()` opened normally. `tests/modes.sh` pins it by giving
`flea --open` a pipe of its own and reading the stub handler's `/proc/$$/fd/1`.

The path is `std::fs::canonicalize`d, which does two jobs at once: a symlink opens its target rather
than the link, and the absolute result cannot be read as a flag by the child, so a file named
`--output=/etc/x` is passed as `/tmp/.../--output=/etc/x`. A broken symlink fails canonicalization,
which is the error path.

The exit statuses are `0` for a successful handoff, `2` for anything that could not
be opened, carrying one elided sentence on stderr, and `3` where the resolved target is a directory,
carrying no output at all; the window's `--open` path adds `4`, asking the run card with one JSON line on
stdout, and `5`, asking Open with where no application claims the file. `2` now covers the launcher refusing as well as failing to start, and the three sentences are
distinct because `std::fs::canonicalize` has already proved the file is there before `gio` is run at
all: a path that does not resolve gets `that file could not be opened, check that it still exists`, a
`gio open` that exits nonzero gets `gio open refused that file, so no application on this system took
it`, and a `gio` that could not be run at all gets `nothing on this system could be asked to open that
file`, so the set tells a refused open from an unimplemented one and neither one blames the path. A directory is refused rather than handed on because
the desktop default for `inode/directory` is a file manager either way, `org.gnome.Nautilus.desktop`
on an unclaimed box and `com.thisisgm.flea.desktop` once `--default` has claimed it, so opening one
through the opener from inside a file manager opens a file manager; the caller navigates instead.
`flea --open` with no path and `flea --open a b` both meet the usage error `--open takes one path`
and exit 2 (tests/modes.sh counts that sentence).

`ui/Opener.qml` is the window side of that contract and the only component in the tree that runs
`flea --open` or `flea --terminal`. It holds a `Process` for each of the three it runs, and the
opener's own `onExited` is the whole mapping for this half: `0` says nothing, `3` raises
`isDirectory` and `Pane` navigates there, `4` hands the stdout line it collected to the card once the
process has exited, and `5` opens Open with for that path, while anything else
raises `failed` and `Pane` writes one sentence to the status line. That sentence names no cause, because `2` covers all three failures and
`std::fs::canonicalize` has already disproved the one the window used to blame. A second `open()`
while that `Process` is running is dropped rather than queued, and the drop raises `busy`, which
`ui/PaneWire.qml` answers with one plain sentence in the same slot; `openTerminal()`'s guard raises
`terminalBusy` the same way. The first press stays silent, so only a press that was refused speaks.

**That window is a third of a second on an archive, not the low tens of milliseconds this file used
to claim.** `gio open` on an `Exec=` entry forks and returns; on a `DBusActivatable` entry it waits
on the `org.freedesktop.Application.Open` reply instead, which is a cold application start.
Twenty-five of the twenty-six MIME types `org.gnome.Nautilus.desktop` claims resolve to it as this
box's default (`gio mime application/zip` and twenty-four more), and an archive is a regular file
that reaches `.status()` like any other. Measured on this box with Nautilus not running: 463, 317,
318 and 340 ms idle, and 748, 709 and 707 ms with twenty-four spinners on twelve cores. With
Nautilus already up it is 34 to 48 ms, and the `text/plain` `Exec=` control on the same harness is
11 to 15 ms. Driven against the real window, `Return`, `Down`, `Return` sent as one `wtype` burst
produced exactly one `gio open` call and an empty status line; the same `Return` on the same row
after the wait cleared produced the second call, so the drop is the guard and not the driver.
Saying something in the status bar is what the tree now does. Bounding the wait was declined because
a deadline above the measured range is a guess and one below it cuts off a legitimate cold start, and
queueing was declined because it fires an open after the operator gave up and answers a double Enter
on one archive with two windows. **Enter no longer reaches that wait on an archive at all**, which
is the ruling below; the numbers stand as the cost of any `DBusActivatable` default and as what the
guard exists for. `Pane.openCursor` sends a row with `d` true to `open()`, an archive to the preview
and every other row to the opener, so a symlink to a directory reaches the opener, comes back 3 and
navigates; `tests/ui.sh open` asserts all four answers on one listing.

**Enter on an archive opens Flea's own view and launches nothing, as decided on
2026-09-05.** `ui/js/Nav.js` `openCursor` carries one branch beside the directory branch: a row whose
`Kinds.quickLookKind(row.i, path)` is `archive` goes to `pane.preview.open(path, row.i, row.s)`, the
same call `ui/js/PreviewKeys.js` already makes for Space and `l`, so the three keys cannot disagree
and the routing test is the same function `ui/Preview.qml` runs on the same two arguments a moment
later. It is a route and not a mechanism: `ui/PreviewArchive.qml`, `ui/js/Archive.js` and
`src/backend/archivelist.rs` already drew that frame and the wire already carried its index. The
classifier is the icon rather than an extension table because the icon is what the frame itself
reads; measured here, all eight extensions in `ui/js/Archive.js` resolve through
`/usr/share/mime/globs2` to a type whose `/usr/share/mime/generic-icons` row is `package-x-generic`,
and so do `.deb`, `.rpm`, `.jar`, `.cab`, `.gz` and `.xz`.

**That one icon covers the whole Nautilus class on this box, derived rather than assumed.**
`org.gnome.Nautilus.desktop` lists 26 `MimeType` entries; 25 resolve to it as the default and the
26th is `inode/directory`, which `flea --default` has claimed. Of those 25, 20 carry
`package-x-generic` in `generic-icons`, and the other five have no glob at all in `globs2`, so
`src/backend/mime.rs`, which classifies by glob and by nothing else, cannot produce them for any
filename. 89 `generic-icons` rows carry `package-x-generic`: 20 default to Nautilus, 69 have no
default handler at all, and none defaults to anything else, so the branch takes over exactly the rows
that used to open another file manager, plus rows `gio open` used to refuse.

**The other half of that ruling was measured and then declined, and the expectation did not hold.**
The candidate was resolving the handler in Flea and running `gio launch <desktop-file> <path>` in
place of `gio open <path>`, on the theory that the default-handler machinery was pulling something
extra in. Measured under `flock /tmp/flea-display.lock` on one real `text/plain` file, two
repetitions of each arm plus a third pass with the whole session bus captured: both launchers
produced the same process tree (`kitty -- nvim <path>` reparented to pid 1, with `nvim` and
`nvim --embed` beneath it), both exited `0` in 8 to 10 ms, both made exactly one
`org.freedesktop.DBus.StartServiceByName` call and both named `org.gtk.vfs.Daemon` in it, and neither
produced a single line of bus traffic or one journal entry naming Nautilus. **They are the same on a
text file, so `gio open` stays** and no desktop-entry parsing enters `src/open.rs`.

**Issue 41: `xdg-open` does not honour `Terminal=true`, which is why the handoff is `gio open`.**
`xdg-mime query default text/plain` is `nvim.desktop` here, whose `Exec` is `nvim %F` and whose
`Terminal` is `true`, and Enter on a text file mapped no window at all. Reproduced against an
isolated `XDG_DATA_HOME` and `XDG_CONFIG_HOME` carrying one `Terminal=true` entry, so the operator's
own MIME state was neither read nor written: `xdg-open` exited `3` with
`no method available for opening`, the handler never ran, no terminal was reached, and `flea --open`
still exited `0` over it. `gio open` on the same fixture ran the handler inside the stub
`xdg-terminal-exec` and exited `0`. `src/terminal.rs` execs `xdg-terminal-exec` by name, which is
why it is a `depends` entry and not an `optdepends` one; `/usr/lib/libgio-2.0.so` on `glib2
2.88.3-1` carries that name too, but alongside other terminal names it can fall back to, so glib
alone would not have required it.

**The entry here used to decline the issue, and its stated remedy was false.** It said the remedy
belonged to the operator and was `omarchy default editor`, which rewrites the association.
`/usr/bin/omarchy-default-editor` is 36 lines, contains zero occurrences of `xdg-mime` and zero of
`mimeapps`, and its one path reference is line 7,
`editor_file="$HOME/.local/state/omarchy/defaults/editor"`, which lines 33 and 34 are the only
writer of. It chooses what `omarchy-launch-editor` runs and changes no MIME association at all, so
it could not have fixed this and never could have.

Flea still decides nothing about which programs are terminal programs. That judgement is the desktop
database's, `gio open` is how it is asked, and there is no desktop-entry parsing anywhere in
`src/open.rs`.

**`flea --terminal <dir>` is the same shape for a terminal**, and the topbar's terminal button and
`Ctrl+T` are its only callers. `src/terminal.rs` canonicalizes the directory the same way, refuses
anything that is not one, and hands the result to `xdg-terminal-exec` as a single `--dir=` argument
built as an `OsString`, because `Path::display` would substitute U+FFFD for a byte that is not
UTF-8. **That `OsString` is load-bearing, and argv is not where the byte comes from.** `main()`
converts `std::env::args_os()` once above every mode and refuses a non-UTF-8 argument there with one
sentence and status 2, so no such byte reaches this mode through argv at all. It arrives from the
filesystem instead: `resolved()` calls `std::fs::canonicalize`, so a UTF-8 argument naming a symlink
whose target carries a byte that is not UTF-8, and a relative UTF-8 argument resolved against a
directory that does, both answer a `PathBuf` that is not valid UTF-8. Measured on this box against a
`target-\377` directory reached through an all-ASCII symlink: the canonical path's last byte is
`0xFF`, `Path::display` renders it as U+FFFD, and the `OsString` keeps it. It carries the same three
guards the opener does: `/dev/null` on all three descriptors,
`process_group(0)`, and `thp::enable()` before the spawn. Unlike `--open` it does not wait, because
the terminal it starts lives as long as the user keeps it open: it returned with the stub's log
still empty, against a stub that slept half a second before its first write. `xdg-terminal-exec` is
the OEM route rather than a terminal name of Flea's own, and `omarchy default terminal` is what
configures it: that command writes `~/.config/xdg-terminals.list`, one of the config files
`xdg-terminal-exec` reads, so whatever the operator set is what opens.
`tests/modes.sh` pins the argument, both refusals, the descriptors, the process group and the huge
page restore against a stub on `PATH`, and `tests/ui.sh openterminal` drives the button and the
chord from both views against a logging `FLEA_BIN`.

**Opening an executable file asks once, on the run card.** `src/runfile.rs` classifies the
canonical target from its first 4096 bytes before `gio open` runs: a `#!` head is a script, a
`\x7fELF` head is an AppImage only when the bytes carry the tag (`AI` at offset 8 followed by
`0x01` or `0x02`) and an ELF otherwise, whatever the file is named, and anything else opens the way
it always did. A target that is neither a directory nor a regular file skips the snapshot and the
decision and opens the way it always did, so a FIFO never blocks the opener. The kind and the identity come off one open file description, so a swap between the
head read and the identity stat cannot pair two files. The execute bit read is the caller's own
(owner, group or other by uid and gids, as `access(X_OK)` would), so an other-only bit on the
caller's own file asks the card. A script missing its execute bit asks no card and opens with
its desktop handler the way it always did, and a binary missing it is refused for every caller,
since an AppImage launcher would run it; the context menu's Make executable
(`ui/Pane.qml` `makeExecutable`, `ui/js/Menu.js` `canMakeExecutable`, one permissions batch, one
undo) is the one route to the bit, and the card draws no chmod of its own. A program on a `noexec`
mount is refused with its own sentence, and nothing ever runs silent. The ask answer is exit 4 with
one JSON line carrying the kind, the size, the desktop handler's id and its entry's own `Name=` for
an AppImage (both empty when it names none or the name cannot be read), the R3 file identity (dev and ino as strings, size and mtime as numbers) with the
resolved symlink target the card names; Run and Run in terminal re-stat and
refuse with exit 6 when the file behind the path changed (the handler button answers 9 when the application
it names was uninstalled since the ask), and both spawn argv-direct with no descriptor above
2 (close-on-exec, never closed, so a failed exec still reports) and no `FLEA_` variable reaching the child. A `gio open` that fails on a file no application claims answers exit 5 instead of the
refused sentence, which opens the approved Open with dialog. Both ride `ui/Opener.qml`, which
reads the stdout Quickshell hands over before the exit, says a launch that never started as refused, and hands the raw line to the card to parse, and `ui/RunCard.qml` draws
the board: Cancel, Open as text, Run and Run in terminal with the ring on the default, no text
row for an ELF or an AppImage, and, where the desktop names a handler for an AppImage, a first row
labelled with that application's own name that it focuses. Whether a file is claimed and which
handler it has come from the type `gio info` names, the same lookup the Open with dialog makes.
`flea --run`, `--run-terminal`, `--open-text` and `--open-handler` answer the card's four launches,
each one argv-direct spawn with the detached guards `src/terminal.rs` owns and never a shell string;
`--open-text` resolves the desktop id through the XDG data dirs to a file `gio launch` takes,
`--open-handler <path> <desktop id>` with the card identity rechecks it (exit 6 on a change) and
hands the file to the named entry's own desktop file through `gio launch`, and
every launch carries the shared environment preparation `src/open.rs` owns, including the tear-off
drop. The menu's Run and Open as text rows ship hidden under Settings, Menus, and Enter stays the key:
there is no new chord.

RunFile advloop round 1 moves two recorded ceilings, each re-derived with `wc -l`: `ui/Pane.qml`
1048 to 1066 for the card's own path routing (the null-host guards, one shared make-executable path
over the matched row or the cursor target, and Open with taking an optional path) and `ui/PaneWire.qml`
592 to 589 for the raw ask line replacing the nine parsed signal arguments.

### No type-ahead, and trash is a pair

Removed in v0.1.3 for issue 7. The type-ahead was the fallback for a printable key that
`keys.toml` did not bind, so it only ever worked for the letters nothing else had claimed:
typing `Vid` toggled the selection on `v`, jumped on `i`, and trashed the row on `d`. An
unbound letter now answers `Press / to filter this folder by name`, which is the filter
that already narrows the held rows without a walk.

Bare `d` is `trashArm`, not `trash`: `ui/js/Trash.js` owns the pair, the first press arms and
says so, a second inside `Trash.ARM_MS` trashes, and `Focus.handleKey` clears
`pane.trashArmedAt` for every other action so an arm never survives the key after it. `Delete`
and `Ctrl+Delete` still go on one press, because neither is a letter a name is typed with. The
sheet draws `dd`, and `tests/js/keymap.js` resolves a doubled cap as that one character.

### Theme roles and sources

The QML paths in this section describe the retained reference components; the production GPUI window opens through `crates/flea-gui/src/window/mod.rs`.

`Theme.qml`'s `applyColors` assigns only Flea's eight palette roles and ignores unknown
keys because Omarchy themes contain more roles than Flea uses. `color.surface` walks the
ladder `ThemeRoles.html` specifies, `dark_background` then `background`, because background
is the neutral fallback when the chrome plane is absent. Alacritty-derived palettes contain
neither of those two, so the measured surface fallback for them is `selection`, the third rung.
Shell parsing consumes `[font]`, `[spacing]` and `[flea] background-alpha`, and nothing else,
because Flea has no bar, popups, tooltip or lock screen. The alpha is read through
`Commons.Color.pickAlpha`, so a value in `~/.config/omarchy/shell.toml` wins over the theme's, a
value that is not a number is the default 1, and a number outside 0 to 1 is clamped the way Omarchy
clamps its own `-alpha` keys.

**Glass.** `ui/Glass.qml` holds that value in whole 8-bit steps, so 0.999 is 1. `ui/WindowBody.qml`
calls `Glass.launch()` before `ui/boot/shell.qml` creates the window: it waits for `Commons.Color`'s
two shell readers (`waitForJob`, which lands both synchronously) and decides `clear` once, because
Quickshell fixes a surface's format when the window is created. A launch below 1 gives the body a
transparent ground, and the entry, seeing that, makes the window's own colour transparent, which is
what makes Quickshell ask for a translucent surface. Any other launch keeps the opaque surface and
the literal colour, so the compositor can still skip what is behind the window, and a glass value
written after an opaque launch takes effect on the next launch. A window launched on glass follows
later changes live, the user's file through `Commons.Color`'s own watch and a theme switch through
`Theme.qml`'s `theme.name` watch, and a change back to 1 included. The file chooser never calls
`launch()`, so its window and its planes stay opaque.

On glass every pixel carries exactly one ground plane at exactly that alpha: the chrome bar, the tab
strip, the rail, the status bar, the search strip and a focused dual path strip on the surface
colour, the listing and an unfocused path strip on the background colour. Translucent planes that
overlap compose to more than either, two at 0.34 to about 0.56, so the body draws no ground of its
own on glass and each region draws its own: `ui/Pane.qml`'s ground lies beside the rail and below
the path strip, or below the header while the search strip stands in it; the tab strip's ground
leaves the current tab's plate out, and while the current tab is dragged its ghosted plate draws
nothing and the strip's ground runs under it, because a ghost at 0.55 over a hole would let the
desktop through; the filter strip, the columns preview's held picture and the preview frame draw no
fill, because the pane's ground already lies under them; and the header's titles, wash and rule go
down while the search strip covers them, because a translucent strip would show them. The held
picture's ground is the one "The preview swap" keeps against heavier text; on glass it draws none,
and measured natively on minipc the frames grabbed straight after three cursor moves differed from
the settled frame by at most 1 of 255 in the preview's text (805 pixels, in the first frame after
each move only; the same burst at alpha 1 differed nowhere). Text, marks, thumbnails, rules, washes,
the selection and focus draw exactly as they do on an opaque theme. Overlays hide what is under them
and keep their opaque roles: menus, dialogs, Quick Look's panel, the share browser, Trash, editors,
the grid's name tip, and the rail that auto-hide reveals over the rows. In the columns preview the
playing video and its transport strip are content and stay opaque; the player's letterbox bars draw
no fill there on glass, so the frame reads the same before play and during it, while Quick Look
keeps its opaque bars. Off glass each of these draws exactly what it drew before. `tests/glass.sh`
grabs the real body twice, once over magenta and once over green. Each channel composes as
c = p*a + m*(1 - a), so what the backdrop adds gives the alpha and the dark channels give the colour
(`tests/glass.js`), and each ground region's modal pixel pair must solve to one plane at the
configured alpha, never two stacked. `tests/glass-boot.sh` reads the window the real entry made
through the seam's `glass` reader, which exists once the first listing has answered.

**Window Transparency and config reloads.** Omarchy's Window Transparency mode (`omarchy-hyprland-window-transparency`) lives in
`$XDG_STATE_HOME/omarchy/toggles/hypr/window-opaque.lua` or `window-glassy.lua`, neither meaning transparent, its default, and
`ui/Glass.qml` reads it as a file test, because `OMARCHY_PATH` is missing over ssh and from some launchers. The ground alpha takes
the first row that applies: the user's own `[flea] background-alpha` in `~/.config/omarchy/shell.toml` (a value that is not a
number reads as 1, as before the mode existed); opaque, which is 1; glassy, the theme's own value, else `Glass.glassAlpha`, the translucent ground; transparent, the theme's value, else 1. Transparent with no value is Hyprland's to draw: when Hyprland opens a Flea window,
`ui/Glass.qml` makes `ui/GlassTagger.qml`, after the first rows, and it gives each Flea window Hyprland leaves at opacity 1 the
`transparent` tag Omarchy's transparency hotkey sets, so Omarchy's own rule draws it at its Transparent opacity, as Omarchy draws
Nautilus. A window Hyprland already draws below 1, which stable Omarchy's catch-all rule does to every window, keeps that rule,
so Flea's alpha never stacks on Hyprland's. Glassy, Opaque and a `[flea]` value take Flea's own tag off, and a tag the user set
with the hotkey is never touched; a recorded window Hyprland draws at 1 again, such as a new window on a closed one's address,
is tagged again. `ui/boot/shell.qml` clears the window from `WindowBody.clearSurface`, not the body colour. Hyprland reloads its config for a Window Transparency, Window Corners or Window Gaps change with no theme
switch, and `ui/Theme.qml` listens for the `configreloaded` raw event of `Quickshell.Hyprland` (only inside Hyprland): it runs
`Style.scheduleRefresh()`, so the corner radius follows, and `Glass.reread()`, so the mode does. Every launch is made clear unless a `[flea]` value pins
1, so a switch between Omarchy's three modes shows on a running window; a pinned launch holds 1 until the next. `tests/glass-mode.sh` runs every precedence row, a reload under a running window, the
same reload through a socket speaking Hyprland's event protocol, and the tag through a stand-in hyprctl: tagged at 1, untagged
in Glassy, tagged again in Transparent, and left alone at 0.985. `tests/theme-contrast.qml` holds muted and metadata text to 4.5 on
the glass planes over a white and a black backdrop dimmed by glass.lua's blur brightness 0.75, and records each palette's floor, the
lowest 8-bit alpha above which every step passes: it is 255 for 16 of the 24 palettes, so no translucent ground keeps those at 4.5
until a look lifts the ink on glass, and a recorded floor only moves down.

### Plain text filenames

Every filename in `Row.qml` uses `Text.PlainText`. `Text.AutoText` treats markup in a
filename as rich text, so it is never safe for backend-provided names.

### A key bound ahead of its feature says so

The rule, applied on 2026-09-02 after two silent-by-accident keys and one silent-by-design: every
row in `keys.toml` either works or says why it does not, in a sentence written for the user and
never an action name. The list is empty as of v0.1.3: both entries that were on it have landed.
`t`, `w` and `1` to `9` open, close and switch directory tabs, one live listing with a snapshot per
hidden tab and the strip only once there are two; closing the last tab is refused and nine is the
cap because those digits are the jump keys. `:` and `Ctrl+L` open the path bar, whose action is
`pathBar` rather than `palette` (the old name read as a command palette, and `ui/js/Palette.js` is
the theme's colour file); `Focus.handleKey` opens it beside the keymap sheet as a global action and
`ui/ChromeBar.qml` carries the field, answered by `keycase_pathBar` in
`tools/flea-acceptance-drive`, which drives the key and reads `pathBarOpen` off the seam. The generic
`<action> is not built yet.` fallback below those stays as the loud failure for a row the
dispatcher does not answer, and should never be reachable from the shipped table. `Ctrl+Shift+N`
was on this list for one afternoon and is not on it now: `295e757` routed the action through
`Focus.act` to `Ops.newFolder`, which sends the backend's `mkdir` (docs/protocol.md), and
`ui/ContextMenu.qml` carries the same action as its New Folder row.

### Finder's chords, Cmd read as Ctrl, beside the vim keys

Hyprland grabs every `SUPER` chord, so no app ever sees one; Omarchy's own universal clipboard
(`/usr/share/omarchy/default/hypr/bindings/clipboard.lua`) works by injecting `CTRL+C`, `CTRL+V`
and `CTRL+X` into the focused surface with `send_key_state`. So the Finder table lands in
`keys.toml` as `[[ctrl]]` and `[[ctrlshift]]` rows, every one additive to the bare vim key it
doubles, and `tools/flea-keymap-gen` checks the shifted rows first and falls through. Two Finder
conventions are refused on purpose and should not be reopened: Enter opens and does not rename
(every Linux file manager and the vim table open on Enter, and the TUI is generated from this
table), and `Cmd+D` is not duplicate because `Ctrl+D` already pages with `Ctrl+U` as its pair.
`Ctrl+E` goes through `ui/js/Eject.js` `release`: the rail's cursor row when the rail has focus,
otherwise `Mounts.holding`, the mounted volume, removable or fixed, whose path holds the listing, and the
verdict is `Mounts.railMenu`'s either way; the system disk's row offers no release, and a folder no row holds
is named a system volume only when the backend's fsinfo line for that folder carries the system flag
(`Pane.systemVolume`, unset in a Recent history). A hidden rail unloads the Sidebar with the poll the
verdict reads, so the key spins `ui/PaneRail.qml`'s transient `DeviceMounts` for one one-shot
listing instead: same component, same verdict, same `RailMenu.release` and `devices.eject` path
with the same messages, unloaded when the verdict lands and no release is in flight (`DeviceMounts.releasing`
holds it through an eject, a power-off chain or a user unmount, because the Loader's end would kill the gio),
and costing nothing while idle. A shown auto-hide rail holds itself open on the same property, and a rail hidden
anyway mid-release (Ctrl+B on a docked rail) keeps its Sidebar loaded, undrawn, through `PaneRail.releaseHold`, a
latch set on the Sidebar's `releasing` edge and cleared on its fall, never derived from `hidden`, which would loop. A
folder no volume row holds answers from the pane's own facts, never from rail rows: "on a system volume" only
when fsinfo's `system` flag says a system-path mount holds the listing's device, otherwise "not on a volume Flea can unmount".
`XF86Eject` and the transport keys are not bound here
and cannot be: `media.lua` binds them through `o.bind`, which is a consuming `hl.bind`, so a
client never receives them; the play key runs `omarchy-shell media playPause` against the MPRIS
player the shell tracks, and Flea's Quick Look (`ui/PreviewMedia.qml`, a plain QtMultimedia
`MediaPlayer`) registers no MPRIS name, so it is untouched by it. On the sheet a chord is `^c`,
`^N` with a capital is ctrl-shift, and two rows keep the bare key alone because `f ^f` beside
`find in subtree` (155 px) and `a ^k` beside `add network place` (169 px) overrun the 154 px
column at base-size 14 in JetBrainsMono; the README carries those two chords.

### Prewarm correlation

The two-line prewarm file's `listed` line names its directory, as every `listed` line does, but
nothing in the file says whether that directory changed after it was written, so the UI cannot tell
whether prewarmed content is stale. Production ignores `FLEA_PREWARM` until the file carries that
proof and a first-paint measurement proves a real win.

### MIME globs

`/usr/share/mime/globs2` on this box holds 1596 lines, two of them the header comment,
leaving 1594 data lines. `mime::Db::from_str` drops 10 of those: 4 use a character class
or `?` (`*.so.[0-9]*`, `*.anim[1-9j]`, `*.[1-9]`, `[0-9][0-9][0-9].vdr`) and 6 carry a
star that is not the leading character, always trailing a literal prefix rather than
buried mid-pattern (`sconscript.*`, `cachegrind.out*`, `callgrind.out*`, `dockerfile.*`,
`makefile.*`, `readme*`). No line in this file actually has a star with literal text on
both sides. The remaining 1584 lines resolve to either a suffix or a literal name entry.

A line carries an optional 4th field, a comma-separated flag list, and `cs` is the only
flag this database uses. Exactly 5 lines carry it: `*.gs`, `perf.data`, `core`, `*.C` and
`*.c`, each immediately followed by an unflagged twin of the same glob and the same MIME
type. `Db` keeps two key spaces per lookup, a case-sensitive one keyed on the glob's
original text and a case-insensitive one keyed on a lowercased copy, and `lookup` probes
the case-sensitive space first. One shared key space cannot work here: the unflagged
`*.C` twin lowercases to the same `.c` key as the flagged `*.c` entry, ties at weight 50,
and wins on file order, which is how every `.c` file resolved to `text/x-c++src` before
this was fixed. The flag is honoured for literal names as well as suffixes, for one rule
rather than two, though no test can tell the difference on this box: both flagged literal
names have twins carrying the same MIME type.

- `thumbspec.rs`: the first file to declare a MIME type keeps it, and `load` sorts each
  directory's files by path as it reads them, so that winner is the same on every box and
  after every reinstall. `image/tiff` is declared by both `evince.thumbnailer` and
  `glycin-image-rs.thumbnailer`, and without the sort the winner was whatever `read_dir`
  happened to yield first, which POSIX does not specify. The sort is per directory and not
  across the whole list, deliberately: sorting the collected list would reorder the search
  path itself, and a `XDG_DATA_HOME` that does not sort before `/usr/share` would then let
  a system thumbnailer beat the user's own, which is backwards. `sorted_by_path` is a
  separate function so a test can prove the ordering without depending on what `read_dir`
  returned.
- `thumbspec.rs`: only a whole-token placeholder substitutes, so `--input=%i` would be
  passed through literally. Every one of the nine shipped files writes its placeholders as
  standalone tokens, so nothing on this box needs the embedded form.
- `thumbspec.rs`: `fields` reads `Exec`, `TryExec` and `MimeType` only inside
  `[Thumbnailer Entry]`, and a duplicate key inside that group is last-wins. All nine shipped
  files open with that group and carry no other, so the group test costs nothing here and
  closes a file that could otherwise hide the program it runs under a second heading.
- `mime.rs`: on a tie in weight for the same key, the first line in file order wins,
  because `insert_heaviest` only replaces a stored entry when the new weight is
  strictly higher. This is exercised by real data: eleven lines share `*.pem` at
  weight 50, and `lookup("cert.pem")` resolves to `application/pkcs7-mime`, the first
  of the eleven in the file.

- `scan.rs`: a `readdir` error fails the whole listing rather than a short success, and a
  typeless entry whose `fstatat` then fails is kept as a dir, because the entry itself is real
  even if the type lookup raced it and a folder must survive xfs ftype=0. Two different
  failures, two different outcomes, on purpose.
- `meta.rs`: a row that existed during `scan` but is gone by the time `stat_range`
  reaches it (deleted, renamed) reports `size: 0, mtime: 0, mode: 0` rather than
  failing the whole window; one vanished file should not blank the screen.
- `ordering.rs`: a `sort` key that is not `"name"`, `"size"`, `"mtime"` (or its `"date"` alias) or
  `"kind"` answers `Err`, and `run.rs` refuses it with an `error` line whose `path` is the key. It used to fall back to
  name order "so a stale sort key never refuses to list a directory", and that reasoning was wrong
  twice: `list` sorts by name itself, so a refused `sort` leaves the listing exactly as it was, and
  the fallback answered `kind` and `mode` as name order in silence, so a header mark reading Kind
  described an order that was not on screen and a descending sort was undone under it.
- `sort.rs`: the name sort is stable and stays stable, and `metasort.rs` holds `SortBy::Size` and
  `SortBy::Mtime` to the same rule. **Neither may reach for `sort_unstable_by`**: those keys are
  not total, because two files can share a size and two can share an mtime, so an unstable sort
  would order them arbitrarily and two runs could differ. `metasort.rs` breaks the tie on the name
  for that reason, and "Why the listing is an arena" says why the unstable sort loses on time here
  anyway.
- `run.rs`: a request line that is not valid JSON, or whose `"c"` is not one of
  `list`/`window`/`sort`/`quit`, becomes `Request::Unknown` and is answered with
  silence, deliberately: no error line, no crash, the loop just reads the next line.
  `tests/protocol.sh` asserts this ("junk produces no output and no crash").
- `run.rs`: `sort` with a `by` this wire never defined, `"mode"` among them, answers
  `{"t":"error","where":"sort",...}` rather than silently sorting by name. An honest error beats a
  silently wrong order.
- `prewarm.rs`: see Predictable path writes above; the pid in the temp file name and
  the unlink-both-on-failure contract both live there rather than being repeated here.
- `scan.rs`: a non-UTF8 filename goes lossy in phase 1 and then cannot be stat'd, so
  it reports zeroes. The fix is a byte arena and belongs to a later plan. **The thumbnail
  path inherits it**: two names in one directory that differ only in their invalid bytes
  collapse to the same lossy string, so `State::asked` sees one path where the client named
  two rows, the dedupe swallows the second and that row is answered only when the listing is
  replaced or the process drains. Not fixed here, because the fix is the same byte arena and
  it is a whole plan's worth of change through `Listing`, `stat_range` and the wire.
- `main.rs`: a non-UTF-8 argv byte is refused rather than carried, and that refusal is the corner.
  `std::env::args_os()` is converted once above every mode, and a byte that is not valid UTF-8
  answers `flea: <name> is not valid UTF-8, and Flea takes text paths` on stderr with status 2, so
  `--open`, `--terminal` and a bare path all answer 2 where `std::env::args()` used to panic with
  101. Only a shell or a `.desktop` file can produce such an argv and the window cannot, because
  `scan.rs` goes lossy in phase 1 and `Pane`'s `join` only ever sees that lossy name. Opening such a
  path at all needs the byte arena in the entry above, so this stays a refusal until that lands.
- `open.rs`: between `canonicalize`, `is_dir` and `spawn` a path that vanishes or is swapped
  for a directory changes the answer, which a concurrent `rm` in the user's own session on
  this single-user machine can produce and which is not exploitable, since no shell and no
  privilege boundary is involved.
- `thumbcache.rs` and `ui/Row.qml`: the cache READ path is not sandboxed, and Plan 5 shipped
  the decoder that reads it. `lookup` reads a PNG out of `~/.cache/thumbnails/large` in the
  backend process and parses its `tEXt` chunks there; that parser is bounds checked and total.
  `Row.iconSource` then hands the same path to a QML `Image`, so Qt's PNG decoder runs
  unconfined in the `qs` process, on bytes this process did not write and that any application
  on the box may have written. **Generation is sandboxed and display is not.** The mitigations
  that exist are real and partial: the path always comes from the backend's own `lookup` and
  never from a filename or from anything a client names, the read is a plain `Image` with no
  scripting surface, and generation is still confined under `bwrap` with `prlimit`. What is
  left is Qt's decoder against a cache the whole desktop shares, and the residual risk is a
  decoder bug in a PNG some other application wrote. **This plan did not close it and closing
  it belongs to a later plan**: the candidates are decoding outside the shell process and
  validating the bytes before one sees them, and choosing between those is a design pass
  rather than a step in an implementation plan.
- `scan.rs`: `d` describes the link, not its target, so a symlink to a directory
  reports false. The UI still does not guess the target: only `d` navigates directly, and
  every other row goes to `flea --open`, which canonicalises and answers 3 for a directory,
  so the pane navigates on that answer rather than on a guess of its own. The ICON does
  follow the target, see "Icons in the row".
- `run.rs`: an unreadable stdin line stops the loop after an error line, because the
  stream's framing cannot be trusted past it.
- `tools/flea-bench`: refuses to run against tmpfs, because `drop_caches` cannot evict
  pages that are already the filesystem.
- `tools/flea-bench`: the directory-exists check it runs before a warm pass reads the
  directory and warms the cache, so a cold run must skip that check entirely; that is
  what `COLD=1` is for.

### Icons in the row

`icon_for` answers a name for every row, and two of those names were wrong before Plan 5
put a renderer in front of them.

A symlink to a directory keeps `d:false`, because `d` is the link's own type and
`Pane.openCursor` navigates on it. Only the icon follows the target, through
`Meta::target_is_dir`, which `stat_range` fills with a second `metadata()` call made for
symlink rows alone. `stat_range` is already scoped to the window a client asked for, so
this cannot become a directory sweep. `t` stays true for every symlink whatever its
target, which is a documented gap the renderer guards against rather than a narrowing
this made.

That second call follows the link, and following a link is a blocking syscall. On a remote
mount that syscall no longer runs on the loop: the window stat goes through `window_metas` in
`backend/run.rs`, which stats inline with no listing clone on a local mount and moves only the
window's rows to a mount-keyed worker on a remote one, bounded by `CALL_DEADLINE` like every
`iomount::call`. The thumbnail path is bounded the same way: `thumb_rows` in `backend/thumbreq.rs`
stats each client-named row through `iomount::call` under `"thumb"` before it queues anything.
`listpaths`' first window takes the same path through `write_window`, so a remote base stats on the worker too.
This box has no NFS, no CIFS and no sshfs, but it does mount `fuse.gvfsd-fuse` at
`/run/user/1000/gvfs` and `fuse.portal` at `/run/user/1000/doc`, so a symlink into a live
gvfs mount whose server goes away is a reachable case here rather than a hypothetical.
The exposure is bounded by the window: `stat_range` only ever runs over the rows a client
asked for, so a 100,000 row listing with 350 rows held makes at most 350 of these calls,
and only for the symlinks among those 350. There is no way to put a timeout on a synchronous
`stat` without giving each row its own thread, which is an absurd price for an icon, so the
bound above is per window rather than per row: one hung symlink in the window still costs the
window its deadline, answered as "<mount> is not responding." on a remote mount. `window_metas` picks the path by the listing's base (`run.rs:506`), so a local listing's symlink into a hung mount still follow-stats on the loop through `stat_range` (`meta.rs:111`) and stalls the backend, and that corner stays undefended on purpose.

190 of the 613 distinct `application/*` types in `globs2` have no `generic-icons` entry on
this box and fall through to the class arm. Both counts move with the installed applications,
because `update-mime-database` merges `/usr/share/mime/packages/`: a root built from
`freedesktop.org.xml` alone gives 138 of 552, which is why no test asserts a row of these
tables, see `docs/protocol.md` and issue 1. That arm used to answer `application-x-executable` for
all 190, so a `.pem` drew as a program. The row's own execute bit now decides, the same
three bits `Row.nameColor` reads: set gives `application-x-executable`, clear gives
`application-x-generic`. Of the 190, three are usually executable and reachable by a glob:
`application/x-sharedlib` (`*.so`), `application/x-object` (`*.o`, `*.mod`) and
`application/x-ms-dos-executable` (`*.com`). They still draw as programs when the mode
says so. `application/x-shellscript` and `application/x-pie-executable` have no `globs2`
entry at all, so `mime::Db` can never produce them; `*.sh` is `text/x-shellscript`, which
`generic-icons` lists as `text-x-script`, so a shell script never reaches this arm.

A name no glob claims has no type at all, and `mime::Db` never reads a file's bytes, so a PIE
named with no extension never becomes `x-pie-executable` or `x-sharedlib` here. Before 0.3.11 that
row answered `text-x-generic` whatever its mode, so an extensionless program drew the document
mark in every listing and on the run card, while `flea --open` had already read its magic and asked.
Now a row with no type and any execute bit on a non-symlink answers `application-x-executable`, the
same rule an unlisted application type gets. A symlink's lstat mode is always `0777`, so its own bits
never count. `peek.rs` passes mode 0 by design, so a neighbour column's extensionless program still
draws the document mark; fixing that needs a stat per peeked row, which the name-order peek cost comment rules out.
On a mount whose filesystem keeps no POSIX modes (the FAT family, NTFS, or CIFS and SMB without mode-preserving extensions), a file's execute bits can come from the mount's fixed mode, so `rows_line` clears them before it asks for the icon and sends `x:false` only on those rows so `Row.nameColor` uses foreground instead of executable ink; `Row.p` keeps the reported mode for the Permissions column and mode cell, and ordinary rows gain no wire bytes. CIFS and SMB3 keep file modes when their mount's super options carry any of `posix`, `unix`, `cifsacl` or `modefromsid`. `rows_line` reads the mount table at most once per window, never for a gvfs base, through `mountinfo::read_body`, and `mountinfo::modes_are_real` decides. A gvfs path is decided from its spelling, unresolved, using its backend: SMB, MTP, AFC, gphoto2, WebDAV, HTTP, Google Drive and OneDrive have no real Unix execute bits; SFTP and unknown backends keep their reported bits. The existing `phone` and `network` classes stay unchanged. For other rows, `mountinfo::modes_are_real_for_dev` decides from the row's held `Meta.dev` and the matching mountinfo entry's filesystem type and super options, cached once per distinct device in the window. The device comes from the existing stat, so a symlinked base or parent keeps the target filesystem's rules without resolving any path on the backend loop, except for a gvfs target whose shared proxy device cannot identify its backend. A zero or unlisted device, including an anonymous btrfs subvolume device, falls back to the lexical mount owning `base.join(name)`; a failed device lookup is cached, and its lexical fallback is cached per (device, borrowed directory part of the name), so plain rows share one decision per device while search and listpaths rows share only the same device and parent spelling. The key borrows the name without allocating, and the fallback builds `base.join(name)` only on a cache miss. `fatnames` uses the source device from its existing lstat, with the same gvfs-first and lexical fallback rules. Row mode decisions add no path resolution or stat, which `rows_mount_tests` gates by counting forbidden path-call sites in the production row and mount helpers; restoring a canonicalize call makes that gate fail. Both icon callers clear the same named `icons::ANY_EXECUTE_BIT` mask. `collide.rs` and `merge.rs` still pass a FAT source's bits through, since neither holds the source's mount type.

`Row.iconSource` turns that name into a source. `Quickshell.iconPath(name, true)` does not
answer a filesystem path: it answers the image provider URL `image://icon/<name>`, and the
empty string for a name the theme lacks. Measured against `Yaru-wartybrown-dark`, both
`application-x-generic` and a made-up name answer empty, which is why the call is the OEM's
own two-step from `AppLibrary.qml`, name then `text-x-generic`. Cutting the second half
leaves every `application/*` row with no source at all.

That lookup needs a Qt platform theme in the process environment. Hyprland's `envs.lua`
exports `QT_QPA_PLATFORMTHEME=gtk3` and the session republishes it, so a user launch
resolves icons; a bare ssh inherits neither, and `omarchy-drive env` does not emit it, so
every name answers empty and every row draws nothing. `tests/ui.sh` imports it from
`systemctl --user show-environment` rather than hardcoding it, and so does
`tools/flea-field-bench`. The harness did not until Plan 5 Task 3, so every Flea row in
`docs/baseline-2026-08-29.md` is a Flea with no icon theme. **Both corrections below come from
Task 3's own A/B at `32a3e68`, three cold runs an arm, differing in nothing but whether Qt
loaded the gtk3 platform theme. Neither is a subtraction across the two documents**, which
would cross a unit boundary; what licenses applying the A/B to that document's rows is that its
theme-off arm reproduces them to within 0.02 MB, 45485 kB against a printed 45.5 and 44805
against a printed 44.8. **PSS: the theme is worth 3285 kB on the scale fixture and 3824 kB on
the media fixture**, 45485 against 48770 and 44805 against 48629, that is 3.21 and 3.73 MiB, so
the understatement is a pair and not the one number it was written as. **CPU: up to 0.08 s, and
that is the scale figure alone**, medians 0.22 s against 0.30; media moved 0.26 to 0.29. **That document's Flea rows are not a valid before for
anything in this plan**, and `docs/baseline-2026-08-30b.md` supersedes them. Of the rest
of the compared apps only `dolphin` is Qt and can move at all; the GTK entrants and the terminal ones
cannot see the variable. The same trap catches a launch by hand: `~/bench/flea-relaunch.sh`
does not publish the variable either, so a window started through it draws blank icon slots
with thumbnails still working, which reads as a renderer bug and is not one. Export it from
`systemctl --user show-environment` for any by-hand launch.

The slot is `Theme.iconSize`, which is `rowHeight` less twice `spacing.rowPaddingY`, which is
by construction the row's own text line box, `Math.round(font.bodySmall * lineBoxRatio)`. It is
23 px at this box's text size, from a `rowHeight` of 37 and a `rowPaddingY` of 7, and it moves
with `omarchy display text size` because every term in it does. No pixel constant appears
anywhere on the path, which is why `tests/ui.sh icons` asserts the rendered pitch off `itemRect`
against `Theme.fileRowHeight`, the text line box with its padding scaled by density, rather than
against a number.

Qt keys the pixmap cache by source URL, so the column is bounded by the number of distinct
names a directory produces and not by the row count. Measured at 256 px against a window
holding no images: 335 `Image`s sharing one source cost 3.2 MB, ten distinct sources over
the same 335 cost 14.3 MB, and decoding per `Image` would have cost about 84 MB. The column
is not memoised. `Quickshell.iconPath` costs 0.6 to 0.9 microseconds per call once warm,
because Quickshell caches it already, so memoising the name would save about 0.3 ms across
a whole 335 row scroll. What the column does cost, 2.3 MB and 0.02 CPU s on both fixtures,
is the decode, the texture upload and the per-delegate `Image`, none of which a lookup memo
touches.

**That same cache is also how a stale frame reaches the screen, and the row carries an mtime query
because of it.** A freedesktop thumbnail path is the md5 of the file URI, so a regenerated thumbnail
lands at the SAME path, `Image.source` never changes, and Qt hands back the pixmap it already has.
Every correctness check on this branch compared URLs and stayed green while the screen was wrong.
Reproduced by pixel on 2026-08-30: a flat red JPEG replaced by a flat blue one, the pane left and
re-entered so `Pane.open` clears the thumbnail map and the row is asked again, the PNG on disk going
from RGB 220,20,20 to 21,20,220, and the icon slot still reading 217,20,20. `iconSource` now appends
`?m=` and the row's own `m`, which is exactly the key the backend validates the cache entry against,
so the URL moves when and only when the entry is no longer valid. `QUrl::toLocalFile` reads the path
alone, so the query changes Qt's key and not the file that opens.

**`cache: false` was the other candidate and it was measured, not argued about.** Five interleaved
rounds, both fixtures, both a settled listing and one after ten scroll bursts, every arm launched
through `flea --gui` so the launcher's `PR_SET_THP_DISABLE` is in the picture. `cache: false` cost
PSS in the settled column of both fixtures in 5 of 5 pairs, median +418 kB on scale and +292 kB on
media, because it stops the whole icon column sharing one pixmap per name. The mtime query keyed only
the thumbnail branch and came out inside the noise, medians -79 and -6 kB with the sign flipping
across rounds. Memory is the one GUI column where Flea is not the lowest on the media fixture, so the arm that
costs nothing is the arm that ships.

**What the mtime query does NOT cover.** The row's `m` comes from the listing while the backend
validates against a fresh `metadata()` call at thumb time, so a row re-asked without a re-list keeps
its old key. The only path to that is the 240-entry `thumbCap` evicting a row, and scrolling far
enough to evict 240 entries crosses `refetchMargin` and refetches the window's rows too, which
refreshes `m`. `cache: false` has no such hole; it was still the wrong trade.

The IPC seam reports `icon.source` through the `iconUrl` alias and not `iconSource()`,
because the two can disagree: cutting the `source` binding while leaving the function
intact drew no icons at all and left the case passing. `sourceSize` caps the themed PNG,
which is the only half that applies here, since Yaru ships mimetypes and places as PNG
alone at 16, 24, 32, 48 and 256, so a 23 px slot resamples the 24 px art by 23/24.

### The row mark: Shape, not a generated font

Plan 5 Task 5 registered a decision rule before running any measurement, writing the rule
down before looking. The contest, spec section 5aa: a lucide mark drawn as `Shape` with
`preferredRendererType: Shape.CurveRenderer`, the same pattern the author's other Omarchy
plugins already use for `AdGuardIcon.qml` and `SlackIcon.qml`, against the same mark shipped
as a generated subset font rendered through `OpticalGlyph`. `Shape` wins unless it costs more
than 2 MiB of PSS or more than 15 percent of scroll CPU against the font arm across nine
interleaved pairs on the media fixture. It wins ties: no toolchain, no binary blob, no build
step, and two prior good results in this project. The 2 MiB threshold is set by the budget:
media PSS is the fourth lowest and 2.80 MiB above the third, a figure this plan may not make worse.

The font arm never reached that measurement. `python3 -c "import fontTools"` on the dev box
raises `ModuleNotFoundError: No module named 'fontTools'`. `pacman -Ss python-fonttools`
answers `extra/python-fonttools 4.63.0-1`: the package exists upstream, nothing on this box
has ever pulled it in, and installing it needs a `pacman -S` this task was told not to spend,
even though the sudo chain to run one works headless. A gate meant to be decidable cheaply
does not buy a win with a box mutation, so the check stopped there rather than installing a
toolchain to measure the thing the toolchain requirement was already an argument against. An
arm that needs a toolchain this box does not carry, a checked-in binary blob no diff can
review, and a name-to-codepoint map loses on the toolchain axis alone, which the registered
rule already treats as decisive by itself. The nine interleaved pairs were never run; there
was no font glyph built to interleave against.

`Shape` wins by the registered rule, on the toolchain axis, without needing the PSS or CPU
numbers. `ui/Glyph.qml` is the one mark this task ships: the `file` outline at lucide's
native 24 unit grid, `strokeWidth` fixed at 2, and a `Scale` transform doing the resize so the
stroke never doubles with it. It is registered in `ui/qmldir` per "Every component needs a
qmldir line" above. Task 6 wires it into `ui/Row.qml` in place of the themed `Image` and
supplies the rest of the set; this task decided only the renderer.

### Lucide path data

Lucide-static 1.38.0, ISC licence, fetched from `https://unpkg.com/lucide-static@1.38.0/icons/
<name>.svg` (unpkg resolves `@latest` to that version as of 2026-08-31); source repository
`https://github.com/lucide-icons/lucide`. `ui/js/Icons.js` carries fifteen marks this way:
`file`, `folder`, `file-text`, `code`, `image`, `film`, `music`, `archive`, `type`, `terminal`,
`table`, `presentation` for the row map, plus `house`, `download`, `folder-git-2` for the
sidebar. `Icons.js` stayed one file at 79 lines against its 300 hard cap, so the split into a
separate `IconPaths.js` this task's brief anticipated was not needed.

The brief expected some marks to carry "a second optional path"; the real set needed as many as
eight shapes in one icon (`film`: one rounded rect plus seven strokes), so `Glyph.qml` instead
takes one `PathSvg` per mark whose `d` string is every source `<path>`, `<rect>` and `<circle>`
joined into one multi-subpath SVG path, which `PathSvg` already draws as a single shape. This
is simpler than a second-path property and needed no change to `Glyph.qml` beyond reading
`Icons.pathFor(name)`, one line. That held until a board filled part of a mark: Invert selection's right half disc
is solid in MenuAdditions040, and a stroke cannot draw a solid half. `Icons.js` `FILLED` names such a sub-path by
mark, and `Glyph.qml` builds `ui/GlyphSolid.qml` over the stroke the first time a mark names one, `contrast` alone
today. It is built on demand because a second `ShapePath` in every glyph put `tests/rowcost.qml` at 18 objects a row
against its ceiling of 17; every other mark keeps one shape and a transparent fill, which `tests/menu-card-sink.qml`
pins. Hatching the half with stroked lines was the stroke-only
alternative and was refused: abutting antialiased edges leave seams and the table would no longer hold the board's path.

A `<rect>` or `<circle>` element has no `d` attribute, so each was converted with the standard
rounded-rect-to-path and circle-to-two-arcs formulas rather than typed by hand. Concatenating
several independently authored `<path>` elements into one string surfaced a real bug in the
first pass: a leading lowercase `m` is only equivalent to an absolute `M` when it is the very
first command in the whole string, and several lucide marks (`code`, `terminal`, `download`,
`presentation`, the diagonal in `image`) lead with `m` followed by a bare-number implicit
lineto tail, whose relativity is inherited from that leading letter. Naively forcing every
leading `m` to `M` reset that tail to absolute too and drew a diagonal line to the wrong point;
the fix re-issues an explicit lowercase `l` before the tail so its relativity survives the
letter change. Every one of the fifteen conversions was checked against this: `rsvg-convert`
rendered the original multi-element SVG and a synthetic single-path SVG carrying the converted
`d` at 240x240, and `magick compare -metric AE` came back under 4 differing pixels out of
57,600 for all fifteen (antialiasing only) once the fix landed, against thousands of differing
pixels for five of them before it.

**The Omarchy cut, 2026-08-31.** The set was recut for the brand pass on `oem-look`: every
rounded corner lucide bakes into a `d` as an `a2 2` arc became a hard corner, keeping the
source glyph's bounding extents (a first pass shrank `file`, `file-text`, `folder` and
`download`; review round one restored them to lucide's box), genuine curves stayed (circles,
the `folder-git-2` pipe), and `Glyph.qml` switched RoundCap/RoundJoin to SquareCap/MiterJoin,
the edge of `omarchy-logo.svg`'s square spiral. The mechanical rule for recutting any stock
lucide glyph is therefore: square the corner arcs, keep the extents. One deliberate geometry
change beyond corners: `music` note heads are squares, not circles, the set's brand tell
(`file-text`'s three rules are lucide's own, restored with the body). The AE compare
above no longer applies to recut marks (they differ from source on purpose); the gate is the
retained QML reference's montage eyeball plus a live `QSG_RHI_BACKEND=vulkan qs -p ui` look on the box, which names the
renderer but not `FLEA_RENDERER_AUTOMATIC` and so leaves the QML fallback arm disarmed (bench
numbers still come from the launcher path, hard rule 7). `rsvg-convert` only
proves a `d` parses; librsvg renders through a bad tail and exits 0, so its exit status is
not a gate.

**The row-name distribution, re-derived 2026-08-31** (the task brief's own figures, checked
against the artefact and not just quoted): `cut -d: -f2 /usr/share/mime/generic-icons | sort |
uniq -c | sort -rn | head -30` gives `x-office-document` 106, `package-x-generic` 89,
`application-x-executable` 57, `text-x-generic` 46, `image-x-generic` 40,
`x-office-spreadsheet` 38, `text-x-script` 36, `font-x-generic` 23, `x-office-presentation` 21,
`video-x-generic` 15, `text-html` 11, then a tail of five or fewer each, exactly matching the
brief.

### The Network row marks: TailscaleMark and DropboxMark

**Both third-party marks are reproduced from the official artwork, not recut**, and neither appears
in `ui/js/Icons.js`. The reproduction was chosen after both recuts were rendered beside their originals
at 19, 24, 32, 48 and 96 px, which brackets the sizes these actually draw at: a 2 unit stroke on a 24
unit grid spends most of a small mark on outline, and a one-weight language cannot carry the
Tailscale mark's own emphasis, because varying size to signal weight destroys the uniform grid that
is the mark's strongest feature. **The recut ruling is superseded and must not be restored.**

`ui/TailscaleMark.qml` is nine circles of radius 18 at centres 30.5, 84.5 and 138.5 on both axes of
the official `0 0 169 169` box, four at full opacity and five at 0.4. The two opacities are
load-bearing: the brand toolkit says the gray dots adapt in opacity and says to avoid single-colour
reproduction, so one palette role at two opacities is the correct reading. `ui/DropboxMark.qml` is
five tiles on a box `iconSize * 1.18` by `iconSize`, centres at 0.25/0.75/0.25/0.75/0.50 across and
0.188/0.188/0.564/0.564/0.812 down, each half the box wide and 0.376 of it tall. The wide box is the
AdGuard lesson: scale to the drawn box, not to a square slot, or the mark renders smaller than its
neighbours.

Both sit outside the 24 grid and outside `Glyph.qml` sizing, where the brand marks sit, and both keep
a `ui/qmldir` line. Monochrome and palette-tinted, no brand colour anywhere.

**The geometry replacement retired the Font Awesome Free 5.4.1 Brands "dropbox" derivation** that the
previous `DropboxMark` carried, and with it the CC BY 4.0 attribution the repo would otherwise have
taken into a public release. Nothing in the tree derives from Font Awesome now.

SMB and WebDAV rows use lucide's `server` glyph instead of a hand-drawn mark, added to
`ui/js/Icons.js`'s `PATHS` the same way as the fifteen marks above: `lucide-static@1.38.0`'s
`server.svg` is two rounded `<rect>`s (converted with the same formula as `image`'s outer
rounded rect) and two near-zero-length `<line>` elements, which is lucide's own technique for
the rack unit's two LED dots since `Glyph.qml`'s `RoundCap` turns a zero-length stroked segment
into a filled circle. `rsvg-convert` at 240x240 plus `magick compare -metric AE` against the
original four-element SVG came back 0 differing pixels, an exact match, because a path made of
straight lines and circular arcs has no curve-fitting error to introduce.

### A second ContextMenu instance breaks the whole window's keyboard focus

Task 15's first cut of the Network group's unmount action reused `ui/ContextMenu.qml` a second
time, one instance owned by `ui/Sidebar.qml` for "Unmount" alongside `ui/Pane.qml`'s own instance
for "Open", reparented to Pane's FocusScope on open so its focus-restore-on-close logic could find
"list" as a real sibling. Live on the dev box this broke every keyboard key in the whole app, not just
near the menu: `j`, `Tab`, arrows, all silently did nothing, from the moment the window opened,
whether or not the second menu was ever actually shown. Bisected by swapping files against the
committed HEAD one at a time (`git stash`, then `git show HEAD:<file> > /tmp/x.qml` and diffing
combinations): the original `Pane.qml` plus original `Sidebar.qml` plus the new `shell.qml` alone
kept working, and the fault isolated to the second `ContextMenu` instance's mere presence in the
tree, not to reparenting (removing only `parent: root.parent` did not fix it) and not to
`NetworkMounts.qml`'s Timer or Process activity (stubbing those out did not fix it either).
`ContextMenu.qml`'s inner `action` Item declares a bare `focus: true`, unconditional on `opened`;
with two such instances live in the same FocusScope, Pane's own `list` (also `focus: true`) loses
the scope's active-focus slot at startup, apparently regardless of either menu's visibility. The
fix was not to explain the exact Qt Quick internals further but to drop the second instance. The
finding is about a SECOND instance, not about the rail: the rail raises `ui/Pane.qml`'s own single
`ContextMenu`, which is the shape below. Reintroducing a second focus-claiming popup anywhere in
this window should re-run this exact bisection before shipping it.

### The rail raises the pane's one menu, and the two-right-click arm is gone

For a while a right click on a mounted rail row armed it and a second right click within 4000 ms
fired the release. That worked and nobody could find it: no menu, no button, no hint, and a
user reported the eject as missing. It was never an accelerator either, because it cost the
same two deliberate acts a menu costs and named neither of them.

So `ui/Sidebar.qml` `openRailMenu` now hands `ui/Pane.qml`'s single `ContextMenu` the rows to draw,
through `openForRail(key, entries, scenePoint)`, and `ui/js/Mounts.js` `rowMenu` decides what a row
offers by returning `railMenu`: `Open` first on a mounted row, then the release, then the place's
own rows, and a row that offers nothing opens no menu at all. Unmount draws the `drive` mark and
Eject keeps its own. A chosen row comes back as `railChosen(action, key)`
and the key, not the index, is what resolves it: the rail rebuilds on a five second poll, so the
position the menu opened over can name a different row by the time a row inside it is chosen (see
`ui/js/Mounts.js` `rowByKey`). Right click never activates a rail row any more, so it cannot mount
and open a stick somebody only meant to ask about.

### A press on the revealed auto-hide rail takes the keyboard from the Sidebar, not from the pane

`ui/Pane.qml` `focusPointer` is a `PointHandler` on the pane, and 29bc8ba5 (w77) taught `ui/PaneRail.qml`
`focusPress` to map its press into the shown Sidebar and give an overlay rail the keyboard. That worked only
while nothing below the pane grabbed the press: a handler that takes the exclusive grab on press starves every
handler on an ancestor, so the pane's `PointHandler` is never activated. be2380ae gave `ui/SidebarRow.qml`'s
`TapHandler` `ReleaseWithinBounds` under auto-hide (so the covered listing row cannot select the same click),
which is such a grab, and from then on a click on a rail row left the keyboard on the list while the rail cursor
moved (native `railpointer`). The rail's blank ground and a right click on a row were starved the same way: the
Sidebar's interactive `Flickable` takes the press on blank ground, the row's tap takes it on a row. Measured in
`tests/sidebar-flows.qml` with the rail an overlay: no `focusPointer` activation and no `focusPress` call for
any of the three.

So `ui/Sidebar.qml` reports its own press: a passive `PointHandler` on an `Item` above the `Flickable` and every
row (z 1) emits `pressed()` on any button, and `ui/PaneRail.qml` sets `railPane.focusView = Focus.RAIL` on it
only when the rail is an overlay. A passive handler on the topmost item sees the press first and leaves it to
the row, so the row still owns its click and the covered listing row is still never selected. A docked rail is
unchanged: its press never moved `focusView` (`focusPress` finds the press inside the Sidebar and does nothing
unless `overlay`), and `tests/sidebar-flows-rail.js` pins that. The pane's `focusPress` stays for presses the
Sidebar does not see. Other press grabbers over a pane (the preview shield, media strip, scroll bar, header
resize handle, the loading shield) are keyboard-neutral today, and nothing here changes that; the row, tile and
column-cell taps in the listing views keep the default `DragThreshold` policy, so the pane's `PointHandler`
still fires for them and a dual view row click still focuses its pane through `focusRequested`.

The reveal's hover flag cannot outlive the Sidebar. `ui/PaneRail.qml` `over` is `railHover.hovered`, a
`HoverHandler` on the rail Loader that is enabled only while the Loader is active, not a flag a handler inside the
Sidebar set: that handler died with the unload and never reported false, so an unload under the pointer (auto-hide
switched on over the docked rail, or Ctrl+B hiding it) left `wanted` stuck true and the edge strip unable to reveal
the rail. Pinned in `tests/sidebar-flows-rail.js`; 0.3.7 had the same defect.

### m raises the listing's menu too, under the cursor row

`m` is one action, `menu`, and `ui/js/Focus.js` routes it by focus view. In the rail
`ui/js/RailKeys.js` `act` calls `Mounts.raiseMenu`, which opens the rail menu whenever `Mounts.rowMenu` has rows and says
`<label> has nothing to eject or unmount.` only when it has none. In the listing `act`'s `menu` case calls `ui/Pane.qml` `openCursorMenu`, whose body is
`ui/js/Menu.js` `openAtCursor`: it scrolls
the cursor row into view (a wheel scroll in the grid can leave it off screen), opens the one
`ContextMenu` at that delegate's bottom-left through `openAt`, and answers whether a delegate was
there at all; an empty directory, a listing still loading and a filter that hid every row all
answer false, and the status line says `No item under the cursor to open a menu on` rather than
nothing. Right click and `m` reach the same menu, so a selection is honoured the same way by both.
`m` stopped being a type-ahead letter for this. The menu's own `keyCatcher` reads
`Keymap.lookup`, so `j` and `k` step it exactly as Down and Up do, and Space chooses like Enter.

### A FileView write can race a reload fired the moment setText() is called

`ui/NetworkDialog.qml`'s bookmark write and `ui/NetworkMounts.qml`'s bare-root share expansion
(below) both write `~/.config/gtk-3.0/bookmarks` through their own `FileView.setText()`, then
signal the caller to `reload()` a DIFFERENT FileView instance (`ui/Sidebar.qml`'s own read-only
`bookmarksFile`) pointed at the same path. `setText()` queues its write rather than blocking, so a
`reload()` fired in the very next line can start its read before that write has actually landed,
seeing stale (pre-write) content; the file's own watcher can self-heal this later, but not
reliably inside a five-second test window, and a real user would see the rail miss its own most
recent edit just as often. Measured live: two of four consecutive runs of `tests/ui.sh network`
passed and two failed with an empty `networkEntries`, and adding an unrelated debug delay before
the check happened to make it pass again, which is what pointed at a race rather than a hard
failure. The fix is `bookmarksWrite.waitForJob()` (blocks until the current async operation
finishes) between `setText()` and the signal that triggers the reload, in both files. Four
consecutive clean runs after the fix, two of four before it, on the same box.

### An unread FileView drops an empty write

`FileView.setText("")` does nothing at all when the view has never loaded: no truncate, no create,
and `loaded` stays false, while the same call on a view that has read truncates the file as asked.
Measured on this box with a four-way probe under `QT_QPA_PLATFORM=offscreen qs -p`, on a path
created after the view was made: writing `"OTHER\n"` landed and left `loaded` true, writing `""`
left the file byte-identical and `loaded` false, and it made no difference whether the file had been
created by another `FileView` in the same process or by an external `sh`. So the trigger is the
empty body and the unread view, not two views on one path.

This is the whole of a defect `tests/ui.sh case_network` caught: removing the only saved place
computes an empty body, and the rail's write view has never read when the bookmarks file did not
exist at launch, so the bar said the place was forgotten while the file kept its line and the row
came back on the next rebuild. `ui/NetworkPlaces.qml write(body)` reloads first when
`!bookmarksWrite.loaded`; `reload()` followed by `waitForJob()` loads synchronously, measured the
same way, after which the empty write truncates. The write view is unread far more often than it
looks: nothing reloads it, `ui/NetworkDialog.qml` creates the file through its own view, and
`watchChanges` would not help because a watch set up before its parent directory existed never
fires.

### A failed FileView read still reports loaded, and empties the text it had

`FileView` has three things that look like a way to ask whether a read worked and none of them is
one. `waitForJob()` returns `true` for every job it waited on, success or not. `loaded` is
`isLoadedOrAsync`, so it reads **true** after a permission-denied read and after a read of a path
that is a directory; only `FileNotFound` leaves it false. And a `reload()` that fails **clears the
text the view already held**, so `text()` answers `""` where a moment before it answered the file.
Measured on quickshell 0.3.1 under `QT_QPA_PLATFORM=offscreen qs -p`, one view on one path: a first
read of a two-line file gave `err=Success textLen=36`, a `chmod 200` and a second `reload()` gave
`err=PermissionDenied loaded=true textLen=0`, and the append that followed left the file holding one
line. `onLoadFailed(error)` is the only report there is, and `FileViewError` is
`Success, Unknown, FileNotFound, PermissionDenied, NotAFile`.

This is the whole of a defect this release's own fix round introduced. `ui/NetworkDialog.qml`
`appendBookmark()` re-reads before it appends, because a view that is not watched still carries a
place the rail has since removed; the re-read closed a duplicate-one-line defect and opened a
delete-the-file one, because a read that failed made `body` empty and the blocking `setText()` that
followed wrote a bookmarks file containing only the new line. The view records the error in
`onLoadFailed` now and the append refuses on anything but `Success` or `FileNotFound`, an absent
file being the one read that is legitimately empty: the dialog stays open over "Saved places could
not be read, so nothing was written." and nothing is written at all. `tests/ui.sh case_network`
drives it with the fixture's own bookmarks file at mode 200, and asserts the file byte-for-byte.

The class reaches every body that grows, so it also reached the rail's rename, where nothing in that
fix round had put it. `ui/NetworkPlaces.qml` `rename()` derived its body from `bookmarksText`, which
is `ui/Sidebar.qml`'s own watched `FileView.text()` and is emptied by the same failed read, so
`Places.relabel("", uri, name)` appended the renamed share to nothing and the blocking `setText()`
wrote a bookmarks file holding that one line. Deriving the body from `bookmarksText` is what makes
the guard impossible there, so `rename()` reads the file itself and refuses on the same two errors.
Only a **live mount** reaches this, because the rail draws a saved place from the very text the
failed read emptied, and the rename arm in `tests/ui.sh case_network` stubs `gio mount -l` for one.
`forget()` needs no guard: it writes only a body it found the line to remove in, and `""` has none.

### A server root with no share segment mounts, but GVFS gives it no FUSE path

A NAS bookmark seen in real use (`smb://192.168.1.10/`, no share name) is exactly the "browse
the whole server" shape nautilus itself writes when you connect to a server without picking a
share first. `gio mount` on it succeeds (or reports "already mounted", itself a live GVFS quirk
for this shape: see "The real gio mount -l sample" above), but `gio info` on it prints no
"local path:" line at all, because GVFS only exposes a FUSE mount point for a mount that names a specific
share, never for a bare server-root browse. Activating this kind of entry used to end at "This
network location has no browsable folder," a dead end for the bookmark shape a NAS user
actually has. `ui/NetworkMounts.qml`'s `isBareRoot()` recognises the shape (`scheme://host/` or
`scheme://host`, nothing after) and, only when `gio info` comes back with no local path for it,
runs `gio list` on the same URI, which DOES enumerate the server's shares over the same connection.
Each discovered share becomes its own `smb://host/share/` bookmark line (deduped against what the
file already carries, so a second activation reports nothing new instead of growing the file every
time), and the message names how many were added. One click on a real "NAS" bookmark
now turns it into five separate, individually mountable rows: appdata, backups, data, isos,
timemachine, each of which opens a real GVFS FUSE path once mounted, verified live against the
real NAS both through gio directly and through Flea's own rail.

**Superseded, fix round 2:** the bookmark-expansion design above was replaced, because clicking
the NAS in the sidebar should show all of its folders and let the user go into them, nothing more. The
detection this section describes (`isBareRoot()`, `gio info` with no local path, then `gio list`)
still stands, but the terminal action changed: `gio list`'s own names are no longer written into
the bookmarks file at all. See "The share browser overlay" below for what replaced it.

### The rotating empty-state caption, exact OEM values

Read directly off `/usr/share/omarchy/shell/Ui/PanelHero.qml`'s `meta` text (the component both
`tailscale/Panel.qml` and `io.github.thisisgm.omapods/Panel.qml` drive with an identical
`phraseTimer`/`activePhrases`/`phraseSwap` triple): the rendered text is `root.meta.toUpperCase()`
even though every source `activePhrases` array is authored in ordinary sentence case ("Encrypting
connections", "All gone Pete Tong"); the colour is `root.dim`, `Qt.darker(foreground, 1.4)`, not
the row's own muted role; the font is `Style.font.caption`, `font.bold: true`,
`font.letterSpacing: 1.2`. The rotation itself is `Timer { interval: 2800 }` driving a
`SequentialAnimation` of two `PropertyAnimation`s on opacity, 180 ms out (`Easing.OutQuad`) then
260 ms in (`Easing.InQuad`), with the phrase index advanced by a `ScriptAction` in between, not a
plain text swap. `ui/EmptyState.qml` keeps the order, the interval and the index swap, and its fades
take Flea's OutCubic at `Motion.durMs` (140 ms out, 180 ms in), the curve every Flea transition uses; the file's own eight-phrase
list stayed sentence case in its source, matching `activePhrases`' own authoring convention, with
`.toUpperCase()` applied only at render time.

The GPUI hero's rotation loops while shown: every 2800 ms, the first after a full interval, the phrase
and FleaMark beat together and the phrase wraps from the last back to the first. A hide resets the count.
The GPUI hero waits on one timer between beats and requests frames only during the fade. The QML oracle
still stops after three beats to save CPU (2.4 s per 20 s against 0.08 s still), and
`tests/emptybeats.sh` gates that stop.

**Correction, fix round 2:** this entry originally recorded the darken factor as `1.55`, read off
an identically-named `dim` property in `tailscale/Panel.qml` governing its own rows rather than
`PanelHero.qml` itself. `PanelHero.qml:22` reads `readonly property color dim: Qt.darker(foreground,
1.4)`; `ui/EmptyState.qml` and the value above are both corrected to `1.4`.

### Motion: what stays animation-free

`ui/js/Motion.js` owns the curve and the two durations every structural transition answers with,
OutCubic at 180 ms to reveal and 140 ms to hide. Scroll, cursor and hover fills stay
animation-free, except `Scroll.js` touchpad momentum, and reduced motion snaps instead of easing.

### Capitalization, derived from the shipped plugins and not from a stale doc comment

Grepped every `text:` literal across `tailscale`, `dropbox`, `agents`, `weather`, `clock`,
`bluetooth` and `network`'s `Panel.qml` files under `/usr/share/omarchy/shell/plugins/`. Three
clear, internally consistent registers, by what the string IS rather than which component renders
it:

- **Group and section headings** (`PanelSectionHeader`, and the compact stat tags like weather's
  "FEELS"/"WIND"): **ALL CAPS** in every real usage found ("CONNECTIONS", "MACHINES", "BALANCE",
  "DNS PROVIDER", "RECENT FILES", "SCANNING WI-FI…"), with no exception. `PanelSectionHeader.qml`'s
  own doc comment gives lowercase examples ("DNS provider", "Wi-Fi networks") that no shipped
  plugin actually follows; `network/Panel.qml` itself writes "DNS PROVIDER" in caps. The doc
  comment is stale against its own callers, so the rule here is read off the plugins, not the
  comment.
- **Buttons and clickable action labels**: sentence case, first word (and proper nouns) capitalised,
  nothing else: "Back to today", "Authorize Tailscale operator", "Start weeks on …", "Confirm"/
  "Cancel" (`ConfirmDialog.qml`'s own defaults).
- **Status, error and empty-state sentences**: ordinary prose, sentence case, a full stop for a
  complete sentence: "Tailscale CLI is not installed or not on PATH.", "No machines found on this
  tailnet.", "Fetching forecast…".

Applied this round: `ui/NetworkDialog.qml`'s two `PanelSectionHeader` texts, "Add network location"
and "Dropbox", became "ADD NETWORK LOCATION" and "DROPBOX" (heading rule); `ui/EmptyState.qml`'s
eight-phrase array was re-cased to sentence case in its source with `.toUpperCase()` at render
(rotating-caption rule, above). Every button label, status sentence and error sentence already in
Flea (`ui/js/Errors.js`, `ui/Pane.qml`'s inline messages, `ui/ContextMenu.qml`'s "Open", the
dialog's "Add"/"Cancel"/"Install Dropbox") already matched their registers and needed no change.
`ui/Row.qml`'s column headers ("Name", "Mode", "Size", "Date Modified") are Title Case, a fourth
register with no shipped-panel counterpart to check against since no OEM panel renders a data
table; left alone as a different genre, not swept.

### The row-swap PSS gate: measured, compared, shipped anyway

The registered rule for this task was that wiring the row through `Glyph` must not regress
media PSS, predicted instead to save memory by retiring `Quickshell.iconPath`'s URL-keyed
pixmap cache. Nine interleaved pairs per fixture, alternated old (this tree at `cabcc0e`,
`Image`-only) against new (this tree with the row swap), launched through the real product
path `flea --gui <fixture>` with `FLEA_UI`/`FLEA_BIN` pointed at each arm so
`PR_SET_THP_DISABLE` stays in the picture, settled PSS read off `smaps_rollup` once three
consecutive 300 ms samples sat within 256 kB. **Warm cache, not cold**: the sudo password
`drop_caches` needs was not available during this
run, so `drop_caches` was never run; the
comparison is still relative and same-process, which is what this gate asks.

Media fixture (`/home/flea-sandbox/flea-media-btrfs`), 9 pairs: old median 67,847 kB, new median 71,058 kB,
every pair higher for new, spread inside each arm under 350 kB. Scale fixture
(`/home/flea-sandbox/flea-bench-btrfs`), 9 pairs: old median 63,774 kB, new median 67,347 kB, same
direction, same magnitude. **The regression is a fixed roughly 3.2 to 3.6 MiB on both
fixtures**, not proportional to row or file count, which points at `QtQuick.Shapes` and
`Shape.CurveRenderer` module load rather than a per-row cost. A five-pair isolation swapping
only `preferredRendererType: Shape.CurveRenderer` for the Qt default renderer on the media
fixture attributes about 1.7 to 1.9 MiB of the total to `CurveRenderer` specifically; the
remaining roughly 1.4 MiB is the cost of having any `Shape`-based `Glyph` wired into the row
at all, even on the default renderer. Task 5's registered rule for `Shape` versus a generated
font never measured either arm's PSS, deciding on the toolchain axis alone; this is the first
real measurement of what `Shape` costs against the `Image` path it replaced, and on this box,
for this row set, it costs rather than saves.

**The plan's predicted saving is falsified.** The spec argued the swap would save memory,
"an icon on every row costs 3.2 MB instead of 84 kB" against the `Image` path's URL-keyed
pixmap cache; the measurement above says the opposite, 9 of 9 pairs on both fixtures, and
that prediction is recorded here as falsified rather than quietly dropped.

**The comparison check**, run against `docs/baseline-2026-08-30b.md` before deciding on the numbers
above. Media PSS, MiB: thunar 37.94, pcmanfm 38.74, nemo 53.41, flea 56.48, dolphin
96.08, nautilus 134.47. Flea plus the measured 3.2 MiB is about 59.7, **still the fourth lowest**; the
difference to nemo widens from 3.07 to about 6.3 MiB, and dolphin's 96.08 stays well
above it. Scale fixture: flea 53.75 against dolphin 194.84; plus the measured 3.5 MiB is
57.25, **still the lowest, 3.4x below dolphin**. The order does not change on either column.

**Ruling: ship the swap as implemented, `Shape.CurveRenderer` included**, not the cheaper
default-renderer arm. The plan this task serves put aesthetics first;
these marks are the single biggest visual change in the design, and the 1.8 MiB
`CurveRenderer` premium buys curve quality at exactly the 23 px size where the default
geometry renderer's tessellation aliases most. `AdGuardIcon.qml` and `SlackIcon.qml`, the two
prior marks built this way in the author's other Omarchy plugins, made the same choice for
the same reason. The budget line ("media PSS must not get worse") does not block this: it was
authored on the saving prediction this measurement falsifies, and the comparison check above shows
the budget's real purpose, keeping the measured order among the compared apps unchanged, is intact on both columns. The
ruling is reversible in one commit either way: swap `Shape.CurveRenderer` for the Qt default
renderer to recover about 1.8 MiB, or swap `Glyph` back for `Image` to recover the rest.

**One follow-on recorded, not built**: a per-glyph rasterised-texture cache (render each of
the fifteen marks to a small `Image`-backed pixmap once, keyed by name and colour, instead of
one live `Shape` per row) could recover most of the `QtQuick.Shapes` module and per-instance
cost without leaving the native-mark design, if this line item ever needs to be reclaimed.
Not attempted here: the ruling above ships without it, and it is a real project on its own.

### The thumbnail decode arm

`sourceSize` is set on the thumbnail `Image`, to the row's icon slot, and Plan 5 Task 6
measured both arms rather than inheriting an earlier finding. That finding measured `sourceSize` on
4K PNGs, where it makes decoding slower, because PNG has no scaled decode path so Qt decodes
the whole image and then pays a smooth scale on top. A freedesktop thumbnail is a different
regime, and that difference is the thing Plan 6 needs from this section: it is at most 256 px
on its longest side, so the common 16:9 case decodes to 256 by 144 rather than 256 by 256,
and the slot it lands in is 23 px, which is `Theme.iconSize` here, `rowHeight` 37 less twice
`rowPaddingY` 7. Nothing below transfers to a full size image.

The harness was two `qs` configs under `~/bench/thumbarm`, identical but for the two
`sourceSize` lines, each drawing 35 of the operator's own cache entries as distinct sources,
read and never written, over ten interleaved pairs. Wall time from the file list landing to
the last `Image` reporting `Ready` came out in the high forties to the mid sixties of
milliseconds for both arms, with medians a few percent apart and the sign of the difference
flipping between the two five-pair batches, so the arms are indistinguishable on time at this
scale. PSS separated cleanly and repeatably: the sized arm sat near the low 170s of MB and
the unsized arm near the high 170s to low 180s, a gap of roughly 6 to 7 MB for one screen of
35 rows, which tracks the 5.55 MB of RGBA those particular 35 files decode to. At 94 rows the
gap grew to about 17 MB against 14.5 MB of decoded pixels, so it scales with pixel count and
not with row count.

The rule was fixed before the numbers: take the `sourceSize` arm unless it costs more than
twice the other arm's time to fill a screen, because memory is the one GUI column where Flea is
not the lowest on the media fixture, while a screen of thumbnails is not on the first paint path at
all. The worst pair of the twenty put the sized arm at 1.25x and the pooled medians put it
under 1x, so the threshold was never approached and the sized arm wins on both columns. Treat
every figure here as a magnitude and not a constant: two batches of this same harness
disagreed on the sign of the time difference, which is the same instability the field
benchmark shows.

**The swap happens in the icon slot and nothing else moves.** `Row.iconSource` answers a
`file://` URL when the pane holds a path for this row and the themed icon lookup otherwise, so
one `Image` draws both and the row's geometry never depends on which of them it is drawing.
There is no animation on the change and no spinner, by construction rather than by a rule: a
`source` change is one binding re-evaluation. The URL is built with `encodeURI` and then `#`
and `?` are replaced by hand, because `encodeURI` leaves both literal and Qt then reads them as
a fragment and a query rather than as path bytes. A cache root holding either byte otherwise
produces a URL a test can read and Qt cannot open, which is why `tests/ui.sh hashcache` asserts
`Image.status` reached `Ready` and not only the string.

**`QSG_TRANSIENT_IMAGES=1` was measured against this tree and DROPPED, so do not re-measure it.**
The variable is real and undocumented, it lives in `libQt6Quick.so.6` on this box's Qt 6.11.2, and
it drops the CPU-side `QImage` once the texture is uploaded. Plan 5 Task 5a measured it on the
media fixture through the shipped product path, nine interleaved pairs: peak PSS 70,091 KiB
without it against 69,989 KiB with, `pss_kb` over 1024, a 0.10 MiB median difference against a
0.56 MiB spread inside the base arm, and the sign flipped 5 to 4. That is nothing. **The reason is
that `sourceSize` above already collected the whole prize**, and that was proven rather than
argued: a third arm with `sourceSize.width: 0` and `sourceSize.height: 0`, five interleaved pairs,
went from 76,775 KiB without the variable to 72,412 KiB with it, 5 of 5, a real 4.26 MiB. So the
variable works, and the CPU-side image it would free in the shipped tree is a 23 px icon slot,
roughly a kilobyte a row. The invalidation battery the candidate would have needed, scene graph
loss, a theme change and a scroll out past the buffer and back, was therefore never run: a lever
worth 0.1 MiB does not earn that risk. If a later plan ever removes `sourceSize`, for a preview
pane or a larger slot, this becomes a 4 MiB lever again and the battery becomes mandatory before
it can ship.

Two things the harness settled in passing. `sourceSize.width: 0` really is Qt's unset value,
so one file can measure both arms: a third config binding it to 0 landed in the unsized band
and the same file binding it to the slot landed in the sized band. And `FileView.onLoaded`
fires after the root's `Component.onCompleted` even with `blockLoading: true`, so a clock
started in `Component.onCompleted` reads an empty list, while `text()` called directly there
does return the content.

### The share browser overlay: fix round 2 replaces the sidebar bookmark expansion

Round 1's fix ("A server root with no share segment mounts..." above) was dropped:
turning a bare `smb://host/` bookmark into five permanent sidebar rows was not what clicking the
NAS to view its folders should do. Fix round 2 keeps round 1's detection (`isBareRoot()`,
the `gio info` no-local-path check, `gio list` to name the shares) but changes what happens with
the names: `ui/NetworkMounts.qml`'s `listShares()` fires a `sharesListed(baseUri, baseLabel,
names)` signal instead of writing bookmark lines, and nothing in this file touches
`~/.config/gtk-3.0/bookmarks` for this path any more (`addShareBookmarks` and `expandBareRoot` are
gone). `ui/ShareBrowser.qml` is a new overlay, the same instantiated-from-shell pattern as
`ui/EmptyState.qml` and `ui/Preview.qml`: `ui/WindowBody.qml` sizes it over `pane.listArea`, exactly
like the empty state, and `ui/Pane.qml` gained a `shareBrowser` property wired the same way
`preview` already was, so its own `Keys.onPressed` can route j/k/Enter/Escape to
`Focus.shareBrowserAct` while it is active, ahead of the normal list/rail routing.

Each share row is a real `Flea.Row` fed a synthetic row object
(`{ n: name, d: true, p: 0, s: 0, m: null, i: "folder" }`) rather than a hand-rolled second row
visual: `Icons.glyphFor("folder")` already maps to the folder mark, `p: 0` renders as the mode
column's own all-dashes string with no special-casing needed, and `d: true` already blanks the
size column (`Row.qml`'s own `root.row.d ? "" : Format.size(...)`). Only the date column needed a
real change: `root.row.m === null` now renders `"--"` instead of calling `Format.date(null, ...)`,
which would have silently printed the 1970 epoch as if it were a real date. This is the only line
`ui/Row.qml` gained; every other column already degraded correctly for a directory it had never
listed a stat for.

Enter on a row calls the exact same `openShare(uri, false, label)` a bookmarked share's own
`activate()` already calls, so a share picked from the overlay is never a second code path: mount,
`gio info` for the local path, `pane.open()`. `ui/WindowBody.qml` closes the overlay on `pane`'s own
`opened` signal (fired once a listing actually lands), not on the mount succeeding, so a share that
mounts but is slow to enumerate over the network still shows the overlay until there is something
real to show instead of it.

### "Already mounted" is not just a bare-root quirk

Proving the overlay's own Enter action live against the real NAS surfaced a second bug in code
round 1 shipped, not round 2's own new code: `gio mount` on a URI GVFS already considers mounted
answers a nonzero exit and the stderr line `"<uri>: Location is already mounted"` for ANY uri
shape, not only a bare server root. Reproduced directly: `gio mount smb://192.168.1.10/isos/`
against an already-mounted `isos` share exits 2 with exactly that stderr, identical in shape to
the bare-root case. `mountProcess.onExited`'s old check, `exitCode !== 0 && !isBareRoot(uri)`,
treated this as a hard failure for any non-bare-root uri, which is wrong: the location IS mounted,
that is what the message says. This is reachable through the overlay in one ordinary sequence: pick
a share, mount it, browse back to the bare root, pick the SAME share again (or a different overlay
instance discovers a share Flea already mounted through some other route). The fix reads the
process's own stderr instead of guessing from the uri's shape: `mountProcess` gained a
`StdioCollector` on `stderr`, and `isAlreadyMountedQuirk(text)` (`/already mounted/i`) replaced the
`isBareRoot()` check at that one call site. `isBareRoot()` itself stays, still used by
`infoProcess.onExited` to choose `listShares()` over the dead-end message, a genuinely
shape-dependent decision. `tests/ui.sh case_sharebrowser` reproduces this with a stubbed `gio`
that answers exit 2 plus that exact stderr for its second fixture share, asserting the mount still
resolves to an open rather than the "could not be mounted" message.

**Superseded, issue #36.** That stderr sentence is gvfsd's, and gvfsd translates it, so no client
locale makes it English and reading it at all was the defect. `isAlreadyMountedQuirk()` and
`mountProcess`'s `stderr` collector are both gone. `mountProcess.onExited` now records the exit code
in `_mountFailed` and always runs `gio info`, under the same 15 s deadline the mount leg has (see
"A single-flight guard needs a deadline" below), and the info result is the whole decision: a FUSE path
opens the share whatever the mount attempt reported, a bare root lists its shares, and only a
location `gio info` could not describe either reports the mount failure or the dead-end sentence.
The case still proves the same two behaviours; its stub now speaks Spanish.

**The bare-root branch reads no exit code, decided in the 0.1.4 composition.** The round that wrote
this also gated the listing on `exitCode === 0`, so a server root whose `gio info` refuses would
never have its shares listed. `gio info` on a reachable root refuses on some servers and answers on
others, and `tests/ui.sh case_network` drives the refusing shape, so the gate cost the share browser
those servers. It is gone: the listing leg carries its own deadline (below), which is what the gate
was really protecting against, and the listing itself is what answers.

### A public key mounts sftp, and a password is asked only after a mount has failed

**The defect, in 0.2.1 as shipped.** A saved `sftp://user@host` place could never be opened with a
key. `openShare` gated the mount on `credentialed(uri)`, true for any `sftp://...@`, so with no
password in the process map it set `result = "missing-credential"` and raised the dialog saying
"Enter the password to mount this location." **before `gio` was ever run**: no key, no agent and no
`~/.ssh` could get a hearing. Typing one did not rescue it either, because the credentialed leg runs
`/usr/lib/flea/flea-gio-auth`, and that helper exits 1 when the mount succeeds without ever asking
(`if {!$password_sent && [lindex $result 3] == 0} { exit 1 }`), so a key that authenticated anyway
came back as a refusal and `failMount` then forgot the password.

**Why sftp is the exception.** gvfs's sftp backend is not libssh, it is the ssh binary: measured on
this box, `gvfsd-sftp` execs `ssh -oForwardX11 no -oForwardAgent no -oPermitLocalCommand no
-oClearAllForwardings yes -oProtocol 2 -oNoHostAuthenticationForLocalhost yes -oControlMaster auto
-oControlPath=/run/user/1000/gvfsd-sftp/%C -l tom -s nas.test sftp`, with a pty. So
everything ssh can do without a password a Flea mount can do too: a key in `~/.ssh`, an agent, and
`ssh_config`'s `User`, `HostName` and `IdentityFile`. smb, ftp and dav have no such source, which is
why the split is on the scheme and not on a preference. `Mounts.keyless()` names it, and it sits
beside `Mounts.credentialed()` in `ui/js/Mounts.js`, where both are pure URI predicates with
`tests/js/network.js` coverage: `credentialed` says a password may be GIVEN, `keyless` says one may
not be NEEDED, and an sftp place with no remembered password now takes the plain `gio mount` leg.

**The password is still asked for, just later.** A refused mount is the only evidence that a
password is what is missing, so that is when it is requested: `infoProcess.onExited` answers
`failed` with `Mounts.keyless(uri) && Mounts.credentialed(uri)` through `failMount`'s new `missing`
argument, which is what makes the two routes into the dialog one behaviour: same
`missing-credential` result, same "Enter the password" sentence, same populated Retry, and the
secret is never forgotten, because a keyless attempt that failed is not a server refusing one. The
extra leg is cheap and it does not hang: measured against the real NAS with a user no key of ours
authorizes, `gio mount sftp://nosuchuser@nas.test/` with stdin on `/dev/null` and
no tty exits **2 in 171 ms**, printing `Authentication Required` and `Password:`. So a host that
wants a password costs one failed attempt, not the 15 s deadline.

**That check runs BEFORE the bare-root listing, and the order is the fix CodeRabbit caught in
review.** `isBareRoot()` matches `sftp://user@host/` as well as `sftp://user@host`, which is the
shape of this box's own saved server root, so with the check below it, a password-protected server
root fell into `listShares()`, failed there, and ended on "Connect failed: location has no
browsable folder" with the prompt unreachable: a listing of a root gvfs could not mount cannot
authenticate either. Phase 3 of `tests/network-keyless.qml` opens exactly that shape against a stub
whose `mount`, `info` and `list` all refuse, and asserting the bare-root half first would print
`NETWORK_KEYLESS FAIL retry=sftp://ask@slot.test/ reason=Connect failed: location has no browsable
folder password= phase=3`.

**A remembered password is a fallback for sftp, not a first resort.** `openShare` used to enter the
credentialed leg whenever `passwordFor(uri)` was non-empty, so an sftp place with a password in the
session map never reached the keyless attempt at all. Because the helper exits 1 when the mount
succeeds without asking, a remembered password turned a place a key opens into "Connect failed:
authentication was refused" and then forgot the secret. Caught by CodeRabbit in review; the gate is
now `!Mounts.keyless(uri) && (password.length > 0 || Mounts.credentialed(uri))`, so sftp always
tries the key first. What a password-authenticated sftp place pays for that is one extra Enter: its
keyless attempt is refused first, and the retry dialog opens **already populated**, because
`failMount` is handed `root.passwordFor(root._pendingUri)` and passes it to `retryRequested`. Phase 4
of the suite proves a key still opens a place a password is remembered for, and phase 5 proves the
refused one comes back with `password=fixture-secret` rather than empty. Reverting the gate prints
`NETWORK_KEYLESS FAIL retry=sftp://key@slot.test/home reason=Connect failed: the network location
could not be opened password=fixture-secret phase=4`.

**The workaround, for a box running 0.2.1 today.** Drop the `user@` from the bookmark line, so the
URI is `sftp://nas.test/home/tom` instead of `sftp://tom@...`: `credentialed` is
false for it, the plain leg runs, and ssh supplies the user from `ssh_config` (or from the local
login name). Verified end to end on this box: the mount succeeds on the key, `gio info` answers
`local path: /run/user/1000/gvfs/sftp:host=nas.test/home/tom`, and the rail row
opens. This box's own bookmarks file had also accumulated three identical `sftp://tom@nas.test/ nas
root` lines, which the rail dedupes on the normalized uri but which no writer should have produced.

**The gate is `tests/network-keyless.sh` and `tests/network-keyless.qml`**, run by
`./tests/run-all.sh`. They drive real `Quickshell.Process` instances against a stub `gio` whose
plain mount succeeds for one sftp place and exits 2 for the refused places. The trace asserts that
the key path opens before any helper call, each refused sftp path reaches `gio mount` and `gio info`
before the exact password prompt, the remembered password reaches `saveLocation` and its helper with
the exact URI and one stdin line before the path opens, and the SMB root still enumerates its shares.
The helper log contains only those nonsecret facts. Stubbing
`Mounts.keyless()` to `return false` reddens both that suite and five checks in
`tests/js/network.js`, and the failure is the reported symptom verbatim:
`NETWORK_KEYLESS FAIL retry=sftp://key@slot.test/home reason=Enter the password to mount this
location. password= phase=1`.

### Issue #36: no network decision reads a translated sentence

`@janoguerra` reported that network shares never open on a non-English box. There were three
locale-dependent decisions, not the two the issue named, and each takes a different fix because the
strings come from two different processes.

**The gio client's own output is pinned, not parsed in every language.** `ui/NetworkMounts.qml` sets
`readonly property var gioEnvironment: ({ "LC_ALL": "C" })` on its own five `Process` objects and
hands the same object to `ui/MountListing.qml`, whose listing is the sixth. Counted off the merged
file, not remembered: `authProcess` is the fifth, the credentialed leg the 0.1.4 base added, and the
locale reaches the `gio` its helper runs through the same property.
`Process.environment` merges into the inherited environment rather than replacing it, measured live
on this box (a probe run under `QT_QPA_PLATFORM=offscreen qs -p` printed `LC_ALL=C PATH_SET=yes
PROBE=yes HOME=/home/gm`), so `PATH` and `HOME` survive and only the locale is added. GNU gettext
ignores `LANGUAGE` whenever the category value is `C` or `POSIX`, so `LC_ALL=C` alone is the whole
pin. That makes `gio info`'s `local path: ` line deterministic, and `ui/js/Mounts.js localPath(body)`
is the one resolver that reads it: the product calls it and `tests/js/network.js` drives it, so the
wording exists in exactly one place.

**gvfsd's strings are not fixable that way, so nothing decides on one.** The already-mounted refusal
and a mount's own display name are composed by the daemon, which has its own locale, so
`LC_ALL=C` on the client changes neither. The refusal is no longer read at all (above). The display
name is no longer parsed for the English word "on": `ui/js/Protocols.js shareName(rawLabel, uri)`
cuts the tail only when it is this URI's own host, which `hostOf(uri)` takes out of the authority
between `://` and the next `/` by dropping the userinfo at its last `@` and any port after the host,
so `sftp://user@host/` measures the label against `host` alone. A bracketed IPv6 literal is kept whole,
brackets included, and a gio label spelling that host without them is therefore never cut: a wrong
label and never a wrong destination, pinned in `tests/js/network.js`. The one assumption left is that gvfsd's connector is a single
whitespace-delimited word in whatever language it renders: the cut is
`head.replace(/\s+\S+\s+$/, "")`, which takes one token, so a translation using two would leave the
other on the name. A wrong label, and never a wrong destination, because activation navigates on the
entry's own uri.

**The old rule also mangled labels that had nothing to do with a host**, which no report had named.
`/^(.*)\s+on\s+\S+$/` never tested that its tail was a host, only that it was a final
whitespace-delimited token, so a mount actually named `backup on tuesday` came out as `backup`.
`tests/js/network.js` drives that exact label, and the pair beside it, `backup on tuesday on nas`,
is what says the fix cuts a real host and only a real host. Deciding from the URI fixes it and leaves every non-share label alone:
an MTP phone whose label is `Pixel 7` under `mtp://Google_Pixel_7_1A2B/` keeps its name, because that
label does not end in that host.

**The suite could not fail before this.** `tests/ui.sh case_sharebrowser`'s `gio` stub hardcoded
English, so it passed on a Spanish box and would have passed after a fix that changed nothing. Both
network stubs now speak Spanish: `case_sharebrowser` answers `ruta local` unless the caller pinned
`LC_ALL=C`, and both its already-mounted refusal and `case_unmount`'s `Mount(0): stubshare en
stubhost` stay Spanish whatever the client asks for, because gvfsd would. Only one of the two is the
pin's control, and live matrix step 0b measured which: with the pin neutralised `case_sharebrowser`
reddens on demand and greens on demand, twice each, on `tests/ui.sh`'s own
`l on share1 never opened` line, while `case_unmount` passes in both arms. It cannot move, because
its `Mount(0)` line is gvfsd's in both arms, the reader never looks at the connector word, and the
stub answers no `gio info`. What `case_unmount` proves is that the parser is robust to a translated
connector, which is worth having and is not a locale control.

### The Network group's plus mark: a lucide glyph, not a font character

The `Text "+"` next to the NETWORK heading, a font character, read more like a
cross than a plus sign. Fetched `lucide-static@1.38.0`'s `plus.svg`
(`https://unpkg.com/lucide-static@1.38.0/icons/plus.svg`, ISC licence): two straight strokes,
`M5 12h14` and `M12 5v14`, joined the same way `Icons.js`'s other multi-subpath marks are (one
string, each subpath its own leading absolute `M`). Straight lines have no curve-fitting error to
introduce, so the `rsvg-convert` plus `magick compare -metric AE` check this project runs for every
lucide conversion came back an exact 0 differing pixels at 240x240, the same result the `server`
glyph's own all-straight-lines conversion got. `ui/Sidebar.qml`'s `addMark` is now a `Glyph` sized
off `Theme.font.caption`, the same token the NETWORK heading beside it renders at, in place of the
`Text` element; the click handler and keyboard path (`a` from the rail) are unchanged, since neither
ever depended on the mark being a `Text` item.

### That plus, and its hit target, sit on the rail's indicator centre line

The contract is that the `+` **and its 24 px hit target** share the centre line of the rail's
right-hand indicators. `ui/SidebarRow.qml`'s `dot` is that line: a `Theme.font.caption`-wide slot
right-anchored at `Style.spacing.rowPaddingX`. `addMark` used to be a 24 px `Item` right-anchored at
the same margin with the glyph inside it, which lines the two **right edges** up and not the two
centres, so the target's centre sat `(Theme.hitMin - caption) / 2` left of the dot's: 6 px at
`caption = 12`, 8.5 px at `caption = 7`. The glyph and the target are now siblings in the heading
row. `addGlyph` carries the dot's own anchors, and `addMark` takes its position from the glyph's box
rather than from an anchor, because a centre anchor quantises an odd size difference: measured at
`caption = 7`, both `anchors.centerIn` and `anchors.horizontalCenter` applied `(24 - 7) / 2` as 8
and not 8.5, leaving the two real centres half a pixel apart even where the rounded ones still
agreed.

The old check could not have caught any of it. `networkMarkGeometry()` measured the **glyph** and
compared it against an arithmetic slot, `heading.x + heading.width - rowPaddingX - caption / 2`,
which is term for term what a right-anchored glyph's centre already is: an identity, and it passed
at all thirteen interface scales it was walked over. `tests/ui.sh` then pinned the same anchoring a
second time by grepping the source. Both are gone. In the retained QML reference, `networkMarkGeometry()` returns three measured boxes through `ui/boot/shell.qml`'s `boxOf`, the
ink, the target and the real `dot`, in one coordinate system, and `tests/ui.sh case_netmark` asserts
their centres agree at Omarchy's seven text sizes, 9, 10, 11, 12, 14, 16 and 20 px, driven through a
fixture `[font] base-size`, plus both ends of the interface zoom.

**A printed edge is a rounded edge, and a printed centre is not.** `boxOf` rounds `x` and `width`,
and at text size 9 the target's real box was `[71.344, 95.344)`. Qt Quick's `contains` is
`x >= 0 && x < width`, so a click at the printed left edge, 71, lands 0.344 px outside and does
nothing, while 72 and 95, the first and last whole pixels inside the real box, both open the dialog.
An earlier run read exactly that miss as evidence the box was somewhere else and inferred
`[136, 160)` from the ink centre; the runtime rectangle was in fact exactly what the source implied.
`probe_network_mark_target` therefore stays a whole pixel clear of each printed edge in both
directions, which is true for any fractional part.

The **centre** is the one field that must not round, because the misalignment above is half a pixel
and 8 and 8.5 are the same integer. The first version of this check rounded all three fields and
compared the centres with `-eq`, so it passed the very anchor form the fix replaced: re-applying
`anchors.centerIn: addGlyph` to `addMark` puts the ink centre at `x + 3.5` and the target centre at
`x + 4`, which `Math.round` maps onto one number. `boxOf` therefore reports the centre as
`toFixed(3)` and `assert_network_mark_alignment` compares the two with `==` rather than `-eq`, the
only comparison in the suite that has to see a fraction. Every layout term here is an integer or a
half, so three decimals is four orders of magnitude of headroom, not a tolerance to tune. The click
probes read `x` and `width` and are unaffected.

### The preview strip's play/pause marks

Task 22's transport strip needed two more lucide marks `Icons.js` did not carry yet.
`lucide-static@1.38.0`'s `play.svg` (`https://unpkg.com/lucide-static@1.38.0/icons/play.svg`) is
a single path, `M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5
19z`, copied across unchanged since it has no `<rect>` or `<circle>` to convert. `pause.svg` is
two rounded `<rect>`s (`x="14" y="3" width="5" height="18" rx="1"` and `x="5" y="3" width="5"
height="18" rx="1"`), run through the same rounded-rect-to-path formula the `archive` and `server`
marks above already use. `rsvg-convert` at 240x240 plus `magick compare -metric AE` against each
source SVG came back 0 differing pixels for both, the same exact-match result straight lines and
circular arcs give every time there is no curve to fit.

### URI normalization: one canonical form, so a trailing slash cannot fork a row in two

The dialog wrote whatever the user typed verbatim, and `gio mount -l` always reports a share URI
with its own trailing slash; nothing forced the two to agree, so `smb://h/data` (typed) and
`smb://h/data/` (gio's own report of the same share once mounted) compared unequal everywhere a
uri was used as a dedup key, and could show as two rows for one share. `ui/js/Mounts.js` gained
`normalize(uri)`: every trailing slash is stripped, except a bare server root (nothing left but
`scheme://host`), which keeps exactly one, the shape `gio` itself needs to parse it back. Applied
in two places, on write and on compare: `ui/NetworkDialog.qml`'s
`appendBookmark()` writes the normalized form rather than the raw typed string, and
`ui/NetworkMounts.qml`'s `rebuild()` dedups live mounts against bookmarks using normalized keys
rather than the raw `uri` fields, so a live mount (gio's trailing-slash form) and a bookmark (now
also normalized, but an old hand-edited line might not be) still collapse to one row. `parseMounts()`
and `nonFileBookmarks()` themselves are left returning whatever form their own source actually
carries; only the write path and the dedup comparison were ever the places two spellings needed to
agree. `tests/js/mounts.js` proves `normalize("smb://h/data") === normalize("smb://h/data/")` and
that a bare root normalizes to its one-trailing-slash form regardless of how it was typed.

### The rail menu, tested with a stubbed gio and a stubbed lsblk

`tests/ui.sh case_unmount` and `case_eject` drive the rail menu through the real window.
`ui/Ipc.qml`'s `railRowCentre(i)` is the same `itemRect`-based reader `rowCentre` uses for list
rows, through `ui/Sidebar.qml`'s `railItemFor(index)` (the rail has no `ListView` and every
Repeater keeps every row instantiated, so this just indexes into whichever group carries it).

`case_unmount` stubs `gio mount -l` to report one fake share as mounted and `gio mount -u` to log
its own call, and proves that a right click opens `Open|-|Unmount|-|Rename|Edit address|-|Remove
from Network` drawing `folder-open|-|drive|-|rename|sliders|-|minus`
without unmounting anything, that Escape closes it with nothing run, that choosing the Unmount row
unmounts and messages "Unmounted \<label\>.", that a favourite opens Remove from Favorites, and that the
list still takes keys afterwards, which is the focus regression the second-instance bisection found.

### PR #21: rename and remove a saved place, and one rail row per share

`@TomFaulkner`'s branch asked for editing and removing network places and for a share to stop
appearing twice when its port is spelled two ways. It is reimplemented here rather than merged,
because the same branch carries a separate connection layer, and because its own `stripDefaultPort`
scanned for the last `@` in the whole URI: `sftp://u@h:22/inbox@2026` made `2026` the host, kept the
`:22`, and produced the exact second rail row the function existed to remove.

**The port.** `ui/js/Protocols.js stripDefaultPort(uri)` finds the authority first, between `://` and
the next `/`, so a `:` or an `@` in the path is never read as a port or a host, and a bracketed IPv6
literal cannot match because it ends in `]`. `defaultPortFor()` reads `SCHEME_PORTS`, one row per
scheme, because the port gio omits belongs to the scheme and not to the protocol the form picked:
one number for `dav` and `davs` both stripped a real `:443` off a `dav://` URI and left a real `:80`
on it, the exact duplicate rail row `stripDefaultPort()` exists to remove. Every row is measured
against gvfs 1.60.2 and glib2 2.88.3 on this box by round-tripping each spelling through `Gio.File`,
the uri mapper `gio mount -l` prints a mount through: it drops smb 445, sftp 22, ftp 21, ftps 21,
dav 80 and davs 443, and keeps every other port it is given, so `ftps` is 21 here and not IANA's
990, and a `dav://host:443` keeps its 443. It canonicalises no `nfs` port at all, which is why 2049
is still in the table for the other half of the job: it is the port an nfs client uses when the line
omits one, so the two spellings of one export still make one row.
`Mounts.normalize()` runs it, so the dedup key, `ui/NetworkDialog.qml`'s
stored bookmark line and `Places.relabel`'s matching all agree with what `gio mount -l` reports.

**And the form prefills out of that same table**, which is the half review round 2 found still
open. `SCHEME_PORTS` said `dav` was 80 while `Protocols.defaultPort()` read a second table keyed by
protocol and prefilled 443 for both WebDAV spellings, so unticking the TLS box built
`dav://host:443/path`: a port the scheme does not use, offered by the dialog, on the one code path
that exists to keep the dialog and the dedup agreeing. `defaultPort(protocol, tls)` now returns
`defaultPortFor(scheme(protocol, tls))` and the protocol-keyed `PORTS` table is gone, so there is
one table again. `ui/NetworkForm.qml` re-prefills on `onTlsChanged` as well as on `pick()`, because
the box picks half the scheme and the chip picks the other half; without it the number the chip left
behind survives the tick. `tests/ui.sh case_network` drives the tick and reads the port back.

**And that prefill has to run before the port, not after**, which the 0.1.4 composition found: the
base reopens the form on a saved URI, and `NetworkForm.load()` set the port and then the box, so
`onTlsChanged` overwrote a real `:443` on a `dav://` place with the scheme default. `load()` assigns
`root.tls` before the port now, so the tick's prefill is the fallback and the saved number wins.
`tests/ui.sh case_networkauth` reparses both spellings and prints `dav-default=80 dav-explicit=443`.

**The two rows.** `ui/js/Mounts.js rowMenu(entry)` is what the rail's right click opens, and it
returns `railMenu(entry)`: `Open` first on a mounted row, then the release, then `Rename`,
`Edit address` and `Remove from Network`. `ui/js/Eject.js releaseFromRows` picks the eject or
unmount action out of those same rows, so Ctrl+E refuses a row with nothing mounted rather than
starting an editor on it.
`Mounts.release()` dispatches all four actions and takes the rail itself, so `Rename` reaches
`Sidebar.startRename()` (the same editor `r` already opened) and `Remove` reaches
`NetworkMounts.forget()`, which hands `ui/NetworkPlaces.qml` the uri; it drops the line through
`Mounts.removeBookmark()` and the blocking write `rename()` already uses. A share that is mounted right now stays on the rail as the live mount
it is until something unmounts it, which is why `Unmount` is still offered beside `Remove`.

**The label on a live row is the bookmark's own.** `ui/js/Mounts.js railLabel(mount, marks)` answers
the label of the first bookmark whose normalized uri matches the mount, matched the way `rebuild()`
dedups, and gio's own name only when nothing has saved that share. Without it a rename typed on a
mounted share was written to the file and then overwritten on the rail by the next five second poll.

**`Remove` says which of the three states the press landed in**, because the row is offered on every
share whether or not a bookmark exists for it. `ui/NetworkPlaces.qml forget(uri)` decides from
`bookmarksText`, the same text the rail was built from, and the body it writes is derived from that
same text, so no write this rail makes can be older than the rail. No line for that uri and nothing
is written at all: the bar says `<name> is not a saved place, and stays on the rail until it is
unmounted.`, and only a mounted row reaches that arm, because an unmounted row is on the rail
precisely because that text carries its line. A line that is there is dropped, and the bar reads
`<name> is forgotten, and stays on the rail until it is unmounted.` over a live mount and
`<name> is forgotten.` otherwise.

`tests/ui.sh case_unmount` drives all three across four presses: a live mount this home never saved,
a saved live mount, a saved place nothing mounts, and a second press on the row the first one left
behind, which also asserts the file's own mtime did not move. `case_network` drives the sequence that
used to be refused forever: Add through the dialog, then Remove in the same session.
`ui/NetworkDialog.qml` appends through a `FileView` of its own and nothing reloads the one this
Service writes with, so a body read back from it was the pre-Add snapshot, `next === body` was true
and the removal was refused on every retry until a restart. Deriving the body from `bookmarksText`,
which `ui/Sidebar.qml`'s own `FileView` reloads on that dialog's own `saved()`, is what closed it for `forget()`;
`rename()` took its body from the same place for the same reason until it had to answer a failed
read as well, and reads the file itself now (see "A failed FileView read" above).

**Every edit reads the file, and no edit reads a copy of it that has aged.** Two more instances of
that one defect were measured on this box in the fix round, both of them the mirror of the first,
and both are why `case_network` now runs an Add after its Remove and a rename after both.

- **Add after Remove put the removed place back.** `ui/NetworkDialog.qml appendBookmark()` read
  `bookmarksWrite.text()` from a `FileView` that is not watched and had last read before the rail
  removed a line, so its "append" wrote the whole stale body plus the new line. Driven: Add
  `198.51.100.1`, Remove it, Add two more, and the rail came back with all three. It re-reads,
  blocking, before it composes the body.
- **Two rail edits in one turn put the first one's line back.** `ui/NetworkPlaces.qml` derives
  `forget()`'s body from `bookmarksText`, which is only newer than the last write if the reload
  that write caused has landed, and `ui/Sidebar.qml reloadBookmarks()` did not wait for it.
  Measured through the real `NetworkMounts.forget()` chain with the wiring under test as the only
  variable: over an asynchronous reload two `forget()` calls in one turn left `smb://nas/one One`
  in the file, over a blocking one they left it empty. `reloadBookmarks()` blocks now, and
  `ui/NetworkPlaces.qml`'s own header says which question that answers, because "no removal can be
  older than the rail" was never an answer about this one.

Not reimplemented, and both are omissions of PR #21 rather than of this rail: the branch's
Add-dialog reuse, which reopens the form prefilled to edit a place's URI (that needs a
`ui/WindowBody.qml` connection and an `ui/Ipc.qml` reader, and the pinned-places store itself is being
replaced, so `Rename` is the edit this rail offers today), and its per-host SFTP collapse.
`gvfsd-sftp` mounts one connection per host, so `gio` lists `sftp://user@host/` while the bookmark
is `sftp://user@host/home/tom`; those normalize to different keys and `rebuild()` keeps both rows.
`stripDefaultPort` cannot collapse them, because the difference is the path and not the port, so the
fix is a containment rule that changes the dedup contract for every scheme and needs a live gvfsd
sftp mount to verify.

`case_eject` stubs `lsblk --json` as well, so a removable volume exists at zero privilege with no
real device anywhere near it. Its negative control is the whole point: the `gio` stub always exits
0, and while the stubbed listing still shows the volume mounted the status bar must refuse rather
than say "safe to unplug", because the verdict is read off the listing and never off the exit code
(`ui/js/Eject.js`). Only when the stub actually drops the mountpoint does the safe sentence appear.
It also asserts the eject went through `gio mount -e <mount point>` and never `mount -d`, which
glib dispatches before it reads `--eject`, and that no `-f` ever reached gio.

A status bar transient clears after 4000 ms and an eject verdict lands on the rail's own 5000 ms
poll, so both cases catch a sentence with `wait_message` as it lands rather than sleeping past it.

### Places.relabel: normalized matching, every duplicate rewritten, control characters stripped

Task 19's network rename writes through `ui/js/Places.js`'s `relabel(body, path, name)`, the one
function in this tree that edits the GTK bookmarks file's own bytes. Matching is normalized the
same way `ui/js/Mounts.js`'s `normalize()` already dedups the rail, so a live `gio`-reported uri
(trailing slash) still finds a bookmark line written without one (`ui/NetworkDialog.qml`'s own
`appendBookmark()` strips it). Fix round 1 found two gaps: the function used to stop at the first
matching line, so a hand-edited file carrying two bookmark lines for the same normalized uri
(never produced by Flea's own writers, but not excluded either) left the second stale after a
rename; `relabel` now rewrites every matching line in one pass and appends only when none match at
all. Second, `name` reached the join with no defence against an embedded `\n` or `\r`; today's
`qs.Ui` `TextField` cannot produce one, but the function is a trust boundary in its own right and
must not depend on its one caller staying that way, so `name` is stripped of both before it is
trimmed. `tests/js/places.js` proves both: a two-line-duplicate fixture ends with both lines
rewritten and every other line byte-identical, and a name carrying an embedded newline still
produces a file with the same line count it started with, the newline gone rather than splitting
one bookmark into two.

Review round 2 closed a third gap, the asymmetry from the other side: `relabel` read each line
raw while `Mounts.removeBookmark()` reads it trimmed, so an indented bookmark line was one `Remove`
could drop and `Rename` could only ever duplicate, appending a second line for a uri the file
already carried. `relabel` trims first now, the same way, and the line it rewrites loses its
indentation along with its old label.

### Flea ends when its last window closes, and a wedged listing no longer freezes the rail

Three fixes on 2026-09-02, one on the way out and two on the rail. None of them had a line here.

**The exit, `5fc2668`, `ui/WindowBody.qml`.** A Quickshell shell outlives its windows by design, and
0.3.1 exposes no exit API: `Qt.quit()` and `Qt.exit()` are no-ops, which the engine says out loud
as "no receivers connected", so signalling its own pid is the only lever left. The window carries
`Connections { target: Quickshell; function onLastWindowClosed() { ... } }` and the body is
`Quickshell.execDetached(["kill", String(Quickshell.processId)])`. Closing the window is what a
user does to quit, so without it the process stayed resident with nothing on screen.

**The NETWORK rail, `8cf5418`, now `ui/MountListing.qml`.** `gio mount -l` against a share whose
server has stopped answering never returns, and nothing bounded it, so `pollMounts` refused every
later poll and no new mount appeared until the app was restarted. A `timeoutMs` of 10000 now
ends it. `_timedOut` gates both the collector's `onStreamFinished` and `onExited`, because a
listing ended that way collected nothing and reading that as "no shares" would empty the rail and
take Unmount with it exactly when a server is misbehaving; it is cleared when the next listing
starts and never in `onExited`, so it still reads true while the ended listing's stream drains. A
re-read asked for mid-listing sets `_pollAgain` and runs when that listing ends, instead of being
dropped and leaving a just-mounted share to wait out the five second poll. All of it moved out of
`ui/NetworkMounts.qml` behaviour-for-behaviour when that file had to make room for the 0.1.4
composition; two names are shorter in a file that owns nothing else, `listTimeoutMs` is `timeoutMs`
and `_listTimedOut` is `_timedOut`, so a grep for the old names finds `ui/DeviceMounts.qml` alone.

### A single-flight guard needs a deadline, and a refusal the user can see

`ui/NetworkMounts.qml openShare()` is single flight over its children and only one of them was
bounded. `mountProcess` had a 15 s `Timer`; `infoProcess` had nothing, and issue #36's fix made
`mountProcess.onExited` run `gio info` on **every** exit, the dead-server path included, which is
exactly where `gio info` never returns. So one hung info left `infoProcess.running` true and the
guard's bare `return` refused every later share for the life of the window, in silence.

Both halves are fixed. The `mountTimeout` `Timer` is restarted by `runInfo()`, so each leg of an
open carries the same 15 s bound; whichever leg is still running when it fires is the one it ends,
`_infoTimedOut` keeps that leg's own `onExited` from reporting a second, contradictory failure, and
the bar gets the same "did not respond" sentence a hung mount already produced. The guard itself
now says so too: "Another network location is still opening. Try again when it opens" `tests/ui.sh`
`case_hangshare` drives all three against a `gio` stub that hangs `info` on one share and answers
for the other, and it went red on the guard's own silence before the fix.

**There are three legs, not two**, which review round 2 caught against the very commit subject that
claimed otherwise. A bare server root ends in `listShares()`, and `gio list` on a server gvfs cannot
reach hangs exactly the way `gio info` does: it had no `mountTimeout.restart()` and nothing else in
the chain was left to end it, so the share browser waited for a listing that never came, forever and
in silence. `listShares()` restarts the same timer, `mountTimeout` gains a third branch and
`_listSharesTimedOut` its own consume-once flag, and `openShare()`'s guard now counts
`listSharesProcess.running` too, because otherwise a new open started over a hung listing would
restart the timer onto itself and leave the listing unbounded again. `case_hangshare` drives it
against a stub whose bare root's `gio list` hangs; it went red on the deadline that never fired.

The rule this follows, and it holds for anything written after it: **a single-flight guard needs a
deadline on the thing it guards, and a refusal the user can see.** A guard that returns in silence
names nothing at all, which is the opposite of an error naming its failing component. Four more
guards in this tree are still unbounded, `ui/NetworkDialog.qml`'s `dropboxCheck`,
`ui/ShareLink.qml`'s `sharelink`, `ui/Taildrop.qml`'s `statusProcess` and `ui/Opener.qml`'s
`copier`. All four are pre-existing, none is a regression of this release, and they are a 0.1.5
ticket rather than a fifth front in this one.

**The DEVICES rail, `b28e992`, `ui/DeviceMounts.qml`.** `lsblk` on the same five second poll was
unbounded too, and one that stopped answering left `poll()` refusing every later listing for the
life of the process. It takes the same 10000 ms bound and the same `_listTimedOut` rule, so a
listing ended that way holds its rows rather than emptying the rail, which is what keeps Eject
reachable while a device is misbehaving. The `_streamPending` interlock is released in `onExited`
rather than in the timer, because a collector whose stream was cut off may never finish it and
`poll()` refuses on that flag alone.

### Closing the window quits the backend, and a copy in flight is cancelled

`ui/WindowBody.qml` answers `Quickshell.onLastWindowClosed` with `backend.quit()` and signals its own pid
only once `Backend.onQuitReady` reports the child gone. Before this it killed the pid immediately and
left the backend to notice EOF by itself.

**The old path was right here by a margin of milliseconds that nothing in this tree owned.** Measured
on this box over three closes at 5 percent of a 4 GiB copy, driven with Omarchy's own SUPER+W bind:
`qs` exited 64 to 82 ms after the key, and the backend exited 18 ms before it once and 1 ms after it
twice. Every run removed the partial, because Quickshell closes the child's stdin during teardown and
`drain()` cancels inside that window. Which of the two dies first is not stable between runs, and
`src/backend/copyfile.rs` creates the destination with `create_new(true)` under its FINAL name, so a
drain that lost the race would leave the file in the destination at partial size with nothing having
said so. Sending `quit` makes the drain happen while `qs` is fully alive instead of racing its
teardown. After the change, three closes at the same point put the backend at 66 to 222 ms and `qs`
at 106 to 261 ms, the backend consistently about 40 ms ahead of it; on an idle close with nothing to
drain the two land within 1 ms, which is this harness's own resolution and not an ordering claim.

**What a close does to a running transfer: it cancels it.** That is the transfer card's own Cancel
control reached by a different gesture, and it is the only answer that holds the invariant without
inventing a surface. Finishing the copy instead would run it for minutes inside a process with no
window, no progress and no way to stop it, which is the resident-with-nothing-on-screen state the
exit exists to prevent. A confirmation dialog would be the only one in the product: `Delete` trashes
with no prompt, as `tools/flea-live-ops` asserts, and a cancelled copy destroys nothing, so the
dialog would guard the least destructive action of the set. A cancelled move keeps its source too,
because `move_any` copies before it removes and the cancel path removes the partial destination.

**The two edges `Backend.quit()` answers.** A quit before `onStarted` would only join the pending
queue and never be written, so `queueing` short-circuits straight to `quitReady`; nothing can be in
flight that early anyway. And a child that never answers at all would leave a shell resident with no
window, the exact defect the exit fixes, so `quitDeadline` bounds the wait at 30 s, longer than the
backend's own 25 s `DRAIN_LIMIT`.

**Measuring a close needs the real bind, and the wrong one reports the window it never touched.**
`hyprctl dispatch closewindow address:0x...` is not valid on this box: Hyprland takes Lua here and
answers `')' expected near 'address'`, then names the shape it wanted, `hl.dsp.window.close()`. A
harness discarding that stderr reads a syntax error as a close and then reports whatever the
untouched window was still doing, which is how a copy still running at 30 percent was first written
down here as a surviving partial. SUPER+W is Omarchy's "Close window" bind, and only a real uinput
key press fires a compositor bind, so `omarchy-drive hotkey --global super w` is the gesture. It acts
on the ACTIVE window, so assert the active window is the one the run launched before sending it, and
never call `hl.dsp.window.close()` with no argument: it returns `ok` rather than printing a signature
and acts on whatever is active, which may be a window you did not open.

e39 final integration records tests/js/columns.js 544 and ui/ColumnsArea.qml 459, preserving e41 routing and the focus wire. Tests/columnsfolder.qml is 396; its manual capture receipt witnesses the actual release state.

w34-r3 holds Row 472 and List 320 untouched and takes tests/listnamebudget.qml 280 to 331 for the search execution probe (searching rows hide the ordinary name and draw the search loader's own decoratedName, Row.qml:240,257,270) and the pass marker reporting the stored checked-400 budget.

w34-r4 removes the resized-sync timing claim root falsified (a per-delegate floor passes it, so values cannot pin recomputation) and takes tests/listnamebudget.qml 331 to 316 with the structural guard in tests/js/listbudget.js at 31: List floors once, Row's ordinary path reads the assignment and floors nothing, laziness is structural with the fallback kept, and the exact seed root.localNameBudget() goes red 3 of 10 natively.

e47 takes tests/js/columns.js 544 to 555 for the folder-hold early-return pins (a held check returns before its release arm, an expired cap returns before its fallback arm, each a structural oracle honestly labelled source since the QML guard runs no JS).

e47 R2 takes tests/js/columns.js 555 to 556 for the structural-only relabels and the preview-swap Qt harness pin; the real folder-guard execution is tests/preview-swap.qml 250 to 379 (three isolated swaps, held check, held cap, unheld positive control) with its judge in tests/preview-swap.sh.

w55 takes ui/js/Names.js 250 to 259 for the fitting fast-name early returns (middleElide returns a short fast name off one isFastText scan, gridCaption returns a fast name fitting one line with the one-cell elide kept, each scan reused for the store below), over the soft budget and under the hard cap; tests/js/names.js keeps 222 with no added line.


w51 moves fit-only metrics and imports to ui/FitMetrics.qml (22 lines), loaded only during Header fitting (391 lines, under the 400-line hard cap). tests/headercost.qml (358 lines) exercises real Qt creation, synchronous fitting, one F4 update, held offsets and release; native empty Loader source values are compared as URLs converted to strings. Caption widths can both clamp to 48 pixels, so the held-offset check asserts the exact selected byte count beside the independent clamped-width calculation.

e51 takes ui/CardScroll.qml 60 to 64 for the configurable gutter (default rowPaddingX, holder reads root.gutter) with its holderWidth test seam, and ui/ContextMenu.qml 582 to 584 with gutter 0 on both bodies each on its own line; the probe is the new tests/menu-scroll-width.qml at 133 with tests/menu-scroll-width.sh at 72, both inside their budgets. R5 gives both wrapper error filters the bare ERROR rejection beside WARN/TypeError/ReferenceError with the menu platform-mask allowance unchanged, prints the complete captured output once on every refusal, and registers menu-scroll-width in tests/run-all.sh headless (scroll-fill registers at the e52 transfer, where its file lands).

e52 takes ui/Row.qml 472 to 475 for the caller-assigned paint extent (paintWidth carrying the view width, state and drop fills painting to it under the reserved lane while delegates keep rowWidth and shared name budgets); ui/List.qml 320 to 321, ui/ColumnPane.qml 353 to 354 and ui/PickerList.qml 216 to 218 for the one-line extent assignments and the picker marked fill, ui/ColumnRow.qml 250 to 253 inside the soft budget for the same extent with its wash Loader widened instead of its loaded root, and tests/scroll-fill.qml at 316 with tests/scroll-fill.sh carrying the Qt geometry gate, each re-derived with wc -l.

e52-r3 transfers the verdict repair onto the real wrapper with no geometry change: tests/scroll-fill.qml 316 to 317 for the one DONE receipt beside the PASS, tests/scroll-fill.sh 41 to 64 for the owned 143/DONE/PASS counts with timeout-124, nonzero, missing and duplicate refusals and the bare ERROR/WARN/TypeError/ReferenceError gate printing the full log once, and tests/run-all.sh carrying scroll-fill once in its headless list; the 11 shipped-wrapper controls run green through native-check against stub qs verdicts with the nodone diagnostic pinned in the printed log, each re-derived with wc -l.

e62 moves Grid-only captions behind the existing Grid loader with no output change: ui/js/Names.js 259 to 142 for keeping middleElide and the shared Unicode/store/span helpers alone, the new ui/js/GridNames.js at 122 for the Grid-exclusive extension/elide/wrap/caption/last-line code importing Names with Names never importing it back, ui/GridTile.qml keeps 208 for the GridNames import with its two call swaps, tests/js/names.js 222 to 250 for executing both real libraries against the frozen reference over the full corpus with import-direction and Row/ColumnRow laziness pins, each re-derived with wc -l; tests/js/namesreference.js keeps 205 frozen.

w56 hoists the empty clipboard check in ColumnPane.qml (358 lines after scrollbar integration), skips per-row lookups while empty and releases the cache when cleared. tests/columnclip-empty.qml (250 lines) drives real delegates through empty, copy, cut, clear and viewport movement; its clear baseline is recorded after cut. The shell wrapper is 41 lines. Native proof uses the actual Qt library counter and marks, with no timing claim.

w57 folds the shared Names library into the already-loaded Format and deletes it: ui/js/Format.js 169 to 296 for the shared elide/Unicode/store/span block moved byte-identical, ui/js/GridNames.js 122 to 138 for the Format import with the array-cell helpers over the common store, ui/js/Names.js deleted at 142, ui/Row.qml 474 to 473 and ui/ColumnRow.qml 253 to 252 for dropping the second library import, tests/js/names.js 250 to 257 for the Format/GridNames rewiring with the removed-library pins, tests/columnrow-geom.qml keeps 409 for the Format import swap; Format stays under the 300 hard cap because the array-cell helpers live in the Grid split, and GridNames imports Format once with no shared-to-Grid edge, each count re-derived with wc -l.
e63 stops hidden List work without changing the shared pane, each re-derived with wc -l: ui/List.qml 321 to 398 for the visible-gated integer model with the rename lifetime, the hidden geometry guard, the queued-drift guard and the cursor-view restore that releases only on a layout-stable park with a bounded callLater guard and empty release; the probe is the new tests/listhidden.qml at 397 over the soft budget and under the hard cap with tests/listhidden.sh carrying the offscreen gate.
w61 preserves the complete footer dismissal hint, each re-derived with wc -l: ui/StatusBar.qml keeps 456 for the hint reservation by advanceWidth (bounding width drops the hint's edge space, 114 against 115, the tree's own FactsTable rule; same text, same ellipsis policy, empty stays 0, Undo split untouched); tests/js/statusfix.js keeps 34 for the repointed metric pin; the proof is the new tests/statusbar-hint.qml at 166 driving the real StatusBar through the long error at 880/1100 and lane 12/14 with Undo, no-hint, sticky and search controls, carried by the new tests/statusbar-hint.sh at 66 with its single tests/run-all.sh headless entry, run on Qt before and after the one-word hunk.

0.3.7 warning closure records durable.rs at 662 lines, one cfg(test) attribute above the 661 ceiling. The helper has only test callers; runtime behavior is unchanged.

e68 R4 retires dismissed menu snapshot replies, re-derived with `wc -l`: `ui/PaneMenuActions.qml` 404 to 407 for the retire guard on the snapshot reply (a stale ok reply retires silently only with the menu and dialog closed and no action awaiting; an open menu, a waiting action or a backend refusal keeps its sentence); the probe is the new `tests/menu-snapshot-retire.qml` at 145 with `tests/menu-snapshot-retire.sh` at 72 carrying the offscreen gate, both inside their budgets and registered in `tests/run-all.sh` headless.

k2-markdown renders Markdown previews (RenderedPreviews callouts 1, 2 and 4), each re-derived with `wc -l`: `ui/Preview.qml` 463 to 574 for the Markdown bar (mark, name, line count, close with the overlay tap policy; the settings segmented control the board drew there was ruled out on 2026-10-03) and the rendered/source pane under it; `ui/PreviewColumn.qml` 499 to 526 for the Markdown branch (tall frame to the name, always rendered, no toggle) with the swap reading its load through `textLoading`; `src/uischema.rs` 439 to 443 for `preview.markdownView` with its rule and default assert (the leaf was removed again on 2026-10-03); `ui/js/Keymap.js` 322 to 323 for the generated preview-context `r` row. The split lives in `ui/js/Markdown.js` at 527, recorded over the 300 hard cap (top-level blocks only: fenced code, quote groups, standalone-image paragraphs, top-level lists, GFM tables, runs of everything else; a quoted fence stays a quote, an unterminated fence runs to the end, whitespace-only runs never block, deeper markers nest into the open item; inline code spans pair by matching backtick runs with CommonMark space stripping and resolve against the chrome hex, inline links and autolinks wrap in a presentational font tag against the ink hex because the importer hardcodes its own link blue over QML linkColor), and the delegates in the new `ui/PreviewMarkdown.qml` at 473, recorded over the 400 hard cap (fence verbatim filled on the chrome surface with no border, quote behind a 2 px muted bar, remote as one dashed box with glyph and sentence on a single left-aligned line, same-folder as an async width-fitted Image, lists as marker rows at the text edge through a two-column grid, tables as a GridLayout with a bold header, muted rules and no verticals hugging content by measured column widths since Markdown tables carry no styling), rather than in `ui/js/Facts.js`, which stands at its 300 hard cap; `ui/js/PreviewKeys.js` takes the guarded toggle at 91, `ui/SettingsSegment.qml` a 20 px height override at 79 that the segment-free bar left without a caller (removed again, the component is back at 75), `ui/ViewState.qml` the one-line normalised reader at exactly 400 (removed with the leaf), `ui/SelectionPreview.qml` keeps 262 for the one-line readiness mapping, and `tests/js/markdown.js` at 167 with `tests/js/previewkeys.js` at 88 carry the URL, placeholder, toggle, keymap, block, code, link, table and list pins. No code colouring renderer exists on this head (Source and the code preview are verbatim text), so a fenced block keeps plain mono: colouring one by its info string would take a real syntax highlighter, new machinery, not a reuse. Three rendering facts were proved on CI Qt offscreen rather than assumed: `Text.Markdown` is undefined there and every delegate reads `Text.MarkdownText`, the importer keeps structural HTML (`<br>`, `<code>`, `<font>`) while dropping every style attribute, and thin markers antialias without a pure core pixel so the suite counts any ink, which `tests/markdown-render.sh` with `tests/markdown-render.qml` at 399 pins from a grab (chip, markers, rules, hug, fence, box, bar, no default link blue, PNG to `$FLEA_CI_SUITE_LOGS`). Headings render foreground bold at Qt's sizes (the board draws h1/h2 in the foreground, so that is exact), and inline code takes no side padding (the board's 4 px was declined without a capture reading it right). `tests/ui-captures-markdown.sh` carries the display-box capture (`case_cap_markdown`, out of the default wanted list).

Advfix-fs1 round 1 records one ceiling, re-derived with `wc -l`: `ui/NetworkMounts.qml` 838 to 849 for the kernel and outside-home network rows.

tp2-r2 answers the six elastic-edge findings, each re-derived with `wc -l`: `ui/js/Scroll.js` 289 to 299, 297 after the advfix-tp2-r1 comment cut, for margin-aware `limits`/`bounded` with `limitsY`/`limitsX`, the `rangesX`/`rangesY` no-range-no-delta rule and `OVER_DECAY` 0.98 with `overDecel`, staying under the 300 hard cap; `ui/FastScrollHandler.qml` 455 to 493, 469 after advfix-tp2-r1 with the dead release settle deleted and the comments cut to one line, for accepting the overscrolled lift End, the `scrollsX`/`scrollsY` direction-plus-range gates with zeroed dead samples, per-axis tail ends, frame-by-frame past-bound integration through banked raw travel, no-snap press with the view's own release fixup as the settle, and the stale-banked-travel drop after a listing swap; `ui/List.qml` 435 to 445, 440 after advfix-tp2-r1 with the dedupe keyed on the bounded position, for the cursor clamp reading the bounded view and firing only when that view moved; `ui/ViewportScrollBar.qml` and `ui/SelectionBand.qml` keep their ceilings for the margin-aware clamps; `tests/touchpad.qml` 540 to 568, 585 after advfix-tp2-r1, for the press-stops-where-it-is update with the production-path settle pins. The proof is the new `tests/touchpad-edge.qml` at 384, 400 after advfix-tp2-r1 (return acceptance plus 12-frame OutCubic, cursor never moves on 1.37/140 px bounces, press holds 140/30 px with the under-pointer row and settles through the view's own fixup, tail peak 55 past with no past-bound frame above the in-bound max) and `tests/scroll-bounds.qml` at 212 (grid rests at -gap through press, Begin and End and returns there, diagonal coasts exactly like the pure stroke at 2028.9 px), each with its 42-line `.sh` offscreen driver, both registered in `tests/run-all.sh` headless; `tests/js/scroll.js` 159 to 184 after advfix-tp2-r1 carries the pure margin, range and brake pins with typeof guards so the old tree fails them clean instead of erroring.

tp2-r2 moved detail (advfix-tp2-r1 cut the runs above to one constraint line each): Flickable eats wheel before a child WheelHandler, so the handler is a MouseArea; touchpad strokes move gained pixels with Finder momentum tail and rubber-band past bounds while a wheel notch keeps the Theme rate with none and never overscrolls; gates read direction first, so a tilt never overscrolls a vertical list sideways nor steals its lift tail; a new stroke snaps the return to its bound while a press stops it where it is, so the tapped row is the row under the pointer; raw travel accumulates past the bound with shown staying under the view, and content back inside with travel still banked is a listing swap under held fingers, so it is dropped rather than spent; reduced motion still overscrolls as direct manipulation but returns at once; the FrameAnimation and the headless probe both enter per-frame, the overshoot grows over frames and peaks instead of jumping, and each axis ends on its own; a vertical list keeps contentX 0 through any diagonal; snapping on press would move the row Qt picked the target from, so the tap would land on nothing; the cursor follows the bounded view because overscroll shows no new rows and the return lands back on the view it left; `bounded` never leaves origin-less-leading to last-page-plus-trailing, and margins default to 0 so margin-less callers read as before.

Advfix-scroll round 2 records two over-cap ceilings, each re-derived with `wc -l`: `tests/touchpad.qml` 452 to 454 as a new over-cap test exception (round 1's end at 452 for the angle-free and real-press and notch stops, plus 2 for the round 2 angleFree mid-list park that makes a notch-routed frame move 9.6 px instead of clamping at the last page) and `ui/PickerWindow.qml` 770 to 776 for pk1's onListed fsinfo ask (the grid plans thumbnails against the storage class, asked only when the listing lands and never for Recent).

Advfix-scroll round 3 names the 400 px park once, re-derived with `wc -l`: `tests/touchpad.qml` 454 to 456 for `parkAbovePx` beside `rowCount` with its one-line comment, used at both parks, so no bare 400 remains.

mg-stage3 records two over-cap ceilings, each re-derived with `wc -l`: `src/backend/undo.rs` at 601 for the shared journal (the pinned Mode with its birth time, the payload-free barrier with its codec helpers, and the manifest reattach) with its undo, redo and codec tests beside it, and `src/backend/undoshare_tests.rs` at 513 for the shared-journal pins (the two-window undo and redo, the barrier consume, the exact-integer round trip with the pinned birth time, and the oversized-entry trims), each over the hard cap and recorded rather than split.

038-stage6 gives each iomount test its own pair of mount names, so parallel tests never share a stuck mark, and its reset no longer clears the shared table; `src/backend/iomount.rs` moves 572 to 606, re-derived with `wc -l`, for the `Mounts` helper and the pin that a reset leaves another test's mark alone.

xw4r2 records four ceilings, each re-derived with `wc -l`: `ui/ViewState.qml` 495 to 529 for the joined settler halves, the queued prune with its settleNext order, the failure-arm pruneFailed and the per-load favourites sync; `ui/js/UiState.js` 432 to 442 for invalidKeys, dropInvalid and the settle join and order helpers; `tests/js/uistate.js` 329 to 363 for the two-key prune, the owed-differing and the settle-order pins; `tests/js/watch.js` 307 to 334 for the .dot-insertion reorder, the partial-selection clear and the anchor-resolution pins. `ui/Pane.qml` keeps 999 with no added line and `ui/js/Anchor.js` keeps 178 inside the soft budget; `tests/xwsettings.sh` stands at 467 lines of shell the budget scan does not read.

Pickfix records `tests/picker-grid.qml` 448 to 450 for the real grid stub's marking capability and range-reset method, each required by the live dispatch contract. Its temporary `src/backend/run.rs` ceiling increase from 662 to 663 for the startup limit raise returns to 662 when that raise moves to the picker budget path. The duplicate `src/backend/iomount.rs` entry is removed while its remaining ceiling stays 549. Each final count is re-derived with `wc -l`; these are all ceiling entries touched by `git diff fdfaa083..HEAD -- tools/flea-file-budget`. Product selection is extracted to `ui/PickerSelection.qml` and the unchanged Rust picker tests move to `src/backend/picker_tests.rs`.

038-usbrca confirms held files before closing their originals and drops the clone while its original remains open, retaining the 64-file cap and sticky failures; ceilings re-derived with `wc -l`: `src/backend/copyfile.rs` 694 to 730 for test-only open/write/mtime counts and partial-write faults, `src/backend/durable.rs` 662 to 678 for errno faults and confirm-before-close ordering, `src/backend/movebatch.rs` 470 to 474 for the new 216-line `usbrca_tests.rs` module, `src/backend/durable_tests.rs` 1248 to 1249 and `src/backend/movebatch_tests.rs` 751 to 752 for stronger confirm/clone/close ordering pins; `src/backend/copyfile_tests.rs` keeps 607 and all global limits stay unchanged.

038-usbrca r2perf hands each confirmed batch's held files to a background close wave, joined before the next wave starts and before the operation reports, plus the r1 test and doc fixes; the sticky failure, the one syncfs per batch and the clone drop before its original's final close are unchanged; ceilings re-derived with `wc -l`: `src/backend/durable.rs` 678 to 790 for the wave type, its closes on `std::thread::Builder` threads, the join and drain, the `Drop` backstop, the `finish` wrapper that joins before the done line, the one syncfs errno knob and the test-only gate, report-site read and open-descriptor count, `src/backend/durable_tests.rs` 1249 to 1305 for the duplicate, fallback rename and redo end pins, `src/backend/opsreq.rs` 551 to 554 for the test-only read where the done line goes out; `src/backend/usbrca_tests.rs` stands at 308, over the 250 soft budget and under the hard cap, for the errno, journal-count, mtime-tracker, bound, order, failure and end pins, and `src/backend/movebatch.rs`, `src/backend/copyfile.rs` and the other recorded ceilings keep their numbers.

038-stage7 joins clipx, recentcol, xw6r2, pickfix and usbrca, and six ceilings are the merged files' `wc -l`, because each merged file keeps the lines of both sides: `src/backend/opsreq.rs` 557 to 560 for usbrca's test-only read where the done line goes out beside clipx's request fields, `ui/TabBar.qml` 607 to 632 for the stage's release-position grab handler and cancel reset beside xw6r2's lift calls, `tests/js/drag.js` 427 to 469 for both sides' cases; `ui/WindowBody.qml` 635 to 631, `ui/js/Drag.js` 372 to 342 and `ui/js/Tabs.js` 723 to 719 fall from xw6r2's numbers because the stage had already shortened code that branch was built on.

038-stage7 then joins permfocus, and two more ceilings are the merged files' `wc -l` for the same reason: `ui/ContextMenu.qml` 691 to 703 and `ui/Ipc.qml` 844 to 861, where permfocus's own branch stood at 694 and 859 before it met the stage's clipboard and menu work; the 859 is the 854 its paragraph records plus the five lines of the read-only `visibleRowMode` seam (17b67227), which that branch added without a record.

038-stage7 then joins sweep038, the capture sweep and its readers, and three ceilings are the merged files' `wc -l`: `ui/Ipc.qml` 861 to 855 because sweep038 moves the preview readers into `ui/IpcPreviewState.qml`, `ui/Preview.qml` 723 to 724 for the `markdownView` reader, and `ui/PickerWindow.qml` 781 to 779, where pickfix had already shortened the file under its row and the sweep's theme fields add one line.

038-mdstage joins the Markdown stack onto the stage, and ten ceilings are the merged files' `wc -l`: `ui/Ipc.qml` 857 (the stage's 855 plus one-line wrappers for `columnMarkdownView` and `previewFigures`, whose bodies sit in `ui/IpcPreviewState.qml` with the other preview readers), `ui/Preview.qml` 646, `ui/PreviewColumn.qml` 585, `ui/Theme.qml` 416, `ui/js/Filter.js` 257, `tests/js/mounts.js` 262, `src/backend/jump_tests.rs` 408, `src/backend/proto.rs` 378, `src/backend/opsdispatch.rs` 2204, and `ui/PreviewMarkdown.qml` 784, the stack's 782 plus the Source measurement the stage's height loop fix needs in the stack's structure. The stack's `ListView` body has no `Column`, but `sourceFlick` still read `sourceText.implicitHeight` inside its `contentHeight`, so the stage's `measuredHeight` copy (updated on `onImplicitHeightChanged` and `onVisibleChanged` while the view is Source) feeds `contentHeight` there. The stage's named bar literals (`ui/Preview.qml`'s inline `markdownBar`) went with that bar, which the stack moved into `ui/MarkdownPane.qml`, where its one remaining literal is now `ruleOpacity`, `segmentHeight` having gone with the segment (2026-10-03). `packaging/flea-bin-tarball` and the root `PKGBUILD` now install the same ui set as the three `package()` functions: `ui/js/*.mjs`, `ui/vendor/*.mjs`, its licences and the four boot files. `tests/figure-package.sh` holds the tarball script to that: every path flea-bin's `package()` installs must be a path `packaging/flea-bin-tarball` stages, which the Release workflow checks only once a tag is being built. The check parses each `install` statement (continuations joined, comments skipped) and stages a source at the `-t` directory plus its basename pattern, or at the last argument when there is no `-t`, so a path installed into the wrong directory, named only in a comment or left out fails by name, the binary included, and three doctored copies of the script prove it. `ui/ViewportScrollBar.qml` measures a flickable's content plus its leading and trailing margins and its origin less the leading one, which is the span `Scroll.limits` lets the view travel: a list with margins (the rendered Markdown list, the grid) otherwise showed no bar for a document inside the margin window and a knob pinned at both ends for a longer one, and `tests/scroll-bounds.qml` pins the knob at the top, a third, the end and after a drag on a short, a long and a marginless list.

038-mdauto 2026-10-03 (the Markdown bar has no Rendered or Source control and no stored view) moves six ceilings, each re-derived with `wc -l`: `src/uistate.rs` 659 to 674 for the test that a state file still carrying `preview.markdownView` loads, keeps the key as read and refuses a patch for it; `tests/markdown-render.qml` 628 to 643 for the long-name pane and the `Bar.nameRoomError` check, with the bar geometry assertions in the new `tests/markdown-bar.js` at 60 (within the JS budget, copied by `tests/markdown-render.sh`); and four lowered with the code they lose: `ui/Preview.qml` 646 to 645, `ui/PreviewColumn.qml` 585 to 582 (the column no longer binds a view), `ui/ViewState.qml` 538 to 536 (the normalised reader) and `src/uischema.rs` 501 to 499 (the leaf and its rule). The 038-mdstage line above records its own day's counts and stays as written.

038-stage8 2026-10-04 sets seven ceilings back to their files' `wc -l`, because a merge that keeps the larger side's ceiling loosens a row whose file the other side shrank: `ui/TrashView.qml` 475 to 466, `ui/ViewState.qml` 538 to 536, `ui/PermissionsDialog.qml` 680 to 674, `ui/PreviewColumn.qml` 585 to 583, `ui/SettingsRow.qml` 410 to 409, `src/backend/undo.rs` 612 to 606 and `src/backend/undoshare.rs` 431 to 403. `ui/ColumnPane.qml` is 400 lines, inside the global QML cap, so it carries no row.

038-setfocus 2026-10-04 (a focused protocol chip draws its own accent frame where the accent has a hue, A's ring where it has none) moves one ceiling, re-derived with `wc -l`: `ui/Theme.qml` 427 to 433 for the named `hueFloor`, the `accentHasHue` role and its `applyColors` line; the hueless checks sit in the new `tests/button-hueless.qml` at 97 lines, inside the global QML cap.

038-mdline 2026-10-04 (Markdown lines centred in their 1.7 box, the Quick Look pane inside its frame) moves two ceilings, each re-derived with `wc -l`: `tests/markdown-render.qml` 718 to 793 for the centred-line probes, the real-block checks and the probe-overlap check (their helpers sit in the new `tests/markdown-centre.js` at 74, within the JS budget, copied by `tests/markdown-render.sh`), and `ui/Preview.qml` 659 to 661 for the Markdown pane's hairline margin and its comment.
038-qlopen 2026-10-04 (Quick Look on a Markdown file shows its content in the first frame) moves three ceilings, each re-derived with `wc -l`: `tests/markdown-linearity.qml` 555 to 556 for the `pointFile` stub its path-handler probe needs, `ui/Preview.qml` 661 to 679 for the held compiled pane unit with its timer, the `QuickLookPrepare` instance and the `inlineMarkdownPath` decision, and `ui/PreviewMarkdown.qml` 778 to 813 for `blockPath`, `pointFile`, `shareParse`, `reusedParses`, the entry take and store in `parseNow`, less the hex helpers that moved to `ui/js/MarkdownPrepared.js` (42). `ui/QuickLookPrepare.qml` is 118, `tests/quicklook-firstframe.qml` 319 and `tests/js/mdprepared.js` 46, all inside their budgets. Merged onto the stage (stage8 merge 5, `wc -l` of the merged files): `tests/markdown-linearity.qml` 598, `ui/Preview.qml` 679 and `ui/PreviewMarkdown.qml` 859, replacing the lane's 556, 679 and 813 and the stage's 597, 661 and 824. `tests/markdown-blockcost.sh` limits are the probe's counts on the merged delegate (run 13, heading 13, table 39, list 24, quote 15, fence 13, remote 19, image 14, images 19): the lane's remote 18 gains the stage's one inert Component, and the stage's 323 falls to 19 with the one-Shape dashes.

038-mdhtml 2026-10-04 (raw HTML and pathological Markdown never render blank) moves two ceilings, each re-derived with `wc -l`: `ui/PreviewMarkdown.qml` 844 to 864 for the nesting-depth Source fallback (`tooDeep`, `shownView`, the notice) and the width and centring of a local image, and `ui/js/MdBlocks.js` 361 to 369 for `NESTING_LIMIT` and the sentinel. The root causes: Qt's Markdown importer drops everything after a raw `<img>` tag in a run (so the GitHub header idiom blanked the whole document after its heading), which is why `ui/js/MdHtml.js` now holds a local picture as a Markdown image and the new `ui/js/MdHtmlImage.js` turns a raw image alone in its `<p>` or `<div>`, on one line or three, into an `image` block carrying its `width` attribute and `align: "center"`; and a document nesting containers deeper than 32 (`Markdown.NESTING_LIMIT`) parsed to one 482,691-character quote Text and 80,052 characters of brackets, so `blockPass` now answers one `{ type: "deep" }` sentinel the moment its container stack passes 32 (the parse stays linear and takes 47 ms where it took 825 ms) and the pane shows the plain Source behind the one-line notice `Markdown.deepNotice()`. Source over the rendered tree was chosen over capping the nesting because the cap needs the deeper levels flattened inside the parser and the sentinel needs no parse at all. `kbd` and `code` raw tags draw as the inline code chip, `<summary>` as a bold line led by the open disclosure mark `\u25be` with the body drawn open (the pane takes no focus and its delegates are one Text per block, so a hidden body would need a new block model), each block-level raw tag after a blank line starts its own run because the importer joins an HTML block onto the paragraph above it, and `tests/markdown-html.sh` with `tests/markdown-html.qml` at 220, `tests/markdown-html.js` at 176 and `tests/js/mdhtmlgh.js` at 92 hold the never-blank check over all fifteen head-to-head fixtures (`tests/md-fixtures.py`) and each element's pixel recipe.

038-mdhtml round 2 2026-10-04 (a run of images in one HTML block is one row, the logo keeps its block gap) moves two ceilings, each re-derived with `wc -l`: `ui/PreviewMarkdown.qml` 868 to 880 for the `Flea.MarkdownImages` instance and the block gap under a picture block, and `tests/markdown-linearity.qml` 555 to 573 for the `htmlBlocks`, `htmlLines` and `htmlRow` inputs. The GitHub badge row (several `<a><img></a>` side by side in one HTML block, the idiom every README header uses) was drawn as stacked image blocks; `ui/js/MdHtmlBlock.js` now answers one `{ type: "images", items, align }` block per run of images with nothing visible between them (a `<br>` starts the next row, at most 64 pictures to a block), and the new `ui/MarkdownImages.qml` places the items in a wrapping row, centred line by line when the block is, each picture keeping its own link and its HTML `width`. A picture block keeps the pane's block gap under it, so the heading after a logo no longer touches it. The new `htmlRow` input (one line of thousands of linked images) measured 62 times the work at 8 times the size: `findImage` lowered and cut the whole line for every image, and the work meter charges a whole-line regex call per image, so the splitter now walks each line by position over one `asciiLower` copy (ASCII only, since `toLowerCase` of U+0130 grows the string and shifted every cut) and holds the ratio at 8. `tests/markdown-html-pictures.js` holds the picture verdicts split from `tests/markdown-html.js`, which stood at 299 lines against a 300 cap.

038-mdhtml round 3 2026-10-04 (one picture size rule, paragraphs stay stacked) moves one ceiling, re-derived with `wc -l`: `tests/markdown-linearity.qml` 573 to 580 for the `htmlNested` input (image wrappers nested k deep, which made `imageUnit` scan to every opener's closer and then decline, O(k^2); `wrapperTail` now answers null at the next line holding an image, so each opener's scan ends there and the work holds at 8 times for 8 times the size). `HtmlImage.pictureSize` is the one size rule of the lone picture and the badge row: a `width` or `height` attribute is honoured past the natural size, both attributes draw their box stretched as a browser does, none draws the natural size, and the pane width caps every case with the ratio kept; `ui/PreviewMarkdown.qml` stays at 880 because the import and the `fit` and `height` lines replace the old `wantWidth` and alignment lines. A `</p><p>` or `<div>` between two pictures of one HTML block now starts the next row (`BLOCK_BOUNDARY`), as GitHub stacks one paragraph per line.

038-stage8 2026-10-04 (mdhtml merged into the stage after mdlean) moves one ceiling, re-derived with `wc -l`: `ui/PreviewMarkdown.qml` 778 to 817 for the mdhtml hunks carried into the stage's per-kind `Loader` layout (the `tooDeep` Source fallback and its notice, the `imagesBlock` component that builds only a badge row, the lone picture's `HtmlImage.pictureSize` and link handlers in `imageBlock`, and the `pictureGap` the delegate adds under a picture block). The merged file's own count, not either parent's 778 or 880.

038-stage8 2026-10-04 (mdspec merged into the stage) moves two ceilings, each re-derived with `wc -l` of the merged file: `tests/markdown-linearity.qml` 597, the sum of both sides' raises over the merge base 555 (the stage's +25 for `htmlBlocks`, `htmlLines`, `htmlRow` and `htmlNested`, mdspec's +17 for `spaceFlood`, `spaceAlternate`, `spaceTrail` and `spaceIndent`; neither parent's 580 nor 572), and `ui/PreviewMarkdown.qml` 817 to 824 for mdspec's empty heading and empty quote rule carried into the per-kind Components (`textBlock` and `quoteBlock` each take no height and draw nothing when empty, as the delegate's old height chain did). The merge also closes a gap between the lanes: a raw HTML anchor's `href` was checked by a numeric-only decode, so `java&Tab;script:` and `&amp;#106;avascript:` survived as hrefs that mdspec's `isLinkTarget` refuses; both now call `MdUrl.targetAllowed` (the multi-pass check moved from `ui/js/MdResolve.js`), pinned by `tests/mdhtmlsecurity.js` through `tests/markdown-security.sh`. `tests/markdown-spec.qml` records `HTML blocks` 16 to 17 (example 185 now draws as the spec's HTML through the stage's block separation), 638 to 639 of 706, and `tests/markdown-linearity.sh` expects 23 inputs, 19 plus the stage's 4.

038-stage8 2026-10-04 (mddraw merged into the stage after fsmlink) moves two ceilings and re-derives the object limits, each from `wc -l` or the probe of the merged tree: `ui/PreviewMarkdown.qml` 824 to 507 (the per-kind Components live in `ui/MarkdownBlockView.qml` as mddraw split them, and the stage's hunks moved there with them: the `images` kind and `imagesBlock`, the lone picture's `HtmlImage.pictureSize` and link handlers, and the `pictureGap` under a picture, now counted against `blockCount` so a nested part follows the same rule as a top-level block; the `tooDeep` Source fallback stays in the pane), and `tests/markdown-linearity.qml` 614, the stage's 597 plus mddraw's 17 for `fenceItems` and `nestMixed` (`tests/markdown-linearity.sh` expects 25 inputs, 23 plus 2). `markdown-blockcost` limits each rise by the one inert Component the `images` kind adds (ten now): run, heading and fence 14, table 40, list 27, quote 18, remote 324, image 15 and images 20, and maths 36, the same one plus the two `FontMetrics` each figure now holds for a diagram's advances (10 objects a distinct formula on a base of 16). `tests/markdown-spec.qml` is 639 to 643 of 706 on the merged tree, each section at or above the stage's record: `Block quotes` 24 to 25 (example 237, the empty fence in a quote) and `List items` 45 to 48 (examples 259, 292 and 293), the nested forms mddraw's parts draw. A nested item or quote goes through the document writer's one `pushRun` order (display-math split, the HTML-image split, `pushPiece` with the block separation, the `<br />` collapse and the bare-text inline pass), so it takes the same conformance and HTML handling as a top-level block; `tests/js/mdnest.js` pins an image, a badge row, a raw picture, the `<br />` collapse, the block-level raw tag split and a mermaid figure inside an item and a quote, and `tests/mdhtmlsecurity.js` (through `tests/markdown-security.sh`) now runs every spelled address, Markdown destination, anchor wrapper, image and mark form at the top, in a list item and in a quote.

038-stage8 2026-10-04 (mddraw merged into the stage after qlopen, the security check fix and contrib; merge 8) re-derives what the two records above left on different trees, each from `wc -l` or the probe of the merged tree: `ui/PreviewMarkdown.qml` 542, mddraw's 507 plus qlopen's `blockPath`, `pointFile`, `shareParse`, `reusedParses`, `sourceChars`, the entry take and store in `parseNow` and the `shownView` Source text, less the hex helpers that live in `ui/js/MarkdownPrepared.js` (the `HtmlImage` import went with the per-kind Components to `ui/MarkdownBlockView.qml`, so the pane imports only `Markdown` and `Prepared`), replacing the stage's 859 and mddraw's 507; `tests/markdown-linearity.qml` 615, mddraw's 614 plus qlopen's `pointFile` stub line (stage 598). `tests/markdown-blockcost.sh` limits are the probe's counts on the merged delegate: run 14, heading 14, fence 14, maths 36, table 40, list 27, quote 18, remote 20, image 15 and images 20; the remote kind is qlopen's one `Shape` (19 on nine inert Components) plus the tenth inert Component, so mddraw's 324 for the old per-dash boxes falls to 20, and the other kinds equal mddraw's. `tests/mdhtmlsecurity.js` keeps mddraw's top, item and quote loop with the security fix's check, no emitted href for any spelled hostile address (the `MdResolve` import stays out). `tests/staticgates-unqualified.tsv` is mddraw's file with both profiles regenerated by `tsv-fails.py` on the merged tree (86 stale or new reads applied to `quickshell-missing`, 112 to `quickshell-present`). `tests/markdown-spec.qml` stays 643 of 706. `tests/run-all.sh` and `tests/js/harness.qml` hold the union (`markdown-nest`, `mddraw` and `mdnest` beside `quicklook-firstframe`, `watch-views`, `colwatch-gate`, `mdprepared` and `navmouse`).
038-mdq2 2026-10-05 (the deep fallback and a faster settle for the 1 MiB document) moves four ceilings, each re-derived with `wc -l`: `ui/PreviewMarkdown.qml` 542 to 520 and `ui/js/MdBlocks.js` 369 to 365 (the Source view is `ui/MarkdownSource.qml`, a list of chunks of whole lines about `Markdown.SOURCE_CHUNK_CHARS` long (none past `SOURCE_CHUNK_MAX`: a longer line is cut at a space, or at the limit, and shows a line break there) whose first row holds the top inset and the too-deep notice and whose last holds the bottom inset, so a screenful of a 297 KB Source is laid out instead of the whole text; `Markdown.deepHead` runs the nesting pass over at most `DEEP_HEAD_BYTES` of the head, so a deep document is refused on the UI thread with no worker), `ui/js/MdDocument.js` is 284, under its cap (`headed`, the projection that hands the first `Markdown.HEAD_BLOCKS` written blocks to the worker's first reply while the rest still parses; blocks written are final, so the whole parse starts with them) and `tests/js/previewswap.js` rises to 389 (`lookReady` takes a Markdown head as a whole first screen, so a move to a big document releases the held picture on the head) and `tests/markdown-linearity.qml` 615 to 638 (its lifecycle stubs run the head request, the head landing over a running parse, the reparse that asks for none and the deep head that lands the sentinel with no worker and no parse). The head is sent only for a file with nothing drawn yet, so a reparse keeps the list and the reader's place until the whole one lands. `tests/quicklook-firstframe.sh` runs the `deep` leg and the `partial` steps of the order leg (at most 6 chunks laid out, at most 40 delegates at the first content frame with the parse still running, the swap released on the head) and `tests/markdown-lazy.sh` bounds the Source list to 24576 characters at the top and at the end of 573019. `tests/staticgates-unqualified.tsv` follows the moved lines (21 reads in `quickshell-missing`).

038-stage8 2026-10-05 (mdfast merged into the stage; merge 10) moves one ceiling, from `wc -l` of the merged file: `ui/PreviewMarkdown.qml` 566, the stage's 542 plus mdfast's 24 for `warmBlocks`, `hasFigures`, the `themeProbe` `Loader`, `warmRequest` and its handler (mdfast's 867 counted the per-kind Components that now live in `ui/MarkdownBlockView.qml`, so neither parent's number holds). The probe mirrors the `figureBlock` Component's `MarkdownFigure`: the same five hex sources, `Theme.font.family`, `bodyPx` and `display: true`, and its `figuresArmed` is the stage's `shownView` form, so a document past the nesting limit warms nothing. Probe and placed figure ask under the same keys; `tests/figure-store.qml` pins it through `MarkdownBlockView`. `tests/staticgates-unqualified.tsv` is the stage's file with both profiles regenerated by `tsv-fails.py` (16 stale or new reads on each), and `tests/run-all.sh` holds the union (`figure-warm` and `figure-store` beside `markdown-nest` and `quicklook-firstframe`).

038-mdq1 2026-10-05 (Quick Look's first open compiles nothing and draws its small pictures; the rawText binding loop) moves two ceilings, each re-derived with `wc -l`: `ui/Preview.qml` 679 to 680 for the held swap unit beside the Markdown unit (one line, the Timer handler stays one line), and `ui/PreviewMarkdown.qml` 542 to 550 for `loadedText`, which the load sets in place of the binding on `file.text()`, and the one blocking read `pointFile` takes. `ui/QuickLookPrepare.qml` is 216, `ui/js/MarkdownPrepared.js` 101, `tests/quicklook-firstframe.qml` 397 (cap 400), `tests/quicklook-pictures.qml` 99 and `tests/js/mdprepared.js` 67, all inside their budgets.

038-qlwarm 2026-10-05 (memfix's lazy Quick Look keeps the first-Space gains) moves two ceilings, each re-derived with `wc -l`: `ui/WindowBody.qml` 644 to 651 for the `warm` property, its trigger and the one `createObject` that builds `ui/QuickLookWarm.qml` (memfix's 644 counted its own `previewLoader`), and `ui/Preview.qml` 680 to 669 for the prepare and the unit warm that left it. `tests/quicklook-firstframe.qml` is 398 lines, under its cap, so it carries no recorded ceiling, and `tests/quicklook-firstframe.js` 147.

038-stage8 2026-10-05 (mdq1 merged into the stage after mdfast, pr253 and mdfix8; merge 13) moves one ceiling from `wc -l` of the merged file: `ui/PreviewMarkdown.qml` 574, the stage's 566 plus mdq1's `loadedText` and its writes in `onLoaded`, `onLoadFailed` and `pointFile`; `tests/quicklook-firstframe.sh` keeps the stage's instrumented ui copy (`qslog_ui_copy`, `QLFF_UI` on it, the generic platform theme and the nullptr gate) beside mdq1's compile trace and binding loop gates; `tests/staticgates-unqualified.tsv` is the stage's file with both profiles regenerated by `tsv-fails.py`.

038-stage8 2026-10-05 (mdq2 merged into the stage after mdq1; merge 14) moves one ceiling from `wc -l` of the merged file: `ui/PreviewMarkdown.qml` 552, the stage's 574 less mdq2's Source view, which moved to `ui/MarkdownSource.qml` (mdq2's 520 counted its own base); `ui/js/Markdown.js` keeps the stage's `figuresIn` beside mdq2's `deepBlocks` and Source chunk cuts, `tests/js/mdnest.js` both sides' checks, and `tests/quicklook-firstframe.qml` the stage's picture count with mdq2's deep step; the firstframe fixture PNG is the stage's valid red pixel (mdq2's black one is valid too). `tests/staticgates-unqualified.tsv` is the stage's file with both profiles regenerated by `tsv-fails.py`.

038-stage8 2026-10-05 (tblq and qlwarm merged into the stage after mdq2) records two ceilings from `wc -l` of the merged files, each the arithmetic of a clean merge: `ui/js/MdLeaf.js` 309 (base 268, mdmath's 278, tblq's 299) and `tests/quicklook-firstframe.qml` 406 (base 369, the stage's 377, qlwarm's 398 for its nobuild leg and the prepared-entry pin). 038-s8qm 2026-10-05 (qlmath merged over that stage) moves `tests/quicklook-firstframe.qml` to 419: the stage's 406 plus qlmath's
13 (377 to 390, the `i-maths` leg, the decoder warm count and the sizer probe), a merge-arithmetic ceiling.

038-qlfirst 2026-10-05 (the first Space only opens a card the rest built) moves one ceiling, re-derived with `wc -l`: `tests/quicklook-firstframe.qml` 419 to 432 (13 lines: the card loader's `loaded` counter and its `Connections` 6, the KEY check that a rest left the card built and closed 4, the key's own check that it built none 3), `noopen` renaming `nobuild` with no line change.

md3 brings the Markdown view to the RenderedPreviews board's rhythm, each re-derived with `wc -l`:
`tests/markdown-render.qml` 422 to 493 for the inset, rhythm, heading, line-box, surface, fence-padding and bar
geometry checks, and `ui/js/MdBlocks.js` at 359 lines (`wc -l`, matching its budget row) for ATX headings as a block of their own. Headings now render bold
in the foreground at 20/14 and 15/14 of `Theme.font.body` through `ui/MarkdownText.qml`'s 1.7 line box, which
supersedes k2's Qt-sized headings above; setext headings keep Qt's sizes. The block gap, fence padding and document
inset come from the existing row tokens rather than new literals.

md3's marker-baseline follow-up keeps list markers on the item's line box through literal rich
text instead of plain text; Markdown would parse an isolated ordered marker as its own list.
Their explicit height is the font-derived 1.7 box, so rich text's extra implicit height cannot stretch a list row.
`tests/markdown-render.qml` grows from 493 to 628 lines (`wc -l`, matching its budget row) for grabs
at the suite's body size and a larger supported stop, mapped baseline checks, quote-bar span,
table header and body alignment, remote-placeholder centres and failed-figure padding. Qt's
`baselineOffset` omits the Markdown fixed-height leading, so `tests/markdown-render.js` also
compares first-line ink against a single-line reference that draws marker and item on one shared
baseline, accounting for raised bullets and descenders without trusting fallback-font glyph bounds.
Only this harness ceiling changes; the global caps and every existing assertion remain in force.

The mdstack merge keeps md2's escaping, bulk escaper and linear span parser alongside md3's headings, figures
and baseline checks. The union of their structure tests, plus escaped-dollar, heading-footnote integration and
final escaped table-pipe checks, records `tests/js/mdstructure.js` at 596 lines (`wc -l`, matching its budget row).

The mx2 merge into the stack keeps one deferred ask in `ui/MarkdownFigure.qml`: every property change restarts the 50 ms
`askTimer` once the figure is `created` (entering the viewport is the one direct ask, for a placed figure that holds nothing), so a burst sends one request and one equal to the last sent is dropped, and
the timer still outlasts ListView's placement so `inView` reads the placed position. The figure cache key
`FigureService.cacheKeyOf` carries every theme role the diagram paints (muted, line, surface, border), and the unexpected-exit
log fires only when a figure fails its second strike. `ui/js/FigureWorker.mjs` records 513 lines at that merge (its `tools/flea-file-budget` row holds the current count), `ui/FigureService.qml` 298. The md2 merge
leaves `tests/markdown-linearity.qml` at 543 lines, the merged `wc -l` where both sides added cases (399 on the stack,
529 on md2), and its budget row was that number at that merge.

mdhunt round 2 closes the three gaps in how round 1 keeps the reader's place, and `ui/PreviewMarkdown.qml` was 782 lines (`wc -l`) there and is 784 on the stage (its budget row). The coalescing window is not
restarted: `onFileChanged` starts `reloadCoalesce` only when it is not running, the same rule as the directory watcher's 400 ms timer, because `restart()` pushed the 50 ms window back on every
event and a file written every 20 ms never reloaded until its writer stopped (`disk-stream` appends 40 times, 20 ms apart, and requires reloads to land while the writer runs, fewer than
the writes, and the final text after it stops). A held place ends the moment the reader moves: the list's `onContentYChanged` calls `releaseHeldPlace`, which drops `keepScroll` and `heldY`
once the view is `samePlacePx` or more from `heldY`, `restoreScroll` records `heldY` before it moves the view so its own move reads as the same place, and `settingBlocks` brackets each
`blockList` assignment (in `landed` and `parseNow`) because a model reset moves the list to its top before `restoreScroll` runs and that is the list, not the reader. `rememberScroll` then keeps a
place only while one still waits, where round 1 compared `contentY` with `heldY` at the next reload and so read a reader who scrolled away and back as not having moved (`disk-regrow` moves the
view up 200 px and back, writes the full text, and requires the view to stay; `disk-partial` still returns to the deep place). `disk-uneven` (60 one line paragraphs then 60 code blocks of 25
lines, the reader past three quarters of the real height, a same-length edit of the first paragraph) found no estimated bound to fix: in the turn the model is replaced `contentHeight` is still the
previous layout's exact 27261, equal to the real height the phase measures by walking the document, and it never reported a smaller one while reloading, so `restoreScroll` keeps its form and
the phase stays as the guard. The new phases live in `tests/markdown-disk-reader.qml` (`tests/markdown-disk.qml` stays under its 400 line cap), `overshoot` is sampled on every `contentY` and
`contentHeight` change, `disk-worker` records `parseRuns` and proves the worker landed the edit (a Qt.callLater probe sees `parsing` still true a turn after the ask) and that one save, a truncate
and a write 10 ms apart that raise two watcher events, is one load. `tests/preview-hunt.sh` runs 28 phases. `rememberScroll` clears `heldY` when it takes a new place, because a path change and a failed load drop `keepScroll` and leave `heldY` set, and a stale one would let the reader's next
move end a place taken for a reload still in flight (F43 in `tests/markdown-preview-state.qml` runs the shipped body on such a state).

mdhunt round 3 closes a hold released while a reload is in flight: `reloadFromDisk` took the place and the reader then moved, so `releaseHeldPlace` dropped `keepScroll`, the landing replaced the model (the list resets to its top) and `restoreScroll` returned on `!keepScroll`, leaving the view at the top. Both landings, the worker reply in `landed` and the synchronous `parseNow`, now call `rememberScroll()` just before they set `settingBlocks` and replace `blockList`: it is a no-op while a place waits for taller content, and otherwise takes the reader's present place, so a landing restores to where the reader is and not where the reload was asked. That made the `rememberScroll()` calls in `reloadFromDisk` and `reparseForTheme`, and the `askParse` skip that released their scroll when nothing would land, dead, so they are gone (`disk-scroll`, `disk-worker`, `disk-partial` and the `theme` phase pin the paths they served). F48 and F49 in `tests/markdown-preview-state.qml` run the shipped bodies on a stub list whose model reset puts `contentY` at 0; `disk-stream` requires `streamMinReloads` (3) reloads while the writer runs, `disk-uneven` ends its walk when the view rests at the end and `contentHeight` stops growing, and `disk-regrow` asserts the hold waits before the reader moves. Round 4 moved `parseNow`'s call behind the parse: `Markdown.blocks` runs first inside the `try`, and only a parse that returned takes the place and replaces the model, because a parse that throws replaces nothing and no landing would release a place taken for it (F50 in `tests/markdown-preview-state.qml`); the `finally` is gone with it, `settingBlocks` now brackets an assignment that cannot throw. Since the mdmath merge a blank line ends a paragraph, so a file of many short blocks has no tall run to average, and after a model reset the list estimates the document from the few delegates it built (`disk-uneven`: 2995 against 53275, and its `originY` sits at -25140 after a walk, so the phase now aims at `list.originY + Math.floor(root.unevenDepth * root.realHeight)` (the measured real height)): a pixel place was clamped to that estimate and `holdEnd` then followed the end, so a reader at 17480 landed at 12066. The place is now also kept by block: `rememberScroll` stores `savedIndex` and `savedOffset` (the built block whose bottom is first below the view's top, and the pixels the view lies past its top, from `topBlockItem`), and `restoreScroll` restores by `positionViewAtIndex` then that offset while the new list still holds the index, bounding by the end only once the last block is built; a model too short for the index takes the pixel path, which `disk-shrink` and `disk-regrow` pin. The list's own `contentY` moves whenever blocks are built, so the phase compares `[index, offset]` before and after the edit, not raw pixels. `ui/PreviewMarkdown.qml` grew from 559 to 584 lines for it (`topBlockItem` 13, the saved block 6, the block branch of `restoreScroll` 6), its budget row, and `tests/markdown-linearity.qml` from 648 to 650 lines (its budget row) to bind `topBlockItem` on its stub root, since a missing function threw inside `callbackChecks` and the harness then waited for its 3000 s backstop. s8ufix takes `tests/markdown-preview-state.qml` 395 to 467 for the by-block place pins (the built-end check also asserts the by-block call), re-derived with `wc -l`.

md3u closing round 4 hardens the Markdown parser's edges. A multi-line `$$` display block closes on the first later line holding `$$`
only inside its own paragraph: the search stops at the first blank line, so a stray opener stays text instead of swallowing the
document up to the next `$$` (a scan ends at the first later line holding `$$`, and every opener is such a line, so no line is scanned twice). The
keyed maps `defs`, `notes` and `numbers` have no prototype and every reader asks `Object.prototype.hasOwnProperty.call`, because a
footnote id of `__proto__` threw and `hasOwnProperty` left its citation unnumbered. `prepare()` runs the one collecting block pass;
a second pass over the same lines was measured to change none of `code`, `escaped`, `hidden`, `dropped` or the three maps on 20,000
generated documents. `MdLeaf.js` names the marker limits `MdContainer.js` names, and `localAbsolute` refuses a backslash in the
reference the document wrote while the folder may hold one (`containedLocal` keeps the containment check as strict as before).
`prepare()` has no caller in `ui/`: the preview draws `blocks()`, whose front matter and `$$` lines are a `PlainText` fence or a
figure, and `tests/markdown-security.sh` now carries both placements and a Source-view instance with zero requests required.
`tests/js/mdround4.js` holds the parser pins. The file budget row of `ui/js/MdBlocks.js` moves to its real `wc -l`, 361.

The mdhunt fixes change four Markdown preview behaviours, with `tests/preview-hunt.sh` phases `links`, `theme`, `disk`,
`disk-rename`, `disk-scroll`, `disk-stale`, `long-list` and `long-table` pinning them (`size-key` keeps 0.3.7: preview
context refuses the listing size chord). Links: `ui/MarkdownText.qml` has the one `onLinkActivated`, and only a link its
`linkGate` passes reaches `Qt.openUrlExternally`; `PreviewMarkdown.qml` sets the gate to `Markdown.isExternalLink` (`http`,
`https`, `mailto` after the parser's own normalisation) on each text, because `markdown-linearity` loads the component
alone, with no `js/` beside it. A relative link, an anchor and any other scheme open nothing. Theme: the link ink and code chrome are parsed into the
runs (the importer hardcodes its link blue over `linkColor`), so `PreviewMarkdown.qml` reparses when `inkHex` or `chromeHex`
changes. Disk: the one `FileView` on the shown file has `watchChanges`, reloads on a change or an editor's rename-over save,
and `savedY` puts the ListView back where it was once the new model has reset it; a path change re-points the watcher and
clears the saved place, so an old file never reloads into the new one. Long containers: `MdLeaf.chunkList` and `chunkTable`
split a list of more than 32 items or a table of more than 24 rows into consecutive blocks the outer ListView already draws
lazily. List chunks carry `start` (numbering continues) and `last` (one marker column width); table chunks carry `measure`,
the widest cell per column over the whole table chosen by its drawn text length (tags, emphasis markers and entities do not
count), so chunks share column widths, and only the first keeps the header. A chunk after the first is `joined`: its grid
sits `blockGap` higher and its delegate is that much shorter, so chunks read as one container. A short list or table is one
block as before. `ui/PreviewMarkdown.qml` was 782 lines (`wc -l`) and is 784 on the stage (its budget row), and `ui/js/Markdown.js` 155. One event is one parse: `askParse` skips a request equal in text, folder, chrome and ink to the last one that parsed, a path change only voids the old result (the new file's load asks), a theme switch asks once through `Qt.callLater`, a landed parse releases the saved scroll even when the list saw no model change, and `parseRuns` counts the parses that ran so the `parse-quick`, `parse-column` and `parse-worker` phases of `tests/preview-hunt.sh` (`tests/markdown-parse-count.qml`) pin one parse per open, disk edit, rename-over save and theme switch, and none for a save that changes nothing.

mdhunt round 1 keeps the reader's place and the error state honest across a save, and `ui/PreviewMarkdown.qml` was 766 lines then (`wc -l`). A good load clears
`readFailed` (a save that unlinks and recreates the file can fail one reload, and the old code kept "This file could not be read." after the file was back). The watcher's
events go through one `reloadCoalesce` timer of 50 ms (`reloadCoalesceMs`: an editor's truncate, write and rename land within a few milliseconds, and 50 ms is well under what a
reader notices), and a place the saved scroll cannot yet reach is kept: `restoreScroll` bounds the view between the list's resting top (`originY - topMargin`) and the end of the new
content, releases the place only once the content is tall enough to hold it, and otherwise records where it left the view (`heldY`) so the next reload restores the same place
until the reader moves (round 2). A path change still clears it, and stops a reload still waiting in the coalescing window: that reload belongs to the old file, and
left alone it read the new file a second time (`disk-switch` switches files in the turn the timer starts and requires one load). `parseNow` is the one synchronous landing, used by `askParse` and by the `parseFallback` timer, which now parses
the request that was sent (`askedText` and its folder, chrome and ink, never `rawText`), releases a remembered scroll, and on a throw voids the memo so the next equal request
parses again; `askParse` clears `parseError` only when it goes on to parse. `loadRuns` counts the loads that landed. `tests/markdown-disk.qml` holds the `disk-fail`,
`disk-partial` (truncate, a half-written file, then the full text), `disk-shrink`, `disk-switch` and `disk-worker` (a file past `workerThreshold`) phases, `theme` scrolls before the switch and
requires the place after it, and `tests/markdown-parse-count.qml` has every disk step wait for `loadRuns` to rise (a named poll limit fails by name), the worker phase switch files
in the turn the first request is asked and prove the late reply is dropped, and a `parse-fallback` phase that runs `tests/preview-hunt.sh`'s scratch copy of `ui` (a worker that
never answers, a 200 ms fallback wait, and a parser that throws on one marker) so no product seam exists for it. The chunk sizes the hunt reads are `MdLeaf.js`'s own
`TABLE_CHUNK_ROWS` and `LIST_CHUNK_ITEMS`, and each of the four rows its pitch check compares must be found first.
`tests/markdown-linearity.qml` (615 lines, its budget row: 555 to 563 for the entity-space floods of mdspec round 4, 563 to 567 for the literal-and-entity `spaceTrail` flood of round 5, 567 to 572 for the `spaceIndent` flood of round 7 and its named indent width, 572 to 597 across the stage for the HTML floods of mdhtml, 597 to 614 for the `fenceItems` and `nestMixed` floods of mddraw, 614 to 615 for qlopen's `pointFile` stub line at stage8 merge 8) and `tests/markdown-preview-state.qml` bind the shipped `dropParse`, `parseNow`, `rememberScroll` and `restoreScroll` bodies to their stub roots,
find the fallback handler from its own id so an earlier Timer is never the one read, and give the list-grid probe a closed link gate; they had failed since the previous round's `dropParse` and `linkGate`.

mlazy 2026-10-05: a slow worker is never a lost one. `MarkdownWorker.js` acks a request that carries a head field at once and beats every
`PROGRESS_EVENTS` (4000) line events on both block passes, the reference-collecting one included (`MdBlocks.blocks` takes the beat as
`onProgress`); `landed` restarts `parseFallback` on an ack, a beat
or a head, never on the full reply, so the UI-thread parse runs only after `parseFallbackMs` (10000) with no message from the worker. Under load the
560 KiB `markdown-lazy` fixture took 6.1 to 9.0 s in the worker against 2.4 to 2.7 s on the UI thread, and the old rule froze the window to parse
it there; quiet, the beat costs nothing measurable (the worker with render-pass beats 3.1 to 3.4 s, the same text
parsed on the UI thread with none 2.9 to 3.6 s; with both passes beating 2.1 to 4.0 s, n=3). A request without `head` (another asker than the pane) gets neither the ack nor a beat. `tests/markdown-linearity.qml` runs the product `landed` body against a
counting fallback stub (red with the restarts removed). Ceilings: `tests/markdown-linearity.qml` 648 (was 638), `ui/PreviewMarkdown.qml` 559
(553), `ui/js/MdBlocks.js` 380 (365).

The menu card takes every press its rows do not (038-menusink). `ui/ContextMenu.qml`'s ground closes the menu on a click, and since 0.3.8 steps the highlight, `ui/CardScroll.qml` is no longer an interactive Flickable, so nothing inside the card took a press on a disabled row, a separator or the padding and it fell to the ground, closing the menu on Taildrop's disabled row. Each frame, main and flyout, now carries one `MouseArea` over its whole area beneath the card that accepts both buttons and does nothing: it takes no hover (the rows' `HoverHandler` and the ground's tracking still see the pointer), no wheel (the card's `FastScrollHandler` still steps one row a notch) and activates nothing, while a click outside either frame still closes on release. `tests/menu-card-sink.sh` pins it with real pointer events, both buttons on a disabled row, a separator and the top padding of each frame, and prints the counted checks as one `MENUSINK PASS <n> checks` line, then `MENUSINK STATUS qs_exit=143 done=1` as its last line (the probe's `MENUSINK DONE failures=0` receipt is required once and not echoed); `ui/ContextMenu.qml` goes from its recorded 703 to 709. `ui/PathJump.qml`'s ground sits inside its frame beneath the rows with every row enabled, so its only dead points are the padding strips, which closed in 0.3.7 as well; the dialogs keep an interactive `CardScroll` and their own card sinks.

A context menu is as wide as its widest row (038-menufit): `ui/MenuRow.qml` states `wantedWidth` from the layout's own terms (mark slot, label, gaps, hint, status square, chevron, paddings; a wrapped caption asks for none), and `ui/ContextMenu.qml`'s frame and flyout take `max(Theme.menuWidth, widest wantedWidth)` capped by the work area, so no label elides beside its hint and a menu whose rows all fit keeps exactly `Theme.menuWidth`. Key hints draw in the muted role (MenuAdditions040), an update's version keeps its row's ink, and a scrolled card's edge fades reach `fadeReach` past the card's padding into the first cut row, on the flyout as well. `tests/menu-fit.sh` pins widths, truncation, work-area placement and the fades at five text stops and both densities (`MENUFIT PASS <n> checks`), and `tests/settings-tail.sh` pins that Settings > Menus never draws its lower fade over the cursor row (`SETTAIL PASS <n> checks`).

A row's first hover at a point its menu has not seen is where the pointer rests (039-qt612menu). Qt 6.12 throttles its frame-synchronous hover to one every 100 ms (`frameSynchronousHoverInterval` in `QQuickDeliveryAgentPrivate`, `QT_QUICK_FRAME_SYNCHRONOUS_HOVER_INTERVAL`), so the hover for rows built under a resting pointer can land after `ui/ContextMenu.qml`'s `pointerSettling` ended on the first frame. With no point seen since `place()`, `ui/MenuRow.qml` read that hover as motion, and a tall Open with flyout opened by key closed itself. Qt 6.11 delivered the hover inside the first frame, before polish, where the ground recorded the point. `ui/MenuRow.qml` now reports motion only away from a point its host already saw (`lastPointerGlobal`, -1 since the host opened), so every host reports the pointer from its ground: ContextMenu and `ui/PathJump.qml` already did, and `ui/OpenWithDialog.qml` and `ui/ConvertDialog.qml` now do from their dimmed ground and card sink, Convert's format rows reading the same point. Real motion keeps the 6.11 answer wherever the pointer reaches a row across its host's ground, which is every path into a context menu row that starts inside its pane; a pointer that reaches a row before its host has seen it anywhere (PathJump's dropdown entered from its side, a dialog row reached straight from a button) is answered on its second motion event, the way every Settings row already is. `ui/SettingsRow.qml` arms on its first point and never reports it, so Settings never had the dependency. A first report under a pixel from the host's point is that point: a hover-enabled MouseArea that appears under the pointer takes Qt's cursor position rounded to whole pixels (`QQuickMouseArea::itemChange`, `ItemVisibleHasChanged`, the same in 6.11 and 6.12), and that is how the context menu's ground records the resting point inside `place()` on minipc. There, on Qt 6.12, `c` pressed right after a list step kept the cursor and the Copy as flyout 5 of 5 on the old rule because of that ground; with the ground's two handlers removed the old rule gave the cursor to the pointer's row and closed the flyout 5 of 5, and the new rule kept both 5 of 5. `tests/menu-card-sink.sh` pins the rule on both Qt versions in a second window whose pointer a lifted touch dropped (QQuickDeliveryAgent clears its last mouse position on a touch release), so the rows stand and the menu settles before a first hover reaches them; `tests/dialog-board.sh` pins the dialogs' grounds, and `tests/ui.sh menurest` drives it natively (a list step, then the menu, Down and Right inside one throttle window).
038-mdfid2 2026-10-05 (review fixes) moves two ceilings, each re-derived with `wc -l`: `ui/js/MdDocument.js` 311 to 341 for `headingBlocks` (the heading's leading picture as an image block, and no lift when the drawn heading text, references resolved, still holds a picture), the open-HTML `depth` that keeps a heading inside other open HTML in its run, and the lone-wrapper lift beside the image unit, and `tests/markdown-render.qml` 793 to 795 for the two fence pads its fallback probe is handed; `ui/js/MdHtmlHeading.js` is 53, `tests/js/mdhead.js` 86 and `tests/markdown-html-chips.js` 64, all inside their budgets.

A Markdown table never breaks a word, and one too wide for its pane scrolls sideways (tblwide). `ui/js/MarkdownTableFit.js` shares the width only while every column's longest word still fits; when even those do not, every column holds its word (or `least`, or its natural width when smaller) and the table is wider than its block. `ui/MarkdownTable.qml` then clips to the block (`overflows`), reserves `Theme.spacing.rowPaddingX` under itself for the bar, slides its rows by `-scroller.contentX`, and builds `ui/MarkdownTableScroll.qml` the first time it overflows (a table that fits owns no flickable, bar or router entry, which the block cost gate counts): a disabled `Flickable` holding the position and the extent (`contentWidth` is the sum of the held columns), the horizontal `ViewportScrollBar` under the table (drawn only while moving, hovered or dragged), and a `FastScrollHandler` whose `handleWheel` the router calls. `ui/MarkdownTableWheel.qml` sits at z 1001 over the document's own handler (z 1000), takes no press, and gives a table the sideways wheel, Shift+wheel and horizontal touchpad strokes over it (a stroke's axis is decided by its first update and held to its End, Begin goes to both), and leaves every vertical one unaccepted for the document. `tests/markdown-tables.qml` and `tests/markdown-tables-scroll.js` pin it (418 checks, real `TestEvent` wheels, the touchpad branch of the router, chunked and nested wide tables, and a widened pane dropping its scroll): no ASCII word wider than its cell, one flickable and bar per overflowing table and none on a fitting one, the wheel, Shift+wheel and the vertical wheel through the router, in Quick Look and the column; `tests/js/mdtable.js` pins the Fit rule. A cell that is one unbreakable run (the `path` fixture's 120-character URL) now draws on one line and scrolls rather than wrapping inside the word. Chunks of one table share `tableKey`, one sideways position kept by the router, and the bar's lane sits under the last chunk only. `ui/PreviewMarkdown.qml` goes from its recorded 559 to 570 for the `tableWheel` property, `tableScroller` and the router (the property and its comment, the function and its comment, 6 and 1 blank lines); `ui/Ipc.qml` 882 to 883 and `ui/js/MdLeaf.js` 309 to 314 for `previewTable` and the chunk key (a comment, the counter and its blank, a comment and the key line); `tests/ui-captures-markdown.sh` shoots `overflow.md` at rest and after a wheel to the right in Quick Look, aimed from `previewTable`'s rects and judged on its scrollX (the preview column has no IPC rect to aim the pointer at, so only its rest shot exists). `tests/markdown-tables.qml` is recorded at 401 against the 400 cap: the positioner host stage (its host component, stage and the `hosted` property), the stage's named failure when the host is not created, and the check that `tableScroller` answers the router's first live scroller (the first table to overflow, in the order tables joined the router, which Quick Look's IPC aims at) and that it is a flickable in the document. On the 038-s8qm line, which already held `ui/PreviewMarkdown.qml` at 584, the merge records it at 595 (584 plus tblwide's 11 lines). 038-tblfix 2026-10-05: `MdLeaf.cellExtent` drops mdfid2's chip pad span before it counts (`&nbsp;` was read as 6 columns a pad, which inflated a code column's weight, lowered `glyphPx` and gave the Kind column 40 px for a 47 px word), and `ui/js/MdLeaf.js` goes from 314 to 316 (the `CHIP_PAD_CELL` comment and pattern; an `MdEscape` import there made the 1 MiB fixture fail to parse, so the pattern repeats `CHIP_PAD_HTML` and `tests/js/mdtable.js` pins that they agree).

038-qlstall 2026-10-05 (the worker beats by the clock and parses in slices): the QLFF `order` leg's stage 4 stall was a UI-thread freeze, not a lost reply. After a first parse's head the render pass of a 1 MiB document sent no beat for 2.6 s quiet and 8 to 13 s under load (4000-event beats are uneven there), so `parseFallback` (10000 ms of silence) fired against a live worker and parsed the whole text on the UI thread for 13 to 31 s (timeline `h6 ... gap31496 ... f1,done`, `offthread=false`). `ui/js/MdBlocks.js` now also beats once `PROGRESS_MS` (500) has passed, reading the clock every `CLOCK_EVENTS` (64) events (`tests/js/mdprogress.js` steps a fake clock through `MdBlocks.useClock`, red before). The leg's second stall (stage 1 in step 8, 3 of 24 loaded runs) and the 5.1 s head of step 4 were one cause: the app has one worker thread per QML engine and a running parse cannot be cancelled, so an abandoned 1 MiB parse held the next document's head and every Quick Look prepare answer until it ended (5 to 20 s). `MdBlocks.blockJob` is the parse as a job that stops at a line boundary (`blockPass` saves frames, leaf, pending, serial, lastQuote, lead and the line in `job.saved`), `MdBlocks.blocks` runs it to the end, and `ui/MarkdownWorker.js` runs a headed request one `SLICE_MS` (200) slice a message: it answers `yielded`, `PreviewMarkdown.landed` answers `cont` only for the live seq, a newer request replaces the held job and `releaseWorker` (path change, close, fallback) sends `cancel`, so the queue behind an abandoned parse is at most one slice. `tests/js/mdslice.js` proves slices of 1, 3 and 17 lines answer the whole parse's blocks and the worker protocol, `tests/markdown-linearity.qml` the pane's side. `ui/PreviewMarkdown.qml` also keeps seven evidence properties (ackSeq, beatSeq, beatCount, headSeq, replySeq, fallbackFires, fallbackRunning) that the QLFF watchdog line prints with a timeline. Ceilings (`wc -l`): `ui/PreviewMarkdown.qml` 559 to 585, `ui/js/MdBlocks.js` 380 to 444, `tests/markdown-linearity.qml` 648 to 667, `tests/quicklook-firstframe.qml` 432 to 438, `tests/quicklook-firstframe.js` new row 214. On the 038-s8qm line the merge records `ui/PreviewMarkdown.qml` at 621 (595 plus qlstall's 26), `tests/markdown-linearity.qml` at 669 (650 plus 19) and `tests/markdown-preview-state.qml` at 469 (467 plus 2), each the merged file's `wc -l`. `tests/markdown-work.js` counts `filter` (`ui/js/MdHtmlHeading.js` is the first parser file to call it) as its whole input plus its result, and `tests/markdown-linearity.qml` goes from 669 to 679 for the probe that pins both halves (a filter over five lines keeping two costs seven, which a flat or one-sided charge misses; a comment, eight probe lines and its ok line).

038-capend 2026-10-05: the end-gap race and the incubation warning. `PreviewMarkdown.endBuilt` was read only on a height change, so a walk that built the last block with no height change left it false and the next growth (a late picture) was not followed, leaving the view short of the end; `onContentYChanged` now reads it too (`tests/markdown-endhold.qml` `uniformChecks`, 80 px short before). The block list builds its cache delegates asynchronously, so the Markdown Loader going inactive mid-build printed `Cannot create delegate` and `Object or context destroyed during incubation`; `Component.onDestruction: body.model = null` cancels them first, and `tests/markdown-teardown.qml` (a stage of `tests/markdown-render.sh`, 300 on/off rounds) fails on either line. `ui/PreviewMarkdown.qml` goes from 621 to 623 for that comment and line (its budget row); `tests/markdown-render.sh` and `tests/markdown-endhold.qml` stay under their caps.

### A list scans off the event loop, so a slow mount cannot freeze progress or cancel

Issue 144: moving a file from an MTP phone left the transfer card frozen and Cancel ignored.
`run.rs`'s `list` arm called `scan()` inline, and that one loop is also the only reader of
`Event::Op` and `Event::Request`, so a `read_dir` (or the gio child the gvfs route spawns) that
waited on the device held every `transferprogress` line and the `transfercancel` behind it. The
cancel itself no longer waits for the loop: `spawn_reader` sets the running operation's flag as it
reads the line (commit 60a07b9, `src/backend/events.rs`), which is what makes a cancel land whatever
the loop is doing. This change is the other half, the loop that keeps writing progress.

`src/backend/listwork.rs` owns it. `listwork::start` ends the walk on the loop, reads the watch fd
and the mount table there, and classifies the path once with `iomount::is_remote` (gvfs, a network
filesystem or any FUSE daemon). A local path runs `iomount::list_dir` inline on the loop, as v0.3.8
did: it returns `Started::Inline`, lands through `finish` before the next request is read, never
holds the gate, and a panic in it ends the process, also as before. A remote path runs `list_dir`
on `Gate`'s one kept thread, started by the first remote list, and the gate holds the `Job` it will
answer with; a thread per list cost the loop 0.03 to 0.06 ms of start-up on a 100-entry folder (a
fixed +0.15 ms on minipc), so no later list pays one. `list_dir` arms the watch descriptor, filters
and orders the listing, and bounds a remote call with `CALL_DEADLINE`. The remote scan posts
`Event::List(Done)` on the loop's one channel, the way a thumbnail or a size posts its own, and
always exactly once: a panic on the scan thread answers `iomount::internal_failure`'s sentence with
mode 0 and the thread goes on to the next list, and a thread that is gone is started afresh with the
scan run where it was asked, so a dead thread never leaves the gate held. Everything below about
the gate concerns that remote route. `listwork::finish` takes either answer on the loop: `set_incoming` plus `commit` then
`adopt_listed`, or `abandon_wd` and the error line with the mode the worker read; `run.rs` then
lays `fsinfo.list_arrived` and `poller.set` beside a landed listing, and
`prefetch::first_rows_sent` still ends every answer.

**Request order is what `Gate` keeps.** A scan takes an unbounded time on a wedged mount, and the
protocol is a sequential command stream: `protocol.sh` sends `list A`, `list B`, `paths [0]` with
listing 1 and requires the refusal that only exists if B replaced A before `paths` ran. So while a
scan is out, every request that arrives waits in `Gate` and replays in the order it arrived once the
scan lands. The loop is still free for events (progress, thumbnails, sizes, figures, the watch),
which is the half the issue asked for; only requests queue, and during a list the client sends none
while its own `listInFlight` holds. A `quit` or a closed stdin while a scan is out is kept too, so a listing a
script asked for is still answered before the process leaves, exactly as the inline scan did.

**A search session's empty query takes the same route.** When a session's query is emptied, the
folder answers itself with one readdir in its own sort, and that readdir is a `listwork::Job` whose
`session` names the gen the empty query claimed and whether the walk was running then. A local
folder lands inline as before; a remote one scans on the gate's thread, so the loop keeps answering
events while it is out. `listwork::finish_session` lands it: the watch descriptor the scan armed goes
back (the folder is not becoming the listing), and the rows and the `searched` line for that gen go
out only while the session still stands at that gen on that folder. Unlike a list, the client keeps
sending while this scan is out (the user is still typing), and those requests wait in the gate and
replay in order like any other. A query burst read while the gate holds still collapses to its
newest query when it is read, because the session a deferred `searchopen` opens is not open yet. A
walk that lands during the scan (`Event::SearchWalked`) is announced after the folder answer, so its
`searched` line never comes before that gen's rows.

**A second list does not overtake the first, and the shipped clients never ask for one.** The gate
answers in request order, which is the only order the protocol test accepts: `protocol.sh` sends
`list A`, `list B`, `paths [0]` with listing 1 and requires both listings adopted and the old
numbering refused. A newer list cannot be answered first without dropping or refusing the older one,
and a client waiting on the older `listed` line would then never clear its own `listInFlight`.
Nothing shipped needs it: `ui/js/Nav.js`'s `openWithoutHistory` refuses a navigation while
`pane.listInFlight`, the TUI's `Model::open` refuses while `self.pending` is set, and dual panes run
one backend each (`ui/WindowBody.qml`), so no one backend ever has two lists out. A slow MTP folder
therefore delays only the pane that asked for it; another pane's local folder is another backend and
answers at once. `peek`, which the columns and network-repair paths use, is the one same-backend
listing still on the loop, and it is named above rather than folded in.

**A watch burst seen during a scan is decided once the scan says what it listed.** The new
descriptor is armed by the worker and becomes incoming only at `finish`, so which of the two is
current is only known once the scan lands. A burst read while the scan is out is therefore kept in the `Gate` and
answered after it, if and only if its descriptor is the one that became current: a change in the
directory just left answers nothing, as it did when the loop was inside the scan. An `Unmounted` event waits the
same way (`Gate::note_unmount`) and replays after `land`, in arrival order, through `unmount::leave_mount`: judged
at once it met the old descriptor and was dropped for the new folder or applied to the folder being left.
A network poll's `PollChanged` and a probe's `FsInfo` wait the same way (`Gate::hold`) and run through `dispatch` after `land`, or once a let-go list has replayed the requests that waited, so each is judged against the folder that is current by then.

**What is still synchronous, honestly.** `peek` scans a preview directory inline, `listpaths`
`lstat`s each named path, and `window`'s `stat_range` and a size/mtime `sort`'s `stat_all` run on the
loop. None of these is on the path the report measured (open
a phone folder, or a re-read of it), and a gvfs listing answers `stat_range`/`stat_all` from its gio
store with no lstat, so a phone folder's default name order pays none of them. They are named here
rather than quietly fixed.

The regression is `tests/mtp-scan.sh`: a `FLEA_GIO_BIN` fake that blocks until the script releases
it makes the scan provably out and blocked, then a copy of a folder of four 1 GiB sparse files is
started and cancelled while it is out (four files, not one, because a lane may cap file size at
2 GiB). Progress and the cancelled `transferdone` must be written before the scan's `listed`
line, which an inline scan cannot do. The script does not lean on the size: a folder reports
`total` 0, so it requires a progress line with `bytes` below `scanned` before the cancel, a
`transferdone` for that id after it
with `cancelled` true, and nothing left at the destination, and it gives the backend its own
`FLEA_UNDO_DIR` so the run never writes the operator's undo journal. A `paths` request sent while
the scan is out must be answered after its `listed` line and name the phone listing's row, the
replay seen through the real binary. `listwork_tests.rs` drives
`start` and the real `list_dir` with the test-only `scanhold` seam holding a path's directory read
or mode stat: neither holds the channel, a scan that panics still answers and frees the gate, and
every scan runs on the one kept thread. `src/backend/run_gate_tests.rs` drives `run.rs`'s own
`dispatch`, the function the loop hands every event to, one event at a time against a held remote
scan: two requests, a watch burst in the folder being left and a closed stdin that arrive while the
scan is out answer nothing; once it lands the listing, its rows and both requests are written in
arrival order (the first, stamped with the old numbering, refused as stale), no `changed` line is
said for the folder left, and the loop leaves. A burst in the folder being scanned answers one
`changed` line after its rows. Answering a request while the scan is out, dropping the `is_current`
filter or every kept burst, replaying one request only, leaving at once on a closed stdin, or
answering a burst before the scan lands each fails one of its assertions.
`tools/flea-file-budget` records `src/backend/run.rs` 719 to 735 for the gate and the two extra
`handle_line` call sites; `src/backend/listwork.rs` is inside the soft budget.

The retained QML launch history records these former ceilings; the GPUI launch policy now lives in `src/gui/gpui_renderer.rs`.

icdexclude moves two ceilings, each re-derived with `wc -l`: `src/gui.rs` 501 to 503 for the one exclusion call ahead of the renderer arms and its comment, and `src/vulkan.rs` 728 to 747 for `icd_library` reading `ICD.library_path` through `jsondoc` and `src/jsonmembers.rs` with its 4 KiB bound, its NUL refusal and the repeat and metadata pins, with three helpers and three PCI vendor constants made `pub(crate)`, and `is_hasvk_intel` the fourth for the hasvk exclusion. The exclusion itself is the new `src/icdexclude.rs` at 180 and its manifest inventory (the directory listing with its 256 cap, the `library_path` reading and the known-library table) the new `src/icdmanifests.rs` at 84, declared inside it the way `src/vkprobecache.rs` declares `src/vkprobekey.rs`, both inside the soft budget, with its decision tests in the new `src/icdexclude_tests.rs` at 232 (the present vendors, the hasvk cases, `decide_exclusion`, the filter order and cap, and the fake-sysfs helpers the other file shares) and its manifest tests in the new `src/icdexclude_manifests_tests.rs` at 105 (the search roots, the parse and regular-file rules and the namesake rule), and the member lookup is the new `src/jsonmembers.rs` at 118, all inside both budgets.

focus68 moves two test ceilings, each re-derived with `wc -l`: `tests/ring-bounds.qml` 669 to 676 for `checkRevealed` deriving each focused card button's look (foreground frame, the ring for a primary whose rest frame is the foreground, the accent for a set member with hue) and checking the button against it, and `tests/button-members.qml` 437 to 438 for a resting empty box's muted frame.
r35-asks moves two ceilings, re-derived with `wc -l`: `ui/ColumnsArea.qml` 491 to 493 for the pre-capture askCursorThumb call, and `tests/js/previewswap.js` 408 to 409 for its check.

The waiting up arrow moves two ceilings, each re-derived with `wc -l`: `ui/js/Nav.js` 309 to 313 for `climbsFromListing`, the one test the Up key and `ui/Pane.qml` `canGoUp` share, so the chrome's up arrow is live wherever the key climbs from a waited folder; `tests/js/nav.js` 436 to 445 for running `canGoUp` as written on a first listing past the wait and once it ends.

b4fix records three ceilings, each re-derived with `wc -l`: `ui/PaneMenuActions.qml` 461 to 475 for the shelf add queue (the `pendingAdds` wait, `startAdd` and the failure line drain), `ui/Theme.qml` 433 to 435 for the `clipMarkSize` token, and `ui/js/MdBlocks.js` 444 to 437 for the removed lines; the probe is the new `tests/shelf-add-queue.qml` at 131 with `tests/shelf-add-queue.sh` at 120 carrying the offscreen gate, both inside their budgets and registered in `tests/run-all.sh` headless.
The shelf mark is drawn as a filled outline so its bounds are its ink; `tests/shelf-mark.sh` checks its measured, transformed and painted bounds at two sizes.

The cursor step cost cut records one ceiling, re-derived with `wc -l`: `ui/Pane.qml` 1043 to 1045 for `rowsIdentity`, the held rows serialized only when they change, so `menuSelectionIdentity` no longer serializes the whole held window on every cursor step (`tests/navcost.sh` counts the serializations). The int 12d1af88 merge re-derives it to 1052 with `wc -l`: those two lines on top of int's background-opacity 1050.

The empty navigation frame and the landing serialization record three ceilings, re-derived with `wc -l`: `ui/Pane.qml` 1052 to 1063 for `menuSelectionIdentity()`, now a function whose held rows serialize on the first read after they are replaced (keyed by the rows array) and `menuSelectionInputs`, which notifies the retry line without serializing; `ui/PaneWire.qml` 586 to 591 for the retry line's own identity, compared only while the line shows; `ui/ContextMenu.qml` 739 to 740 for `identitySource`, read at open and at a choice only. The listing gate moved from the `enabled` bindings of `ui/SelectionBand.qml` and `ui/PaneStates.qml` into their press and wheel handlers, because a MouseArea's `enabled` flip calls `setAcceptHoverEvents`, which dirties the item's content and draws an unchanged frame on every Enter and Back (`tests/navframe.sh` counts the frames and dirty items between the key and the landing in three views).

The rename focus return (issue #265) records one ceiling, re-derived with `wc -l`: `ui/ColumnPane.qml` 399, under the 400 cap, to 401 for the rename `Loader`'s `onActiveChanged`, which hands the pane's focus back to the columns area when the editor ends, and its comment.

Qt 6.12 palette 2026-10-09: qt6-declarative 6.12 adds a `Color` singleton to `QtQuick`, which shadows `qs.Commons`' `Color` where both are imported unqualified, so the palette read undefined and text drew black on black. The nine files that read it import `qs.Commons as Commons` beside the plain import and say `Commons.Color`, the fix Omarchy made in its shell (omacom/omarchy #14553, #14626); comments still say `Color`. `ui/Theme.qml` goes from 433 to 434, `shelf/Panel.qml` 430 to 431 and `shelf/ShelfCard.qml` 443 to 444 for that import line; `tests/staticgates-unqualified.tsv` moves the `ui/Theme.qml` rows below it down one line in both profiles and names `Commons` where it named `Color`.

2026-10-09 moves one ceiling, re-derived with `wc -l`: `ui/ColumnPane.qml` 403 to 423 for the hidden column that keeps its rows until a new count reaches it and then holds none (`keptTotal`, which its model's total reads), the offset a dropped column follows its cursor in (`heldY`), and the return that rebuilds a dropped column at that offset in the handler that shows it; `tests/columnshidden.sh` drives it over the real pane and backend.

The union (gm/039x-s3union: the 271, b6, b1, b3, b5, b2, tuicombo and stage3 fix batches merged on int d79462fc) records the merged ceilings, each re-derived with `wc -l` at the merge: `src/backend/iomount.rs` 622, `src/backend/run.rs` 1060, `src/backend/undoshare.rs` 415, `tests/js/focus.js` 759, `tests/js/nav.js` 456, `ui/ChromeBar.qml` 324, `ui/GridArea.qml` 429, `ui/Ipc.qml` 897, `ui/NetworkMounts.qml` 901, `ui/Pane.qml` 1073, `ui/PaneWire.qml` 561, `ui/js/Devices.js` 363, `ui/js/Nav.js` 315, `ui/js/Tabs.js` 717. `src/backend/run.rs` carries both the list gate that keeps a scan off the event loop and the search session's burst collapse and index patches, in one `dispatch`.

Preview focus line 2026-10-09 (the PDF viewer's marks and the Markdown Quick Look's close draw the Network form's focus
line) moves one ceiling, re-derived with `wc -l`: `ui/PreviewColumn.qml` 583 to 588 for `focusLine: true` on the
Columns PDF strip's five marks. The int 12d1af88 merge re-derives it to 590 with `wc -l`: those five lines on top
of int's background-opacity 585.
The union of this focus line with polish batch 4 re-derives it to 589 with `wc -l`: the five lines on top of
batch 4's 584.
The same union re-derives three more ceilings from the merged files with `wc -l`: `tests/js/previewswap.js` 463 (int's
460 plus batch 4's follow pin line and the Quick Look hold's two), `ui/List.qml` 444 (batch 4's de-slopped 425 plus the
rename error contain's 19) and `ui/PermissionsDialog.qml` 667 (batch 4's figure; no other member touches it).

Merging int's stable model and pool into the union re-derives one ceiling with `wc -l`: `ui/List.qml` 454 (int's 446
plus the union's 8 over their common base, with the model line taking int's `StableModel` gated on `renameHeld`);
`ui/ContextMenu.qml` keeps the union's 740, and `ui/ColumnPane.qml` loses its exception, since the merged file
is 397 lines (int's 407 less the union's 10), under the cap.

Button own-frame focus 2026-10-10 (no button holds a ring item, no mark takes an accent focus line, and a focused button whose hairline would match its rest draws its own frame 2 px wide, inward) moves three ceilings, each re-derived with `wc -l`: `ui/PreviewColumn.qml` 589 to 584 for the five `focusLine: true` lines removed from the Columns PDF strip, `tests/button-members.qml` 438 to 444 for the focused-equals-rest geometry check and the thick primary's width, geometry and wash checks, and `tests/ring-bounds.qml` 676 to 678 for each card button's frame width and own-box checks and its frame's clip chain.

e039f-dropmods records #259's drop-time keys, each count re-derived with `wc -l`: `ui/js/Drag.js` 342 to 349 for dropHeld and the seen threading through dropVerb, dropInto and verbAt, track living in DropKeys instead; `ui/Pane.qml` 1043 to 1045 for the press and release wiring into `DropKeys.track`, no logic and no new import; `ui/RowDrag.qml` 123 to 155 and `ui/DropInto.qml` 101 to 122, #259's share the enter, leave, spend and clear calls beside the Drops branch's own growth; `ui/FileDrag.qml` 191 to 230, #259's share the verbAt threading and the lift stamp beside the same; the new `ui/DropKeys.qml` at 62 for the marker, lift and key store with stamped, enter, leave, settle, clear and track, each drag numbered at its lift so a leave ends none of its own and a second drag of the same rows starts unseen, while a drag with no lift ends at a leave no entrance follows in the same turn; `ui/qmldir` 158 to 160, #259's share the DropKeys singleton line; the new `tests/js/dropkeys.js` at 145 for the moved decision checks with track proven against the real store and RowDrag's path and by-index drops over it, registered in `tests/js/harness.qml` at 224; `tests/js/collidefixture.js` 46 to 55 and `tests/dragwire-floor.qml` 87 to 156 for the shared realDropKeys loader both fixtures build the store from, and the floor's keyed drops stepped a turn apart so a leave's queued settle runs between them.

The int 927f4a20 merge into gm/039f-dropmods re-derives each conflicting ceiling with `wc -l` on the merged tree.

039f-cursor 2026-10-06 (Back and Forward restore the cursor by name, then index, then top): entries are
{path, name, index} plain data in the history stacks, captured by ui/js/Nav.js travel, armed per listing on
ui/PaneSwap.qml restore, and resolved by the new ui/js/BackReturn.js (50 lines, no budget row) in the rows
reply's own turn, so the listing and its cursor land in one frame. Ceilings, each re-derived with `wc -l`: ui/js/Nav.js 306 to 300, its budget row 308 to 300 (the travel calls replace the
inline push and pop), ui/PaneSwap.qml 187 to 196 for the restore property, its hold() arm and the two one-line
landings, both inside their recorded ceilings with no raise; tests/js/nav.js 362 to 367 for the entry-path
reads. New suites tests/js/cursorreturn.js (143, inside its budget) and tests/cursorback.qml (284, over soft
with a warning only) with tests/cursorback.sh running the probe in list/grid/columns plus a second launch
straight into dual, because the dual second pane builds at startup and not on the live view switch.

039f-cursor R1 2026-10-10 (a name outside the first window): the first `rows` reply of a listing holds only the
viewport's rows, so a Back into a large folder whose row moved out of that window (an insertion above it, a
re-sort) used to land on the saved index, which is then another file. `BackReturn.deferForLocate` now decides, in
`PaneSwap.takeRows`, that the owed name is absent from that window while the kept `listed` total is larger: it sends
the existing single-path `locate` (docs/protocol.md, no new command), parks the rows and keeps the hold, so the old
listing stays drawn. `BackReturn.takeLocated`, first in `PaneWire.onLocated`, matches `directory` and `path` and asks
`window` at the located index, or at the clamped saved index on `index:-1`; that window's `rows` reply then lands the
count, the rows and the cursor in one turn, the same landing as before. Every state between is the old listing, or
the loading state once the `Swap.CAP_MS` hold of 66 ms runs out: one landing, never a lit stale row
(`tests/cursor-swap.qml` records
the drawn state after each turn, R0 included: the saved index inside the first window with the name moved out). A
Back that cannot hold (out of a walk, or a loading pane) keeps its `listed` line too (`Swap.begin` `waits`), so it
shows loading until that one landing instead of a counted, empty list. The parked state ends on every path: any
failure that reaches `PaneWire.onFailed`'s listing decision lands the parked first window at the top through
`PaneSwap.landParked` (a refused located window on a stuck mount, a failed locate, a dead backend, whose own state
then draws), except a `stale` refusal, which answers an older numbering's request and never the locate. A locate
that never answers lands them the same way after `Startup.LISTING_WAIT_MS` (`PaneSwap.parkedWaitMs`), the bound a
listing that never answers already gets. A refused sort or rename returns before that decision; the wait covers it.
The restore is copied at `hold()`, so nothing written while it waits reaches a history entry.
`tests/cursor-swap.qml` compiles the real `onLocated` and `onFailed` from ui/PaneWire.qml, as
tests/js/xwanchor-races.js does, so the hand-off order and every failure path run as shipped. Ceilings, each
re-derived with `wc -l`: ui/js/BackReturn.js 50 to 83; ui/PaneSwap.qml 196 to 233 (the restore, the parked landing and its wait, built on the first park because a declared Timer measured one object over `startupObjectLimit` in the union with int, whose value `tests/startup-objects.qml` pins); ui/PaneWire.qml 586 to 590, its
budget row 586 to 590 (the BackReturn import, the `onLocated` hand-off and the parked landing on a failure);
ui/js/Swap.js 108 to 110; tests/js/cursorreturn.js 143 to 249 (over soft, under hard); tests/cursorback.qml 284 to
292 (over soft, under hard); tests/js/swap.js 281 to 285; tests/js/xwanchor-races.js 184 to 185; new
tests/cursor-swap.qml (346, over soft, under hard) and tests/cursor-swap.sh (70), a headless run-all suite.

The flasher line's merge of int re-derives three ceilings with `wc -l`: `ui/Pane.qml` 1062 (the flash pane's 12 lines on top of int's 1050), `ui/WindowBody.qml` 681 (its 21 on top of int's 660) and `ui/Ipc.qml` 897 (its 11 on top of int's 886).

The flasher line's merge of the features union2 `f8ecfd98` re-derives each ceiling both sides moved with `wc -l` on the merged tree, each the two sides' lines together on int's: `src/backend/proto.rs` 412, `src/backend/run.rs` 746, `tests/js/menu.js` 538, `ui/Backend.qml` 540, `ui/ContextMenu.qml` 749, `ui/Ipc.qml` 938, `ui/Pane.qml` 1123, `ui/PaneMenuActions.qml` 481, `ui/StatusBar.qml` 463, `ui/WindowBody.qml` 737 and `ui/js/Menu.js` 476. Where the union's unlock meets the flasher, a locked removable row offers Unlock and then Flash image in its own group, since the locked partition has no holder, and an open crypt's row offers no Flash image: its `/dev/mapper/` or `/dev/dm-` name is no drive the flasher lists, and the partition under it is held, which the flasher refuses.

The merge of 536ef159 into gm/039x-s3union-s8 re-derives each ceiling both sides moved with `wc -l` on the merged tree, with both sides' lines together: `src/backend/copyfile.rs` 714, `src/backend/opsdispatch.rs` 2247, `src/backend/opsreq.rs` 612, `src/backend/opsreq/tests.rs` 487, `src/backend/run.rs` 1060, `src/backend/undo.rs` 659, `src/backend/undoshare.rs` 415, `src/backend/undoshare_tests.rs` 550, `src/backend/collide_replace_tests.rs` 492, `src/backend/proto.rs` 422, `src/gui.rs` 505, `ui/Backend.qml` 500, `ui/ChromeBar.qml` 327, `ui/ColumnsArea.qml` 539, `ui/ColumnPane.qml` 432, `ui/ContextMenu.qml` 744, `ui/GridArea.qml` 438, `ui/Ipc.qml` 929, `ui/Pane.qml` 1100, `ui/PaneMenuActions.qml` 468, `ui/PaneWire.qml` 576, `ui/DeviceMounts.qml` 527, `ui/Sidebar.qml` 679, `ui/ViewState.qml` 538, `ui/WindowBody.qml` 708, `ui/js/Devices.js` 375, `ui/js/Filter.js` 259, `ui/js/Focus.js` 447, `ui/js/Nav.js` 330, `ui/js/Menu.js` 461, `ui/js/Drag.js` 350, `ui/js/Ops.js` 427, `ui/js/Settings.js` 459, `ui/js/Tabs.js` 717 and `tests/js/rail.js` 424. `tests/staticgates-unqualified.tsv` is regenerated: the `quickshell-present` profile from qmllint on the merged tree, and `quickshell-missing` remapped from both parents by line content, the method that reproduces the present profile exactly.

Merging the features stage (gm/039-stage8) into this union re-derives each conflicting ceiling with `wc -l` on the
merged tree: `ui/Pane.qml` 1090, `ui/PaneWire.qml` 606, `ui/ContextMenu.qml` 743, `ui/ViewState.qml` 536 and
`ui/js/Drag.js` 336 carry both sides' changes; `ui/PermissionsDialog.qml` 649 and `src/backend/permissions.rs` 699
tighten to the union's files, which the stage left alone; every other conflicting row takes the side that changed it.

The flasher line's merge of the features line's final head `ae06c08d` re-derives three ceilings with `wc -l` on the merged tree, each the two sides' lines together on their merge base: `ui/Backend.qml` 542 (506, the flasher's 34 and the features line's 2), `ui/Ipc.qml` 939 (927, 11 and 1) and `ui/Pane.qml` 1128 (1111, 12 and 5).

The flasher line's zip, move-choice and probe-move commits re-derive their ceilings with `wc -l`: `22845ca1a` takes `ui/Pane.qml` 1128 to 1141 (the `FlashPane` import, the `flashZipAsked` prop, the five probe wires and the backend `Connections`) and `ui/FlashView.qml` 399 to 398 (the reply routing through `Flash.imageFrom`); `1c20293f` records the new `ui/FlashView.qml` 402 (moving onto a drive chooses it); `b44821f51` takes `ui/js/Ops.js` 441 to 470 and `tests/js/ops.js` 514 to 551 (the probe moved out of the first frame into the already-loaded `Ops.js` with its tests) and `ui/Pane.qml` 1141 to 1140 (the import out). The review fix folds the move choice into `FlashPane.chooseKept`, re-derived with `wc -l`: `ui/FlashView.qml` 402 to 399, back under the 400 hard cap instead of ceilinged over it, with `ui/js/Ops.js` 470 to 479 (the rewrite-aware ask key) and `tests/js/ops.js` 551 to 561.

look3 2026-10-11 moves one ceiling, re-derived with `wc -l`: `tests/preview-decode.qml` 516 to 560 for the reopen check that the interim is Ready in its own turn and lands without the cap, with the heldbad missing-cache carve-out and the fresh-item close before every held open. A later review joins one comment, so the ceiling tightens to 559.

The release-line settle after the union joins moves one ceiling, re-derived with `wc -l`: `tests/search-live.qml` 414 to
419 for the `BackReturn` import, the shipped `stampTags` the replayed `takeRows` now calls, and the stub's `tagState`.

tagc2 2026-10-11 moves one ceiling, re-derived with `wc -l`: `src/backend/copyfile_tests.rs` 607 to 633 for the file and directory copy tag pins and the same-device and cross-device move tag pins.
The picker path snapshot 2026-10-11 moves one ceiling, re-derived with `wc -l`: `ui/PickerWindow.qml` 787 to 793 for the snapshot fields `pathBarOpen`, `pathFieldText` and `jump` the native path cases read, beside the `pathBarOpen` seam the visual lane keeps.
The tab count notice withdrawal (the serial on a notice, `StatusBar.withdraw`, and the pins that prove a count verdict stands before it goes) moves four ceilings on the stage, re-derived with `wc -l` on the merged tree: `tests/js/tabs.js` 710 to 797, `ui/Pane.qml` 1214 to 1215 (the `tabCountNotice` property), `ui/StatusBar.qml` 463 to 471 (the serial and `withdraw`) and `ui/js/Tabs.js` 720 to 738 (the count mark, `sayCount`, `sayPlain` and `dropCount`). It also raises the first-frame `bytes` line in `tests/startup-units.txt` from 1,614,048 to 1,615,189 (+1,141, no unit added): +36 in `ui/Pane.qml`, +409 in `ui/StatusBar.qml` and +696 in `ui/js/Tabs.js`, the sum the startup-objects probe takes over the units.
The picker chip inset 2026-10-11 moves one ceiling, re-derived with `wc -l`: `ui/PickerChrome.qml` 429 to 441, because each filter chip draws its frame and caption in a box inset from the strip top and its rule, as the chrome marks do, on whole pixels from the strip top the host hands it.

The release-line join of the flasher line `c92728d7` re-derives each ceiling both sides moved with `wc -l` on the merged tree: `src/backend/run.rs` 1089 (the flash context, its channel and forwarder join the gated loop, `dispatch` takes the flash context beside the gate, and `rows_needed` classes `Request::Flash` with the requests that name paths), `src/backend/run_session_tests.rs` 525, `src/backend/proto.rs` 442, `tests/js/menu.js` 553, `tests/js/mounts.js` 327, `ui/Backend.qml` 550, `ui/ContextMenu.qml` 751, `ui/Ipc.qml` 949, `ui/Pane.qml` 1175 (the zip probe and the rows identity share one `onRowsChanged`), `ui/PaneMenuActions.qml` 490, `ui/WindowBody.qml` 739, `ui/js/Menu.js` 475 and `ui/js/Ops.js` 480. `tests/staticgates-unqualified.tsv` is regenerated: the `quickshell-present` profile from qmllint in the CI image on the merged tree, and `quickshell-missing` remapped from both parents by line content, keeping only rows whose name stands on their line and none of the flasher line's `qs.Commons` rows, which the merged gate's always-loaded stub resolves. `tests/startup-units.txt` keeps its 170 first-frame units and its `bytes` line moves from 1576212 to 1587595, the sum `tests/startup-objects.sh` takes over them, for the flasher line's first-frame lines in fifteen of them, the most in `ui/Backend.qml`, `ui/Pane.qml`, `ui/js/Ops.js` and `ui/WindowBody.qml`.

The release-line join of the unlock line `d70145a6` re-derives each ceiling both sides moved with `wc -l` on the merged tree: `ui/Ipc.qml` 952 (the flasher's host delegates beside the unlock card's readers) and `ui/DeviceMounts.qml` 528 (the drive row carries `fsver` beside `fstype` and no `volumeMenu`, wrapped onto a second line). `tests/startup-units.txt` keeps its 170 units, and its `bytes` line moves from 1587595 to 1588099, the sum the probe's tool takes over them with the unlock line's lines in `ui/DeviceMounts.qml`, `ui/DialogButton.qml`, `ui/Ipc.qml` and `ui/js/Devices.js`. `tests/staticgates-unqualified.tsv` is regenerated the same way as for the flasher line's join.

The release-line join of the button frame line `075bb903` moves no ceiling past the side that set it, and `tests/startup-units.txt` keeps its 170 units while its `bytes` line moves from 1588099 to 1588235, the sum the probe's tool takes over them with the strip rule's lines in `ui/ChromeBar.qml`, `ui/ChromeButton.qml` and `ui/ChromeModes.qml`.

The release-line join of the picker answer strip, the Flash file-menu identity and the bench Version column moves no ceiling past the side that set it, and `tests/startup-units.txt` keeps its 170 units while its `bytes` line moves from 1588235 to 1588400, the sum the probe's tool takes over them with the answer buttons' press area in `ui/DialogButton.qml` and the selection identity call in `ui/PaneMenuActions.qml`.

The release-line join of the approved fix, bench, search, picker and Flash strip lines re-derives each ceiling both sides moved with `wc -l` on the merged tree: `src/backend/copyfile_tests.rs` 694, `ui/Pane.qml` 1180, `ui/PickerChrome.qml` 426, `ui/PickerWindow.qml` 797, `tests/button-members.qml` 488 and `tests/js/picker.js` 343, and records `tests/chromering.qml` at 413 for the Flash strip's Back checks beside the seven picker strip stops. `tests/startup-units.txt` keeps its 170 units, `ui/PathEntry.qml` leaving for the first edit and `ui/PrebuildDriver.qml` joining, while its `bytes` line moves from 1588400 to 1596965, the sum the probe's tool takes over them.

The newest-navigation line moves the `bytes` line of `tests/startup-units.txt` from 1596965 to 1598938, the sum the probe's tool takes over the same 170 units after `ui/Pane.qml` and `ui/PaneWire.qml` took the `supersede` hop, the wait restart and the failed-scan guard; no unit joined or left. Ceilings, each re-derived with `wc -l`: `src/backend/run_gate_tests.rs` 399 and `ui/js/Nav.js` 324 stay under theirs, and the let-go scan's late-descriptor test lives in `src/backend/run_gate_alias_tests.rs` (85 lines) so `run_gate_tests.rs` stays under the 400 cap. The line moves again from 1598938 to 1599464 (+526, no unit added) for the later lines of this unit in `ui/js/Nav.js`, `ui/js/BackReturn.js`, `ui/js/Tabs.js`, `ui/js/RecentMode.js` and `ui/js/Swap.js` (the drawn-place return, the owed-return drop and the tab and Recent routes through `supersede`), the sum `tests/startup-objects.sh` takes over the same 170 units.

The Network Tab ring moves three ceilings, each re-derived with `wc -l`: `ui/NetworkDialog.qml` 447 to 457 for the ids, `tabHandle` and `onTabbed` lines that put Cancel, the primary and Install Dropbox in the form's walk after the TLS box, with Escape moved from the form to the card's content so a focused button closes the dialog too, and `ui/NetworkForm.qml` 406 to 413 for the `trailing` prop, the joined ring, the pass-over step and the button labels in `focusName`, and `tests/button-members.qml` 445 to 485 for the trailing-walk checks that Tab from TLS reaches each button and wraps to the first chip, Backtab returns, an unavailable button is passed over and `focusName` names the focused button.

The OutCubic and surface-safe frame line keeps `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1597129, the sum the probe's tool takes over them with the press easing in `ui/ChromeButton.qml` and `ui/DialogButton.qml`, the `Motion.durMs` fades in `ui/EmptyState.qml` and `ui/FleaMark.qml`, and the accent frame role and reduced-motion switch in `ui/TransferCard.qml`.

Quick Look's warm kind units keep `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1597222, the sum the probe's tool takes over them with the warm `Loader` in `ui/WindowBody.qml` that replaces the `createObject` of `QuickLookWarm.qml`.

The Quick Look image zoom keeps `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1597661, the sum the probe's tool takes over them with the image branch of the zoom keys in `ui/js/Focus.js` (its own gate for `e`, which stays a PDF key), the `0` row in `ui/js/Keymap.js` and the shared ladder step in `ui/js/PreviewKeys.js`.

drp4 2026-10-11 raises the first-frame `bytes` line in `tests/startup-units.txt` from 1,596,985 to 1,598,921 (+1,936, no unit added): +1,260 are the recorded hover-open destination in `ui/FileDrag.qml` (`dropDest`, `dropDestDev`) and `ui/RowDrag.qml` (`lit()` and `landing()`, the recorded address `refuseDrop()` and `dropped()` read), both loaded by the first window; the other 676 were already over the old pin on the DRP3 head `4d1132c3`.

The transfer close fix adds the closing-sample branch to the first-frame `ui/js/Transfer.js`, so `tests/startup-units.txt` keeps its units while its `bytes` line moves from 1596965 to 1597218, the sum the probe's tool takes over them; recompute again at merge, since IP208B edits the same line.

The Quick Look volume track keeps `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1598076, the sum the probe's tool takes over them with the stored level in `ui/ViewState.qml` and the held level and its gain in `ui/MediaSound.qml`.

The Dual Tab stops, the bulk rename row inset and the "Rename N items" label keep `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1598251, the sum the probe's tool takes over them with the tab walk in `ui/js/Focus.js`, the label template in `ui/js/Menu.js`, the rail hop and the left pane's list hand-back in `ui/WindowBody.qml`, its signal in `ui/Pane.qml`, and the Dual arm leaving `sheetFor` in `ui/js/Keymap.js`.

The rail heading adds one first-frame unit to `tests/startup-units.txt`, `ui/RailHeading.qml`, the rail heading that `ui/Sidebar.qml` draws four times and `ui/TagRailSection.qml` once, so the units go from 170 to 171 and the `bytes` line from 1,596,985 to 1,596,206. The sum before it was 1,597,661 bytes; the new unit adds 633 and the moved code takes out 2,088: the four hand-built heading `Text` blocks in `ui/Sidebar.qml`, the hand-built scrims, the `fill` wrappers and the long comments. `ui/TagSwatch.qml` is not a first-frame unit, because the tag swatches build with the first tagged row.

The stage join of the fix, tag, drop, rail, guidance and contributor lines re-derives the one ceiling it moves past the hard cap with `wc -l` on the merged tree: `src/backend/thumbspec.rs` 410, where two thumbnailer fixes meet, the user thumbnailer outside `/usr` with the failure marker per table (399 lines on its own side) and the link chain bound hop by hop with the PATH search that skips empty and relative entries (372 on its own side). `tests/startup-units.txt` gains `js/DropDev.js` (the drop guard's device map, which `Backend.qml` and `js/Messages.js` import) and `js/FolderViews.js` (the per-folder view, which `Pane.qml` imports), 173 units, while its `bytes` line moves from 1596985 to 1614048, the sum the probe's tool takes over them.

The stagereds fixes move two ceilings, each re-derived with `wc -l`: `tests/rename-grid-bottom.js` 344 to 345 for the `PrebuildDriver` fixture file and the Theme stub's `clipMarkSize` (the fixture list stops at the files `ui/GridArea.qml` imports, and the `SelectionBand` stub gains the `rowAt` it is handed), and `tests/rename-grid-bottom.qml` 438 to 439 for the stub pane's `rows`, which Grid tiles now draw from through the held snapshot (`GridGeometry.heldRow`) instead of `pane.rowFor`. `tests/poolpark.qml` counts its grid leg's past reads on that same seam, since a Grid tile no longer calls `pane.rowFor`: a read of the rows array from a tile's `row` binding while the view holds no count, or of a row at or past the landed count. The control pulls the bound to 1 and counts 239; removing both `live` guards in `ui/GridArea.qml` fails the leg. The rename-grid-bottom suite needs node, which the flea-ci image lacks, and the host's qml6 is Qt 6.8.2, where a `DelegateModel` handed to a view reports no delegate, so its cases are read with the image's qml6 as `QML_BIN` and node on the host. The advloop round 1 fixes move a third ceiling, `tests/poolpark.qml` 388 to 409 (recorded in `tools/flea-file-budget`, re-derived with `wc -l`), for `pastHeld`, the one predicate behind the grid leg's count (any read while the view holds no count, an index at or past the landed count after), the sample frame of the stack it matches, and `emptyWindowControl`, which reads a key through `pastHeld` while the view holds no count and fails the leg when it is not counted; reverting `pastHeld` to the length-only form turns that control red in all three views. `GridGeometry.heldRow` tests the length before any index (`ui/js/GridGeometry.js:82`), so a real tile reads only the length of an empty array, and moving `counts.empty = true` after `p.total = 0` leaves the leg green; the order is kept so that a read the `p.total = 0` assignment itself sets off already counts as a read while the view holds no count. The rename-grid-bottom stub's `rows` now counts to `shownTotal`, not a second `1202`.

Two signals on the `tests/columnsfilter.qml` backend stub, `changed(string path)` and `shebang(string path, bool hasShebang, int id)`, which `ui/Backend.qml` declares and the Connections in `ui/ColumnsArea.qml` and `ui/IpcMenuState.qml` target, keep the suite free of the two Connections warnings its noise check rejects, and move one ceiling, re-derived with `wc -l`: `tests/columnsfilter.qml` 400 to 402.

A drop on a Columns neighbour row reads its device from the backend (DRP5, 2026-10-12). A peek row is `{n,d,i}` and a carried one keeps that shape, so `row.v` was undefined and `Drag.js` `verbFor` copied a plain drop onto `deep` where the case expects the Move line. `ui/RowDrag.qml` now takes `destDev` as `row.v` when present, else `DropDev.devFor` of the backend's `pathDevs` for the row's full path, at the entry, the pointer motion, the recorded destination and the landing; an answer landing while the folder is hovered says its line again once. `ui/Backend.qml` `askDropDevs` sends one `devs` request for the visible folder rows with no `v` of the drawn neighbour columns (`ui/ColumnsArea.qml` `askDropDevs`, `ui/ColumnPane.qml` `dropAsks`, `ui/js/DropDev.js` `folderAsks`) when a drag enters a row, once per drag and set, never from a chooser's backend; a known answer stands until its new one lands, and a path nobody answered still copies. `tests/columns-peekdrag.qml` drives a real peek folder row (red at `67d55d0c`) and a real folder row that carries `v`, which drops on its own device while the backend answers its path with the source's device (red with `destDev` ignoring `row.v`), and `tests/js/dropsflow.js` the asks. Ceilings from `wc -l`: `ui/Backend.qml` 567 to 576, `ui/ColumnsArea.qml` 539 to 547, `ui/ColumnPane.qml` 438 to 446. `tests/startup-units.txt` keeps its units and its `bytes` line moves from 1614048 to 1615935, the sum `tests/startup-objects.sh` takes over them.

The Dual divider moves four ceilings, each re-derived with `wc -l`: `src/uischema.rs` 502 to 504 for the `dual.split` default and its rule in `DUAL`, `src/uistate.rs` 674 to 701 for the one test that holds the leaf to a whole share from 0 to 10000 and keeps the pane's `{focus, paths}` save from touching it, `ui/Theme.qml` 443 to 446 for the `dualMinWidth` token, and `ui/WindowBody.qml` 739 to 795 for the share and its clamp, the left pane's width and the divider's Loader, which holds the resting hairline, the drag and the double click. `tests/startup-units.txt` keeps its 170 units and its `bytes` line moves from 1596965 to 1600480, the sum `tests/startup-objects.sh` takes over them, for those `ui/WindowBody.qml` lines, the Dual minimum in `ui/Theme.qml` and the accent line's `lineX` in `ui/ResizeHandle.qml`, which the divider's handle sets to its first pixel where a column edge keeps the centre; `tests/staticgates-unqualified.tsv` moves the `ui/WindowBody.qml` rows the new lines shift or add.
The Dual divider's cancel moves no file ceiling (`ui/WindowBody.qml` stays 798: the three resets of `end()` became one `reset()` that `onCanceled` also calls) and one `tests/startup-units.txt` byte line, 1600480 to 1601388, the sum `tests/startup-objects.sh` takes, for the `canceled()` signal, the `FLEA_TRACE_RESIZE` log and its `Quickshell` import in `ui/ResizeHandle.qml` and the `reset()` and `onCanceled` lines in `ui/WindowBody.qml`. A grab the compositor takes mid-drag now aborts the divider's drag and writes no split; a column edge still commits on a cancel as before.

The Dual divider keeps its grab through a real drag: the divider zone overlays the second listing's row padding, where each row's `DragHandler` in `ui/RowDrag.qml` takes over from an item grab after its threshold, so the handle's `MouseArea` in `ui/ResizeHandle.qml` sets `preventStealing` (the Columns edges, above any row, are unaffected). `tests/dualdivider.qml` drives a real press and two moves through `TestEvent` and fails on a canceled grab. No file ceiling moves; the `tests/startup-units.txt` byte line goes from 1601388 to 1601521 for the comment and property in `ui/ResizeHandle.qml`.

The dual pane Quick Look fix qualifies the right pane's `previewLoader` binding in `ui/WindowBody.qml` as `primaryPane.previewLoader`, since the bare name resolved to the right pane's own property and stayed null. `tests/startup-units.txt` keeps its 170 units while its `bytes` line moves from 1596965 to 1596977, the 12 characters of `primaryPane.`, and `tests/quicklook-firstframe.qml` moves its ceiling from 438 to 452 for the dual leg's checks.

The Contents walk's mount charge moves one ceiling, re-derived with `wc -l`: `src/backend/dirsize.rs` 400 to 412, for the lines in `list_into` that look up a directory entry's device from the mount table before its metadata call, tell `entering` when it crosses onto another device and hand the walk back to the listed folder once that stat returns, so a hang in the stat that crosses into a mount is charged to that mount and a later sibling's is not. The remote walk's test seams (its pace and the silence that marks a mount) live in `src/backend/menucontents_seams.rs`.

The minor-fix pass adds `a_frame_held_for_the_preview_body_swallows_a_trash_key` to `src/tui/hold_tests.rs`, the case where the hold comes from the first preview body with the parent peek already landed, so `src/tui/hold_tests.rs` goes from 393 to 426 lines (`wc -l`) and its recorded ceiling in `tools/flea-file-budget` is 426.

The Tags flyout that stays open through a choice and redraws from the re-asked answer, and the queue that keeps a second tag choice made before the menu snapshot answers, add bytes to `ui/ContextMenu.qml`, `ui/Pane.qml` and `ui/PaneMenuActions.qml`, which moves `tests/startup-units.txt`'s `bytes` line from 1596965 to 1598119, the sum the probe's tool takes over the same 170 units. The same change takes `ui/PaneMenuActions.qml`'s recorded ceiling from 491 to 507, `wc -l` on the file, for the queue, the one-shot gate that now lets a tag toggle repeat, and the queue's resets wherever a pending action is cleared. The queue then drains one choice per activate reply, because the backend's menu slot holds one job and a second activate sent at once was refused as busy and lost: `tagInFlight`, `sendNextTag` and the resets take the `bytes` line to 1598752 and the ceiling from 507 to 519, each re-derived (the probe's tool sum, `wc -l`).

The auto-hidden rail slides on x: `ui/PaneRail.qml` keeps the Sidebar built after its first reveal, eases a `slide` property and waits a dwell at the edge, and `ui/Pane.qml` passes `railRevealed` and `railX` to `railState`, which moves `tests/startup-units.txt`'s `bytes` line from 1596965 to 1599516, the sum the probe's tool takes over the same 170 units.

tbn1 2026-10-12 moves one ceiling, re-derived with `wc -l`: `tests/js/tabs.js` 797 to 800 for the three checks that each tab-count verdict stood before the next action withdraws it.

S2FIXA 2026-10-11 raises the first-frame `bytes` line in `tests/startup-units.txt` from 1,629,493 to 1,630,014 (no unit added), the sum the probe takes over the same units: 71 in `ui/Backend.qml` (the archive refusal names its archive) and 450 in `ui/js/Errors.js` (the archive refusal sentence names what stopped).

The Q8 copy pass of 0.3.10 moves the first-frame `bytes` line in `tests/startup-units.txt` from 1596965 to 1597296 (+331, no unit added): `ui/Pane.qml` names the no-window sentence once in `noWindowSentence` and shows `Menu.EMPTY_CLIPBOARD` for an empty Paste as, both of which the first window already loads. The round 1 fix fills the "Could not open" refusal in `ui/js/Places.js` in one pass, so a favourite label holding `%2` or `$&` prints literally, for the remaining +32.

The shared reveal and the pinned dialog frame move eight ceilings, each re-derived with `wc -l`: `ui/ContextMenu.qml` 751 to 775 for the lazily built fade, the held flyout rows and the root's input gate, `ui/KeymapSheet.qml` 549 to 551, `ui/SettingsPanel.qml` 543 to 547, `ui/UnlockDialog.qml` 439 to 446 and `ui/ConvertDialog.qml` 446 to 452 for the card frame's title and button slots, `tests/dialog-board.qml` at 548 for the geometry, pinned-title and pinned-button legs over fourteen cards, their pinned rows and the Network scroller's own height, `ui/Ipc.qml` 966 to 976 for the card rect read that adds the open rise back out, and `tests/ring-bounds.qml` 678 to 680 for the wait on the fade end. `ui/Reveal.qml`, `ui/CardFrame.qml`, `ui/FlyoutFade.qml` and `tests/reveal.sh` stay under the soft budgets; `tests/reveal.qml` is 320 lines, over the 250 soft budget, for its rise, flyout (open, close and empty) and Network legs. `tests/startup-units.txt` keeps its 170 units while its `bytes` line moves from 1596965 to 1598588, the sum the probe's tool takes over them with the menu's lazily built fade in `ui/ContextMenu.qml` and the rest-rect read in `ui/Ipc.qml`; no unit joins, because `ui/Reveal.qml` and `ui/CardFrame.qml` load with the first card, not the first frame. `tests/staticgates-unqualified.tsv` moves the rows of the ten files whose lines moved, in both import profiles.

An errored menu row says its state in its hint slot (Q13, MOTION-01): `ui/js/StateWords.js` is the one table of words (files only, signed out, no devices, offline, not running, symlink, special file, not yours), keyed by trigger and never a provider's reason, and `ui/js/Menu.js` and `ui/js/LockedMenu.js` set the entry's `hint` beside `errored`; a dead row still draws its hint muted and its label stays red. The word is part of its row's wanted width, so a menu holding an errored row may grow past `Theme.menuWidth` but never elides a label or passes its work area; `tests/menu-fit.qml` adds an errored scene per file kind at every text stop that reads each word drawn whole and counted in `wantedWidth`, and keeps its rows-all-fit check for every scene without one. Logic never reads a sentence: `ui/js/Taildrop.js status` and `ui/js/Dropbox.js account` and `status` return a `code` beside the `reason`, it rides as `taildropCode` and `dropboxCode` from `ui/Taildrop.qml` and `ui/NetworkMounts.qml` through `ui/Pane.qml` and `ui/ContextMenu.qml`, `StateWords` keys its words by it, and an empty or unknown code reads as offline (Taildrop) or not running (Dropbox); `files only` follows `rowIsFile`, the fact that empties the peers, never `rowMode`, so a linked folder and a folder row whose mode was never read say it too; Permissions with no selection, or with a mode never read, names no kind and carries no word. `tests/startup-units.txt` takes `js/StateWords.js` as its 171st unit and its `bytes` line moves from 1598588 to 1602983, the sum the probe's tool takes over them. `tools/flea-file-budget` records `ui/js/Menu.js` at 487, `tests/js/menu.js` at 589, `tests/menu-fit.qml` at 411, `ui/ContextMenu.qml` at 778, `ui/NetworkMounts.qml` at 909, `ui/Pane.qml` at 1182 and `ui/PaneMenuActions.qml` at 493, each re-derived with `wc -l`.

A flyout is at least its parent frame's width and wider where its own widest peer needs more, never past the work area (the Open with board's rule 2 once a state word widens the menu); `tests/menu-fit.qml` reads the flyout against both, and `tools/flea-file-budget` records `ui/ContextMenu.qml` at 781, re-derived with `wc -l`.

The cardsizes menu check sums what each row laid out for itself, never `widestRow`: `ui/MenuRow.qml` `parts()` states a row's label, hint, status and chevron widths with its paddings as drawn and whether its label elides, `ui/IpcMenuState.qml` states them as `rows`, `tests/ui-card-layout.sh` expects the frame at min(work area, max(token, the widest row's sum under `wantedWidth`'s rule)) and no elided label under the cap, and `tests/menu-fit.qml` runs the same sum headless on every scene. `tests/startup-units.txt` keeps its units while its `bytes` line moves from 1653005 to 1653627, the sum the probe's tool takes over them (+779 `ui/MenuRow.qml`, -157 `ui/IpcMenuState.qml`). Ceilings, re-derived with `wc -l`: `tests/menu-fit.qml` 411 to 446 for the probe stub, the parts checks and their sample input.

Quick Look's owed thumbnail settles through the planner: `ui/Preview.qml` asks for the row's thumbnail once a show owes one and swaps it in when the reply lands, and `ui/js/ExtThumbs.js` `thumbReply` keeps the thumbed line's miss and remember rule for the pane and the harness, which moves `tests/startup-units.txt`'s `bytes` line from 1596965 to 1597623, the sum the probe's tool takes over the same 170 units.

The tag place's live walk gets the names it asks for. `src/backend/searchtags.rs` (444 lines, with `src/backend/searchtags_tests.rs` at 519 and `src/backend/searchtags_write_tests.rs` at 235) splits `tag:` terms off a session query and reads each indexed row's `user.xdg.tags` once on a worker thread that starts after the walk lands, so the loop thread (`src/backend/run.rs` Request::SearchQuery, on_search_walked and the own-write hook all call `Session::answer` there) never touches an attribute; a query with no tag term starts no worker and reads none. The answer stands `walking` true until the names are in and the worker's landing, through the walk's own `SearchWalked` event and `Session::poll_walked`, re-runs the query once. `rank_cmp` and `select_head` move from `src/backend/searchsession.rs` 397 to 372 into `src/backend/searchindex.rs` 345 to 395 to make room, `src/backend/search.rs` 325 to 329 shares one `names_carry` rule with the legacy walker, and `ui/js/Search.js` gains `openScope`, the open-and-reset lines `start` and a tag place's reopen now share (its ceiling move is recorded with the stage join below). `tests/startup-units.txt` keeps its 170 units while its `bytes` line moves from 1598752 to 1598905, the sum the probe's tool takes over them. Measured on East, 200,000 entries on tmpfs with 1 in 100 tagged: the first `tag:Red` answer costs the loop 2.1 ms, the names are in 141 ms later off the loop, the re-run is 21 ms and a narrowed extension 0.1 ms.

The font page keeps `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1598587, the sum the probe's tool takes over them with the `fontlook` request and `fontLooked` signal in `ui/Backend.qml`, the `fontlooked` route in `ui/js/Messages.js` and the font kind, sample sizes, shared ask count and facts line in `ui/js/Kinds.js`.

The tall previews line keeps `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1596965 to 1597596, the sum the probe's tool takes over them with the `previewFacts` key in `ui/ViewState.qml` and `ui/js/UiState.js`, the `factsToggle` route in `ui/js/Focus.js` and its generated rows in `ui/js/Keymap.js`.

The Trash order unit raises the first-frame `bytes` line in `tests/startup-units.txt` from 1,596,965 to 1,597,470 (+505, no unit added): `ui/TrashView.qml` opens the cursor row's menu under that row (`openMenu`), lands the cursor on the newest row and draws the Deleted title with `Sort.title` in `ui/js/Sort.js`, which `ui/Header.qml` now asks for its own sort mark, and `ui/js/Trash.js` gains `menuTarget`.
Its review round 1 raises the `src/backend/trashbrowse.rs` ceiling from 834 to 898 (`wc -l`) for `list_with`, `same_items`, `listed_uris` and their five tests; no code or comment was cut.

The preview column width keeps `tests/startup-units.txt` at its 170 units while its `bytes` line moves from 1598076 to 1598360, the sum the probe's tool takes over them with the `preview.width` reader in `ui/ViewState.qml` and the stored-width layout in `ui/ColumnsArea.qml`.

Coloured code lines (the UI half of the code engine) move three recorded ceilings, each re-derived with `wc -l`: `ui/Theme.qml` 443 to 446 for the stored `palette` that `CodeLines` lifts its inks from, `ui/Backend.qml` 550 to 560 for `askCode`, `codeResult` and the token, and `ui/PreviewColumn.qml` 585 to 586 for the two lines that hand the backend to the first-lines reader and let it name a binary file under the row's own mark. The drawing is the new `ui/CodeLines.qml` at 219 lines and the pure helpers `ui/js/CodeRuns.js` at 72 (the ink lift and its six fallbacks live there, so only a launch that previews code loads them), both inside their budgets. The first frame gains only `Backend.qml`'s `askCode` and its signal and `Messages.js`'s one route line (plus Theme's one stored `palette`), which is why `tests/startup-units.txt` pins 1596965 to 1597706 bytes.

The picker answer row moves one test ceiling, re-derived with `wc -l`: `tests/button-members.qml` 515 to 518 for mounting `ui/PickerAnswers.qml` in the picker host and reading Cancel, Open and the filter trigger from the answer row instead of the chrome.

The picker answer row's test, `tests/picker-040.qml`, joins the ceiling table at 603, re-derived with `wc -l`: 567 lines for the answer row, the title line, the filter dropdown and its menu, and 36 more for the requester's own Text and its elision, the Tab walk through Filter, Cancel, Open, Up, the rail and the listing, the Tab that reaches the trigger from the listing, and the refused choice while the backend is unavailable. `ui/PickerWindow.qml` moves 797 to 798 for the line that says why `stepFocus(null)` lands on Cancel, `tests/picker-hunt.qml` 448 to 447 for the one shared `answerOpen` finder, and `tests/button-members.qml` keeps 491.

The column's continuous PDF stack moves the first-frame `bytes` line in `tests/startup-units.txt` from 1597596 to 1597959 (+363, no unit added): `ui/js/PreviewKeys.js` routes the column's page keys to the stack's scroll instead of the removed strip, while `ui/Ipc.qml` and `ui/IpcPreviewState.qml` lose the strip's state reads.

The two lines together take the `bytes` line to 1598292.

The tall Quick Look rect (`ui/js/QuickLookRect.js`) moves `ui/Preview.qml` from 740 to 773 lines for the card's `shape`, the next card's `shapeNext` and `panes` size, the size reply, the image meta ask for every row and the image pane built after that reply, and `ui/Theme.qml` from 443 to 445 for `preview.edge`, and takes the `bytes` line of `tests/startup-units.txt` to 1598459 over the same 170 units. The card draws `shape`, which takes `shapeNext` only while no swap holds a picture, so a held picture is never stretched, while `panes` already has the next card's size, so the preview built under the hold is whole when the card changes size in the swap's release frame; a reply that comes after the reveal cap leaves the card at today's 82% by 82%. A PDF followed from a PDF holds the old picture until the new document's first page lands, because `lookReady` reads `ui/PdfViewer.qml`'s `opened` against the shown path, so the page-size reply arrives while the swap still holds and the card never draws the unknown shape.

### Case twins are matched by lower case, and a cifs share is not read as folding

`src/backend/casefold.rs` decides which folders fold case from the filesystem type: the FAT family and exFAT always, ext4 and f2fs only inside a folder carrying the casefold flag (`FS_IOC_GETFLAGS`), every other type never. Where a folder folds, two names are twins when their lower cases match (`casefold::key`). The collisions question (`collide.rs` `ask`), the merge card and the extract refusal (`extractnames.rs`) all use that key.

- The kernel folds more widely than lower case. exFAT reads its own up-case table and ext4 casefold compares Unicode-normalized folded names, so two names that differ only in Unicode normalization (NFC against NFD) or in a fold that lower case does not make are not seen as twins in advance. A copy or move of such a pair still never writes one over the other: the second name finds the first entry when it lands and is refused with `already exists`, since the collisions question never counted it. An extract of such a pair is not refused up front and lands as the extract tool writes it.
- A cifs or smb3 mount reads as not folding. A Samba or Windows share folds case on the server, under a rule the client cannot read, so twins sent to a share are found only when they land, with the same `already exists` refusal, and the collisions question does not count them beforehand.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
