## Show in folder

`org.freedesktop.FileManager1` is the interface a desktop's "Show in folder" goes through, and issue
54 is what happens with nothing of Flea's owning it: Chromium reveals a download and Nautilus opens.
Measured with `strings` against Chromium 151.0.7922.173 on this box, the binary carries exactly
`org.freedesktop.FileManager1`, `/org/freedesktop/FileManager1` and `ShowItems`, so that one call is
the whole of what has to be answered for the reported symptom.

`tools/flea-filemanager1` answers it, and it is the third non-Rust helper in this tree for the same
reason as the second: a D-Bus service has to own a name, export an object and answer method calls,
and this crate has no dependencies at all. It copies `tools/flea-portal`'s shape, its `FLEA_BIN`
seam, its `say()` elision and its release-then-quit idle exit.

**Every window it opens is `flea --select` or `flea <dir>`**, which is why the service is small.
`--select` already resolves a `file://` URI to its parent and puts the cursor on the entry, so
`ShowItems` is that flag and nothing more; `ShowFolders` wants the folder itself open, which is the
positional argument. Two items in one directory are one window, because a window can only put the
cursor on one row, and the first URI named under a directory is the one its window selects.

**`ShowItemProperties` answers `org.freedesktop.DBus.Error.NotSupported`.** Flea has no properties
dialog. Falling back to `ShowItems` would answer a different question than the caller asked and
would look, to the caller, exactly like success; a reply that did nothing at all would be worse.
The signature returns no values, so the error reply is the only channel a refusal has.

**The URIs come from another application, so the boundary is the whole of the rest of the file.**
The decoder is `GLib.filename_from_uri`, glib's own, rather than a second one: measured here with
PyGObject 3.56.3 on Python 3.14.7 it refuses a foreign scheme, a bare path, `file:`, `file://` and
a `%00`, and it hands back the authority separately. The two rules it does not carry are the two the
service adds: an authority that is neither empty nor `localhost` is refused, and a decoded path
holding a control character is refused, because `%0A` decodes to a newline without complaint. A path
that is not there and a target with no parent are refused too. Each refusal is per URI and elided to
one sentence on stderr, so one bad URI beside a good one does not take the call down; a call left
with nothing to open answers `org.freedesktop.DBus.Error.InvalidArgs`.

**The shape was verified with a real round trip, not assumed.** `tools/flea-portal` had shipped an
`isinstance(value, bytes)` test where PyGObject 3.56.3 delivers a list of ints for a D-Bus `ay`. A
`GLib.Variant("(ass)", ...)` unpacked on this box gives a `list` of `str`, so `isinstance(uri, str)`
is the right test here, and it is there because the check costs nothing and the last guess was wrong.

**Registration is `packaging/com.thisisgm.flea.FileManager1.service`, named for Flea and not for the
interface.** Nautilus owns `/usr/share/dbus-1/services/org.freedesktop.FileManager1.service` on this
box, so a file of that name is a pacman conflict; dolphin, thunar and nemo each ship their own
vendor-named file carrying `Name=org.freedesktop.FileManager1`, which makes a vendor name the
measured convention rather than a workaround. The user-level file `flea --default` writes takes the
plain interface name instead, because no package installs into `$XDG_DATA_HOME` and there is nothing
there to collide with; D-Bus keys on `Name=` either way.

**Installing does not decide which one answers, and `flea --default` is what does.**
D-Bus keeps the FIRST registration it reads, and inside one directory the two buses disagree about
which that is. Measured on 2026-09-06 with all five files in one directory, every `Exec` rewritten
to a marker: dbus-daemon 1.16.2 ran nautilus's, which is first in `ls -U` and third alphabetically;
dbus-broker 37 kept `com.thisisgm.flea.FileManager1.service`, which is last in `ls -U` and first
alphabetically, and logged the other four as duplicates. So dbus-daemon takes readdir order and
dbus-broker sorts, neither is newest-wins, and an installer can steer neither. Omarchy ships
nautilus in `omarchy-base.packages`, so a stock box always has a second claimant; on this box on
2026-09-06, four installed and no Flea package, dbus-broker threw away nautilus's, dolphin's and
thunar's, so nemo's is the one answering.

The directory order is what settles it, and it is not readdir's. `$XDG_DATA_HOME/dbus-1/services` is
read before every `$XDG_DATA_DIRS` entry, so one file there outranks all four: that is the same
shadowing that once let a development portal backend override the installed one as a bug, used on
purpose. `defaults::claim_service()` writes it, `defaults::release_service()` removes it and the two
directories it created when nothing else is in them, and both are per-user preferences pacman cannot
own, which is why they are steps of `flea --default` and not PKGBUILD lines. Measured on this box
with dbus-broker 37 and a sandbox XDG ladder: with the user file present the broker logged
`Ignoring duplicate name 'org.freedesktop.FileManager1' in service file '<path>'` for all five
system files and none for the user one.

**The file leads with a comment saying who wrote it.** dbus-broker 37 and dbus-daemon 1.16.2 both
accept a `#` comment in a `.service` file, before the group header and after it, measured on this
box; the untraceable user-level service file was this morning's production bug, so the first line
is the provenance and it is also how `release_service()` tells Flea's file from somebody else's. A
file at that path whose first line is not the marker is left alone and named, never overwritten.
The `Exec` is copied out of the installed packaged registration, so the two can never disagree and
no install path is written down twice.

**`tests/filemanager1.sh` drives the real interface on a private session bus** it starts inside its
own fixture, with service directories of its own and `FLEA_BIN` pointing at a stub that records its
argv. That makes D-Bus activation itself part of the test rather than a manual step, and the first
case is the negative control: on a bus with an empty service directory the same call fails
`ServiceUnknown`. The registration under test is the shipped file with only its `Exec` repointed at
the checkout, so a broken `Name=` in `packaging/` reddens the suite.

**Its last five cases are `flea --default`'s claim on the name.** They run the real binary against a
sandbox `HOME` and a sandbox XDG ladder, with `xdg-mime` and `hyprctl` stubbed on `PATH`, because a
reachable `hyprctl` would reload the operator's live Hyprland config rather than anything in the
fixture. The last of them is the ordering one, and it is a driven negative control rather than an
assertion: the bus is given the sandbox data home and then a directory of four rival registrations,
the rival first in `ls -U` answers, `flea --default` makes Flea answer over it, and
`flea --default off` hands it back. The suite is already in `tests/run-all.sh`'s `headless` list, so
this coverage needed no new entry there.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
