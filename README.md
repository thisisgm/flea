<p align="center">
  <img src="docs/images/icon.svg" width="72" height="72" alt="Flea">
</p>

<h1 align="center">Flea</h1>

<p align="center">
  <strong>The fastest GUI file manager on Linux.</strong><br>
  Keyboard-first. Built for Omarchy.
</p>

<p align="center">
  <a href="https://github.com/tcballard/omarchy-badges"><img src="https://raw.githubusercontent.com/tcballard/omarchy-badges/85f859029e236e784e7b05ada6dbe73506d07a91/badges/v1/built-for-omarchy.svg" alt="Built for Omarchy"></a>
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#views">Views</a> ·
  <a href="#performance">Performance</a> ·
  <a href="#keyboard">Keyboard</a> ·
  <a href="https://github.com/thisisgm/flea/releases">Releases</a>
</p>

<p align="center">
  <img src="docs/images/release-0.3.0-themes.gif" alt="Flea 0.3.0 cycling through ten Omarchy themes, with the iPhone on the Devices rail and a photo in the preview column">
</p>

Flea combines a Rust backend with a Quickshell interface that follows your Omarchy theme.
Large directories stay responsive: the window loads rows and requests thumbnails as you need them.

- **Three views.** List, columns and grid, with tabs and natural filename sorting.
- **Quick Look.** Press Space for images, PDFs, text, media and archive contents.
- **File operations.** Copy, move, rename, trash, compress and extract, with an undo journal.
- **Network and sharing.** SMB, SFTP, FTPS, WebDAV, NFS, Dropbox and Taildrop.
- **Devices.** USB drives, phones and cameras over MTP, PTP and AFC, mounted from the rail.
- **Desktop integration.** Default file manager, “Show in folder” and Open/Save dialogs.
- **Your settings.** Omarchy text sizes, configurable menus and Default, Vim, Mac or Windows keys.

An iPhone lists once it has been unlocked and trusted on this machine: AFC needs the pairing record
that leaves behind, not an unlocked screen every time. Pairing also puts the phone on its tethering
USB configuration, so iOS shows Personal Hotspot as active while it is plugged in, and Flea never
uses it. If this machine should never route through the phone, tell NetworkManager so with a
`conf.d` drop-in carrying `unmanaged-devices=driver:ipheth`; that is your call, not the package's.

## Install

```bash
omarchy pkg add flea
```

Make Flea your default file manager, file chooser and the app opened by **Super+Shift+F**:

```bash
flea --default
systemctl --user restart xdg-desktop-portal
```

Update through Omarchy:

```bash
omarchy update
```

<details>
<summary>File chooser only, development package and removal</summary>

Use Flea for portal Open/Save dialogs without changing your default file manager:

```bash
flea --picker
systemctl --user restart xdg-desktop-portal
```

For the rolling development build:

```bash
omarchy pkg aur add flea-git
```

Before removing Flea, undo its desktop integration:

```bash
flea --default off
omarchy pkg drop flea
```

If you previously pinned another directory handler, restore that handler explicitly.
[Installation details](docs/install.md) cover dependencies, desktop integration and removal.

</details>

## Views

**List** for file details. **Grid** for photos and clips. **Columns** for browsing folders with a preview beside them.

<p align="center">
  <img src="docs/images/grid.png" width="49%" alt="Grid view with image and video thumbnails">
  <img src="docs/images/list.png" width="49%" alt="Columns view with the preview column, file details and the iPhone on the Devices rail">
</p>

**Space** opens Quick Look from any view. Page through a PDF, play media or inspect an archive.

<p align="center">
  <img src="docs/images/pdf.png" alt="PDF preview with page navigation">
</p>

**The shelf** lives in the Omarchy bar: send files there from any menu, drag them out into any app, pin the ones you keep coming back to. **Phones** are Devices rows over MTP, AFC and PTP.

<p align="center">
  <img src="docs/images/shelf.png" width="49%" alt="The shelf card open over Flea, with thumbnails, pins and the last three screenshots">
  <img src="docs/images/iphone.png" width="49%" alt="An iPhone camera roll browsed over AFC, with HEIC thumbnails and a preview">
</p>

## Performance

Measured on **22 September 2026**, using the **0.3.2** source tree.
Cold caches, three runs per app, medians below. Seven GUI apps on the same Omarchy machine.

Flea's memory does not grow with the directory: first at 100,000 entries, mid-field at 2,000.
At 100,000 files Flea's window used 70.5 MiB against Strata's 107.9 MiB, the next lightest; on
2,000 media files it used 83.4 MiB, fifth of seven.

### 100,000 files

Flea led every column of this run.

| File manager | First window | Settled | Window PSS | CPU |
|---|---:|---:|---:|---:|
| **Flea** | 277 ms | 1.13 s | 70.5 MiB | 0.73 s |
| Strata | 576 ms | 3.23 s | 107.9 MiB | 3.15 s |
| Dolphin | 715 ms | 5.26 s | 198.6 MiB | 9.18 s |
| Nemo | 792 ms | 22.60 s | 459.8 MiB | 26.01 s |
| Thunar | 561 ms | 23.06 s | 346.3 MiB | 22.88 s |
| PCManFM | 449 ms | 36.13 s | 109.9 MiB | 25.01 s |
| Nautilus | 869 ms | 83.80 s | 333.6 MiB | 18.80 s |

### 2,000 mixed files

