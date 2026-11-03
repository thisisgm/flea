# Flea shelf

A drop shelf for the [Omarchy](https://omarchy.org) bar. Files gathered from anywhere are held in
one pile and taken out together, so a copy that spans five folders is one drag rather than five.

The shelf is a bar widget: the mark sits beside its neighbours at all times, dim while the pile is
empty and at full strength while it is holding something. The count is never drawn in the bar
itself; it is in the hover tooltip, in the card and on the screen edge, where nothing competes
with it.

## What ships today

Everything the boards draw: the bar presence, the card the mark opens, the sizes it asks for row by
row, the row's own remove, the tray of recent captures, the ways back in (the keybind, a cleared pile
and the last five piles), the screen-edge rail you throw a drag at, the drag out of the card, the
subset gesture, and the six actions.

| Piece | What it is |
|---|---|
| `manifest.json` | the plugin the shell loads, one bar widget and its settings |
| `Panel.qml` | the bar presence, and the card hosted in the shell's own popup host |
| `ShelfService.qml` | the only thing that touches the outside world: the pile's file and `flea shelf` |
| `Model.js` | pure functions, no QML imports: what the file's bytes mean |
| `ShelfCard.qml` | the card: the pile, the pins and the captures as one list of OEM panel rows |
| `ShelfMenu.qml` | the card's own menu: the last five piles, and the way back to one |
| `ShelfRail.qml` | the screen edge you throw a drag at, and the notch that says what is held |
| `ShelfActions.qml` | the doing half: every action, what it says, and the rows a flyout offers |
| `ShelfFlyout.qml` | one flyout, two verbs and the send: places and peers, numbered |
| `ShelfRun.qml` | the transfer surface while an action runs, and its cancel |
| `Run.js` | pure functions again: the running line, and the sentence each action lands with |
| `ShelfActionButton.qml` | Ui/PanelActionButton's shape with a mark on the Omarchy cut as its ink |
| `ShelfRow.qml` | one row of the card: its group's separator and header, then the row itself |
| `ShelfGlyph.qml` | one mark on the Omarchy cut, the way Flea draws its own |
| `ShelfCheck.qml` | the box a row is marked with, filled rather than tinted |
| `ShelfTailscaleMark.qml` | Tailscale's own mark, because Send is Taildrop |
| `FleaShelfMark.qml` | Flea's own mark, reproduced rather than recut |

## Requirements

