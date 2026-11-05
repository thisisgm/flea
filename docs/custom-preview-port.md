# Custom preview port

Base: Flea v0.4.0, commit `22fa92223d7b53d8efd01f31bfebfd42d0fb5e0e`.
Branch: `custom/preview-040`.

The port keeps Flea's GPUI readers and its TUI preview pipeline.
It does not carry the old QML fixes into the new interface.
The old checkout at `/home/adam/Code/flea-custom` stays available for rollback.

## Launch

- GUI: `tools/flea-custom --gui [folder]`
- TUI: `tools/flea-custom --tui [folder]`
- Standard v0.4.0 behavior: `target/release/flea --gui [folder]` without `FLEA_CUSTOM_PREVIEW`.

The launcher enables `FLEA_CUSTOM_PREVIEW=1`.
The two release binaries must stay beside each other in `target/release`.
The desktop entry and `~/.local/bin/flea-custom` now use this launcher.
The previous desktop entry is saved under `~/.local/state/flea-custom/backups`.

## Controls

| Control | GUI | TUI |
| --- | --- | --- |
| Space on a file | Open preview | Open preview |
| Space on a folder | Preview its files | Preview its files |
| Arrows or J/K | Previous or next file | Previous or next file |
| I | Toggle info panel | Toggle file facts |
| T | Toggle image filmstrip | Toggle file name strip |
| Space on media | Play or pause | Play or pause |
| P | Play or pause | Play or pause |
| M | Toggle mute | Toggle mute |
| H/L on media | Native seek actions | Seek 5 seconds |
| Enter | Open the file in its default app | Open the file in its default app |
| Escape | Close preview | Close preview |
| F11 | Toggle preview window fullscreen | Use the terminal's fullscreen control |

The GUI opens Quick Look in a separate window at 80 percent of the display size.
Closing it keeps the browser cursor in place.
The TUI restores its original folder and cursor when the preview closes.
The TUI uses a text strip. It retains the native terminal image and media support.

## Settings

Settings > Preview includes the custom Quick Look switches.
The saved `preview` fields are shared with the TUI:

- `preloadAdjacent`: prepare the two adjacent GUI images for first paint.
- `mediaControls`: show media controls.
- `infoPanel`: open file info with the preview.
- `hideNavbarFullscreen`: hide the GUI toolbar in fullscreen.
- `navbarHideMs`: set the GUI toolbar delay, from 1000 to 10000 milliseconds.

The native decoder owns final image quality.
Thumbnail work stays inside the visible strip.
Preload work stays inside the two adjacent files.
Folder peeks use the existing bounded backend request.

## Validation

- `cargo check --locked --workspace` passed.
- `cargo build --locked --offline --release --workspace` passed.
- Five focused GUI custom preview tests passed.
- Three focused TUI and backend tests passed.
- Live checks passed for image preview, GUI navigation, fullscreen, PDF preview, TUI navigation, and cursor restore.
- `git diff --check` and the file budget check passed.

The release profile disables GPUI debug selectors.
Selector based GUI tests need a debug test build.

## Plugin scope

Flea 0.4.0 preview plugins return text or PNG output for unsupported file types.
They cannot replace Quick Look, draw its chrome, or create this preview window.
The port therefore stays in the core.
