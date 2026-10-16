# Installing Flea

Flea is for Omarchy: `omarchy` and `quickshell` are hard dependencies, so it will not install on a
plain Arch box.

Flea installs as an Arch package, so pacman owns both ends: `makepkg -si` puts it on, `pacman -Rns`
takes it off, and pacman's own file list is what makes the second claim provable. There is no
install script here because there is nothing for one to do. The steps pacman cannot own are
per-user preferences, and they are subcommands of the binary: `flea --default` makes Flea your
default file manager, puts it in front of the other file managers for "Show in folder", and routes
the desktop's file chooser to it, and `flea --picker` does that last part alone. Both are described
below.

## Which package?

The recommended install is `omarchy pkg aur add flea-bin`, and `omarchy update` keeps it current: a
release reaches it minutes after it ships. Omarchy's own repository carries the same release as
`flea`, a day or more behind. Install one of these, never two: all three own `/usr/bin/flea`, so pacman refuses a pair rather than leaving two
half-installed.

| Package | What you get | Built where | Install | Updates |
|---|---|---|---|---|
| `flea-bin`, AUR, recommended | the tagged release | Flea's release workflow, for x86_64 and aarch64; nothing compiles on your machine | `omarchy pkg aur add flea-bin` | `omarchy update`; a release arrives minutes after it ships |
| `flea`, Omarchy's repository | the same tagged release | Omarchy's build host, signed; nothing compiles on your machine | `omarchy pkg add flea` | `omarchy update`; a release arrives a day or more after it ships, once Omarchy has reviewed and built it |
| `flea-git`, AUR | current `main`, unreleased fixes included, for testers | your machine, with `cargo` | `yay -S flea-git` | `yay -Sua --devel` follows `main`; `omarchy update` rebuilds it only when its AUR PKGBUILD changes |

The AUR also carries a `flea` that compiles the tagged release on your machine. Omarchy's repository
package shares its name, so `omarchy update` replaces it with the repository build; use `flea-bin`
instead.

**Switching.** From `flea` to `flea-bin`, run the interactive command and answer `y` when pacman
asks whether to remove `flea`:

```
yay -S flea-bin
```

`omarchy pkg aur add flea-bin` cannot make that swap: it passes `--noconfirm`, pacman then answers
its own "Remove flea?" with the default No, and the install stops at "unresolvable package
conflicts". On a box with no Flea installed yet, both commands work. `flea-git` switches the same
way, and going back to Omarchy's package is `sudo pacman -S flea`, answering `y` to remove the other.
Omarchy refuses `yay -Syu` and `pacman -Syu`; `omarchy update`, or `yay -Sua` for the AUR alone, are
the update commands.

What lands on disk is the table below whichever package it is, the licence directory aside, which
takes the package's name: every AUR PKGBUILD runs the source `PKGBUILD`'s `package()` commands, and
the release workflow refuses a tag where one has drifted. `flea --default` and `flea --picker` write
per-user files, so a switch keeps them. The AUR also carries `flea`, the same release built from
source on your machine, but on Omarchy the repository package of the same name comes first, so
there is no reason to pick it there. [docs/release.md](release.md) is how each package is made.

## Build and install

```
git clone https://github.com/thisisgm/flea.git
cd flea
makepkg -si
```

Three lines, and the third one is the whole build: `-s` pulls in anything missing from `depends`
and `makedepends`, and `-i` hands the finished package to pacman.

`source=()` is empty on purpose, and it is worth saying why, because it is not obvious: with no
source array makepkg builds from `$startdir`, the directory the PKGBUILD sits in. So a fresh clone
is the source, and so is a checkout you are editing. `build()` reads that directory in place and
redirects `CARGO_TARGET_DIR` into makepkg's own source directory so a package build never disturbs
the tree's `target/` under a benchmark. `check()` runs `cargo test --release`, so a package that
builds is a package whose suite passed. **Build from a clean tree:** the checkout is the source, so
uncommitted edits are what gets packaged.

