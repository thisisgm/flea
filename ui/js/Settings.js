.pragma library
.import "TextSize.js" as TextSize
.import "Places.js" as Places
.import "Keymap.js" as Keymap

// Sections follow the current Desktop boards; their state uses the shared ui.json updater.
var SECTIONS = [
    { id: "view", label: "View", glyph: "sliders" },
    { id: "places", label: "Places", glyph: "star" },
    { id: "preview", label: "Preview", glyph: "preview" },
    { id: "keys", label: "Keys", glyph: "keyboard" },
    { id: "display", label: "Display", glyph: "maximize" },
    { id: "menus", label: "Menus", glyph: "list" },
    { id: "about", label: "About", glyph: "info" }
]

// Where a section id sits in SECTIONS, or 0 for an id no section carries.
function sectionIndex(id) {
    for (var i = 0; i < SECTIONS.length; i++) {
        if (SECTIONS[i].id === id)
            return i
    }
    return 0
}

// The six SettingsMenus.html puts under one master row, and the ids ui/js/Menu.js gives those rows.
var BASIC = ["cut", "copy", "paste", "duplicate", "rename", "trash"]

// Storage ids remain stable while the action dispatcher uses descriptive verbs.
var MENU_GROUPS = [
    { label: "Basic file actions", master: true, ids: BASIC },
    { label: "Destructive", master: false, ids: ["delete"] },
    { label: "Open and inspect", master: false,
      ids: ["openwith", "openTerminal", "moveto", "copyto", "properties", "permissions", "copypath"] },
    { label: "Extras", master: false,
      ids: ["compress", "extract", "convert", "taildrop", "dropbox", "sharelink"] }
]

// Open and Show hidden files draw the lock mark instead of a box, and the board says why: a menu
// that cannot open the row under the cursor is not a menu, and the hidden toggle is the one
// background row with no keyboard-independent alternative.
var LOCKED = ["open", "toggleHidden"]

var LABELS = {
    cut: "Cut", copy: "Copy", paste: "Paste", duplicate: "Duplicate", rename: "Rename",
    trash: "Move to Trash", openTerminal: "Open in terminal", copypath: "Copy path", permissions: "Permissions",
    delete: "Delete permanently", openwith: "Open with", moveto: "Move to", copyto: "Copy to", properties: "Properties",
    compress: "Compress", extract: "Extract",
    convert: "Convert", taildrop: "Send with Taildrop", dropbox: "Move to Dropbox",
    sharelink: "Copy Share Link", open: "Open", toggleHidden: "Show hidden files"
}

// The four values of the Keys row, in SettingsKeys.html's own chooser order. The first is what a
// missing or unrecognised stored name resolves to, which that board says is Default.
var PRESETS = ["default", "vim", "mac", "windows"]
var PRESET_LABELS = { "default": "Default", vim: "Vim", mac: "Mac", windows: "Windows" }

// Every board row carries a left mark, and a switch wears the mark of the row it governs: these are
// ui/js/Menu.js's own glyphs by action id, which tests/js/settings.js asserts the two agree on.
var GLYPHS = {
    cut: "scissors", copy: "copy", paste: "clipboard", duplicate: "file-plus", rename: "rename",
    trash: "trash", openTerminal: "terminal", copypath: "file-text", permissions: "lock", compress: "archive",
    delete: "trash", openwith: "app-window", moveto: "folder-plus", copyto: "copy", properties: "info",
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
    return row.kind === "check" || row.kind === "master" || row.kind === "choice" || row.kind === "action" || row.kind === "favourite" || row.kind === "favouriteActions"
}

// state: { textSize, hidden, keyHints, preset, baseSize, monitorScale, cornerRadius, presetKeys }
function rows(section, state) {
    if (section === "columns")
        return columnRows(state)
    if (section === "places")
        return placesRows(state)
    if (section === "view")
        return viewRows(state)
    if (section === "preview")
        return previewRows(state)
    if (section === "about")
        return aboutRows(state.about || {})
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
    out.push({ kind: "check", id: "display.hyprlandIcons", label: "Hyprland-aware icons",
               on: ((state.data || {}).display || {}).hyprlandIcons === true })
    return out
}

