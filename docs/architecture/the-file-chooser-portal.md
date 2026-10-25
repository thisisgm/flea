## The file chooser portal

Every application that asks the desktop to pick a file goes through the XDG portal, and the dialog
that opens is whichever backend owns `org.freedesktop.impl.portal.FileChooser`. That is the whole
reason the Tailscale bar widget raised a GTK dialog in the middle of Omarchy: its Panel calls
`omarchy-tailscale-send` with no file arguments, that script runs `omarchy-file-select`, and
`omarchy-file-select` is a 124-line portal client. Nothing about it is Tailscale specific, so
neither is the answer: Flea implements the backend interface, and every portal caller on the box
gets the same chooser.

**Three moving parts.**

- `tools/flea-portal` owns the bus name `org.freedesktop.impl.portal.desktop.flea`, exports
  `FileChooser` at `/org/freedesktop/portal/desktop` and a `Request` object at each request's own
  handle, turns one call into one JSON request, runs `flea --pick`, and turns the reply into a
  response code and results. It exits after 30 s with no request open, because D-Bus starts it
  again on the next call.
- `flea --pick <reply>` is `gui::pick`: it refuses without `FLEA_PICKER` or a reply path, refuses
  without a display, resolves the UI with the same `paths::ui_dir()` the window uses, and `exec`s
  `qs -p <ui>/boot/picker.qml` with the same renderer choice `--gui` makes. One code path chooses Vulkan
  for both front doors.
- `ui/PickerWindow.qml` is the window, loaded by file: URL from `ui/boot/picker.qml`; see "The
  first window". It instantiates the same `Backend`, draws the same `Row` behind a
  check box, reads the same `Theme` and the same `Places.favorites`, and carries none of the
  window's operations: a chooser that can rename or delete is a file manager wearing a dialog's
  clothes. `SendPicker.html` draws that row as the name, a 70 px size and an 80 px date, so
  `ui/PickerList.qml` hands `Row` `Picker.HIDDEN_COLS` and the chooser never inherits Mode or Kind
  from `ViewState`, whatever the header menu has switched on for the browser window.

**Stalled chooser listings.** `PickerListing.qml` owns a separate read-only backend for list,
listpaths and window. Navigation invalidates its output immediately, kills and reaps that worker,
then starts only the latest request with a fresh stdout parser. The chooser's other `Backend`
opts into `pickerOnly`: only picker checks and capability reads may be sent, and shutdown can
terminate a stuck identity check without touching a file-manager write backend. `PickerLifecycle`
waits for both workers only after the reply file has been saved. `tests/picker-stall.sh` drives
blocked FIFO reads, latest-navigation recovery, incomplete stale output, early cancellation,
failed spawn and process reaping under offscreen Quickshell; it needs no production mount.

**Recent, and why it is read-only.** `SendPicker.html` draws a Recent row above Home in the rail,
says the location's own name where the path would be, and draws Parent disabled with the words
"unavailable in Recent". The history it lists is the desktop's own,
`$XDG_DATA_HOME/recently-used.xbel` and `~/.local/share` when the session set no data home: Flea
keeps no history of its own, writes nothing to that file, and reads it only when the location is
opened. Qt's XML reader does the parsing in `ui/PickerRecent.qml`, because a hand-rolled XBEL
reader would be a second implementation of a file this application does not own. Every application
on the box appends to it, so `ui/js/Recent.js` treats a bookmark as untrusted text: only a real
local `file://` URI with an empty or `localhost` authority becomes a path, a control character in
the decoded form refuses it, and the read stops at 500 entries. That cap is
`ui/PickerRecent.qml`'s and not only the rail's: it reads the bookmarks off the model with `data()`
rather than instantiating one QObject per bookmark, which cost seconds against a fraction of one on
a 50,000 bookmark history here, a magnitude and not a number to cite. The rows come back from the
backend through `listpaths`, which stats each path and drops the ones that are gone, so a stale entry is
removed rather than drawn against a failed stat; the listing's base is `/` and each row is named by
its path under it, which is why `ui/PickerList.qml` draws a Recent row by its own leaf and
`Picker.rowPath` answers with the whole path. Recent is a location and never a directory: it is
named by the token `flea:recent`, which no absolute path can equal, Parent refuses in it, Back
works out of it, a caller's own `folder` still wins at startup, and a save is not offered it at all
because a history is not a directory to write into. The window's Recent listing draws each file's folder in its
Location column through `Recent.locationUnder`: relative to home with no leading `~/` ("Documents/claude"), `~` for a
file directly in home, and the absolute parent outside home (Sidebar040 draws no such row); it uses `Format.tilde`, the
home test every path display shares, and Settings, Places draws a favourite's path through the same function while the
row's `value` keeps the stored path for removal, reorder and the keyboard seam.

