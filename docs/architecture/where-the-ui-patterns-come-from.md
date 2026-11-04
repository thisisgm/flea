## Where the UI patterns come from

`ui/ContextMenu.qml` is a plain overlay `Item`, not a Controls `Popup`. It fills the pane, sits
at `z: 1` above the list, and stays invisible until `openAt` places its frame and sets `opened`.
A backdrop `MouseArea` gives close-on-press-outside and the action's `Keys.onPressed` gives
close-on-escape, which together are what the popup's `closePolicy` used to provide. `Pane.qml`
reads `menu.opened` and never `menu.visible`, because a full-pane overlay's visibility is not the
menu's state.

That was the only Controls import in the tree, and it cost 10 to 12 ms of warm startup and 2.2 MB
of PSS, measured across nine interleaved pairs and slower in 9 of 9. Dropping it moved the warm
product path from about 205 ms to about 193 ms, faster in 16 of 18 pairs over two nine-pair runs
with the arms swapped between them.

Closing a popup restored focus to the item that had it for free. The overlay does not, so `openAt`
records the sibling holding active focus and `close` hands it back. Without that the list stopped
answering the keyboard after Escape, which is exactly what `./tests/ui.sh menu` catches by pressing
a cursor key after Escape. The pointer convention the menu answers is
`/usr/share/omarchy/shell/Ui/WidgetButton.qml:94-116`, where left and right arrive through one
handler and the button decides. No QML is imported from `shell/Ui`: the pointer convention was
copied, not imported. `qs.Commons` is a different matter: not a public API, but already
load-bearing tree-wide (Theme, NetworkDialog, SidebarRow, ContextMenu and Preview all import it
for Style and Color), so an `omarchy update` reshaping it is a standing, tracked risk rather
than a rule any one file breaks.

The Trash strip's Empty Trash is the one button, not a second recipe (decided 2026-10-03, after two Empty Trash
buttons were found drawing differently). `ui/TrashView.qml` instantiates `Flea.DialogButton` with `destructive: true` and
`inStrip: true`: the host's height (`Theme.chromeHeight` for the press area, `Theme.chromeControlHeight` for the frame,
centred in the strip less its rule) at caption size, a muted frame that never changes, the error label, hover 8% and
press 14% of the ink with whole pixel insets on frame and wash only, keyboard focus as its own frame in one foreground hairline, 0.55 when
disabled, and Tab in the focus chain. The strip's former own control, whose frame rose to the ink and whose focus was a wash, is deleted.
`tests/button-system.sh` pins the ladder in five states against a destructive dialog button, plus the sweep table of
the `ui/` files that declare the Button accessible role and draw a `border.width` (the control, the ruled set members,
the marks, the picker's filter trigger `PickerFilter`, and the deferred NetworkForm TLS box); a new file of that shape
fails its completeness check, and a hand-built button that spells neither is not seen by it. The retired
second recipe is searched for across the whole tree bar the changelog. The harness is destructive-capable (it drives the
real TrashView and activates Empty Trash), so the suite pins every XDG root under its marked `mktemp -d` root, the QML
proves the environment it was given and the paths the product resolved from it (the view's home, the scripts directory,
the state file, and the data home of `Recent.historyPath` with gio's `/Trash` under it, which is the one row the QML
only derives, because the backend that resolves the Trash never starts here) lie under it before anything is activated,
and assigns the view state only after that passed. No backend answers, so a `prepare` request is dropped, the
confirmation card never opens and a `delete` or `restore` request fails the run. Its in-QML cap is elapsed time, half
the outer `timeout` passed down, so the "harness did not finish" branch fires first whenever `qs` starts in under that
half; a slower start meets the outer `timeout` instead. In `tests/ui-trash.sh` each Empty Trash shot waits on
`ipc trashEmptyState` (`hovered|focused|pressed|available`), and the case hands the pointer and the keyboard back
afterwards: `ipc trashFocusListing` releases the strip button before forcing the view, because a focus scope forced alone
returns to its last child, and `tests/button-system.qml` drives that same body headless.

A held `DialogButton` keeps its native label untransformed. Its frame and wash inset each side by `Math.round(size * (1 - Buttons.PRESS_SCALE) / 2)`, independently for width and height, with int properties animated over `Buttons.PRESS_MS` using `Easing.OutCubic`. The pressed wash and inset draw only while the held point is inside the press area; dragging off returns to rest and dragging back presses again. Reduced motion draws the wash without the inset. `ChromeButton` keeps its existing scale because its mark uses `Shape.CurveRenderer`. `tests/button-press.qml` pins mapped label geometry and whole pixel frame geometry at rest, during animation and held, plus drag-off, drag-back and reduced motion at text sizes 12 and 14.

The rail has one filled row and accent edge: `Sidebar.highlightedIndex` chooses `cursorIndex` while `cursorDrawn`, otherwise `litIndex`. No rail row draws a keyboard cursor frame. Trash takes `Style.selectedAccentFill` and an accent glyph with its label in the foreground; other highlighted rows take `Style.selectedFill` while focused and `Style.normalFill` otherwise. `SidebarRow.lit` remains current-place identity for IPC, separate from `highlighted`; `Places.heldBy`, `settleLit`, open-menu suppression and the eject mark's brightening and focus frame keep their rules.

Focus on a button or mark is its own frame, never a ring and never an accent line. `ui/ChromeButton.qml` draws one
`Theme.spacing.hairline` square frame in `Theme.color.foreground` for the keyboard on every mark, the window chrome's,
the PDF viewer's in Quick Look and in the Columns preview, and the Markdown Quick Look's close alike, on every theme.
`ui/DialogButton.qml` draws focus in its `buttonFrame`, the shape `Buttons.focusFrame` names: `"foreground"`, one
foreground hairline, for every button; `"accent"`, one accent hairline, for a set member (`ui/ProtocolChip.qml`) where
the accent has a hue; and `"thick"`, the same frame `Buttons.RING` (2 px) wide in the foreground, where a hairline
would not read apart from the rest: a primary whose `accentFrame` is already the foreground, as on kanagawa, and a set
member where the accent has no hue, whose current member already rests in a foreground hairline. A border draws inside
its rectangle, so the thick frame grows inward: the button's size, position and label stay put, and the hover and
press wash (`buttonWash`) is inset by the frame's width so it never paints over it. No item draws outside a button.
Fields and chips keep the accent frame, and a focused filled `ui/CheckBox.qml` keeps its keyline. Over a strip rule
(`ruleRows`) the glyph centres with its frame in the rows above the rule, and each Quick Look strip's kind mark takes
the same centre. Quick Look's PDF viewer opens on the document with no control picked: Tab enters its strip at the
first enabled control and Shift+Tab at Close, as Tab reaches the Markdown strip's close. `tests/chromering.sh` holds a
strip mark's frame to the foreground hairline on a coloured and a hueless theme, `tests/button-members.qml` and
`tests/button-hueless.qml` hold the thick frame's width, colour and unmoved geometry, `tests/js/buttons.js` checks that
no host gives a mark a focus line and that `ui/DialogButton.qml` holds no `buttonRing`, and `tests/ui-pdf.sh` reads every
mark's frame and Next's hover and press back through `ipc pdfState` and drives keys with no control picked.
A focused `DialogButton` or `ChromeButton` answers Return, keypad Enter and Space itself, and a dialog that owns its
Tab order hosts the button directly with `tabHandle` and `tabStop` (an unavailable stop leaves the walk and keeps its
look), never a focus wrapper that copies the keys; `tests/button-keys.qml` drives all three keys on the Copy to and
Permissions buttons and the Permissions close mark.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