// The SettingsScale board's own cell: the word first, then the compositor's number as Hyprland writes
// it, 1.00 and 1.25; an unanswered query says so rather
// than reading as 1x, because a wrong number here looks exactly like a right one.
function scaleLabel(scale) {
    if (!(scale > 0))
        return "not reported"
    return "Read-only " + (Math.round(scale * 100) / 100) + "x"
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
                       role: group.ids[i] === "delete" ? "error" : "",
                       value: group.ids[i] === "delete" ? "off by default" : "",
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

// Settings shows six examples; the keyboard sheet retains the complete generated inventory.
function keyPreview(preset) {
    var bindings = Keymap.bindingRows(preset, "gui")
    var enter = Keymap.lookupFor(preset, Qt.Key_Return, "", 0, "listing", "gui")
    var items = [
        { keys: "arrows", label: preset === "mac" ? "move, open, up" : "move cursor", glyph: "" },
        { keys: "enter", label: enter, glyph: "" },
        { keys: "space", label: "quick look", glyph: "" }
    ]
    var actions = ["copy", "paste", "trash"]
    for (var i = 0; i < actions.length; i++) {
        var action = actions[i]
        var mods = preset === "mac" ? "super" : preset === "windows" ? "ctrl" : "text"
        if (action === "trash" && (preset === "mac" || preset === "windows")) mods = "none"
        for (var j = 0; j < bindings.length; j++) {
            var binding = bindings[j]
            if (Keymap.actionGroup(binding.action) === action && binding.mods === mods) {
                items.push({ keys: binding.keys, label: action, glyph: action === "trash" ? "" : GLYPHS[action] })
                break
            }
        }
    }
    return items
}

function keyRows(state) {
    var out = [
        { kind: "group", label: "Preset" },
        { kind: "choice", id: "preset", label: "Keybinding preset", glyph: "keyboard",
          options: PRESETS.map(function (p) { return PRESET_LABELS[p] }),
          value: PRESET_LABELS[state.preset] || state.preset },
        { kind: "hint", label: "Default \u00b7 Vim \u00b7 Mac \u00b7 Windows" },
        { kind: "group", label: "This preset" },
        { kind: "keyPreview", id: "keyPreview", items: keyPreview(state.preset) }
    ]
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

// Choice values are stored separately from labels so presentation never becomes a persistence format.
function choice(id, label, glyph, values, labels, value, segmented) {
    return { kind: "choice", id: id, label: label, glyph: glyph, values: values,
             options: segmented ? labels : undefined, labels: labels,
             value: labels[Math.max(0, values.indexOf(value))], selected: value }
}

function viewRows(state) {
    var data = state.data || {}
    var sort = data.sort || {}
    var columns = data.columns || ["name", "size", "date"]
    return [
        { kind: "group", label: "View" },
        choice("view", "Last-used view", undefined, ["list", "columns", "grid", "dual"],
               ["List", "Columns", "Grid", "Dual pane"], data.view || "list", true),
        choice("density", "Row density", "list", ["compact", "normal", "comfortable"],
               ["Compact", "Normal", "Comfortable"], data.density || "normal"),
        { kind: "action", id: "columns", label: "Columns", glyph: "columns",
          value: columns.map(function (key) { return key.charAt(0).toUpperCase() + key.slice(1) }).join(", ") },
        choice("addressBar", "Address bar", undefined, ["path", "breadcrumb"],
               ["Path", "Breadcrumb"], data.addressBar || "breadcrumb", true),
        { kind: "group", label: "Sorting" },
        choice("sort.key", "Sort by", "sort", ["name", "size", "date", "kind"],
               ["Name", "Size", "Date", "Kind"], sort.key || "name"),
        { kind: "check", id: "foldersFirst", label: "Folders first", glyph: "folders-first", on: data.foldersFirst !== false },
        { kind: "check", id: "groupByKind", label: "Group by kind", caption: "folders, photos, files", glyph: "grid", on: data.groupByKind === true },
        { kind: "check", id: "hidden", label: "Show hidden files", glyph: "eye", on: data.hidden === true },
        { kind: "group", label: "Cursor" },
        { kind: "check", id: "wrapAtEnds", label: "Wrap at list ends", caption: "arrow-up at the top", glyph: "arrow-up", on: data.wrapAtEnds === true },
        { kind: "group", label: "Opening" },
        choice("startIn", "Flea opens in", "house", ["home", "last", "folder"],
               ["Home", "Last folder", "Chosen folder"], data.startIn || "home"),
        // The action writes the folder the panel was opened over and selects the mode with it, so the
        // row above never names a chosen folder that was never chosen. The value is the path itself.
        { kind: "action", id: "startFolder", label: "Chosen folder", glyph: "folder", indented: true,
          value: data.startFolder || "Use this folder" },
        choice("newTab", "New tabs open in", "columns", ["current", "home", "start"],
               ["Current folder", "Home", "Start folder"], data.newTab || "current"),
        { kind: "hint", footer: true, label: state.saveStatus || "Saved · applied in this process",
          role: (state.saveStatus || "").indexOf("Could not") === 0 ? "error" : "accent" }
    ]
}

function previewRows(state) {
    var data = (state.data || {}).preview || {}
    var load = choice("preview.loadOn", "Load", "eye", ["automatic", "manual"],
                      ["Automatic", "Manual"], data.loadOn || "automatic", true)
    load.indented = true
    var size = choice("preview.thumbSize", "Thumbnail size", "maximize", ["small", "medium", "large", "xlarge"],
                      ["Small", "Medium", "Large", "Extra large"], data.thumbSize || "medium")
    size.indented = true
    size.caption = [48, 64, 96, 128][Math.max(0, size.values.indexOf(size.selected))] + " px"
    return [
        { kind: "group", label: "Preview column" },
        { kind: "check", id: "preview.column", label: "Preview column", glyph: "columns", on: data.column !== false },
        load,
        { kind: "group", label: "Thumbnails" },
        choice("preview.thumbnails", "Thumbnails", "image", ["off", "images", "media"],
               ["Off", "Images", "Images and video"], data.thumbnails || "media"),
        size,
        { kind: "check", id: "preview.ctrlZoom", label: "Zoom with ctrl and scroll", indented: true, on: data.ctrlZoom !== false },
        { kind: "hint", footer: true, role: "foreground", label: data.loadOn === "manual" ? "Ctrl+Space loads the current selection." : "Automatic follows keyboard or pointer selection." }
    ]
}

function aboutRows(facts) {
    return [
        { kind: "hero", label: "Flea", value: "A file manager for Omarchy" },
        { kind: "fact", label: "Version", value: facts.version || "Not reported" },
        { kind: "fact", label: "Built", value: facts.built || "Not recorded in this build" },
        { kind: "fact", label: "Installed from", value: facts.source || "Not reported" },
        { kind: "fact", label: "Package", value: facts.package || "Not reported" },
        { kind: "fact", label: "Licence", value: "MIT, © 2026 GM" },
        { kind: "group", label: "Language" },
        { kind: "fact", label: "Language", glyph: "globe", value: "English · read-only" },
        { kind: "group", label: "Updates" },
        { kind: "fact", label: "Update owner", glyph: "download", value: "Omarchy · read-only" },
        { kind: "group", label: "This box" },
        { kind: "fact", label: "File manager", glyph: "folder", value: (facts.handler || "Not reported") + " · status only" },
        { kind: "action", id: "keyboardSheet", label: "Keyboard sheet", glyph: "keyboard", value: "?" },
        { kind: "action", id: "reportIssue", label: "Report an issue", glyph: "network", value: "Open" },
        { kind: "action", id: "support", label: "Support Flea", glyph: "star", value: "buymeacoffee" }
    ]
}

function columnRows(state) {
    var columns = (state.data || {}).columns || ["name", "size", "date"]
    var rows = [{ kind: "group", label: "Columns" }, { kind: "lock", label: "Name", glyph: "file" }]
    for (var i = 0; i < 4; i++) {
        var id = ["mode", "size", "date", "kind"][i]
        rows.push({ kind: "check", id: "column:" + id, label: ["Mode", "Size", "Date", "Kind"][i], on: columns.indexOf(id) >= 0 })
    }
    rows.push({ kind: "action", id: "backView", label: "Back to View", value: "Back" })
    return rows
}

function placesRows(state) {
    var data = (state.data || {}).places || {}
    var entries = Places.storedEntries(data.favourites || [], state.home || "")
    var rows = [{ kind: "group", label: "Favorites" }]
    for (var i = 0; i < entries.length; i++) {
        rows.push({ kind: "favourite", id: "favourite:" + i, label: entries[i].label,
            value: entries[i].storedPath, glyph: entries[i].glyph, error: entries[i].error || (state.favouriteStatuses || {})[i] || "", favouriteIndex: i })
    }
    rows.push({ kind: "favouriteActions", id: "favouriteActions", label: "Add current folder", value: "Remove",
        actionIndex: state.favouriteAction || 0, canRemove: state.selectedFavourite >= 0 && state.selectedFavourite < entries.length })
    rows.push({ kind: "group", label: "Built in" })
    var builtins = [["showHome", "Home", "house"], ["showNetwork", "Network", "network"],
                    ["showDevices", "Devices", "drive"], ["showTrash", "Trash", "trash"]]
    for (var b = 0; b < builtins.length; b++) {
        rows.push({ kind: "check", id: "places." + builtins[b][0], label: builtins[b][1], glyph: builtins[b][2], on: data[builtins[b][0]] !== false })
    }
    rows.push({ kind: "group", label: "Rail" })
    rows.push({ kind: "check", id: "places.driveSize", label: "Show drive size", glyph: "drive", on: data.driveSize === true })
    rows.push({ kind: "check", id: "places.trashCount", label: "Show Trash count", glyph: "trash", on: data.trashCount === true })
    rows.push(choice("places.sidebarWidth", "Sidebar width", "maximize", Places.WIDTH_STOPS,
        ["160 px", "192 px", "224 px", "256 px"], Places.sidebarWidth(data.sidebarWidth)))
    // Trash lives in Places, and the sweep is off until switched on: permanent deletion is outside the undo journal.
    rows.push({ kind: "group", label: "Trash" })
    // The eyebrow says TRASH, so the label does not repeat it; a fuller one elided the caption to "permanently, once a...".
    rows.push({ kind: "check", id: "trashAutoEmpty", label: "Empty after 30 days",
        caption: "permanently", glyph: "history",
        on: (state.data || {}).trashAutoEmpty === true })
    return rows
}
