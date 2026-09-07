.import "../../ui/js/Keymap.js" as Keymap

function run(check) {
    var none = Qt.NoModifier
    var ctrl = Qt.ControlModifier
    var shift = Qt.ShiftModifier

    check("j moves down", Keymap.lookup(Qt.Key_J, "j", none), "cursorDown")
    check("k moves up", Keymap.lookup(Qt.Key_K, "k", none), "cursorUp")
    check("down arrow moves down", Keymap.lookup(Qt.Key_Down, "", none), "cursorDown")
    check("up arrow moves up", Keymap.lookup(Qt.Key_Up, "", none), "cursorUp")
    check("g goes first", Keymap.lookup(Qt.Key_G, "g", none), "cursorFirst")
    check("shift G goes last", Keymap.lookup(Qt.Key_G, "G", shift), "cursorLast")
    check("ctrl d is half a page down", Keymap.lookup(Qt.Key_D, "d", ctrl), "pageDown")
    check("ctrl u is half a page up", Keymap.lookup(Qt.Key_U, "u", ctrl), "pageUp")

    // Issue 28: the full-size keyboard's own four, reusing the actions the vim keys already carry so
    // no new name reaches tools/flea-acceptance's checklist and no second control does the same job.
    check("home goes to the first row", Keymap.lookup(Qt.Key_Home, "", none), "cursorFirst")
    check("end goes to the last row", Keymap.lookup(Qt.Key_End, "", none), "cursorLast")
    check("page down is the page key, not only the chord", Keymap.lookup(Qt.Key_PageDown, "", none), "pageDown")
    check("page up is the page key, not only the chord", Keymap.lookup(Qt.Key_PageUp, "", none), "pageUp")
    // The chord pair keeps its own half-page meaning: the page keys joined it rather than replacing it.
    check("ctrl d still pages, so the page key did not take the chord's action away",
          Keymap.lookup(Qt.Key_D, "d", ctrl), "pageDown")
    check("ctrl u still pages too", Keymap.lookup(Qt.Key_U, "u", ctrl), "pageUp")
    check("enter opens", Keymap.lookup(Qt.Key_Return, "", none), "open")
    check("keypad enter opens", Keymap.lookup(Qt.Key_Enter, "", none), "open")
    check("backspace goes up a directory", Keymap.lookup(Qt.Key_Backspace, "", none), "parent")
    check("h goes up a directory", Keymap.lookup(Qt.Key_H, "h", none), "parent")
    check("space previews", Keymap.lookup(Qt.Key_Space, " ", none), "preview")
    check("slash filters", Keymap.lookup(Qt.Key_Slash, "/", none), "filter")
    check("colon opens the path bar", Keymap.lookup(Qt.Key_Colon, ":", shift), "pathBar")
    check("t opens a tab", Keymap.lookup(Qt.Key_T, "t", none), "tabNew")
    check("w closes a tab", Keymap.lookup(Qt.Key_W, "w", none), "tabClose")
    check("1 selects a tab", Keymap.lookup(Qt.Key_1, "1", none), "tab1")
    check("9 selects a tab", Keymap.lookup(Qt.Key_9, "9", none), "tab9")
    check("y copies", Keymap.lookup(Qt.Key_Y, "y", none), "copy")
    check("x cuts", Keymap.lookup(Qt.Key_X, "x", none), "cut")
    check("p pastes", Keymap.lookup(Qt.Key_P, "p", none), "paste")
    check("delete trashes", Keymap.lookup(Qt.Key_Delete, "", none), "trash")
    check("r renames", Keymap.lookup(Qt.Key_R, "r", none), "rename")
    check("dot toggles hidden", Keymap.lookup(Qt.Key_Period, ".", none), "toggleHidden")
    check("escape retreats", Keymap.lookup(Qt.Key_Escape, "", none), "escape")

    check("v toggles selection", Keymap.lookup(Qt.Key_V, "v", none), "toggleSelect")
    // GM ruling 2026-09-01: d trashes, which Delete also does. Since v0.1.3 the letter arms the
    // pair instead, because a single d sat among the letters a name is typed with; see issue 7.
    check("d arms the trash pair", Keymap.lookup(Qt.Key_D, "d", none), "trashArm")
    check("Delete still trashes on one press", Keymap.lookup(Qt.Key_Delete, "", none), "trash")
    check("ctrl a selects all", Keymap.lookup(Qt.Key_A, "a", ctrl), "selectAll")
    check("shift J extends down", Keymap.lookup(Qt.Key_J, "J", shift), "extendDown")
    check("shift K extends up", Keymap.lookup(Qt.Key_K, "K", shift), "extendUp")
    check("shift down arrow extends down", Keymap.lookup(Qt.Key_Down, "", shift), "extendDown")
    check("shift up arrow extends up", Keymap.lookup(Qt.Key_Up, "", shift), "extendUp")
    check("plain a still opens Add Network, ctrl did not leak into it", Keymap.lookup(Qt.Key_A, "a", none), "addNetwork")
    // The rail's release menu was mouse-only, in a product whose own tagline is keyboard-first.
    check("m raises a menu", Keymap.lookup(Qt.Key_M, "m", none), "menu")

    // Finder's table with Cmd read as Ctrl, every chord beside the bare key it doubles; the ctrl
    // letters carry their control character as text, which is what a real key event delivers.
    check("ctrl c copies", Keymap.lookup(Qt.Key_C, "\u0003", ctrl), "copy")
    check("ctrl v pastes", Keymap.lookup(Qt.Key_V, "\u0016", ctrl), "paste")
    check("ctrl x cuts", Keymap.lookup(Qt.Key_X, "\u0018", ctrl), "cut")
    check("ctrl z undoes", Keymap.lookup(Qt.Key_Z, "\u001a", ctrl), "undo")
    check("ctrl f searches", Keymap.lookup(Qt.Key_F, "\u0006", ctrl), "search")
    check("ctrl e ejects", Keymap.lookup(Qt.Key_E, "\u0005", ctrl), "eject")
    check("ctrl t opens a terminal here", Keymap.lookup(Qt.Key_T, "\u0014", ctrl), "openTerminal")

    // ui/MenuRow.qml's hint slot. Menus.html draws exactly these seven beside the listing menu's
    // rows and leaves every other row blank, Duplicate included.
    check("the listing menu's own hints",
          ["open", "cut", "copy", "paste", "rename", "trash", "toggleHidden"]
              .map(Keymap.hintFor).join(" "),
          "enter x y p r d .")
    check("an action with no key at all leaves the slot blank", Keymap.hintFor("duplicate"), "")
    check("and so does one reachable only by a chord", Keymap.hintFor("newFolder"), "")
    check("Move to Trash advertises the key that arms it, not the Delete beside it",
          Keymap.hintFor("trash"), "d")
    check("a printable character outranks the key code bound to the same action",
          Keymap.hintFor("rename"), "r")
    check("a row with no action at all is blank", Keymap.hintFor(undefined), "")
    check("every hint names a key something is really bound to",
          Object.keys(Keymap.HINTS).filter(function (a) { return Keymap.HINTS[a].length === 0 }).length, 0)
    // The seven Finder chords the Mac preset overlays, so the preset is named here rather than
    // assumed: the map opens on Default, whose own three rows are the view chords alone.
    Keymap.setPreset("mac")
    check("ctrl k connects to a server", Keymap.lookup(Qt.Key_K, "\u000b", ctrl), "addNetwork")
    check("ctrl delete trashes", Keymap.lookup(Qt.Key_Delete, "", ctrl), "trash")
    check("ctrl up goes to the parent", Keymap.lookup(Qt.Key_Up, "", ctrl), "parent")
    check("ctrl down opens", Keymap.lookup(Qt.Key_Down, "", ctrl), "open")
    check("ctrl 1, 2 and 3 pick the list, columns and grid views",
          [Keymap.lookup(Qt.Key_1, "1", ctrl), Keymap.lookup(Qt.Key_2, "2", ctrl),
           Keymap.lookup(Qt.Key_3, "3", ctrl)].join("|"),
          "viewList|viewColumns|viewGrid")
    Keymap.setPreset("default")
    // The one Finder chord not taken: Ctrl+D already pages, with Ctrl+U as its pair.
    check("ctrl d still pages, so Finder's duplicate chord is not taken", Keymap.lookup(Qt.Key_D, "d", ctrl), "pageDown")
    check("ctrl shift n makes a folder", Keymap.lookup(Qt.Key_N, "N", ctrl | shift), "newFolder")
    check("plain ctrl n is nothing, the shift is the chord", Keymap.lookup(Qt.Key_N, "n", ctrl), "")
    check("ctrl shift period shows hidden files, whichever key code the layout delivers",
          Keymap.lookup(Qt.Key_Greater, ">", ctrl | shift) + "|" + Keymap.lookup(Qt.Key_Period, ".", ctrl | shift),
          "toggleHidden|toggleHidden")
    check("an unmatched ctrl shift chord falls through to the ctrl row", Keymap.lookup(Qt.Key_D, "D", ctrl | shift), "pageDown")

    // The PDF viewer's own three. Plus arrives shift-modified on this layout, so it also proves the
    // shift table still falls through to the character table instead of returning early.
    check("minus zooms out", Keymap.lookup(Qt.Key_Minus, "-", none), "zoomOut")
    check("shift plus zooms in", Keymap.lookup(Qt.Key_Plus, "+", shift), "zoomIn")
    check("e expands", Keymap.lookup(Qt.Key_E, "e", none), "expand")

    // The Settings board's own two doors onto the panel, and both are in the shared tables so the
    // key answers under either preset.
    check("comma opens settings", Keymap.lookup(Qt.Key_Comma, ",", none), "settings")
    check("and so does ctrl comma, which is how both presets spell it",
          Keymap.lookup(Qt.Key_Comma, "", ctrl), "settings")

    check("an unbound key is empty", Keymap.lookup(Qt.Key_Q, "q", none), "")
    check("ctrl j is not plain j", Keymap.lookup(Qt.Key_J, "j", ctrl), "")

    // Issue 9's pair. Equal and Underscore are the same chords on an unshifted key, bound so the
    // hand gets the scale whichever way the layout reports the keypress.
    check("ctrl shift plus grows the text", Keymap.lookup(Qt.Key_Plus, "", ctrl | shift), "textSizeUp")
    check("ctrl shift equal is the same chord", Keymap.lookup(Qt.Key_Equal, "", ctrl | shift), "textSizeUp")
    check("ctrl shift minus shrinks it", Keymap.lookup(Qt.Key_Minus, "", ctrl | shift), "textSizeDown")
    check("ctrl shift zero follows Omarchy again", Keymap.lookup(Qt.Key_0, "", ctrl | shift), "textSizeReset")
    // Bare minus is the PDF zoom and must not have been taken by the chord above.
    check("bare minus still zooms a PDF out", Keymap.lookup(Qt.Key_Minus, "-", none), "zoomOut")

    check("shift ? opens the keymap sheet", Keymap.lookup(Qt.Key_Question, "?", shift), "keymapSheet")
    check("l turns a PDF page forward", Keymap.lookup(Qt.Key_L, "l", none), "pageForward")

    // The sort pair. S arrives shift-modified, so it also proves the shift table still falls through
    // to the character table rather than returning early, the same way shift plus does above.
    check("s steps the sort column", Keymap.lookup(Qt.Key_S, "s", none), "sortNext")
    check("shift S reverses the sort", Keymap.lookup(Qt.Key_S, "S", shift), "sortReverse")

    // The sheet promised a key that did nothing for a whole release, so every cap it draws is
    // resolved back through the table it was generated from. A row can lose its binding in
    // keys.toml without anyone editing the sheet, and this is what catches that.
    check("every key the sheet draws is bound to the action the sheet names",
          Keymap.SHEET.map(sheetAction).join("|"),
          Keymap.SHEET.map(function (row) { return row.action }).join("|"))
    check("the sheet is not empty, so the check above has a denominator",
          Keymap.SHEET.length, 30)
    // A chord shares the row of the key it doubles, so every caret token must resolve to that row's
    // own action, or the sheet advertises a chord bound to something else.
    check("every chord the sheet draws is bound to the action of its own row",
          Keymap.SHEET.map(chordActions).join("|"),
          Keymap.SHEET.map(function (row) { return chordTokens(row).map(function () { return row.action }).join("+") }).join("|"))
    check("and the sheet draws chords at all, so that check has a denominator",
          Keymap.SHEET.filter(function (row) { return chordTokens(row).length > 0 }).length, 11)
    check("slash filters, and the sheet now draws the row for it",
          Keymap.SHEET.filter(function (r) { return r.keys === "/" }).length, 1)
    check("and the sheet draws m, so eject and unmount are not mouse-only affordances",
          Keymap.SHEET.filter(function (r) { return r.keys === "m" }).length, 1)
    check("and the sheet draws l, so browse-forward is discoverable",
          Keymap.SHEET.filter(function (r) { return r.keys === "l" && r.action === "pageForward" }).length, 1)
    // Issue 30 asked for a way to search the folder the pane is in. It is tab on the query line, so
    // the sheet has to draw tab or the only control the issue got is one nobody is told about.
    check("and the sheet draws tab, so the search scope is not an undocumented key",
          Keymap.SHEET.filter(function (r) { return r.keys === "tab" }).length, 1)

    runSheetStability(check)
    runPreset(check)
}

