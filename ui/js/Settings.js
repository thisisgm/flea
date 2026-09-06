.pragma library
.import "TextSize.js" as TextSize

// The settings panel's whole model, so every row it draws is a value a test can read without a
// window: ui/SettingsPanel.qml only paints what rows() returns. The Settings, SettingsScale,
// SettingsMenus and SettingsKeys boards are what the tables below are copied from.

// Three sections, in the boards' own rail order less the four they draw beside them. View, Places,
// Preview and About have no working consumer in this release, and a rail row opening an empty pane
// is the dead entry point the release ruling keeps out of the shipped UI.
// Each glyph is the mark those boards draw on that rail row; "sliders" is the View row's, not Display's.
var SECTIONS = [
    { id: "keys", label: "Keys", glyph: "keyboard" },
    { id: "display", label: "Display", glyph: "maximize" },
    { id: "menus", label: "Menus", glyph: "list" }
]

// The six SettingsMenus.html puts under one master row, and the ids ui/js/Menu.js gives those rows.
var BASIC = ["cut", "copy", "paste", "duplicate", "rename", "trash"]

// Every action the listing menu can build, grouped as the board groups it. An id that this release
// cannot draw is absent rather than switched off: a toggle over a row no menu has is a mock control.
var MENU_GROUPS = [
    { label: "Basic file actions", master: true, ids: BASIC },
    { label: "Open and inspect", master: false, ids: ["openTerminal", "copypath"] },
    { label: "Extras", master: false,
      ids: ["compress", "extract", "convert", "taildrop", "dropbox", "sharelink"] }
]

// Open and Show hidden files draw the lock mark instead of a box, and the board says why: a menu
// that cannot open the row under the cursor is not a menu, and the hidden toggle is the one
// background row with no keyboard-independent alternative.
var LOCKED = ["open", "toggleHidden"]

var LABELS = {
    cut: "Cut", copy: "Copy", paste: "Paste", duplicate: "Duplicate", rename: "Rename",
    trash: "Move to Trash", openTerminal: "Open in terminal", copypath: "Copy path",
    compress: "Compress", extract: "Extract",
    convert: "Convert", taildrop: "Send with Taildrop", dropbox: "Move to Dropbox",
    sharelink: "Copy share link", open: "Open", toggleHidden: "Show hidden files"
}

// The two values of the Keys row. SettingsKeys.html draws four; this release ships the toggle the
// public list named, Mac against Windows, over the one key table rather than a preset system.
var PRESETS = ["mac", "windows"]
var PRESET_LABELS = { mac: "Mac", windows: "Windows" }

// Every board row carries a left mark, and a switch wears the mark of the row it governs: these are
// ui/js/Menu.js's own glyphs by action id, which tests/js/settings.js asserts the two agree on.
var GLYPHS = {
    cut: "scissors", copy: "copy", paste: "clipboard", duplicate: "file-plus", rename: "rename",
    trash: "trash", openTerminal: "terminal", copypath: "file-text", compress: "archive",
    extract: "archive-out",
    convert: "sliders", sharelink: "network", open: "folder-open", toggleHidden: "eye"
}

// Taildrop and Dropbox are brand reproductions rather than cut glyphs, so they name a component the
// way a menu entry does; ui/SettingsRow.qml draws the pair exactly as ui/MenuRow.qml does.
var MARKS = { taildrop: "tailscale", dropbox: "dropbox" }

function label(id) {
    return LABELS[id] || id
}

function glyph(id) {
    return GLYPHS[id]
}

function contains(list, id) {
    for (var i = 0; list && i < list.length; i++) {
        if (list[i] === id)
            return true
    }
    return false
}

function isHidden(hidden, id) {
    return contains(hidden, id)
}

// GM's ruling: the count is of ENABLED actions, so "5 of 6" means one of the six is switched off.
function basicEnabled(hidden) {
    var on = 0
    for (var i = 0; i < BASIC.length; i++) {
        if (!isHidden(hidden, BASIC[i]))
            on += 1
    }
    return on
}

// All, some and none are read off the six ids, which is the tri-state the SettingsMenus board draws.
// The master is derived here and never stored, so there is no second value that could disagree.
function masterState(hidden) {
    var on = basicEnabled(hidden)
    if (on === BASIC.length)
        return "all"
    return on === 0 ? "none" : "some"
}

// Activating a checked master switches all six off; an unchecked or partial one switches all six on,
// so the recovering move is always the one keystroke. Unrelated hidden ids are preserved either way.
function toggleMaster(hidden) {
    var enable = masterState(hidden) !== "all"
    var next = []
    for (var i = 0; hidden && i < hidden.length; i++) {
        if (!enable || !contains(BASIC, hidden[i]))
            next.push(hidden[i])
    }
    if (!enable) {
        for (var b = 0; b < BASIC.length; b++) {
            if (!contains(next, BASIC[b]))
                next.push(BASIC[b])
        }
    }
    return next
}

function toggleId(hidden, id) {
    var next = []
    var had = false
    for (var i = 0; hidden && i < hidden.length; i++) {
        if (hidden[i] === id) {
            had = true
            continue
        }
        next.push(hidden[i])
    }
    if (!had)
        next.push(id)
    return next
}

// One row per line of the panel's pane. kind decides what ui/SettingsPanel.qml draws and whether the
// row is a focus stop: group, hint, fact and lock rows are read-only and the cursor steps over them.
// A ruler reports the effective size while Omarchy owns it and only becomes a control on an override.
function focusable(row) {
    if (row.kind === "ruler")
        return row.on === true
    return row.kind === "check" || row.kind === "master" || row.kind === "choice"
}

