.import "../../ui/js/PickerKeys.js" as PickerKeys
.import "../../ui/js/Keymap.js" as Keymap

// A key press as ui/PickerList.qml's Keys.onPressed sees it, unaccepted until handle says so.
function press(key, text, modifiers) {
    return { key: key, text: text || "", modifiers: modifiers || Qt.NoModifier, accepted: false }
}

// ui/PickerState.qml reduced to what lookup and act read, with a log of every call they make.
function stubState(over) {
    var state = {
        fetching: false,
        cursorIndex: 4,
        shownTotal: 9,
        filterQuery: "",
        filterTyping: false,
        calls: [],
        said: [],
        fetcher: { cancel: function () { state.calls.push("stopFetch") } },
        cancel: function () { state.calls.push("cancel") },
        toggleMark: function (i) { state.calls.push("mark " + i) },
        activate: function (i) { state.calls.push("activate " + i) },
        goUp: function () { state.calls.push("parent") },
        goBack: function () { state.calls.push("back") },
        toggleHidden: function () { state.calls.push("hidden") },
        selectAll: function () { state.calls.push("selectAll") },
        message: function (text) { state.said.push(text) }
    }
    for (var key in over) {
        state[key] = over[key]
    }
    return state
}

// ui/PickerList.qml reduced to the cursor mover, its viewport height and the field ":" takes.
function stubOps() {
    var ops = {
        visibleRows: 7,
        moved: [],
        focused: 0,
        moveCursor: function (delta) { ops.moved.push(delta) },
        entry: { takeFocus: function () { ops.focused += 1 } }
    }
    return ops
}