**Why the backend is Python.** It is the second non-Rust helper in this tree, after
`tools/flea-gio-auth`. A portal backend has to own a bus name, export objects, answer calls out of
order and stay reachable while a window is open; this crate has no dependencies at all, so the Rust
answer was a hand-rolled D-Bus marshaller. `python-gobject` is what `omarchy-file-select`, the
client this serves, is already written against, so it is an already-installed dependency rather
than new code, and the backend is a request-scoped daemon and not a hot path.

**Discovery, measured against xdg-desktop-portal 1.22.1's own source, not its documentation.** The
backend guide says `.portal` files live in `{DATADIR}/xdg-desktop-portal/portals`, but
`src/xdp-portal-config.c` `load_installed_portals()` scans `$XDG_DATA_HOME/.../portals` first and
the data dirs after, so a user-local registration is discovered too; that is what the box test uses
and what the package path does properly. `load_portal_configurations()` is the other half and works
differently from the `.portal` scan: it collects EVERY configuration file it finds into an ordered
list, `$XDG_CONFIG_HOME` first, and `xdp_portal_config_find()` walks that list per interface,
trying the interface key and then that file's own `default` before moving to the next file. A user
`portals.conf` naming only `FileChooser` therefore leaves `default=hyprland;gtk` in
`/usr/share/xdg-desktop-portal/hyprland-portals.conf` answering for everything else, which is why
`chooser::claim()` writes one key and never a default. Inside one directory a
`<desktop>-portals.conf` shadows the plain `portals.conf` entirely, so when the session's desktop has
one in `$XDG_CONFIG_HOME/xdg-desktop-portal`, `claim()` writes the same one key into that file, the
only user file the portal reads. Omarchy ships no user portal configuration in any release from
3.0.0 to 4.0.4, so such a file is the user's own or another tool's, and 0.3.3's refusal to touch it
left `flea --default` exiting 1 with file dialogs still on Nautilus for everyone who had one. A value
the claim replaces is kept on a `# flea replaced: <key>=<value>` comment above Flea's line, and
`release()` puts it back in every user portal file it finds, reading the directory rather than the session, because an older Flea wrote `portals.conf` there. A claim with no `XDG_CURRENT_DESKTOP` beside a desktop file refuses rather than guess which file the portal reads, before either half writes, so the picker's window rule is not left behind a routing that never happened.

**The two exit codes the caller distinguishes, and which of ours map to them.** In
`omarchy-file-select`, exit 1 is nothing picked, a decision, and `omarchy-tailscale-send` exits 0
without sending; exit above 1 is a chooser that never opened, a fault, and the caller raises a
critical notification. Those are CLIENT behaviours and not this backend's interface. What crosses
the wire is a response code: 0 with `uris`, 1 for the user's own refusal, 2 for everything else.
xdg-desktop-portal collapses 1 and 2 into the same client exit, so 2 is still the honest answer for
a request the caller withdrew, a window that died and a chooser that could not open: the pair is
never mixed at the boundary that can tell them apart. A response that never arrives is the real
defect, because the client waits 600 s for it, so `flea --pick` refuses loudly before opening
anything and every path out of `Pick.answer` answers exactly once.

**The reply is a file, not the child's stdout.** `qs` writes its own logs to stdout, so the answer
travels in a JSON file inside a `mkdtemp` the backend owns and removes. `ui/PickerWindow.qml` writes it
with `FileView` and only kills its own process on `saved()`: the backend reads the file after the
child exits, and a write still in flight would be a lost answer read as a fault.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