// state: { textSize, hidden, keyHints, preset, baseSize, monitorScale, cornerRadius, presetKeys }
function rows(section, state) {
    if (section === "display")
        return displayRows(state)
    if (section === "menus")
        return menuRows(state.hidden, state.keyHints)
    return keyRows(state)
}

// The SettingsScale board's own division: Flea owns its text override and Omarchy owns the rest.
// The size follows the desktop until one of TextSize's seven stops is pinned, and the monitor
// scale and the corner rounding are the compositor's, drawn as the read-only facts they are.
function displayRows(state) {
    var follows = TextSize.following(state.textSize)
    var out = [
        { kind: "group", label: "Text size" },
        // Two named values and no more, so the board draws them side by side rather than as a walk.
        { kind: "choice", id: "textMode", label: "Text size", glyph: "type",
          options: ["Follow Omarchy", "Override"],
          value: follows ? "Follow Omarchy" : "Override" },
        // The board's seven-stop ruler, the override's own control and the one place the effective
        // size is read; a size Omarchy invented that is not a stop fills to the nearest one.
        { kind: "ruler", id: "textStop", label: "Effective", value: state.baseSize + "px",
          stops: TextSize.STOPS, on: !follows,
          index: TextSize.STOPS.indexOf(TextSize.nearest(state.baseSize)) }
    ]
    out.push({ kind: "hint", label: "Omarchy owns the size until you override it, and an override "
                                    + "takes one of its own stops, " + TextSize.STOPS.join(", ")
                                    + " px. Ctrl+Shift+Plus and Ctrl+Shift+Minus walk them, and "
                                    + "Ctrl+Shift+0 follows Omarchy again." })
    out.push({ kind: "group", label: "Scale" })
    out.push({ kind: "fact", label: "Scale", glyph: "maximize",
               value: scaleLabel(state.monitorScale) })
    out.push({ kind: "hint",
               label: "Flea follows the compositor value and does not step or cycle it." })
    out.push({ kind: "group", label: "Appearance" })
    out.push({ kind: "fact", label: "Hyprland-aware corners",
               value: "rounding " + Math.round(state.cornerRadius) })
    return out
}

// The compositor's number as Hyprland writes it, 1.00 and 1.25; an unanswered query says so rather
// than reading as 1x, because a wrong number here looks exactly like a right one.
function scaleLabel(scale) {
    if (!(scale > 0))
        return "not reported"
    return (Math.round(scale * 100) / 100) + "x"
}

// The one row of this section that is not a menu action: it governs how every menu row is drawn
// rather than whether it exists, so it sits in its own group and never in MENU_GROUPS.
function menuRows(hidden, keyHints) {
    var out = []
    for (var g = 0; g < MENU_GROUPS.length; g++) {
        var group = MENU_GROUPS[g]
        out.push({ kind: "group", label: group.label })
        if (group.master) {
            out.push({ kind: "master", id: "basic", label: "All basic file actions", glyph: "list",
                       state: masterState(hidden),
                       value: basicEnabled(hidden) + " of " + BASIC.length })
        }
        for (var i = 0; i < group.ids.length; i++) {
            out.push({ kind: "check", id: group.ids[i], label: label(group.ids[i]),
                       glyph: GLYPHS[group.ids[i]], mark: MARKS[group.ids[i]],
                       on: !isHidden(hidden, group.ids[i]) })
        }
    }
    out.push({ kind: "group", label: "Shortcuts" })
    out.push({ kind: "check", id: "keyHints", label: "Show keyboard hints", glyph: "keyboard",
               on: keyHints === true })
    out.push({ kind: "hint", label: "Draws each row's key beside it, and the next move under an "
                                    + "empty folder. Every key stays bound either way." })
    out.push({ kind: "group", label: "Always shown" })
    for (var l = 0; l < LOCKED.length; l++)
        out.push({ kind: "lock", id: LOCKED[l], label: label(LOCKED[l]), glyph: GLYPHS[LOCKED[l]] })
    return out
}

// The chord rows come from keys.toml through the generated Keymap.PRESET_KEYS, so a preset cannot
// advertise a binding it does not have; the caller passes the table in rather than importing it,
// which keeps this file free of the generated one.
function keyRows(state) {
    var out = [
        { kind: "group", label: "Preset" },
        { kind: "choice", id: "preset", label: "Keybinding preset", glyph: "keyboard",
          value: PRESET_LABELS[state.preset] || state.preset },
        { kind: "hint", label: "Mac and Windows, over the one key table. Every other binding is "
                               + "shared, and the change lands in this window at once." },
        { kind: "group", label: "This preset" }
    ]
    var table = state.presetKeys || []
    for (var i = 0; i < table.length; i++) {
        if (table[i].preset === state.preset)
            out.push({ kind: "fact", label: table[i].label, value: table[i].keys })
    }
    out.push({ kind: "hint", label: "Press ? for the keyboard sheet." })
    return out
}

// A read-only row is never the cursor, so both key steps and the opening cursor skip over one; the
// same shape ui/ContextMenu.qml's stepCursor uses, because a settings row and a menu row step alike.
function stepRow(list, from, delta) {
    var i = from + delta
    while (i >= 0 && i < list.length) {
        if (focusable(list[i]))
            return i
        i += delta
    }
    return from
}

function firstRow(list) {
    return list.length > 0 && focusable(list[0]) ? 0 : stepRow(list, 0, 1)
}