## What lands on disk

| Path | What it is |
|---|---|
| `/usr/bin/flea` | the binary, backend and launcher both |
| `/usr/share/flea/ui/` | the Quickshell UI, which `paths.rs` looks for by `boot/shell.qml` |
| `/usr/share/flea/ui/Commons`, `/usr/share/flea/ui/Ui` | symlinks into `/usr/share/omarchy/shell/`, reached from QML as `qs.Commons` |
| `/usr/lib/flea/flea-portal` | the XDG portal backend, which answers `org.freedesktop.impl.portal.FileChooser` |
| `/usr/lib/flea/flea-filemanager1` | the D-Bus service, which answers `org.freedesktop.FileManager1` for "Show in folder" |
| `/usr/share/dbus-1/services/com.thisisgm.flea.FileManager1.service` | what D-Bus activates that with |
| `/usr/share/xdg-desktop-portal/portals/flea.portal` | what registers that backend with xdg-desktop-portal |
| `/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.flea.service` | what D-Bus activates it with |
| `/usr/lib/flea/flea-gio-auth` | the mount helper, which answers one GIO mount password prompt |
| `/usr/lib/flea/flea-udisks` | the udisks helper, which the flasher runs to write an image to a drive |
| `/usr/share/applications/com.thisisgm.flea.desktop` | the desktop entry |
| `/usr/share/icons/hicolor/scalable/apps/com.thisisgm.flea.svg` | the icon |
| `/usr/share/libalpm/hooks/flea.hook` | the removal note, which prints the per-user undo commands below |
| `/usr/share/licenses/flea/LICENSE` | the licence |

The count is whatever the built archive declares, not a number written down here: the UI grows a file
whenever a component is added, so a figure pinned in this paragraph would be stale by the next commit.
`packaging/flea-package-test` reads the count out of the archive and fails if the fake root does not
hold exactly that many. The two symlinks are why `omarchy` is a hard dependency: they point into a
directory that package owns.

## An iPhone's tethering connection

Pairing an iPhone also switches it to its tethering USB configuration, so iOS shows Personal Hotspot
as active while it is plugged in; Flea does not use that connection. To keep this machine from routing
through the phone, tell NetworkManager, which Omarchy runs, to leave that interface alone, for example in
`/etc/NetworkManager/conf.d/90-ipheth-unmanaged.conf`:

```ini
[keyfile]
unmanaged-devices=driver:ipheth
```

Then run `sudo systemctl restart NetworkManager`.

## User fonts and font preferences

Flea can preview and install fonts for the current user, and apply installed families as
desktop or terminal defaults. See [font management](fonts.md) for supported formats,
installation paths and restoring previous preferences. These per-user files survive
package removal; restore defaults and uninstall unwanted fonts first if needed.

## Uninstall

```
sudo pacman -Rns flea
```

Or `flea-bin`, or `flea-git`, whichever is installed. Everything above goes, including the
directories the install created. The package carries no `.INSTALL` scriptlet, so nothing is ever
created outside the file list pacman tracks, and the desktop and icon caches are re-indexed by
Arch's own `update-desktop-database` and `gtk-update-icon-cache` hooks, which fire on Remove as
well as on Install.

Removal also runs Flea's own `flea.hook`, before any file goes. It prints `flea --default off` and
`flea --picker off`, because what those two commands undo lives in each user's home, where pacman
never reaches. Run them before the removal; once Flea is gone, the two "What `pacman -Rns flea`
leaves behind" sections below name each file to delete by hand. Swapping `flea-bin` for `flea` or
`flea-git` removes one package too, so the note appears then as well; its first line says it only
matters when Flea is leaving for good.

## Make Flea the default