The apps produced different amounts of thumbnail work, so settled time and CPU are
**not ranked** across the whole table. Flea and Strata made the same work, 43 and 42
thumbnails, and Flea settled in 1.92 s against 2.87 s on less CPU.

| File manager | First window | Settled | Window PSS | CPU | Thumbnails |
|---|---:|---:|---:|---:|---:|
| **Flea** | 296 ms | 1.92 s | 83.4 MiB | 6.02 s | 43 |
| Dolphin | 724 ms | 13.71 s | 92.2 MiB | 66.70 s | 500 |
| Nautilus | 856 ms | 19.85 s | 180.4 MiB | 122.70 s | 641 |
| Nemo | 855 ms | 3.45 s | 54.8 MiB | 8.97 s | 60 |
| PCManFM | 421 ms | Timed out | 39.6 MiB | 158.35 s | 690 |
| Strata | 557 ms | 2.87 s | 73.8 MiB | 7.13 s | 42 |
| Thunar | 536 ms | 12.81 s | 38.8 MiB | 20.58 s | 221 |

### 1,700 matched files

The same set less HEIC and MKV, the two formats some of these apps decline, so none of them
finishes sooner by skipping a format. Work still differs by app, so the same rule holds. Nemo left
nothing in the thumbnail cache on this set, so its work was not measured and its time is not a
result. Flea and Strata again made the same work, and Flea settled in 1.93 s against 2.93 s. One
of Flea's three runs mapped in 3.7 s, and three more, Flea alone, mapped in 267 to 329 ms.

| File manager | First window | Settled | Window PSS | CPU | Thumbnails |
|---|---:|---:|---:|---:|---:|
| **Flea** | 296 ms | 1.93 s | 82.0 MiB | 6.39 s | 43 |
| Dolphin | 695 ms | 14.18 s | 92.9 MiB | 65.20 s | 500 |
| Nautilus | 945 ms | 18.88 s | 179.8 MiB | 115.52 s | 641 |
| Nemo | 814 ms | 1.74 s | 46.4 MiB | 1.58 s | not measured |
| PCManFM | 421 ms | 113.50 s | 41.5 MiB | 178.95 s | 700 |
| Strata | 570 ms | 2.93 s | 74.1 MiB | 7.15 s | 42 |
| Thunar | 551 ms | 13.25 s | 40.5 MiB | 20.79 s | 221 |

"First window" records compositor registration. "Settled" means 500 ms without process-tree
CPU activity; neither measures input readiness.
PSS covers the window process, excluding helpers and GPU memory. Flea's separate backend used a
median **6.4 MiB** on the large listing and **3.1 MiB** on media. CPU is everything the app
started, counted from a control group of its own, so sandboxed thumbnailers are in it; a figure
from before 0.3.2 counted only reaped children and is not comparable. Shared libraries affect PSS.
PCManFM did not settle in any media run. Results describe this machine and workload.

[Method](docs/benchmarks.md) · [Scale CSV](docs/bench/scale-0.3.2-20260922.csv) ·
[Scale manifest](docs/bench/scale-0.3.2-20260922.manifest.md) ·
[Media CSV](docs/bench/media-0.3.2-20260922.csv) ·
[Media manifest](docs/bench/media-0.3.2-20260922.manifest.md) ·
[Matched CSV](docs/bench/matched-0.3.2-20260922.csv) ·
[Matched manifest](docs/bench/matched-0.3.2-20260922.manifest.md) ·
[0.1.6 run and TUI results](docs/bench/results-0.1.6.md)

## Keyboard

Press **?** for the full keymap, or **,** to change settings.

| Action | Keys |
|---|---|
| Move / parent / enter | `j` `k` / `h` / `l` |
| Open with the default app | `Enter` |
| Quick Look | `Space` |
| Select / extend selection | `v` / `Shift` + arrows |
| Copy / cut / paste | `y` `x` `p`, or `Ctrl+C` `Ctrl+X` `Ctrl+V` |
| Copy selected file path | `Ctrl+Shift+C` |
| Rename / trash / undo | `r` or `F2` / `dd` or `Delete` / `z` |
| New folder | `Ctrl+Shift+N` |
| Search / filter the list | `f` / `/` |
| Enter a path | `:` or `Ctrl+L` |
| List / columns / grid | `Ctrl+1` / `Ctrl+2` / `Ctrl+3` |
| New tab / close tab / switch tab | `t` / `w` / `1`–`9` |
| Open terminal / context menu | `Ctrl+T` / `m` |
| Show hidden files | `.` |

The Windows preset uses `Ctrl+Shift+1/2/3` for views. Menu visibility does not disable shortcuts.
See [the full key table](keys.toml) for preset bindings and pointer actions.

## Build

Requires Omarchy, Quickshell 0.3.1 or newer, and Rust. See
[installation details](docs/install.md) for runtime and optional dependencies.

```bash
cargo build --release
FLEA_UI="$PWD/ui" ./target/release/flea --gui
```

Pass a directory to open it, or `--select <path>` to reveal a file. The terminal interface
is not implemented yet.

```bash
./tests/run-all.sh
```

The headless runner builds both profiles and reports suites that need a display, credentials
or a package. [The test suites](tests/) cover file operations, previews, persistence and input;
[the protocol guide](docs/protocol.md) documents the backend.

## Support

If this saved you an afternoon, you can
[sponsor me on GitHub](https://github.com/sponsors/thisisgm) or
[buy me a coffee](https://buymeacoffee.com/thisisgm).

## Licence

[MIT](LICENSE).