// Every cap the sheet draws is in the shared tables, never in a preset overlay: a sheet advertising
// a Mac-only chord would be wrong for half its readers the moment the Keys toggle moved. Asked as
// "does the whole sheet resolve to the same actions under both presets", which is the invariant.
function runSheetStability(check) {
    var opened = Keymap.preset
    Keymap.setPreset("mac")
    var keysUnderMac = Keymap.SHEET.map(sheetAction).join("|")
    var chordsUnderMac = Keymap.SHEET.map(chordActions).join("|")
    Keymap.setPreset("windows")
    check("no cap the sheet draws depends on the selected preset",
          Keymap.SHEET.map(sheetAction).join("|"), keysUnderMac)
    check("and no chord it draws does either",
          Keymap.SHEET.map(chordActions).join("|"), chordsUnderMac)
    Keymap.setPreset(opened)
}

// SettingsKeys.html's four-value chooser. lookup() consults the overlay first, so the same call
// answers differently with the preset moved; every preset carries the three view chords, GM's ruling
// of 2026-09-06 over that board's dash, and every other row it governs is a bare shared key.
function runPreset(check) {
    var ctrl = Qt.ControlModifier
    var shift = Qt.ShiftModifier
    check("the preset this build opens on is Default", Keymap.preset, "default")
    check("Default picks the list view on Ctrl+1, and Finder's other chords stay unbound under it",
          [Keymap.lookup(Qt.Key_1, "1", ctrl), Keymap.lookup(Qt.Key_H, "\u0008", ctrl),
           Keymap.lookup(Qt.Key_Up, "", ctrl), Keymap.lookup(Qt.Key_K, "\u000b", ctrl)].join("|"),
          "viewList|||")
    check("and the shared map is the rest of it, bare keys and chords alike",
          [Keymap.lookup(Qt.Key_J, "j", Qt.NoModifier), Keymap.lookup(Qt.Key_H, "h", Qt.NoModifier),
           Keymap.lookup(Qt.Key_C, "\u0003", ctrl)].join("|"), "cursorDown|parent|copy")

    // Every vim key is shared and preset-independent, so Vim differs from Default in nothing at all.
    Keymap.setPreset("vim")
    check("Vim answers exactly as Default does, chord and bare key alike",
          [Keymap.lookup(Qt.Key_1, "1", ctrl), Keymap.lookup(Qt.Key_H, "\u0008", ctrl),
           Keymap.lookup(Qt.Key_J, "j", Qt.NoModifier), Keymap.lookup(Qt.Key_G, "G", Qt.NoModifier)]
              .join("|"), "viewList||cursorDown|cursorLast")

    Keymap.setPreset("mac")
    check("Finder's Cmd+1 read as Ctrl+1 picks the list view under Mac",
          Keymap.lookup(Qt.Key_1, "1", ctrl), "viewList")
    check("and Explorer's Ctrl+H is not bound under it",
          Keymap.lookup(Qt.Key_H, "\u0008", ctrl), "")

    Keymap.setPreset("windows")
    check("the Windows preset binds Ctrl+H to the hidden files toggle",
          Keymap.lookup(Qt.Key_H, "\u0008", ctrl), "toggleHidden")
    check("and Explorer's own layout chords to the three views",
          [Keymap.lookup(Qt.Key_1, "", ctrl | shift), Keymap.lookup(Qt.Key_2, "", ctrl | shift),
           Keymap.lookup(Qt.Key_3, "", ctrl | shift)].join("|"), "viewList|viewColumns|viewGrid")
    check("Finder's Ctrl+1 goes quiet under it, which is what makes this a preset and not an addition",
          Keymap.lookup(Qt.Key_1, "1", ctrl), "")
    check("and so does Connect to Server", Keymap.lookup(Qt.Key_K, "\u000b", ctrl), "")
    // Everything the two platforms agree on stays in the shared tables and answers under both.
    check("the shared chords are untouched by the preset",
          [Keymap.lookup(Qt.Key_C, "\u0003", ctrl), Keymap.lookup(Qt.Key_F2, "", Qt.NoModifier),
           Keymap.lookup(Qt.Key_Backspace, "", Qt.NoModifier)].join("|"), "copy|rename|parent")

    // The three views are bound in no shared table, so a preset overlaying nothing left the keyboard
    // with no route to them at all. GM's ruling: asked of all four now, not only Mac and Windows.
    var views = function (mods) {
        return [Keymap.lookup(Qt.Key_1, "1", mods), Keymap.lookup(Qt.Key_2, "2", mods),
                Keymap.lookup(Qt.Key_3, "3", mods)].join("|")
    }
    var spelling = [["default", ctrl], ["vim", ctrl], ["mac", ctrl], ["windows", ctrl | shift]]
    var reached = []
    for (var p = 0; p < spelling.length; p++) {
        Keymap.setPreset(spelling[p][0])
        reached.push(spelling[p][0] + " " + views(spelling[p][1]))
    }
    check("every preset reaches all three views from the keyboard, each in its own spelling",
          reached.join(", "),
          "default viewList|viewColumns|viewGrid, vim viewList|viewColumns|viewGrid, "
          + "mac viewList|viewColumns|viewGrid, windows viewList|viewColumns|viewGrid")
    check("and all four were asked, so that check has a denominator", spelling.length, 4)

    // ui/ViewState.qml resolves a stored name that is not one of the four to default before it ever
    // reaches here, so an unknown name matching no overlay row is the validator's business, not this
    // module's: the shared tables still answer under it, and the overlay stays silent.
    Keymap.setPreset("marzipan")
    check("a preset name this build does not have reaches no overlay row, and no shared key with it",
          [Keymap.lookup(Qt.Key_1, "1", ctrl), Keymap.lookup(Qt.Key_H, "\u0008", ctrl),
           Keymap.lookup(Qt.Key_Backspace, "", Qt.NoModifier)].join("|"), "||parent")
    Keymap.setPreset("default")
}