Installing registers Flea for `inode/directory` and for `org.freedesktop.FileManager1`; it makes
Flea the answer for neither, because another file manager is registered for both, and it does not
touch Omarchy's file-manager keys. All of those are per-user preferences, so pacman cannot own them, and
Omarchy's own `default` verbs (`omarchy default browser`, `editor`, `terminal`) set exactly this
kind of thing without a package's help. There is no `omarchy default filemanager`, and
`/usr/share/omarchy/` is the package's to overwrite, so Flea carries the verb itself:

```
flea --default
```

It does every step below, each reported on its own line, and it is honest about state: run it twice
and the second run says every step it ran is already Flea's and rewrites nothing. It needs no root,
because every file it writes is yours, and it takes no argument, because Omarchy's `default` verbs
take one to name which program and here the program is Flea. The files it writes are
`~/.config/mimeapps.list`, `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service`,
`~/.config/hypr/bindings.lua` and `~/.config/xdg-desktop-portal/portals.conf`.

1. **The `inode/directory` handler.** `xdg-mime default com.thisisgm.flea.desktop inode/directory`,
   the stock tool, which writes one line to `~/.config/mimeapps.list`. The line printed names the
   previous handler, `org.gnome.Nautilus.desktop` on a stock Omarchy, which comes from
   `/usr/share/applications/mimeapps.list`. Only that one type: the entry registers nothing else,
   and a file manager that takes image or archive types is a bad citizen. The answer is read back
   with `xdg-mime query default` rather than trusted, because `xdg-mime default` exits 0 whatever it
   wrote.
2. **"Show in folder".** Chromium, Firefox, Steam and everything else that reveals a downloaded
   file call `org.freedesktop.FileManager1` on the session bus. Installing Flea registers for that
   name in `/usr/share/dbus-1/services`, but so do nautilus, dolphin, thunar and nemo, each in a
   file of its own in that same directory, and D-Bus keeps whichever registration it reads first.
   Omarchy ships nautilus in `omarchy-base.packages`, so a stock box always has a second claimant,
   and which one wins inside a directory is the bus's business: dbus-broker sorts it, dbus-daemon
   takes it in readdir order, and neither can be steered by installing a package. So this step
   writes one more registration, in the directory that is read before every system one:

   ```
   ~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service
   ```

   ```
   # Written by `flea --default`; `flea --default off` removes it.
   [D-BUS Service]
   Name=org.freedesktop.FileManager1
   Exec=/usr/lib/flea/flea-filemanager1
   ```

   The first line is there because a user-level service file with no provenance is a bug nobody can
   trace; `dbus-broker` and `dbus-daemon` both accept it, measured on this box. The `Exec` is copied
   out of the registration the package installed rather than written down by the command, so a box
   with no `com.thisisgm.flea.FileManager1.service` installed has nothing to put in front and this
   step refuses instead of naming a path that is not there.

3. **Omarchy's two file-manager keys.** `SUPER + SHIFT + F` and `SUPER + ALT + SHIFT + F` are bound
   to Nautilus in `/usr/share/omarchy/default/hypr/bindings/applications.lua`, which
   `omarchy update` overwrites, so the override goes where the Omarchy manual says an override
   goes: appended to `~/.config/hypr/bindings.lua`, between two marker lines, in the manual's own
   `hl.unbind` then `o.bind` shape:

   ```lua
   -- flea --default: begin. Written by `flea --default`; `flea --default off` removes the block whole.
   hl.unbind("SUPER + SHIFT + F")
   o.bind("SUPER + SHIFT + F", "File manager", { launch = 'flea --gui' })
   hl.unbind("SUPER + ALT + SHIFT + F")
   o.bind("SUPER + ALT + SHIFT + F", "File manager (cwd)", { launch = 'flea --gui "$(omarchy-cmd-terminal-cwd)"' })
   -- flea --default: end.
   ```

   The cwd key keeps its meaning: `omarchy-cmd-terminal-cwd` is the helper Omarchy's own Nautilus
   binding reads the active terminal's directory with, and Flea's positional argument is a path.
   After writing, `hyprctl reload` runs and `hyprctl configerrors` is read; if the config no longer
   loads, the file is put back as it was and the command fails saying what `configerrors` said. The
   output names what each key ran before, read off `hyprctl binds`. From outside the session,
   where `hyprctl` cannot be reached, the block is still written and the output says to run
   `hyprctl reload` yourself.

