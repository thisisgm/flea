## The key table is generated

`keys.toml` at the repository root defines the shared key and pointer tables.
Running `tools/flea-keymap-gen` without arguments generates `ui/js/Keymap.js` for the QML
reference frontend and `src/keys_generated.rs` for the Rust key API. It also refreshes the
`keys.*` entries in `i18n/en/flea.json`. Change the table and regenerate these outputs rather
than editing generated bindings by hand. The QML-specific notes below describe that reference
frontend; the Rust and TUI consumers are described at the end of this chapter.

`docs/images/glyphs.svg` is generated the same way, by `tools/flea-glyph-sheet` from the `PATHS`
table in `ui/js/Icons.js`. It has no diff guard, so re-run the tool whenever a mark joins or
leaves that table: the sheet kept drawing `send` twelve minutes after `e4b4911` took it out, and
never drew `drive`, `filter` or `lock` at all.

`./tests/keymap-gen.sh` regenerates JavaScript and Rust into a temporary directory and
compares them with `ui/js/Keymap.js` and `src/keys_generated.rs`, reporting stale output.
The same script hands every emitted `Qt.Key_*` to a `qml6` probe and fails naming any that
is `undefined`; the generator also rejects invalid key names before emission.
`tests/js/keymap.js`, run by `./tests/js.sh`, asserts lookup behavior. These checks cover the
cases they assert, not every possible binding or frontend input path.

The generator is Python and parses `keys.toml` with `tomllib`; `tools/keymap_rust.py` supplies
validation, Rust emission and catalog entries. The JavaScript output carries binding data and
lookup functions. Preset rows are checked before shared rows, with context and frontend filters.
Text rows match the character with no modifier except optional Shift, so `G` remains distinct
from `g`. Other rows match the key code and modifier mask exactly, including `[[ctrl]]` and
`[[shift]]` rows; an unbound Ctrl chord therefore cannot match a plain text binding.

A `[[pointer]]` table joined this on 2026-09-02, when a single left click stopped opening a
row and the second tap started to. It declares what each button, modifier and tap count does
in five places, and the generator emits it as `Keymap.POINTER`: the listing, the columns view's
two neighbour columns, the rail, the `window` itself and the `chrome`. `ui/js/Tap.js` is the
only code that decides the first three and `tests/js/tap.js` drives every listing and neighbour
row of the table through that file, so neither side can move without the other. `tests/ui.sh`
case `click` then drives real clicks at the window, which is the half a JavaScript suite cannot
reach: it is what says a delegate hands `Tap.tapped` the tap count and the modifiers the click
actually carried.

The last two places landed with issues 20 and 45 and are not `Tap.js`'s. `window` is the mouse's
two side buttons, which belong to no row: `ui/WindowBody.qml` carries one handler for both, `ui/js/Nav.js`
`mouseBack` decides between the history and the climb and `ui/js/MouseNav.js` `forward` hands a press
to `Pane.goForward`, which is the key's own entry and so refuses in Trash and in Recent. Both go
through `MouseNav.refused`, the context-menu and collision-card refusal. `chrome` is the path above the listing and,
in the dual view, each pane's own path, whose segments `ui/Crumb.qml` draws as their own click
targets for both, placed by `ui/ChromeBar.qml` and by `ui/PanePath.qml`, from `ui/js/Crumbs.js`.
Until 0.3.2 a dual pane's path was one `Text` answering only the double click, so a tap on a parent
there did nothing, measured on 0.3.1 and on the 0.3.2 candidate after the second report on issue 45;
the `row` column reads `parent` there because the leaf is the directory already listed and answers
no single click. Neither `where` has a `pointercase_` driver in `tools/flea-acceptance-drive`, so
both report as derived and undriven in that battery; `tests/js/tap.js` holds their counts and drives
neither, because both are QML bindings rather than `Tap.js` calls, and **`tests/js` is structurally
unable to press either one**. `tests/ui.sh` case `click` is what presses them at the real window,
and case `dual` presses a pane's crumb from `paneCrumbCentre`, then double clicks that pane's path
in its padding, past its last crumb and on its own folder's crumb from `panePathRect`. A pane's path builds no crumbs while it is hidden,
because the single view keeps it at height 0 and rule 6 is exactly that cost.

The `Qt.BackButton` binding is now driven at the product, and the probe that used to stand in for it
is the reason to say what changed. That probe was a standalone Quickshell `FloatingWindow` carrying
nothing but a `TapHandler`, clicked with `ydotool click 0xC3`: it shows Hyprland delivering that
button to a Quickshell surface and Qt reporting it as `Qt.BackButton`, and it never loaded the
shipped tree, whose `ListView`, delegates, rail and overlays all sit under the same handler.
`grep -rn acceptedButtons ui/*.qml` finds no child that accepts the button, so nothing should take
it first. `tests/ui.sh` case `click` now closes that gap the way this paragraph said it had to:
after the crumb press it parks the pointer over a listing row with `omarchy-drive move`, sends
`ydotool click 0xC3` twice, and reads the path back through the IPC seam both times, so the history
branch and the climb branch of `mouseBack` are each pressed through the shipped tree. The crumb half
is pressed the same way, from `crumbCentre`, which is the seam `ui/Ipc.qml` grew for it. Between the
two back presses the case sends `ydotool click 0xC4`, button 4, which Qt reports as `Qt.ForwardButton`,
requires the path forward again, and backs once more so the climb still finds the history spent;
`tests/js/navmouse.js` drives `MouseNav.forward` and the window's own handler (sliced from `ui/WindowBody.qml`, every
overlay in turn, Trash and Recent), because `tests/js/nav.js` is at its hard cap.

