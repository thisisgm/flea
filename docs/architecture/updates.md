## Updates

Omarchy installs; Flea may check and launch. That revises the About board's "no duplicate
updater" rule (decided 2026-09-22), and it is the whole design: Flea asks the source that will install an
update whether it has one, and hands installing to Omarchy's own updater. Nothing in Flea downloads,
verifies, unpacks or writes a package, and pacman stays the only writer of `/usr/bin/flea`.

**What is checked.** `flea --update check` is `src/update.rs`. It asks `pacman -Qqo` which package owns
the running binary, `std::env::current_exe()` rather than a literal `/usr/bin/flea`, so a checkout build
run beside an installed package reports itself and not the package. `pacman -Qi` under `LC_ALL=C`, because
pacman translates its field names, then gives the installed version and "Validated By", and the owner
decides the kind:

- `flea` validated by a signature is OPR's package, and `checkupdates` is asked. pacman-contrib is a hard
  dependency of `omarchy`, so it is on every box Flea installs on. It syncs a private copy of the sync
  databases without root and exits 0 with updates, 2 with none and 1 on an error; it is never given
  `--nosync`, which answers 2 when it has no copy to read. GitHub is never asked for OPR, because it
  would announce a tag OPR cannot install yet.
- `flea-bin`, signed or not, is `release`: the prebuilt release binary, which About names a release build
  that Omarchy update keeps current, never a checkout to rebuild.
- Any other owner is `local`: a `flea` no signature validated (a local `makepkg -si` build), the former
  AUR name `flea-git`, and any other name. A binary no package owns is `unowned`. All three are
  `unchecked`: no source here describes them, and none is asked. A box that still has an AUR package moves
  to OPR's `flea` through `omarchy update` once the OPR recipe declares `replaces=('flea-bin' 'flea-git')`:
  pacman's `-Syu` takes that replacement, and upgrades a same-name AUR `flea` to a newer repository build.

Every version that reaches `vercmp` or a screen matches `^[0-9]+(\.[0-9]+){2}-[0-9]+$` first, which is
how network text is kept out of the line; `vercmp` then decides. The line is
`<state> <kind> <installed> <latest>`, the states `available`, `current`, `failed` and `unchecked`, the
kinds `opr`, `release`, `local` and `unowned`, and `-` for a version the check does not have. See
"Modes" for the exit statuses.

**What is launched.** `flea --update` spawns `omarchy-launch-floating-terminal-with-presentation
omarchy-update`, the command Omarchy's own menu and its bar widget run, as a fixed argv; nothing the
network said reaches argv or a shell. It updates OPR `flea` through pacman, with Omarchy's snapshot,
keyring, migrations and pacman guard in step, and it asks for sudo in a real terminal. It goes through
Rust for the same reason `--terminal` does, "Transparent huge
pages": `terminal::detach` is the one set of guards both use, the huge page hand-back, the display-GPU
pin and theme trade dropped, `/dev/null` on all three descriptors and a process group of its own.

**Why nothing is replaced in place.** A self-replacing download was rejected in the research: the
binary is package-owned, so `pacman -Qkk` would fail and the next upgrade would overwrite it; the
package also ships the QML under `/usr/share/flea/ui`, the helpers, the portal and two D-Bus services,
so a newer binary against older QML is a skew bug; a `~/.local/bin` copy would put two Fleas on the box;
and it would need HTTP, TLS and archive code this crate does not have, for no more than same-origin
checksum integrity. OPR's signature remains the authenticity.

**The window.** `ui/UpdateCheck.qml` is a singleton that owns both processes, the state (pure, in
`ui/js/Update.js`) and two `Timer`s. A check starts four ways: Enter on Settings > About's Update Flea
row, Settings > About opening (`ui/AboutFacts.qml`), a single look one minute after the window opens,
which asks only when About would, and a six hour timer that counts from the last answer, whichever
trigger asked for it. `ui/WindowBody.qml` starts both timers with the window. `updates.autoCheck` in
`src/uischema.rs`, "Check automatically" under the row and on by default, governs the three automatic
triggers and never a press. About opening does not ask again while
an answer from the last six hours stands, and a failed check never stands. The row, its eight states,
the caption and the note line follow the Update Flea boards (`UpdateStates`, `Main` and `UpdateMenu` in
the 0.3.3 design canvas). Enter checks from idle and up to date, opens Omarchy's updater from available
and from a check that failed, because the updater can still run, and does nothing while a check or an
update runs; a local or unpackaged build is a fact row. A launch is final for the process: the row reads
"Updating in terminal", the note asks for a restart, and no automatic check runs again, because pacman
may replace QML this process still loads lazily.

The background menu carries Update Flea only while a check has found a newer build, in the last group
under Settings, with the version in the hint slot beside the rail's 6 px status square; every other
state leaves that menu exactly as it was. Choosing it launches, the footer says "Opening Omarchy update
· restart Flea when it finishes", and the row is gone, because the state is then `launched`. It is an
Extras switch in Settings > Menus and ships visible. `ui/js/Focus.js` reaches the singleton through
`ui/Opener.qml`'s `updateFlea`, because a `.pragma library` cannot name a QML singleton.

**Privacy.** This is the first request Flea can make on its own: `Cargo.toml` has no dependencies, and
everything else that reaches another machine, a network mount or a LocalSend send through `localsend-cli`,
starts from something someone did. For an OPR install it is `checkupdates` fetching the
sync databases from the mirrors pacman already uses. A local or unpackaged build makes no request at
all, only the two local pacman queries. Requests happen only on a press of Update Flea, when Settings >
About opens, one minute after a window opens (off the launch
path, so no benchmark or first frame pays for it) and every six hours after that, all but the first
only with "Check automatically" on; off means nothing leaves the box until someone presses the row.
Nothing is written: the answer lives in the running process.

**Tests.** `src/update_tests.rs` feeds each parser canned output and never runs pacman.
`tests/update.sh` drives the binary with only a stub directory on `PATH`, so a program the suite forgot to
stub fails to start rather than reaching a mirror, and asserts the printed line, the exit status, that curl
never runs, the presenter's argv, descriptors and process group, and the huge page hand-back through a stub
`qs`. `tests/js/update.js`, `tests/js/settingsabout.js`, `tests/js/menu.js` and `tests/js/settingsmenus.js`
hold the row states, the About rows, the menu row and its switch. The GUI suites launch a `target/`
binary, which no package owns, so an automatic check there answers `unchecked unowned` and asks nothing.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
