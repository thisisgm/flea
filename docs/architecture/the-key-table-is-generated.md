## The key table is generated

`keys.toml` at the repository root is the single source of truth for every binding.
`ui/js/Keymap.js` is generated from it by `tools/flea-keymap-gen` and must not be hand
edited: change `keys.toml`, run the tool, commit its output.

`docs/images/glyphs.svg` is generated the same way, by `tools/flea-glyph-sheet` from the `PATHS`
table in `ui/js/Icons.js`. It has no diff guard, so re-run the tool whenever a mark joins or
leaves that table: the sheet kept drawing `send` twelve minutes after `e4b4911` took it out, and
never drew `drive`, `filter` or `lock` at all.

Three guards cover it, and each catches what the others cannot. `./tests/keymap-gen.sh`
regenerates into a temp file and diffs that against the committed one, so an edit to
`keys.toml` with no regeneration and a hand edit to `Keymap.js` both turn it red, and the
diff names the stale file either way. The same script then hands every `Qt.Key_*` the
generator emitted to a `qml6` probe and fails naming any that is `undefined`, because a
mistyped `key = "Downn"` generates `key === Qt.Key_Downn`, which is false forever and diffs
perfectly clean. `tests/js/keymap.js`, run by `./tests/js.sh`, asserts the lookups
themselves, so it catches a binding wired to the wrong action; it is a hand-maintained list,
so it says nothing about a binding nobody wrote an assertion for.

The reasoning the hand-written `Keymap.js` used to carry lives in `keys.toml` now, one
comment above the block it explains: control-modified keys are checked first, so plain `d`
can mean cut while ctrl-d pages, and the character is authoritative for letters, so shift-g
is distinguishable from `g`. The generated file carries only the header saying where it came
from.

A `[[shift]]` table joined this for Task 9's shift+arrow selection extend. Unlike `[[ctrl]]`,
whose block returns `""` on no match so an unbound ctrl-combo never falls through to a plain
binding, the generated `[[shift]]` block has no such return: a shift-modified letter is
already distinguished by its own capital text (the `G` idiom above), so an unmatched
shift+key needs to fall through to the ordinary key and text switches rather than being
swallowed. The table exists only to give the arrow keys, which carry no text at all, a
shift-modified meaning.

The generator's block parser is a plain `while read` loop over `name = value` lines, not the
multi-statement `awk` the plan sketched, because the project's code rules send a
multi-statement `awk` back for exactly that rewrite and the loop came out shorter and needs
no second language. Do not restore the `awk`.

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
the chord as the Keys section prints it, the action and a label. The generator emits it twice
from that one row: as `Keymap.PRESET_KEYS`, which the settings panel lists, and as the if-chain
inside `Keymap.lookupPreset`, which `lookup()` consults before every shared table. Emitting both
from one row is what stops the panel advertising a chord the preset does not bind, and emitting
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

The tool emits JavaScript only. Plan 6 adds the Rust output together with the terminal key
type that consumes it; a generated module with no caller is dead code, so the second output
waits for its consumer.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