4. **The file chooser.** Everything `flea --picker` does, described in the next section. A box
   updating from 0.1.3 has a Flea with no chooser routing, and one command should finish the job.

   **This step is the skipped-not-failed one, and no other step is.** It needs
   `flea.portal`, which only the package installs. Run a binary you built with `cargo build` on a
   box whose installed package predates the chooser, which is every box updating from 0.1.3, and
   the command prints `flea: no file chooser service is installed, so Flea could not become the file chooser`
   and carries on. On such a box the honest-about-state line above is a claim about the steps that
   ran alone: the chooser was never claimed, so a second run cannot say it is already Flea's. That
   box has no packaged `com.thisisgm.flea.FileManager1.service` either, and step 2 says so and fails
   rather than skipping, so the command exits 1 having done steps 1 and 3. With no Flea package
   installed at all, step 1 refuses first and nothing is written, because
   `com.thisisgm.flea.desktop` is the proof the package landed.

Run it from a terminal inside the session, so the keys and file dialogs take effect at once: there it
also restarts xdg-desktop-portal, as described under the file chooser below.

### Or from Settings

Settings > About has the same switch, "Make Flea the default", directly under the File manager row.
Ticking it runs `flea --default` and unticking it runs `flea --default off`: the same commands, the
same steps, and the same four files. After either one goes through, the switch also runs
`systemctl --user try-restart xdg-desktop-portal.service`, so file dialogs follow at once instead of
at the next login; `try-restart` leaves a portal that is not running alone. If that restart fails
the switch still stands, and the line under it says "File dialogs follow after xdg-desktop-portal
restarts." The command makes the same restart itself only when its output is a terminal, so the
switch's run, whose output it reads, restarts nothing and the portal restarts once. The box is
ticked when `xdg-mime query default inode/directory` answers `com.thisisgm.flea.desktop`, and that
answer is read again after every run, so the box shows what the desktop will do rather than what was
clicked. The line under it says what happened: that
folders, Show in folder and file dialogs open Flea; that file dialogs were left out because the
package's portal files are missing (step 4's skip); or the first line flea printed when a step failed.
On a build with no Flea desktop entry installed the switch is greyed and asks you to install a
package first, which is step 1's refusal said before you press it. It stores nothing in Flea's
settings.

### Undo

```
flea --default off
```

Removes Flea's `inode/directory` line from `~/.config/mimeapps.list`, so the handler falls back to
whatever the system default is (Nautilus on stock Omarchy), deletes
`~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service` and the two directories it
created when nothing else is in them, so "Show in folder" goes back to whichever packaged
registration D-Bus reads first, and removes the marked block from
`~/.config/hypr/bindings.lua` byte for byte, then reloads, and undoes the file-chooser step exactly
as `flea --picker off` does. Unticking "Make Flea the default" in Settings > About runs the same
command. If you had pinned another handler in
`~/.config/mimeapps.list` before running `flea --default`, the first run printed its id as
`was <id>`; `xdg-mime default <id> inode/directory` puts that pin back.

### What `pacman -Rns flea` leaves behind

Everything the package installed goes, as above. The edits `flea --default` made are per-user state,
and pacman neither knows nor should know about them, so they stay. Its chooser edits are covered by
the next section:

- `inode/directory=com.thisisgm.flea.desktop` in `~/.config/mimeapps.list`. Inert once the binary
  is gone: `xdg-mime query default` skips an entry whose `Exec` is not on `PATH`, and answered
  `org.gnome.Nautilus.desktop` with that line in place when this was exercised without a `flea` on
  `PATH`. Still litter. Delete the line, or run
  `xdg-mime default org.gnome.Nautilus.desktop inode/directory`.