A crumb click answers on the first tap (decided 2026-09-22). `ui/Crumb.qml` used to carry
`exclusiveSignals: TapHandler.SingleTap | TapHandler.DoubleTap`, which makes the tap count decide by
holding `singleTapped` until the double-click interval expires: measured on this box, 392 to 433 ms
after the press against Qt's 400 ms interval, on every crumb click. Nobody double-clicks a
breadcrumb, so a parent now opens at once, a double click on a parent is simply two taps, and the
double click that types the path lives only where a single tap opens nothing: the leaf segment, the
collapsed marker and the strip beside the crumbs, which `keys.toml` calls `inert`. Ctrl+L and `:` still
open the bar anywhere. `tests/ui.sh` case `click` double clicks a parent with the backend stopped,
because the path is written only when a listing answers: a listing landing between the two taps
rebuilds the crumbs and hands the second tap to a fresh delegate, which would pass with the guard in
`onDoubleTapped` deleted. The same case double clicks the collapsed marker, the guard's other arm.

Its effect field is `does` and not `action`, and that is load bearing. `tools/flea-acceptance`
derives its whole key checklist with one `sed` for `^action = ` over this file, with no table
scoping, so an `action =` in a pointer block would put an item on that checklist that no key
can press. The generator refuses an empty or unreadable `[[pointer]]` table for the same
reason `[digits]` is expanded by hand in that battery: a checklist derived from an empty table
passes by having nothing in it.

A `[[preset]]` table joined this with the settings panel's Keys section, and it is the whole of
the four-value chooser SettingsKeys.html draws. Each row names the preset it belongs to, the modifier state, the Qt key,
the chord as the Keys section prints it, the action and a label. The generator emits
`Keymap.PRESET_KEYS`, which the settings panel lists and
`Keymap.presetMatch` searches before the shared table in `lookupFor`. Using the same rows
for display and lookup keeps their bindings aligned, and emitting
the binding as a literal `Qt.Key_*` is what keeps it inside the `qml6` probe above.

The live preset is a module-level `var preset` in the generated file, set by `ui/ViewState.qml`'s
`onKeysPresetChanged`. A `.pragma library` holds one copy per QML engine, so that one assignment
reaches every caller of `lookup()` with no second wire and no plumbing through `ui/js/Focus.js`
or `ui/Pane.qml`, both of which sit against their file budget. Every one of the four presets owns a
row in that table. SettingsKeys.html's inventory gives `default` and `vim` a dash on the three view
rows, but view switching is bound in no shared table, so a Default that overlaid nothing shipped
with no keyboard route to the views at all; since 2026-09-06 both carry the Mac spelling,
`ctrl-1`, `ctrl-2` and `ctrl-3`, which is safe because only one preset is ever live. Every other row
a preset governs is already a bare shared key. `ui/ViewState.qml` is the validator: it resolves a
stored name that is not in `Settings.PRESETS` to the first of them, `default`, so an unknown name,
which would match no overlay row, never reaches the generated module.

**A chord claimed twice inside one preset fails the build.** That is SettingsKeys.html's "Conflicts
fail the build", and until this change it was not true: a second `mac` `ctrl-1` claiming `viewGrid`
emitted two rows into `lookupPreset`'s if-chain, exited 0, and let the first silently win.
`preset_conflict` keys on the preset with the modifier state and the Qt key, never the printed
chord, because that is what the if-chain matches, and it names the preset, the chord and both
actions. Two presets may hold the same chord, because only one is ever live. The tool takes the
table to read as its second argument, so `tests/keymap-gen.sh` proves the refusal against a broken
copy in its own probe directory rather than by editing the table this repo ships.

**A preset overlay must never hold a chord the `[[sheet]]` table draws.** The sheet is one static
array with no preset of its own, so a cap it draws for a Mac-only chord would be wrong for half
its readers the moment the toggle moved. `tests/js/keymap.js` resolves the whole sheet under both
presets and fails if any row answers differently.

**A mirrored pair shows the same number of spellings on the sheet.** `sheetFor` adds a second spelling while the cap
fits `SHEET_CAP_BUDGET`, which gave next tab `]` beside prev tab `[ / ctrl-pageup`. The generator's `SHEET_MIRRORS`
(cursor, page, history, tab, tab move, zoom, text size and extend pairs) now shows a second spelling on both members or
on neither; Windows Page Up loses `pageup` beside `ctrl-u` because ctrl-d is not bound there. Pairs that are not
mirrors by data, undo and redo, copy and cut, browse in and parent, are left alone. `tests/js/keymap.js` asserts the
pairs across every preset and both frontends.

`src/keys.rs` includes and re-exports `src/keys_generated.rs` as `flea::keys`. The GPUI window
uses that API for keyboard lookup in `crates/flea-gui/src/window/keyboard.rs`. The TUI's
`src/tui/keymap.rs` instead parses the embedded `keys.toml` through `include_str!`, including
its terminal-specific `[[tui]]` rows; `src/tui/key_sheet.rs` uses `flea::keys::display` for
keycap labels. Generating Rust bindings does not replace the TUI's map parser.

For temporary output, pass a JavaScript destination as the first argument and use
`--rust <path>` for Rust. A second positional argument selects an alternate input table;
`--catalog <path>` selects the catalog to refresh. With an explicit JavaScript destination,
Rust and catalog files are written only when their options are supplied.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
