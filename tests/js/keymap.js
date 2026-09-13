.import "../../ui/js/Keymap.js" as Keymap

function run(check) {
    var none = Qt.NoModifier, ctrl = Qt.ControlModifier, shift = Qt.ShiftModifier
    var alt = Qt.AltModifier, meta = Qt.MetaModifier
    function key(preset, name, text, mods, expected, context, frontend) {
        check(preset + " " + (context || "listing") + " " + (frontend || "gui") + " " + name + " " + text + " " + mods,
              Keymap.lookupFor(preset, Qt["Key_" + name] || 0, text, mods, context, frontend), expected)
    }
    for (var i = 0; i < Keymap.PRESETS.length; i++) {
        var preset = Keymap.PRESETS[i]
        key(preset, "Down", "", none, "cursorDown")
        key(preset, "Up", "", none, "cursorUp")
        key(preset, "Space", " ", none, "preview")
        key(preset, "P", "p", alt, "togglePreview")
        key(preset, "Space", " ", ctrl, "loadPreview")
        key(preset, "PageDown", "", ctrl, "tabNext")
        key(preset, "PageUp", "", ctrl, "tabPrevious")
        key(preset, "Tab", "", ctrl, "focusPreview")
        key(preset, "Tab", "", none, "focusNext")
        key(preset, "W", "", ctrl, "tabClose")
        key(preset, "S", "s", none, "sortNext")
        key(preset, "S", "S", shift, "sortReverse")
        key(preset, "Slash", "/", none, "filter")
        key(preset, "Question", "?", shift, "keymapSheet")
        key(preset, "Insert", "", ctrl, "copy", "listing", "tui")
        key(preset, "Insert", "", shift, "paste", "listing", "tui")
        key(preset, "Insert", "", ctrl, "", "listing", "gui")
        key(preset, "1", "1", none, "tab1", "listing", "tui")
        key(preset, "9", "9", none, "tab9", "listing", "tui")
        key(preset, "1", "1", none, "", "listing", "gui")
        key(preset, "N", "", preset === "mac" ? meta : ctrl, "windowNew")
        key(preset, "N", "", preset === "mac" ? meta : ctrl, "", "listing", "tui")
        key(preset, "1", "", preset === "windows" ? ctrl | shift : ctrl, "viewList")
        key(preset, "2", "", preset === "windows" ? ctrl | shift : ctrl, "viewColumns")
        key(preset, "3", "", preset === "windows" ? ctrl | shift : ctrl, "viewGrid")
        key(preset, "Comma", "", ctrl, "settings")
        for (var f = 0; f < 2; f++) {
            var frontend = ["gui", "tui"][f]
            var menuContexts = ["listing", "rail", "menu", "panel", "preview", "pdf", "media", "editor"]
            for (var m = 0; m < menuContexts.length; m++) {
                var menuContext = menuContexts[m], menuAction = m < 2 ? "menu" : ""
                key(preset, "Menu", "", none, menuAction, menuContext, frontend)
                key(preset, "F10", "", shift, menuAction, menuContext, frontend)
            }
        }
        key(preset, "N", "", ctrl | shift, "newFolder")
        key(preset, "Q", "q", alt, "")
        key(preset, "J", "j", meta, "")
        key(preset, "D", "D", ctrl | shift, "")
        for (var c = 0; c < 5; c++) {
            var context = ["menu", "panel", "preview", "pdf", "media"][c]
            key(preset, "Return", "", none, "open", context)
            key(preset, "Enter", "", none, "open", context)
            key(preset, "Space", " ", none, "preview", context)
            key(preset, "Escape", "", none, "escape", context)
            key(preset, "D", "d", none, "", context)
            key(preset, "X", "", ctrl, "", context)
            key(preset, "N", "", ctrl, "", context)
        }
        key(preset, "Right", "", none, "menuRight", "menu")
        key(preset, "Left", "", none, "parent", "menu")
        key(preset, "J", "j", none, "cursorDown", "menu")
        key(preset, "K", "k", none, "cursorUp", "menu")
        key(preset, "Minus", "-", none, "zoomOut", "pdf")
        key(preset, "Plus", "+", shift, "zoomIn", "pdf")
        key(preset, "E", "e", none, "expand", "pdf")
        key(preset, "Tab", "", shift, "focusPrevious", "pdf")
        key(preset, "Tab", "", ctrl, "focusPreview", "preview")
        key(preset, "J", "j", none, "", "pdf")
        key(preset, "K", "k", none, "", "pdf")
        key(preset, "Up", "", none, "cursorUp", "pdf")
        key(preset, "Down", "", none, "cursorDown", "pdf")
        key(preset, "Return", "", none, "open", "pdf")
        key(preset, "Return", "", none, "", "editor")
    }
    key("default", "Return", "", none, "open")
    key("default", "Backspace", "", none, "parent")
    key("default", "H", "H", shift, "historyBack")
    key("default", "L", "L", shift, "historyForward")
    key("default", "Y", "y", none, "copy")
    key("default", "X", "x", none, "cut")
    key("default", "P", "p", none, "paste")
    key("default", "Z", "z", none, "undo")
    key("default", "D", "d", none, "trashArm")
    key("default", "D", "", ctrl, "pageDown")
    key("default", "T", "t", none, "tabNew")
    key("default", "Delete", "", shift, "")
    key("vim", "L", "l", none, "open")
    key("vim", "H", "h", none, "parent")
    key("vim", "Y", "y", none, "copyArm")
    key("vim", "D", "d", none, "cutArm")
    key("vim", "P", "p", none, "pasteArm")
    key("vim", "G", "g", none, "cursorFirstArm")
    key("vim", "G", "G", shift, "cursorLast")
    key("vim", "D", "D", shift, "trash")
    key("vim", "U", "u", none, "undo")
    key("mac", "Return", "", none, "rename")
    key("mac", "Enter", "", none, "rename")
    key("mac", "Right", "", none, "open")
    key("mac", "Left", "", none, "parent")
    key("mac", "C", "", meta, "copy")
    key("mac", "C", "", ctrl, "copy")
    key("mac", "V", "", meta, "paste")
    key("mac", "V", "", ctrl, "paste")
    key("mac", "V", "", meta | alt, "movePaste")
    key("mac", "X", "", ctrl, "")
    key("mac", "D", "", meta, "duplicate")
    key("mac", "Z", "", meta, "undo")
    key("mac", "Z", "", meta | shift, "redo")
    key("mac", "A", "", meta, "selectAll")
    key("mac", "I", "", meta, "properties")
    key("mac", "BracketLeft", "", meta, "historyBack")
    key("mac", "BracketRight", "", meta, "historyForward")
    key("mac", "Period", ".", meta | shift, "toggleHidden")
    key("mac", "Delete", "", shift, "deletePermanently")
    key("mac", "T", "", ctrl, "tabNew")
    key("windows", "Return", "", none, "open")
    key("windows", "F2", "", none, "rename")
    key("windows", "X", "", ctrl, "cut")
    key("windows", "D", "", ctrl, "trash")
    key("windows", "Delete", "", shift, "deletePermanently")
    key("windows", "Y", "", ctrl, "redo")
    key("windows", "Return", "", alt, "properties")
    key("windows", "Enter", "", alt, "properties")
    key("windows", "Left", "", alt, "historyBack")
    key("windows", "Right", "", alt, "historyForward")
    key("windows", "Up", "", alt, "parent")
    key("windows", "H", "", ctrl, "toggleHidden")
    key("windows", "T", "", ctrl, "tabNew")
    Keymap.setPreset("mac")
    check("Mac menu advertises its actual Return action", Keymap.hintFor("rename"), "enter")
    check("Mac Open hint uses native Right", Keymap.hintFor("open"), "right")
    Keymap.setPreset("vim")
    check("Vim Copy hints the key it starts with", Keymap.hintFor("copy"), "y")
    check("Vim Cut hints the key it starts with", Keymap.hintFor("cut"), "d")
    Keymap.setPreset("unknown")
    check("unknown stored preset resolves to Default", Keymap.preset, "default")
    check("Default Copy hint remains y", Keymap.hintFor("copy"), "y")
    // Menus.html and the OpenWith overseer board both draw this row with d; the sheet below keeps dd.
    check("Default Trash hints the key it starts with", Keymap.hintFor("trash"), "d")
    check("Default sheet never advertises a lone destructive d", Keymap.sheetFor("default", "gui").filter(function (row) {
        return row.action === "trash"
    })[0].keys.split(" / ").indexOf("d"), -1)
    check("menu-only actions invent no shortcut", Keymap.hintFor("emptyTrash"), "")
    check("sheet is populated from effective current bindings", Keymap.SHEET.length > 30, true)
    // One cap names one key. Joining every spelling an action answers to produced caps of 40
    // characters on Default and 78 on Mac, wider than the card, and they drew over the next column.
    var widestCap = 0, identifierLabel = ""
    for (var p = 0; p < Keymap.PRESETS.length; p++) {
        var sheet = Keymap.sheetFor(Keymap.PRESETS[p], "gui")
        for (var r = 0; r < sheet.length; r++) {
            if (sheet[r].keys.length > widestCap) widestCap = sheet[r].keys.length
            if (/[a-z][A-Z]/.test(sheet[r].label)) identifierLabel = sheet[r].label
            if (sheet[r].keys.split(" / ").length > 2) identifierLabel = "too many spellings: " + sheet[r].keys
        }
    }
    check("no cap in any preset outgrows its half of the card", widestCap <= 18, true)
    check("no row prints an action id where its wording belongs", identifierLabel, "")
    check("pointer contract remains populated", Keymap.POINTER.length > 10, true)
    var effective = Keymap.bindingRows("mac", "gui")
    check("suppressed Mac Ctrl+X never appears in sheet", effective.some(function (r) { return r.mods === "ctrl" && r.key === "X" }), false)
    check("every effective Mac binding resolves to advertised action", effective.every(function (r) {
        return Keymap.lookupFor("mac", r.keycode, r.text, r.mask, "listing", "gui") === r.action
    }), true)
}