- The block between `-- flea --default: begin` and `-- flea --default: end` in
  `~/.config/hypr/bindings.lua`. With no `flea` on `PATH` the two keys would do nothing. Delete the
  block and run `hyprctl reload`.
- `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service`. This one is not inert, and
  it is the reason to run the undo before the removal: it still names
  `/usr/lib/flea/flea-filemanager1`, which pacman has taken away, and D-Bus does not fall through to
  the next claimant when the first one will not start. Measured here with the file in place and no
  such binary, the call answers
  `org.freedesktop.DBus.Error.Spawn.ExecFailed: Failed to execute program`, and the packaged
  registration behind it never runs. Its first line says it was written by `flea --default`; delete
  it, or run `flea --default off` while the binary is still there.

The clean order is `flea --default off` before `sudo pacman -Rns flea`, after which there is nothing
to do by hand. `flea --default off` leaves `~/.config/mimeapps.list` in place even when it was the
one that created it, holding an empty `[Default Applications]` section: the file is the desktop's,
other tools write to it too, and an empty section is harmless.

## Make Flea the file chooser

Every application that asks the desktop to pick a file, from `omarchy tailscale send` to a Flatpak,
goes through the XDG portal: it calls `org.freedesktop.portal.FileChooser`, and xdg-desktop-portal
hands that to whichever backend the configuration prefers. On a stock Omarchy box that is
xdg-desktop-portal-gtk, which is why a GTK dialog appears in the middle of Omarchy. Installing Flea
registers a backend; it does not prefer it. That is a per-user preference, so:

```
flea --picker
```

writes one key to `~/.config/xdg-desktop-portal/portals.conf`:

```
[preferred]
org.freedesktop.impl.portal.FileChooser=flea;gtk
```

If `~/.config/xdg-desktop-portal/hyprland-portals.conf` exists, xdg-desktop-portal reads that file and
ignores `portals.conf`, so the same key goes there instead. A backend it replaces, such as the GNOME
portal's Nautilus chooser, is kept on a comment above Flea's line for `flea --picker off` to restore:

```
# flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk
org.freedesktop.impl.portal.FileChooser=flea;gtk
```

and it writes one more thing, an additive block in `~/.config/hypr/bindings.lua` beside the one
`flea --default` writes:

```lua
-- flea --picker: begin. Written by `flea --picker`; `flea --picker off` removes the block whole.
o.window("com.thisisgm.flea.picker", { tag = "+floating-window" })
-- flea --picker: end.
```

That is Omarchy's own treatment for a prompt, not a size Flea invented:
`/usr/share/omarchy/default/hypr/apps/system.lua` tags `xdg-desktop-portal-gtk`'s windows the same
way, and Omarchy's tag rules are what then float, centre and size them. The picker carries its own
app id, `com.thisisgm.flea.picker`, so this rule reaches the chooser and never the file manager
window. As with the keys, the file is written, `hyprctl reload` runs, `hyprctl configerrors` is read,
and a config that no longer loads is put back as it was.

**It writes no `default=` line**, and that is the whole design. xdg-desktop-portal
collects every configuration file it can find into an ordered list, the user's first, and resolves each
interface through them in turn: an interface this file does not name falls through to the next file,
which on Omarchy is `/usr/share/xdg-desktop-portal/hyprland-portals.conf` and its `default=hyprland;gtk`.
So ScreenCast, Screenshot, GlobalShortcuts and InputCapture still resolve to hyprland, and Account,
Email and DynamicLauncher still resolve to gtk, exactly as before. `gtk` stays behind `flea` on Flea's
own line for the same reason: if `flea.portal` ever goes missing, there is still a chooser.

