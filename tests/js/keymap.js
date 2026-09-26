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
        // #182: Shift+Delete deletes permanently in every preset, not only Mac and Windows; plain Delete still trashes.
        key(preset, "Delete", "", shift, "deletePermanently")
        key(preset, "Delete", "", none, "trash")
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
        key(preset, "Equal", "", ctrl | alt, "thumbSizeUp")
        key(preset, "Plus", "", ctrl | alt | shift, "thumbSizeUp")
        key(preset, "Minus", "", ctrl | alt, "thumbSizeDown")
        key(preset, "0", "", ctrl | alt, "thumbSizeReset")
        key(preset, "Equal", "", ctrl | shift, "textSizeUp")
        key(preset, "Minus", "", ctrl | shift, "textSizeDown")
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
    // The mac hints above filled the cache, so a setPreset that kept it would answer vim's rename with mac's.
    check("a preset change drops the cached hints", Keymap.hintFor("rename"), Keymap.hintsFor("vim").rename || "")
    check("and mac and vim hint rename differently, so that check can go red",
          Keymap.hintsFor("mac").rename !== Keymap.hintsFor("vim").rename, true)
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
    check("a shift chord fills an action no plain key names", Keymap.hintFor("deletePermanently"), "shift-delete")
    // A preset shift row for an action the table binds plain: the plain key must speak for it, since a shift chord only fills what no text or plain row names.
    var keepRows = Keymap.bindingRows
    Keymap.bindingRows = function (name, frontend) {
        return [{ mods: "none", key: "Delete", keys: "delete", action: "trash", preset: "all" },
                { mods: "shift", key: "X", keys: "shift-x", action: "trash", preset: name }]
    }
    Keymap.setPreset("vim")
    check("a preset shift row never replaces the plain hint", Keymap.hintFor("trash"), "delete")
    Keymap.bindingRows = keepRows
    Keymap.setPreset("vim")
    check("deletePermanently still gets its shift hint where no plain row exists", Keymap.hintFor("deletePermanently"), "shift-delete")
    // The status strip asks for this one key while the first window builds; the whole table was about 9,700 calls there.
    Keymap.setPreset("default")
    var keepLookup = Keymap.lookupFor, lookups = 0
    Keymap.lookupFor = function () { lookups++; return keepLookup.apply(null, arguments) }
    var firstAsk = Keymap.hintFor("deletePermanently")
    Keymap.lookupFor = keepLookup
    var ownRows = Keymap.PRESET_KEYS.concat(Keymap.SHARED_KEYS).filter(function (row) {
        return (row.preset === "all" || row.preset === "default") && Keymap.applies(row, "listing", "gui")
            && Keymap.actionGroup(row.action) === "deletePermanently"
    }).length
    check("the action asked for has rows of its own, so the count below can go red", ownRows > 0, true)
    check("a first hint ask looks up only the rows of the action it asks for", lookups, ownRows)
    check("and still answers the shift chord", firstAsk, "shift-delete")
    // Asked one action at a time, every preset answers exactly what its whole table says.
    var hintMismatches = []
    for (var hp = 0; hp < Keymap.PRESETS.length; hp++) {
        var hintTable = Keymap.hintsFor(Keymap.PRESETS[hp])
        Keymap.setPreset(Keymap.PRESETS[hp])
        var hintActions = Object.keys(hintTable).concat(["emptyTrash"])
        for (var ha = 0; ha < hintActions.length; ha++) {
            var wanted = Object.prototype.hasOwnProperty.call(hintTable, hintActions[ha]) ? hintTable[hintActions[ha]] : ""
            if (Keymap.hintFor(hintActions[ha]) !== wanted)
                hintMismatches.push(Keymap.PRESETS[hp] + ":" + hintActions[ha])
        }
    }
    check("one-action hints match the whole table on every preset", hintMismatches.join(" "), "")
    Keymap.setPreset("vim")
    check("sheet is populated from effective current bindings", Keymap.sheetFor(Keymap.preset, "gui").length > 30, true)
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
    // MediaMute rule 4: the sheet lists the preview's own mute key once per preset, under Look,
    // while the listing goes on advertising m as its menu key.
    var muteRows = 0, muteCap = "", menuStillM = true
    for (var m = 0; m < Keymap.PRESETS.length; m++) {
        var preset = Keymap.PRESETS[m], listed = Keymap.sheetFor(preset, "gui")
        for (var q = 0; q < listed.length; q++)
            if (listed[q].action === "mute") { muteRows++; muteCap = listed[q].keys }
        if (Keymap.lookupFor(preset, 0, "m", 0, "listing", "gui") !== "menu") menuStillM = false
        if (Keymap.lookupFor(preset, 0, "m", 0, "media", "gui") !== "mute") menuStillM = false
    }
    check("every preset lists mute once", muteRows, Keymap.PRESETS.length)
    check("and lists it under its own key", muteCap, "m")
    check("while m still opens the menu in the listing and mutes in a media preview", menuStillM, true)
    check("the sheet group that claims it is Look", Keymap.SHEET_GROUPS.look.indexOf("mute") >= 0, true)

    check("no cap in any preset outgrows its half of the card", widestCap <= 18, true)
    check("no row prints an action id where its wording belongs", identifierLabel, "")
    check("pointer contract remains populated", Keymap.POINTER.length > 10, true)
    var effective = Keymap.bindingRows("mac", "gui")
    check("suppressed Mac Ctrl+X never appears in sheet", effective.some(function (r) { return r.mods === "ctrl" && r.key === "X" }), false)
    check("every effective Mac binding resolves to advertised action", effective.every(function (r) {
        return Keymap.lookupFor("mac", r.keycode, r.text, r.mask, "listing", "gui") === r.action
    }), true)
}