// The three caps that name a key rather than printing one; everything else on the sheet is the
// character itself, a two-key cap like "j k" is checked on the first of the pair, and a doubled
// character like "dd" is one key pressed twice, so it resolves as that character.
var NAMED = { "enter": Qt.Key_Return, "space": Qt.Key_Space, "esc": Qt.Key_Escape, "tab": Qt.Key_Tab }
// The one shifted character a chord prints; a capital letter after the caret is the other case.
var SHIFTED = { ">": Qt.Key_Greater, "+": Qt.Key_Plus, "-": Qt.Key_Minus }

function sheetAction(row) {
    var first = String(row.keys).split(" ")[0]
    if (NAMED[first] !== undefined) {
        return Keymap.lookup(NAMED[first], "", Qt.NoModifier)
    }
    if (first.charAt(0) === "^") {
        return chordAction(first)
    }
    if (first.length === 2 && first.charAt(0) === first.charAt(1)) {
        first = first.charAt(0)
    }
    // Qt.Key_unknown is 0 and matches no case in the code switch, so the character decides.
    return Keymap.lookup(0, first, Qt.NoModifier)
}

// "^c" is ctrl-c; "^N" and "^>" are ctrl-shift, the capital-is-the-variant rule the bare caps use.
function chordAction(token) {
    var ch = token.substring(1)
    var shifted = SHIFTED[ch] !== undefined || ch !== ch.toLowerCase()
    var code = SHIFTED[ch] !== undefined ? SHIFTED[ch] : Qt["Key_" + ch.toUpperCase()]
    return Keymap.lookup(code, "", Qt.ControlModifier | (shifted ? Qt.ShiftModifier : 0))
}

function chordTokens(row) {
    return String(row.keys).split(" ").filter(function (t) { return t.charAt(0) === "^" })
}

function chordActions(row) {
    return chordTokens(row).map(chordAction).join("+")
}