xdg-desktop-portal reads its configuration once, at startup, so a live session keeps the old routing
until it is restarted. Run at a terminal, `flea --picker`, `flea --default` and both `off` forms do
that themselves with `systemctl --user try-restart xdg-desktop-portal.service`, the Settings switch's
own restart, and say `xdg-desktop-portal restarted if it was running, so file dialogs follow now`.
With their output piped or redirected, as Settings runs them and as a script capturing them does,
they restart nothing, since the caller may own the restart, and a refused restart is reported the same
way: the line names the command to run instead:

```
systemctl --user restart xdg-desktop-portal
```

A claim refused before it wrote anything restarts nothing and prints neither line.

The picker that then opens is Flea: the same rows, icons, theme and keys as the window, with a check
box in front of every row a caller can receive. Space marks, Enter walks into a directory or submits
what is marked, Backspace climbs, Escape refuses. `.` shows and hides the directory's dotfiles, the
same re-read the window makes; a preset's toggleHidden chord does the same. Nothing marked and Enter
does nothing, because a chooser that sends on a stray keypress is worse than one that asks twice.

### Undo

```
flea --picker off
```

puts back the backend a `# flea replaced:` comment names, or removes Flea's line when there is no
comment, in `portals.conf` and in any `<desktop>-portals.conf` beside it, and removes `portals.conf`
when Flea's line was all it held. It removes the Hyprland block byte for byte. Restart
xdg-desktop-portal again and the previous chooser is back.

### What `pacman -Rns flea` leaves behind

`~/.config/xdg-desktop-portal/portals.conf` is per-user state like `mimeapps.list` above, so it stays.
With no `flea.portal` installed, xdg-desktop-portal logs that the requested backend does not exist and
takes the next name on the line, which is `gtk`, so the desktop keeps a working chooser either way.
The same goes for `hyprland-portals.conf` when Flea's line went there. The `flea --picker` block in
`~/.config/hypr/bindings.lua` stays too, inert with no Flea window to match: delete it and run
`hyprctl reload`. The clean order is `flea --picker off` before `sudo pacman -Rns flea`.

## Why the Exec line reads `flea --gui %f`

`%f` because Flea's positional argument is a path. A `%u` entry advertises that the program
understands URI schemes, and Flea's positional does not: only `--select` strips a `file://` prefix
and percent-decodes. For a local directory the two field codes measure the same, both hand over one
decoded path, so the difference only shows on a remote URI, where `%u` would give Flea an
`smb://host/share` string to treat as a relative path.

`--gui` because a desktop entry should name the mode it means, not because the launch would
otherwise land somewhere else. Bare `flea` opens the window too, and reads no stdio to decide it, so
the flag changes nothing today: it is the contract that keeps this entry correct if bare ever stops
meaning the window. Launcher stdio is beside the point either way, and every launcher measured here
hands over none: glib routes the launch through the session bus, so the child's stdio is the user
manager's.

`StartupWMClass` because the window's app id comes from the `AppId` pragma at `ui/boot/shell.qml:1` and
is not the binary name. `packaging/flea-package-test` reads both and fails if they drift apart.

## Proving it

```
printf '%s\n' "$OMARCHY_SUDO_PASS" | packaging/flea-package-test
```

Builds the package, installs it into a fake root, checks the fake root holds exactly the file count
the archive declares, removes it, and checks nothing survives. Every write is inside a `mktemp -d`
the shared guard has cleared; pacman is confined by `--root` and `--dbpath`, and the one root
`rm -rf` runs on a path `sandbox_require` has just checked. Without a password on stdin the round
trip is skipped and the rest still runs.

## Which file manager answers "Show in folder"

Ask the box, and it names the file it is reading the answer out of:

```bash
journalctl --user -b | grep "duplicate name 'org.freedesktop.FileManager1'"
```

Every line names a registration D-Bus threw away, so the one claimant that is not on that list is
the one answering. Run `flea --default` and Flea's own file is the one that is not on it, because
`$XDG_DATA_HOME` is read before `/usr/share`; run `flea --default off` and it goes back to whichever
packaged registration is read first. Removing the file manager you are not using works too, and is
the only thing that changes the answer for a user who never runs `flea --default`.
