# Installing Flea

Flea is for Omarchy: `omarchy` and `quickshell` are hard dependencies, so it will not install on a
plain Arch box.

Flea installs as an Arch package, so pacman owns both ends: `makepkg -si` puts it on, `pacman -Rns`
takes it off, and pacman's own file list is what makes the second claim provable. There is no
install script here because there is nothing for one to do. The steps pacman cannot own are
per-user preferences, and they are subcommands of the binary: `flea --default` makes Flea your
default file manager and routes the desktop's file chooser to it, and `flea --picker` does that
second half alone. Both are described below.

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
| `/usr/share/flea/ui/` | the Quickshell UI, which `paths.rs` looks for by `shell.qml` |
| `/usr/share/flea/ui/Commons`, `/usr/share/flea/ui/Ui` | symlinks into `/usr/share/omarchy/shell/`, reached from QML as `qs.Commons` |
| `/usr/lib/flea/flea-portal` | the XDG portal backend, which answers `org.freedesktop.impl.portal.FileChooser` |
| `/usr/share/xdg-desktop-portal/portals/flea.portal` | what registers that backend with xdg-desktop-portal |
| `/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.flea.service` | what D-Bus activates it with |
| `/usr/share/applications/com.thisisgm.flea.desktop` | the desktop entry |
| `/usr/share/icons/hicolor/scalable/apps/com.thisisgm.flea.svg` | the icon |
| `/usr/share/licenses/flea/LICENSE` | the licence |

The count is whatever the built archive declares, not a number written down here: the UI grows a file
whenever a component is added, so a figure pinned in this paragraph would be stale by the next commit.
`packaging/flea-package-test` reads the count out of the archive and fails if the fake root does not
hold exactly that many. The two symlinks are why `omarchy` is a hard dependency: they point into a
directory that package owns.

## Uninstall

```
sudo pacman -Rns flea
```

Everything above goes, including the directories the install created. The package carries no
`.INSTALL` scriptlet, so nothing is ever created outside the file list pacman tracks, and the
desktop and icon caches are re-indexed by Arch's own `update-desktop-database` and
`gtk-update-icon-cache` hooks, which fire on Remove as well as on Install.

## Make Flea the default

Installing registers Flea for `inode/directory`; it does not make it the default, and it does not
touch Omarchy's file-manager keys. Both are per-user preferences, so pacman cannot own them, and
Omarchy's own `default` verbs (`omarchy default browser`, `editor`, `terminal`) set exactly this
kind of thing without a package's help. There is no `omarchy default filemanager`, and
`/usr/share/omarchy/` is the package's to overwrite, so Flea carries the verb itself:

```
flea --default
```

It does three things, each reported on its own line, and it is honest about state: run it twice and
the second run says every step it ran is already Flea's and rewrites nothing. It needs no root,
because every file it writes is yours, and it takes no argument, because Omarchy's `default` verbs
take one to name which program and here the program is Flea. The three files are
`~/.config/mimeapps.list`, `~/.config/hypr/bindings.lua` and
`~/.config/xdg-desktop-portal/portals.conf`.

1. **The `inode/directory` handler.** `xdg-mime default com.thisisgm.flea.desktop inode/directory`,
   the stock tool, which writes one line to `~/.config/mimeapps.list`. The line printed names the
   previous handler, `org.gnome.Nautilus.desktop` on a stock Omarchy, which comes from
   `/usr/share/applications/mimeapps.list`. Only that one type: the entry registers nothing else,
   and a file manager that takes image or archive types is a bad citizen. The answer is read back
   with `xdg-mime query default` rather than trusted, because `xdg-mime default` exits 0 whatever it
   wrote.
2. **Omarchy's two file-manager keys.** `SUPER + SHIFT + F` and `SUPER + ALT + SHIFT + F` are bound
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

3. **The file chooser.** Everything `flea --picker` does, described in the next section. A box
   updating from 0.1.3 has a Flea with no chooser routing, and one command should finish the job.

   **This step is the conditional one, and the other two are not.** It needs
   `flea.portal`, which only the package installs. Run a binary you built with `cargo build` on a
   box whose installed package predates the chooser, which is every box updating from 0.1.3, and
   the command prints `flea: no portal backend is installed, so the file chooser step was skipped`
   and returns having done the first two. On such a box the honest-about-state line above is a
   claim about those two alone: the chooser was never claimed, so a second run cannot say it is
   already Flea's. With no Flea package installed at all, step 1 refuses first and nothing is
   written, because `com.thisisgm.flea.desktop` is the proof the package landed.

Run it from a terminal inside the session, so the keys take effect at once.

### Undo

```
flea --default off
```

Removes Flea's `inode/directory` line from `~/.config/mimeapps.list`, so the handler falls back to
whatever the system default is (Nautilus on stock Omarchy), and removes the marked block from
`~/.config/hypr/bindings.lua` byte for byte, then reloads, and undoes the file-chooser step exactly
as `flea --picker off` does. If you had pinned another handler in
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
until it is restarted, which the command's second line says:

```
systemctl --user restart xdg-desktop-portal
```

The picker that then opens is Flea: the same rows, icons, theme and keys as the window, with a check
box in front of every row a caller can receive. Space marks, Enter walks into a directory or submits
what is marked, Backspace climbs, Escape refuses. Nothing marked and Enter does nothing, because a
chooser that sends on a stray keypress is worse than one that asks twice.

### Undo

```
flea --picker off
```

removes that one key, and removes the file too when the key was all it held, and removes the
Hyprland block byte for byte. Restart xdg-desktop-portal again and the GTK chooser is back.

### What `pacman -Rns flea` leaves behind

`~/.config/xdg-desktop-portal/portals.conf` is per-user state like `mimeapps.list` above, so it stays.
With no `flea.portal` installed, xdg-desktop-portal logs that the requested backend does not exist and
takes the next name on the line, which is `gtk`, so the desktop keeps a working chooser either way.
The clean order is `flea --picker off` before `sudo pacman -Rns flea`.

## Why the Exec line reads `flea --gui %f`

`%f` because Flea's positional argument is a path. A `%u` entry advertises that the program
understands URI schemes, and Flea's positional does not: only `--select` strips a `file://` prefix
and percent-decodes. For a local directory the two field codes measure the same, both hand over one
decoded path, so the difference only shows on a remote URI, where `%u` would give Flea an
`smb://host/share` string to treat as a relative path.

`--gui` because a desktop entry always means the window. Without it the mode is inferred from
whatever stdio the launcher hands over, and while every launcher measured here hands over none (glib
routes the launch through the session bus, so the child's stdio is the user manager's), an inference
is a worse contract than a flag.

`StartupWMClass` because the window's app id comes from the `AppId` pragma at `ui/shell.qml:1` and
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

## Known gap: Open containing folder

Applications that offer "Open containing folder" ask the `org.freedesktop.FileManager1` D-Bus
interface for `ShowItems` first. Flea does not implement it, so those applications get whatever
fallback they carry; the common one, opening the parent directory through the `inode/directory`
handler, lands on Flea and works. Wiring `ShowItems` to the `--select` that already exists needs a
D-Bus service in the backend, which is not part of this packaging work.