function run(check) {
    var none = Qt.NoModifier
    var ctrl = Qt.ControlModifier
    var shift = Qt.ShiftModifier
    var alt = Qt.AltModifier
    var state = stubState()
    function look(key, text, modifiers) { return PickerKeys.lookup(press(key, text, modifiers), state) }

    // ---- the picker's own verbs, every meaning the if-chain had ----
    check("escape cancels", look(Qt.Key_Escape), "cancel")
    check("escape mid-fetch stops the download, not the dialog",
          PickerKeys.lookup(press(Qt.Key_Escape), stubState({ fetching: true })), "stopFetch")
    check("space marks", look(Qt.Key_Space, " "), "mark")
    check("return activates", look(Qt.Key_Return), "activate")
    check("keypad enter activates", look(Qt.Key_Enter), "activate")
    check("l activates", look(Qt.Key_L, "l"), "activate")
    check("backspace goes up", look(Qt.Key_Backspace), "parent")
    check("h goes up", look(Qt.Key_H, "h"), "parent")
    check("colon takes the field", look(Qt.Key_Colon, ":", shift), "location")
    check("ctrl l takes the field", look(Qt.Key_L, "l", ctrl), "location")
    check("alt left goes back", look(Qt.Key_Left, "", alt), "back")
    check("bare left is nobody's", look(Qt.Key_Left), "")
    check("j moves down", look(Qt.Key_J, "j"), "cursorDown")
    check("down moves down", look(Qt.Key_Down), "cursorDown")
    check("k moves up", look(Qt.Key_K, "k"), "cursorUp")
    check("up moves up", look(Qt.Key_Up), "cursorUp")
    check("page down pages", look(Qt.Key_PageDown), "pageDown")
    check("page up pages", look(Qt.Key_PageUp), "pageUp")
    check("home goes first", look(Qt.Key_Home), "cursorFirst")
    check("end goes last", look(Qt.Key_End), "cursorLast")

    // ---- the shared actions keys.toml binds, through the allowlist ----
    check("ctrl c copies", look(Qt.Key_C, "c", ctrl), "copy")
    check("ctrl x cuts", look(Qt.Key_X, "x", ctrl), "cut")
    check("ctrl v pastes", look(Qt.Key_V, "v", ctrl), "paste")
    check("ctrl a selects all", look(Qt.Key_A, "a", ctrl), "selectAll")
    check("delete trashes", look(Qt.Key_Delete), "trash")
    check("f2 renames", look(Qt.Key_F2), "rename")
    check("ctrl shift n makes a folder", look(Qt.Key_N, "N", ctrl | shift), "newFolder")
    check("ctrl t opens a terminal", look(Qt.Key_T, "t", ctrl), "openTerminal")
    check("shift y copies the folder path", look(Qt.Key_Y, "Y", shift), "copydirpath")
    check("ctrl z undoes", look(Qt.Key_Z, "z", ctrl), "undo")
    check("dot toggles hidden", look(Qt.Key_Period, "."), "toggleHidden")
    check("slash filters", look(Qt.Key_Slash, "/"), "filter")
    check("tab focuses the next surface", look(Qt.Key_Tab), "focusNext")

    // A preset's rebinding reaches the picker for an allowlisted action and for nothing else.
    var was = Keymap.preset
    Keymap.setPreset("windows")
    check("the windows preset's ctrl h toggles hidden here too", look(Qt.Key_H, "h", ctrl), "toggleHidden")
    check("the windows preset's ctrl shift 3 is still refused", look(Qt.Key_3, "#", ctrl | shift), "")
    Keymap.setPreset("default")
    check("the default preset's ctrl h is nobody's", look(Qt.Key_H, "h", ctrl), "")
    Keymap.setPreset(was)

    // ---- the browser window's other actions are refused ----
    check("comma is not settings", look(Qt.Key_Comma, ","), "")
    check("ctrl comma is not settings", look(Qt.Key_Comma, ",", ctrl), "")
    check("t is not a new tab", look(Qt.Key_T, "t"), "")
    check("f is not search", look(Qt.Key_F, "f"), "")
    check("ctrl f is not search", look(Qt.Key_F, "f", ctrl), "")
    check("ctrl 3 is not grid view", look(Qt.Key_3, "3", ctrl), "")
    check("ctrl e is not eject", look(Qt.Key_E, "e", ctrl), "")
    // The browser's own meanings for the picker's keys never come through: Space is a mark and
    // not a preview, l walks in and never pages forward, and d arms nothing.
    check("d is not the trash arm", look(Qt.Key_D, "d"), "")
    check("l is not page forward", look(Qt.Key_L, "l") === "pageForward", false)
    check("space is not preview", look(Qt.Key_Space, " ") === "preview", false)
    check("delete is trash through the allowlist alone", "trash" in PickerKeys.SHARED, true)

    // ---- handle: acceptance follows resolution ----
    var ops = stubOps()
    var taken = press(Qt.Key_Space, " ")
    check("a resolved key is accepted", PickerKeys.handle(taken, state, ops), "mark")
    check("and the event says so", taken.accepted, true)
    var refused = press(Qt.Key_Comma, ",")
    check("a refused key answers nothing", PickerKeys.handle(refused, state, ops), "")
    check("and stays unaccepted for the window above", refused.accepted, false)

    // ---- act: each verb reaches the state or the list ----
    state = stubState()
    ops = stubOps()
    PickerKeys.act("cancel", state, ops)
    PickerKeys.act("stopFetch", state, ops)
    PickerKeys.act("mark", state, ops)
    PickerKeys.act("activate", state, ops)
    PickerKeys.act("parent", state, ops)
    PickerKeys.act("back", state, ops)
    PickerKeys.act("toggleHidden", state, ops)
    PickerKeys.act("selectAll", state, ops)
    check("the state verbs land on the state", state.calls.join(","),
          "cancel,stopFetch,mark 4,activate 4,parent,back,hidden,selectAll")
    PickerKeys.act("cursorDown", state, ops)
    PickerKeys.act("cursorUp", state, ops)
    PickerKeys.act("pageDown", state, ops)
    PickerKeys.act("pageUp", state, ops)
    PickerKeys.act("cursorFirst", state, ops)
    PickerKeys.act("cursorLast", state, ops)
    check("the cursor verbs move by row, viewport and listing", ops.moved.join(","), "1,-1,7,-7,-9,9")
    PickerKeys.act("location", state, ops)
    check("location hands the keyboard to the field", ops.focused, 1)
    PickerKeys.act("location", state, { moveCursor: ops.moveCursor, visibleRows: 7, entry: null })
    check("and without a field it does nothing", ops.focused, 1)
    check("no verb spoke in the footer", state.said.length, 0)

    // A shared action with no verb yet says so, so a key the sheet advertises never falls silent.
    PickerKeys.act("copy", state, ops)
    check("an unbuilt shared action says so", state.said.join(""), "Copy is not built in the chooser yet.")
    PickerKeys.act("settings", state, ops)
    check("an action outside the allowlist says nothing", state.said.length, 1)
    check("the state is untouched by either", state.calls.length, 8)
    // The hidden toggle is built: a . reaches the state through handle and says nothing in the footer.
    var dotted = stubState()
    check("dot through handle flips hidden", PickerKeys.handle(press(Qt.Key_Period, "."), dotted, ops), "toggleHidden")
    check("and lands on the state in silence", dotted.calls.join(",") + "|" + dotted.said.length, "hidden|0")

    // ---- the rail: Tab swaps the surface, and the rail's six keys answer while it has the keyboard ----
    var railed = stubState({ focusView: "list", places: { entries: [1, 2, 3], cursorIndex: 0, opened: [],
                             activate: function (i) { this.opened.push(i) } } })
    PickerKeys.act("focusNext", railed, ops)
    check("tab takes the keyboard to the rail", railed.focusView, "rail")
    function railLook(key, text, modifiers) { return PickerKeys.lookup(press(key, text, modifiers), railed) }
    check("down walks the rail", railLook(Qt.Key_Down), "cursorDown")
    check("j walks the rail", railLook(Qt.Key_J, "j"), "cursorDown")
    check("up walks the rail", railLook(Qt.Key_Up), "cursorUp")
    check("k walks the rail", railLook(Qt.Key_K, "k"), "cursorUp")
    check("home is the rail's first row", railLook(Qt.Key_Home), "cursorFirst")
    check("end is the rail's last row", railLook(Qt.Key_End), "cursorLast")
    check("return opens the rail row", railLook(Qt.Key_Return), "open")
    check("keypad enter opens the rail row", railLook(Qt.Key_Enter), "open")
    check("escape leaves the rail", railLook(Qt.Key_Escape), "escape")
    check("tab still swaps back", railLook(Qt.Key_Tab), "focusNext")
    // Sidebar-only members: a rename, a mount dialog, a menu and an eject reach nothing in the
    // chooser's rail, and neither does the list's own Space, so the rail never marks a row unseen.
    check("a is not add network in the rail", railLook(Qt.Key_A, "a"), "")
    check("f2 is not a rail rename", railLook(Qt.Key_F2), "")
    check("m is not a rail menu", railLook(Qt.Key_M, "m"), "")
    check("ctrl e is not a rail eject", railLook(Qt.Key_E, "e", ctrl), "")
    check("space marks nothing from the rail", railLook(Qt.Key_Space, " "), "")
    check("ctrl c copies nothing from the rail", railLook(Qt.Key_C, "c", ctrl), "")
    var railMoves = stubOps()
    PickerKeys.act("cursorDown", railed, railMoves)
    PickerKeys.act("cursorLast", railed, railMoves)
    check("the rail's cursor keys move the rail and never the list",
          railed.places.cursorIndex + "|" + railMoves.moved.length, "2|0")
    PickerKeys.act("open", railed, railMoves)
    check("open activates the rail's cursor row", railed.places.opened.join(","), "2")
    check("and touches no list verb", railed.calls.length, 0)
    PickerKeys.act("escape", railed, railMoves)
    check("escape returns the keyboard to the list without cancelling", railed.focusView + "|" + railed.calls.length, "list|0")
    PickerKeys.act("focusNext", railed, railMoves)
    PickerKeys.act("focusNext", railed, railMoves)
    check("two tabs come back to the list", railed.focusView, "list")
}
