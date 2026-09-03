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
    // GM ruling 2026-09-01: d trashes, which Delete also does.
    check("d trashes", Keymap.lookup(Qt.Key_D, "d", none), "trash")
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
    check("ctrl k connects to a server", Keymap.lookup(Qt.Key_K, "\u000b", ctrl), "addNetwork")
    check("ctrl delete trashes", Keymap.lookup(Qt.Key_Delete, "", ctrl), "trash")
    check("ctrl up goes to the parent", Keymap.lookup(Qt.Key_Up, "", ctrl), "parent")
    check("ctrl down opens", Keymap.lookup(Qt.Key_Down, "", ctrl), "open")
    check("ctrl 1, 2 and 3 pick the list, columns and grid views",
          [Keymap.lookup(Qt.Key_1, "1", ctrl), Keymap.lookup(Qt.Key_2, "2", ctrl),
           Keymap.lookup(Qt.Key_3, "3", ctrl)].join("|"),
          "viewList|viewColumns|viewGrid")
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

    check("an unbound key is empty", Keymap.lookup(Qt.Key_Q, "q", none), "")
    check("ctrl j is not plain j", Keymap.lookup(Qt.Key_J, "j", ctrl), "")

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
          Keymap.SHEET.length, 22)
    // A chord shares the row of the key it doubles, so every caret token must resolve to that row's
    // own action, or the sheet advertises a chord bound to something else.
    check("every chord the sheet draws is bound to the action of its own row",
          Keymap.SHEET.map(chordActions).join("|"),
          Keymap.SHEET.map(function (row) { return chordTokens(row).map(function () { return row.action }).join("+") }).join("|"))
    check("and the sheet draws chords at all, so that check has a denominator",
          Keymap.SHEET.filter(function (row) { return chordTokens(row).length > 0 }).length, 8)
    check("slash filters, and the sheet now draws the row for it",
          Keymap.SHEET.filter(function (r) { return r.keys === "/" }).length, 1)
    check("and the sheet draws m, so eject and unmount are not mouse-only affordances",
          Keymap.SHEET.filter(function (r) { return r.keys === "m" }).length, 1)
}

// The three caps that name a key rather than printing one; everything else on the sheet is the
// character itself, and a two-key cap like "j k" is checked on the first of the pair.
var NAMED = { "enter": Qt.Key_Return, "space": Qt.Key_Space, "esc": Qt.Key_Escape }
// The one shifted character a chord prints; a capital letter after the caret is the other case.
var SHIFTED = { ">": Qt.Key_Greater }

function sheetAction(row) {
    var first = String(row.keys).split(" ")[0]
    if (NAMED[first] !== undefined) {
        return Keymap.lookup(NAMED[first], "", Qt.NoModifier)
    }
    if (first.charAt(0) === "^") {
        return chordAction(first)
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
