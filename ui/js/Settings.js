.pragma library
.import "TextSize.js" as TextSize
.import "ThumbSize.js" as ThumbSize
.import "Places.js" as Places
.import "Keymap.js" as Keymap
.import "SettingsShelf.js" as Shelf
.import "SettingsAbout.js" as About

// Sections follow the current Desktop boards; their state uses the shared ui.json updater.
var SECTIONS = [
    { id: "view", label: "View", glyph: "sliders" },
    { id: "places", label: "Places", glyph: "star" },
    { id: "shelf", label: "Shelf", glyph: "shelf" },
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

// The six SettingsMenus.html puts under the Basic file actions heading, and the ids ui/js/Menu.js gives those rows.
var BASIC = ["cut", "copy", "paste", "duplicate", "rename", "trash"]

// Storage ids remain stable while the action dispatcher uses descriptive verbs. SettingsMenus rule 3: the master rides its group's heading, so a group is its own id here and a group of one row has no master to ride it, which is why Destructive carries neither box nor count.
var MENU_GROUPS = [
    { id: "basic", label: "Basic file actions", ids: BASIC },
    { id: "destructive", label: "Destructive", ids: ["delete"] },
    { id: "openInspect", label: "Open and inspect",
      ids: ["openwith", "openTerminal", "moveto", "copyto", "properties", "permissions", "copypath"] },
    { id: "extras", label: "Extras", features: ["placeMenu"],  // features gate a surface, not a row
      ids: ["shelf", "compress", "extract", "convert", "taildrop", "localsend", "dropbox", "sharelink", "runScript", "placeMenu", "updateFlea"] }
]

// Open and Show hidden files draw the lock mark instead of a box, and the board says why: a menu that cannot open the row under the cursor is not a menu, and the hidden toggle is the one background row with no keyboard-independent alternative.
var LOCKED = ["open", "toggleHidden"]

var LABELS = {
    cut: "Cut", copy: "Copy", paste: "Paste", duplicate: "Duplicate", rename: "Rename",
    trash: "Move to Trash", openTerminal: "Open in terminal", copypath: "Copy path", permissions: "Permissions",
    delete: "Delete permanently", openwith: "Open with", moveto: "Move to", copyto: "Copy to", properties: "Properties",
    compress: "Compress", extract: "Extract", localsend: "Send with LocalSend",
    convert: "Convert", taildrop: "Send with Taildrop", dropbox: "Move to Dropbox",
    sharelink: "Copy Share Link", open: "Open", toggleHidden: "Show hidden files", shelf: "Enable shelf", placeMenu: "Places row menu", runScript: "Run script",
    updateFlea: "Update Flea"
}

// The four values of the Keys row, in SettingsKeys.html's own chooser order. The first is what a missing or unrecognised stored name resolves to, which that board says is Default.
var PRESETS = ["default", "vim", "mac", "windows"]
var PRESET_LABELS = { "default": "Default", vim: "Vim", mac: "Mac", windows: "Windows" }

// Every board row carries a left mark, and a switch wears the mark of the row it governs: these are ui/js/Menu.js's own glyphs by action id, which tests/js/settings.js asserts the two agree on.
var GLYPHS = {
    cut: "scissors", copy: "copy", paste: "clipboard", duplicate: "file-plus", rename: "rename",
    trash: "trash", openTerminal: "terminal", copypath: "file-text", permissions: "lock", compress: "archive",
    delete: "trash", openwith: "app-window", moveto: "folder-plus", copyto: "copy", properties: "info",
    extract: "archive-out",
    convert: "sliders", sharelink: "network", open: "folder-open", toggleHidden: "eye", placeMenu: "folder-open", runScript: "terminal",
    updateFlea: "download"
}

// Taildrop, LocalSend and Dropbox are brand reproductions rather than cut glyphs, so they name a component the way a menu entry does; ui/SettingsRow.qml draws them exactly as ui/MenuRow.qml does.
var MARKS = { taildrop: "tailscale", localsend: "localsend", dropbox: "dropbox", shelf: "flea", "display.hyprlandIcons": "hyprland" }

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
function basicEnabled(hidden, list) {
    var on = 0
    for (var i = 0; i < list.length; i++) {
        if (!isHidden(hidden, list[i]))
            on += 1
    }
    return on
}

// All, some and none are read off the group's own ids, which is the tri-state the SettingsMenus board draws. A master is derived here and never stored, so there is no second value that could disagree.
function masterState(hidden, list) {
    var on = basicEnabled(hidden, list)
    if (on === list.length)
        return "all"
    return on === 0 ? "none" : "some"
}

// Activating a checked master switches its whole group off; an unchecked or partial one switches the group on, so the recovering move is always the one keystroke. Ids outside the group are preserved either way.
function toggleMaster(hidden, list) {
    var enable = masterState(hidden, list) !== "all"
    var next = []
    for (var i = 0; hidden && i < hidden.length; i++) {
        if (!enable || !contains(list, hidden[i]))
            next.push(hidden[i])
    }
    if (!enable) {
        for (var b = 0; b < list.length; b++) {
            if (!contains(next, list[b]))
                next.push(list[b])
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

// One row per line of the panel's pane. kind decides what ui/SettingsPanel.qml draws and whether the row is a focus stop: group, hint, fact and lock rows are read-only and the cursor steps over them. A ruler reports the effective size while Omarchy owns it and only becomes a control on an override.
function focusable(row) {
    // SettingsGrammar rule 7: a dependent greyed by its parent cannot be operated, so it is no stop either.
    if (row.available === false)
        return false
    // SettingsMenus rule 3 and SettingsRest rule 3: a heading carrying its group's master or its one action is a control, and one carrying neither stays a heading.
    if (row.kind === "group")
        return row.master === true || row.action !== undefined
    if (row.kind === "ruler")
        return row.on === true
    return row.kind === "check" || row.kind === "choice" || row.kind === "action" || row.kind === "favourite"
}

// state: { textSize, hidden, keyHints, preset, baseSize, monitorScale, cornerRadius, presetKeys }
function rows(section, state) {
    if (section === "columns")
        return columnRows(state)
    if (section === "places")
        return placesRows(state)
    if (section === "shelf")
        return Shelf.rows(state, choice)
    if (section === "view")
        return viewRows(state)
    if (section === "preview")
        return previewRows(state)
    if (section === "about")
        return About.rows(state)
    if (section === "display")
        return displayRows(state)
    if (section === "menus")
        return menuRows(state.hidden, state.keyHints)
    return keyRows(state)
}

// The sentence beside Effective has a job only while the ruler cannot state the running size: in Follow the two differ whenever Omarchy's size is not one of the seven, and an override is always a stop.
function effectiveNote(follows, baseSize) {
    var nearest = TextSize.nearest(baseSize)
    return !follows ? "Your override." : nearest === baseSize ? "Omarchy's own size."
        : "Omarchy's own size. The ruler marks " + nearest + ", the nearest stop."
}

// The SettingsScale board's own division: Flea owns its text override and Omarchy owns the rest. The size follows the desktop until one of TextSize's seven stops is pinned, and the monitor scale and the corner rounding are the compositor's, drawn as the read-only facts they are.
function displayRows(state) {
    var follows = TextSize.following(state.textSize)
    var out = [
        { kind: "group", label: "Text size" },
        { kind: "choice", id: "textMode", label: "Text size", glyph: "type",
          labels: ["Follow Omarchy", "Override"],
          value: follows ? "Follow Omarchy" : "Override" },
        // The board's seven-stop ruler, the override's own control, now carrying the numbers it stands for; a size Omarchy invented that is not a stop marks the nearest one.
        { kind: "ruler", id: "textStop", stops: TextSize.STOPS, on: !follows,
          index: TextSize.STOPS.indexOf(TextSize.nearest(state.baseSize)) }
    ]
    // SettingsRest rule 2 keeps only the chords, and HANDOFF rule 8 keeps them to the one line the panel can draw: the board's own sentence wrapped onto two at this width.
    out.push({ kind: "hint", label: "Ctrl+Shift +/- walks them, Ctrl+Shift+0 follows." })
    // And Effective stays: it is state.baseSize, the size actually running, where the ruler marks TextSize.nearest() and a tie takes the smaller stop, so a base of 13 marks 12.
    out.push({ kind: "fact", id: "textEffective", label: "Effective", role: "live",
               value: state.baseSize + " px", caption: effectiveNote(follows, state.baseSize) })
    out.push({ kind: "group", label: "Scale" })
    out.push({ kind: "fact", label: "Scale", glyph: "maximize",
               value: scaleLabel(state.monitorScale) })
    out.push({ kind: "hint",
               label: "Flea follows the compositor value and does not step or cycle it." })
    out.push({ kind: "group", label: "Appearance" })
    out.push({ kind: "check", id: "display.hyprlandIcons", label: "Hyprland-aware icons",
               mark: MARKS["display.hyprlandIcons"],
               on: ((state.data || {}).display || {}).hyprlandIcons === true })
    return out
}

// The compositor's own number, as Hyprland writes it: 1.00 and 1.25. An unanswered query says so rather than reading as 1x, because a wrong number here looks exactly like a right one. The row carries no control, which is what says it cannot be changed; SettingsGrammar rule 5.
function scaleLabel(scale) {
    if (!(scale > 0))
        return "not reported"
    return (Math.round(scale * 100) / 100) + "x"
}

// The one row of this section that is not a menu action: it governs how every menu row is drawn rather than whether it exists, so it sits in its own group and never in MENU_GROUPS.
function menuRows(hidden, keyHints) {
    var out = []
    for (var g = 0; g < MENU_GROUPS.length; g++) {
        var group = MENU_GROUPS[g]
        var master = group.ids.length > 1
        out.push({ kind: "group", label: group.label, id: master ? group.id : "", master: master,
                   ids: group.ids, state: master ? masterState(hidden, group.ids) : "",
                   value: master ? basicEnabled(hidden, group.ids) + " of " + group.ids.length : "" })
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
    // HANDOFF rule 8: one short line. The switch's own label says what it draws, so the hint keeps
    // only the part it cannot: turning it off binds nothing differently.
    out.push({ kind: "hint", label: "Every key stays bound either way." })
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
          labels: PRESETS.map(function (p) { return PRESET_LABELS[p] }),
          value: PRESET_LABELS[state.preset] || state.preset },
        // Rule 3: the hint says what the preset does, where the old one listed the same four labels the control draws.
        { kind: "hint", label: "Keys change, actions do not." },
        { kind: "group", label: "This preset" },
        { kind: "keyPreview", id: "keyPreview", items: keyPreview(state.preset) }
    ]
    out.push({ kind: "hint", label: "Press ? for the keyboard sheet." })
    return out
}

// A read-only row is never the cursor, so both key steps and the opening cursor skip over one; the same shape ui/ContextMenu.qml's stepCursor uses, because a settings row and a menu row step alike.
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

// Choice values are stored separately from labels so presentation never becomes a persistence format. Which control a choice gets is decided by fit, in ui/SettingsRow.qml, not declared here: rule 1 of the SettingsGrammar board, after two named values were a segment and three were a chevron.
function choice(id, label, glyph, values, labels, value) {
    return { kind: "choice", id: id, label: label, glyph: glyph, values: values, labels: labels,
             value: labels[Math.max(0, values.indexOf(value))], selected: value }
}

function viewRows(state) {
    var data = state.data || {}
    var sort = data.sort || {}
    var columns = data.columns || ["name", "size", "date"]
    return [
        { kind: "group", label: "View" },
        choice("view", "Last-used view", undefined, ["list", "columns", "grid", "dual"],
               ["List", "Columns", "Grid", "Dual pane"], data.view || "list"),
        choice("density", "Row density", "list", ["compact", "normal", "comfortable"],
               ["Compact", "Normal", "Comfortable"], data.density || "compact"),
        { kind: "action", id: "columns", label: "Columns", glyph: "columns",
          value: columns.map(function (key) { return key.charAt(0).toUpperCase() + key.slice(1) }).join(", ") },
        choice("addressBar", "Address bar", undefined, ["path", "breadcrumb"],
               ["Path", "Breadcrumb"], data.addressBar || "breadcrumb"),
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
        // The action writes the folder the panel was opened over and selects the mode with it, so the row above never names a chosen folder that was never chosen. The value is the path itself.
        { kind: "action", id: "startFolder", label: "Chosen folder", glyph: "folder",
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
                      ["Automatic", "Manual"], data.loadOn || "automatic")
    // Rule 7: with the column off, SelectionPreview.canRead is false and both load paths return, so Load is the one real dependent here and it greys rather than lying.
    load.available = data.column !== false
    // One value per row, rule 4: the board drops the figure outright, because "64 px  Medium" is one fact twice.
    var size = choice("preview.thumbSize", "Thumbnail size", "maximize", ThumbSize.NAMES,
                      ThumbSize.LABELS, ThumbSize.parse(data.thumbSize))
    var speed = choice("preview.thumbSpeed", "Thumbnail generation", "cpu", ["default", "fast"],
                       ["Default", "Fast"], data.thumbSpeed === "fast" ? "fast" : "default")
    return [
        { kind: "group", label: "Preview column" },
        // Preview board rule 4: the heading names the group, so the row under it names the switch.
        { kind: "check", id: "preview.column", label: "Show the preview column", glyph: "columns", on: data.column !== false },
        load,
        // Rule 3: a hint sits under the control it explains, not two hairlines below it in a footer.
        { kind: "hint", label: data.loadOn === "manual" ? "Ctrl+Space loads the current selection." : "Automatic follows the cursor." },
        { kind: "group", label: "Thumbnails" },
        choice("preview.thumbnails", "Thumbnails", "image", ["off", "images", "media"],
               ["Off", "Images", "Images and video"], data.thumbnails || "media"),
        size,
        // Rule 7: GridArea gates ctrl-scroll on ViewState.ctrlZoom alone and sizes the tiles from it with thumbnails off, so it is grid zoom, it is named that, and it never greys with them.
        { kind: "check", id: "preview.ctrlZoom", label: "Zoom the grid with ctrl and scroll", glyph: "move-horizontal", on: data.ctrlZoom !== false },
        // Grid and columns force cache off, because a regenerated thumbnail keeps its path. This is the opt-in.
        { kind: "check", id: "preview.thumbCache", label: "Keep thumbnails in memory", glyph: "hard-drive", on: data.thumbCache === true },
        speed
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
    // SettingsRest rule 3: the one action that governs the whole group rides its heading, the slot the Menus master takes.
    var rows = [{ kind: "group", label: "Favorites", id: "addFavourite", action: "addFavourite", value: "Add this folder" }]
    for (var i = 0; i < entries.length; i++) {
        rows.push({ kind: "favourite", id: "favourite:" + i, label: entries[i].label,
            value: entries[i].storedPath, glyph: entries[i].glyph, error: entries[i].error || (state.favouriteStatuses || {})[i] || "", favouriteIndex: i })
    }
    rows.push({ kind: "group", label: "Built in" })
    var builtins = [["showHome", "Home", "house"], ["showNetwork", "Network", "network"],
                    ["showDevices", "Devices", "drive"], ["showTrash", "Trash", "trash"]]
    for (var b = 0; b < builtins.length; b++) rows.push({ kind: "check", id: "places." + builtins[b][0], label: builtins[b][1], glyph: builtins[b][2], on: data[builtins[b][0]] !== false })
    rows.push({ kind: "group", label: "Rail" })
    // The fourth field is src/uischema.rs's shipped value, which is what a key the file never stored reads as.
    var rail = [["driveSize", "Show drive size", "drive", false], ["trashCount", "Show Trash count", "trash", false], ["showUnmounted", "Show unmounted drives", "drive", true], ["autoHide", "Auto-hide sidebar", "maximize", false]]
    for (var r = 0; r < rail.length; r++) rows.push({ kind: "check", id: "places." + rail[r][0], label: rail[r][1], glyph: rail[r][2], on: rail[r][3] ? data[rail[r][0]] !== false : data[rail[r][0]] === true })
    // Directive 74: the rail is remembered as a word, so this row reads it as one; ctrl-b writes the same leaf. Directive 77: auto-hide answers the pointer instead, so the remembered choice is greyed while that is on rather than quietly ignored.
    rows.push({ kind: "check", id: "places.rail", label: "Show sidebar", glyph: "columns", on: data.rail !== "hidden", available: data.autoHide !== true })
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
