.pragma library
.import "KeyBindings.js" as Bindings

// Generated from keys.toml by tools/flea-keymap-gen. Do not edit.
var preset = "default"
var PRESETS = Bindings.PRESETS
var PRESET_KEYS = Bindings.PRESET_KEYS
var SHARED_KEYS = Bindings.SHARED_KEYS
var POINTER = Bindings.POINTER
var BASE_SHEET = Bindings.BASE_SHEET
var DIGITS = Bindings.DIGITS

function applies(row, context, frontend) {
    return (row.context.split(",").indexOf(context) >= 0 || row.context === "all")
           && (row.frontend === frontend || row.frontend === "all")
}
function matches(row, key, text, modifiers) {
    var mask = modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
    if (row.mods === "text")
        return (mask & ~Qt.ShiftModifier) === 0 && text === row.text
    return mask === row.mask && key === row.keycode
}
// A matching empty action suppresses fallback, as Mac Ctrl+X requires.
function presetMatch(name, key, text, modifiers, context, frontend) {
    for (var pass = 0; pass < 2; pass++) {
        for (var i = 0; i < PRESET_KEYS.length; i++) {
            var row = PRESET_KEYS[i]
            if (row.preset !== (pass === 0 ? name : "all")) continue
            if (applies(row, context, frontend) && matches(row, key, text, modifiers)) return row
        }
    }
    return null
}
function lookupPreset(name, key, text, modifiers, context, frontend) {
    var row = presetMatch(name, key, text, modifiers, context || "listing", frontend || "gui")
    return row ? row.action : ""
}
function lookupFor(name, key, text, modifiers, context, frontend) {
    context = context || "listing"
    frontend = frontend || "gui"
    var row = presetMatch(name, key, text, modifiers, context, frontend)
    if (row) return row.action
    if (context !== "listing" && context !== "rail") return ""
    for (var i = 0; i < SHARED_KEYS.length; i++) {
        if (name === "nautilus" && frontend === "gui" && SHARED_KEYS[i].mods === "text") continue
        if (matches(SHARED_KEYS[i], key, text, modifiers)) return SHARED_KEYS[i].action
    }
    if (frontend === "tui" && modifiers === 0 && text >= String(DIGITS.from) && text <= String(DIGITS.to))
        return DIGITS.prefix + text
    return ""
}
function lookup(key, text, modifiers, context, frontend) {
    return lookupFor(preset, key, text, modifiers, context, frontend)
}
function actionGroup(action) {
    var arms = { copyArm: "copy", cutArm: "cut", pasteArm: "paste", cursorFirstArm: "cursorFirst", trashArm: "trash" }
    return arms[action] || action
}
function bindingRows(name, frontend) {
    var rows = [], candidates = PRESET_KEYS.concat(SHARED_KEYS)
    for (var i = 0; i < candidates.length; i++) {
        var row = candidates[i]
        if (row.preset !== "all" && row.preset !== name) continue
        if (!applies(row, "listing", frontend || "gui") || !row.action) continue
        if (lookupFor(name, row.keycode, row.text, row.mask, "listing", frontend || "gui") !== row.action) continue
        var duplicate = rows.some(function (kept) { return kept.keys === row.keys && kept.action === row.action })
        if (!duplicate) rows.push(row)
    }
    return rows
}
function hintFor(action) {
    return HINTS[action] || ""
}
// How wide a cap may get before a second spelling stops earning its place. The sheet draws two
// columns of a 300 unit card, so a cap past this elides and the wording beside it has nowhere to go.
var SHEET_CAP_BUDGET = 16

// An action id is not wording. A row the base sheet does not name printed its own identifier, so the
// pane advertised "pageDown" and "textSizeReset" beside sentences like "hidden files".
function spelledOut(action) {
    return String(action).replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase()
}

// Which spelling speaks for an action: the preset's own before an inherited one, and a plain key
// before a chord. setPreset ranks the menu hint the same way, so the sheet and the menus agree.
function capRank(row, preset) {
    return (row.preset === preset ? 0 : 2) + (row.mods === "text" ? 0 : 1)
}

function sheetFor(name, frontend, dual) {
    var result = [], groups = {}, rows = bindingRows(name, frontend || "gui")
    for (var i = 0; i < rows.length; i++) {
        var row = rows[i], action = actionGroup(row.action)
        var group = groups[action]
        if (!group) {
            var label = row.label || spelledOut(action)
            for (var j = 0; j < BASE_SHEET.length; j++)
                if (actionGroup(BASE_SHEET[j].action) === action) label = BASE_SHEET[j].label
            group = { action: action, label: label, keys: "", context: "listing", spellings: [] }
            result.push(group)
            groups[action] = group
        }
        group.spellings.push(row)
    }
    // One cap names one key. Joining every spelling an action answers to built caps of 40 characters
    // on the default preset and 78 on mac, wider than the whole card, so the pane drew them across
    // the column beside it. The best spelling always shows, a second only while both still fit.
    for (var g = 0; g < result.length; g++) {
        var spellings = result[g].spellings.slice()
        spellings.sort(function (left, right) { return capRank(left, name) - capRank(right, name) })
        var keys = spellings.length ? spellings[0].keys : ""
        for (var k = 1; k < spellings.length; k++) {
            var both = keys + " / " + spellings[k].keys
            if (spellings[k].keys === keys || both.length > SHEET_CAP_BUDGET) continue
            keys = both
            break
        }
        result[g].keys = keys
        delete result[g].spellings
    }
    if (dual && groups.focusNext) {
        groups.focusNext.keys = "tab"
        groups.focusNext.label = "focus other pane"
    }
    return result
}
function setPreset(name) {
    preset = PRESETS.indexOf(name) >= 0 ? name : "default"
    SHEET = sheetFor(preset, "gui")
    HINTS = {}
    var ranks = {}, rows = bindingRows(preset, "gui")
    for (var i = 0; i < rows.length; i++) {
        var row = rows[i]
        if (row.mods !== "text" && row.mods !== "none") continue
        var action = actionGroup(row.action), rank = (row.preset === preset ? 0 : 2) + (row.mods === "text" ? 0 : 1)
        if (ranks[action] !== undefined && ranks[action] <= rank) continue
        // A menu hint is the key the operator presses: Menus.html and the OpenWith overseer board
        // both draw Move to Trash with d. The full dd chord stays on the keymap sheet below.
        HINTS[action] = row.mods === "text" ? row.key : row.keys
        ranks[action] = rank
    }
}
var SHEET = []
var HINTS = {}
setPreset("default")