- Omarchy with `omarchy-shell` (the plugin runs inside the shell, not as its own process).
- [Flea](https://github.com/thisisgm/flea) 0.3.0 or newer, which writes the pile.

## Settings

| Key | What it does | Default |
|---|---|---|
| `refreshIntervalSec` | how often the pile is re-read when no change has been signalled | 5 |
| `fleaCommand` | the flea that owns the pile, a name on PATH or a path of its own | `flea` |

The pile's file is watched, so a change is drawn as it happens; the interval is what finds the
first item, because a watch cannot fire for a file that does not exist yet.

The rail is four pixels of the middle 60 percent of its edge, drawn only while something is held (a
notch stepped by the pile) and while a drag is over it (the whole region in the accent). It gives up
no space: no window ever shrinks for it. A drop on it is taken the moment it lands; the dwell only
decides whether the card has opened on the way.

The card is an Omarchy panel, hosted the way every other panel is: a click outside it, a focus loss,
`esc` or another panel opening closes it. It is one list of three sections, the pile, `Pinned` and
`Screenshots & recordings`, each a one-line row with its mark, its name, its size and, while the
pointer is on it, a Copy to Clipboard button, and the six
actions as icon buttons under the last separator, one of which, Send, is absent on a box with no
Tailscale. A row whose file has a thumbnail in the
freedesktop cache draws it in the mark slot instead of the kind mark; the cache is Flea's own, asked
for by path through `flea shelf thumb`, only for the rows the card is drawing and never while it is
closed. A row is also the handle it is carried out by: press it and drag, and the drag carries the marked
rows, or that one row when none are marked. The pointer marks the way Flea's own listing does: a
row's box appears under the pointer and while anything is marked, a click on the box toggles that
row, ctrl and a click toggles it too, shift and a click takes the range from the last row clicked,
and a plain click clears the marks. An empty shelf says one word, `Empty`.

The captures are the newest few Omarchy has taken, from the directories its own capture commands
write to (`OMARCHY_SCREENSHOT_DIR`, else `XDG_PICTURES_DIR`, else `~/Pictures`;
`OMARCHY_SCREENRECORD_DIR`, else `XDG_VIDEOS_DIR`, else `~/Videos`), listed before the card's first
frame and re-read every second while it is up. A capture row takes the actions and the subset
gesture like any other, and `p` pins one. A pinned row is always there: its drag out is a copy, a
move takes the file and the pin follows it, and `x` is what takes it off the shelf.

Every switch is Flea's own, in its Settings panel under Shelf: the master, `Show in bar`, the edge
rail, which capture kinds to list and how many, and the pinned rows. This plugin reads them from
Flea's `ui.json` and never writes them, and its manifest keeps only the two technical values above.
With the master off there is no bar mark, no rail and Super+D does nothing, and the pile is kept. An action's tooltip carries its key only while Flea's own
keyboard-hints setting is on.

## State

`$XDG_STATE_HOME/omarchy/flea-shelf/`, or `~/.local/state/omarchy/flea-shelf/` when that is unset:
`shelf.json` is the pile, `piles.json` the last five it has held, `drags.json` the tokens a drag out
carries, `undo.json` the one move `z` can walk back, and `summon.json` the count the keybind writes.
The plugin never writes any of them: `flea shelf` does, and the plugin watches.

## Keyboard

| Key | What it does |
|---|---|
| `super + d` | opens the shelf, or closes it, once you have installed the bind below |
| `esc` | closes the card, and steps out of its menu |
| `j` `k` | down, up. The arrows do the same |
| `v` | takes the cursor row, and a second `v` gives it back |
| `shift + j` `shift + k` | extend the range |
| `ctrl + a` | takes every row, and a second `ctrl + a` clears |
| `x` | takes the cursor row off the shelf. The file is untouched |
| `enter` | opens the file, or reveals the folder in Flea |
| `tab` | jumps to the action buttons, and back; the arrows walk them and `enter` runs one |
| `m` `c` | move or copy the chosen rows, or the whole pile, to a folder |
| `a` | zip them into one archive, which lands on the shelf |
| `t` | send them with Taildrop, on a box that has it |
| `p` | pins the chosen rows, or the cursor row, and unpins a pinned one |
| `y` | yank the paths to the clipboard |
| `shift + c` | copy the files themselves to the clipboard: the chosen rows, or the cursor row when none are chosen |
| `1` to `9` | in a flyout, take that destination or that peer |
| `shift + x` | clears the shelf, and it becomes the last pile |
| `z` | undo: puts the last move back where it came from, or brings the last cleared pile back, whichever happened later |
| `1` to `5` | in the menu, takes that pile back |
| right click | on the bar mark: brings back the last pile you cleared; on the card: its menu |

While an action runs, the card is its transfer surface and `esc` cancels it rather than closing the
card. Every file action is chosen-or-whole: with rows chosen it takes those, with none chosen it takes
the pile, and the card's own line says which while a subset is being chosen.

Copy to Clipboard puts the files on the clipboard the way Flea's own copy does, so a file manager
pastes copies of them. A single regular file of at most 32 MiB also offers its own bytes under its
type, unconverted, so a chat app or a browser pastes the file (a PNG screenshot pastes as a picture);
several files paste as files only where the target accepts files. The row's button and `shift + c`
take the chosen rows, or that one row when none are chosen, the way a drag does, and never the whole
pile. While any row is chosen, Copy to Clipboard also leads the action strip and copies
those rows.

An action's tooltip names its key, and only while Flea's keyboard-hints setting is on: the strip
draws no legend of its own, and this table is the map.

The bind is a line in your own Hyprland config, not one this plugin writes. Add it to
`~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + D", "Drop shelf", "flea shelf toggle")
```

The empty card names that chord once the line is there, and says nothing about it while it is not.

## Install

The shelf ships inside Flea, so there is nothing to clone. Install Flea, open its Settings, and turn
on **Enable shelf**: Flea copies the plugin into `~/.config/omarchy/plugins/` and enables it, and the
mark appears on the bar. Turning the switch off disables it again and leaves the pile where it is.

The shelf is bundled with Flea; enable it through Flea’s Settings. It is not a separate marketplace package.

## Uninstall

Turn **Enable shelf** off in Flea's Settings. To remove the copy as well:

```bash
omarchy plugin remove io.github.thisisgm.flea-shelf
rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/omarchy/flea-shelf"
```

That directory is the pile itself and the drag tokens Flea mints for it, so removing it empties the
shelf. Flea and the files the shelf was holding are untouched.

## Support

If this saved you an afternoon, you can
[sponsor me on GitHub](https://github.com/sponsors/thisisgm) or
[buy me a coffee](https://buymeacoffee.com/thisisgm).

## Licence

MIT.
