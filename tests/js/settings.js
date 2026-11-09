.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/TextSize.js" as TextSize

// The settings panel's model. ui/SettingsPanel.qml only paints what rows() returns, so every row a section can draw, and every value a control can hold, is assertable here without a window.

function run(check) {
    runRows(check)
    runCursor(check)
    runPresets(check)
    runCompletionRows(check)
}

// The Display state ui/SettingsPanel.qml passes in: the stored mode, the size ui/Theme.qml resolved from it, and the two numbers Flea reads off the compositor and never writes.
function displayState(textSize, baseSize, monitorScale) {
    return { textSize: textSize, baseSize: baseSize,
             monitorScale: monitorScale === undefined ? 1 : monitorScale, cornerRadius: 8 }
}

// Runs body with the rows for the given actions taken out of the generated tables, then puts them back where they were.
function withoutKeys(actions, body) {
    var tables = [Keymap.PRESET_KEYS, Keymap.SHARED_KEYS], held = []
    for (var t = 0; t < tables.length; t++) {
        for (var i = tables[t].length - 1; i >= 0; i--) {
            if (actions.indexOf(tables[t][i].action) >= 0) held.push({ table: tables[t], at: i, row: tables[t].splice(i, 1)[0] })
        }
    }
    Keymap.setPreset(Keymap.preset)
    try {
        body()
    } finally {
        for (var h = held.length - 1; h >= 0; h--) held[h].table.splice(held[h].at, 0, held[h].row)
        Keymap.setPreset(Keymap.preset)
    }
}

function kinds(rows) {
    return rows.map(function (row) { return row.kind }).join("|")
}

function find(rows, id) {
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].id === id)
            return rows[i]
    }
    return {}
}

function runRows(check) {
    var display = Settings.rows("display", displayState(TextSize.follow(), 14))
    // The board's Display card: the text-size mode over its effective size, then the compositor's two read-only facts. No monitor-scale control, because Flea does not step or cycle that one.
    check("the Display section is text size, then Scale, then Appearance",
          kinds(display), "group|choice|ruler|hint|fact|group|fact|hint|group|check")
    check("its one control opens on Follow Omarchy", find(display, "textMode").value,
          "Follow Omarchy")
    // The board draws the mode as both names side by side, so the row names them rather than leaving ui/SettingsRow.qml to invent a second list that could disagree with the writer.
    check("and it names both its values, in the board's own order",
          find(display, "textMode").labels.join("|"), "Follow Omarchy|Override")
    // SettingsRest rule 2: the ruler carries the seven numbers, so it states the stop it marks and the hint keeps only the chords.
    check("the ruler stands for the stops themselves", display[2].stops.join(","), "9,10,11,12,14,16,20")
    check("and marks the one in force, five of the seven", display[2].index, 4)
    // An Omarchy size that is not one of the seven still marks a stop, the nearest one.
    var between = Settings.rows("display", displayState(TextSize.follow(), 13))
    check("a size between two stops marks the nearer one", between[2].index, 3)
    // The keys come from the running preset, spelled as a chord; the default preset binds these three.
    check("the hint names the keys that step through the stops and the one that follows Omarchy", display[3].label,
          "Ctrl+Shift++ and Ctrl+Shift+- step through them; Ctrl+Shift+0 follows Omarchy again.")
    // And Effective stays, because in Follow it is the size running and the ruler's mark is not.
    check("Effective reports the size actually running", display[4].label + "|" + display[4].value + "|" + display[4].role,
          "Effective|14 px|live")
    check("and says so when the ruler cannot state it", between[4].value + "|" + between[4].caption,
          "13 px|Omarchy's own size. The ruler marks 12, the nearest stop.")
    check("a size that is a stop needs no such sentence", display[4].caption, "Omarchy's own size.")
    check("the monitor scale is the compositor's own number, and the row carries no control", display[6].value, "1x")
    check("a fractional one keeps its fraction",
          Settings.rows("display", displayState(TextSize.follow(), 14, 1.25))[6].value, "1.25x")
    check("and an unanswered query says so rather than claiming 1x",
          Settings.rows("display", displayState(TextSize.follow(), 14, 0))[6].value, "not reported")
    check("its hint is the board's own sentence, so no reader expects a control",
          display[7].label, "Flea follows Hyprland's scale and does not change it.")
    check("the board icon override defaults off", display[9].id + "|" + display[9].on, "display.hyprlandIcons|false")

    // Switching to Override adds the stop row, and nothing else about the section moves.
    var pinned = Settings.rows("display", displayState({ mode: 16 }, 16))
    check("an override adds no row, it turns the ruler into the control", pinned.length,
          display.length)
    check("the mode row says which mode it is in", find(pinned, "textMode").value, "Override")
    check("Effective carries the pinned size, and names whose it is",
          find(pinned, "textEffective").value + "|" + find(pinned, "textEffective").caption, "16 px|Your override.")
    check("and the ruler marks one stop further along", pinned[2].index, 5)
    check("the ruler is live only while the override is", find(pinned, "textStop").on, true)
    check("and while following it reports rather than sets",
          find(display, "textStop").on, false)

    var menus = Settings.rows("menus", { hidden: ["paste"] })
    check("the Menus section leads with the heading its master rides, and the rows follow it",
          kinds(menus).indexOf("group|check|check") === 0, true)
    check("the master's count is drawn on that heading", menus[0].value, "5 of 6")
    // The mirror of the same ruling: five ids in the set is one action left ON, never "5 of 6".
    check("and five hidden ids read as one enabled, not five",
          Settings.rows("menus", { hidden: ["cut", "copy", "paste", "duplicate", "rename"] })[0].value,
          "1 of 6")
    // Rule 5: the row that can destroy a file carries the urgent role and the caption the model gives it.
    check("Delete permanently is the one row drawn urgent, and it says it is off by default",
          menus.filter(function (r) { return r.role === "error" })
               .map(function (r) { return r.id + "|" + r.value + "|" + r.on }).join(","),
          "delete|off by default|true")
    check("a hidden action's row is drawn unchecked, not dropped",
          find(menus, "paste").on, false)
    check("and an enabled one is checked", find(menus, "copy").on, true)
    check("the current menu controls include Permissions, Update Flea and the retained hints preference",
          menus.filter(function (r) { return r.kind === "check" })
               .map(function (r) { return r.id }).join(","),
          "cut,copy,paste,duplicate,rename,trash,delete,openwith,run,openAsText,openTerminal,moveto,copyto,properties,permissions,makeExecutable,copyAs,showOriginal,tags,shelf,compress,extract,convert,flashDrive,taildrop,localsend,dropbox,sharelink,runScript,placeMenu,updateFlea,toggleHidden,toggleLangMarks,extThumbs,pasteAs,invertSelection,keyHints")
    // The one check that is not a menu action: it says how every row is drawn, not whether it is. Since 2026-09-10 it is off by default, with the rail's own detail rows, and src/uischema.rs stores that default, so an absent preference reads off and not on.
    check("the hints row defaults off, as ruled over the boards",
          find(menus, "keyHints").label + "|" + find(menus, "keyHints").on,
          "Show keyboard hints|false")
    check("and it reads the value it is given",
          find(Settings.rows("menus", { hidden: [], keyHints: true }), "keyHints").on, true)
    check("an explicitly disabled hints preference stays off",
          find(Settings.rows("menus", { hidden: [], keyHints: false }), "keyHints").on, false)
    // Open in terminal was drawn by every menu with no way to switch it off, because the shipped hidden set named it "terminal" and ui/js/Menu.js builds the row as "openTerminal".
    check("Open in terminal is a switch like any other action row",
          find(menus, "openTerminal").label + "|" + find(menus, "openTerminal").glyph,
          "Open in terminal|terminal")
    // The board shows the locked row so the section is a complete list of what a menu can hold; Show hidden files has a switch.
    check("Open is listed, locked rather than omitted",
          menus.filter(function (r) { return r.kind === "lock" })
               .map(function (r) { return r.id }).join(","), "open")
    check("and no locked row is a checkbox", find(menus, "open").kind, "lock")

    var keys = Settings.rows("keys", { preset: "mac", presetKeys: Keymap.PRESET_KEYS })
    check("the Keys section leads with the preset choice", keys[1].kind, "choice")
    check("and shows the selected preset by name", keys[1].value, "Mac")
    // The board draws all four, and SettingsRow decides segment against chevron by whether they fit.
    check("the preset row carries all four names, in the chooser's own order",
          (keys[1].labels || []).join("|"), "Default|Vim|Mac|Windows")
    // Rule 3: a hint that repeats its control's own four labels is not a hint; this one says what a preset changes.
    check("the preset hint says what changes rather than listing the labels again", keys[2].kind + "|" + keys[2].label, "hint|Keys change, actions do not.")
    check("the Windows preset is shown by name too",
          Settings.rows("keys", { preset: "windows", presetKeys: Keymap.PRESET_KEYS })[1].value,
          "Windows")
}

function runCursor(check) {
    var menus = Settings.rows("menus", { hidden: [] })
    check("a heading with no master is never a focus stop", Settings.focusable(menus[7]), false)
    check("so the opening cursor lands on the master heading the section leads with", Settings.firstRow(menus), 0)
    check("a check row is a focus stop", Settings.focusable(menus[2]), true)
    check("a locked row is not", Settings.focusable(menus[menus.length - 1]), false)
    // Stepping past the last focus stop keeps the cursor where it is, the way the context menu's own stepCursor does, so the two locked rows at the bottom cannot swallow it.
    check("stepping down off the end holds the cursor on the last control",
          Settings.stepRow(menus, menus.length - 3, 1), menus.length - 3)
    check("stepping up off the top holds it on the first", Settings.stepRow(menus, 0, -1), 0)
    check("a step down crosses the heading of a group that has no master",
          Settings.focusable(menus[Settings.stepRow(menus, 6, 1)]), true)

    var display = Settings.rows("display", displayState(TextSize.follow(), 14))
    check("the Display section's only control is where its cursor opens",
          Settings.firstRow(display), 1)
    check("and no read-only fact below it takes the cursor",
          Settings.stepRow(display, 1, 1), 9)
    var pinned = Settings.rows("display", displayState({ mode: 16 }, 16))
    check("an override gives the cursor a second stop to walk to",
          Settings.stepRow(pinned, 1, 1), 2)
    check("and the compositor's rows still take none",
          Settings.stepRow(pinned, 2, 1), 9)
}

// SettingsKeys.html's four-value chooser over the one key table. Each row the Keys section lists is resolved back through the generated overlay, so a listed chord cannot advertise a binding the preset lacks, and every one of the four claims a chord rather than drawing a heading over nothing.
function runPresets(check) {
    check("preset chooser preserves authoritative order", Settings.PRESETS.join(","), "default,vim,mac,windows")
    check("preset chooser labels remain explicit", Settings.PRESETS.map(function (id) { return Settings.PRESET_LABELS[id] }).join(","), "Default,Vim,Mac,Windows")
    check("missing preset resolves to Default", Settings.PRESETS[0], "default")
    var total = 0
    for (var i = 0; i < Settings.PRESETS.length; i++) {
        var preset = Settings.PRESETS[i]
        var section = Settings.rows("keys", { preset: preset })
        var table = Keymap.bindingRows(preset, "gui")
        check(preset + " section uses its selected label", section[1].value, Settings.PRESET_LABELS[preset])
        var preview = find(section, "keyPreview")
        check(preset + " section shows the six board examples", preview.items.length, 6)
        check(preset + " examples never take keyboard focus", Settings.focusable(preview), false)
        var primary = { default: "y,p,dd", vim: "yy,pp,D", mac: "super-c,super-v,delete", windows: "ctrl-c,ctrl-v,delete" }
        check(preset + " preview uses its primary bindings", preview.items.slice(3).map(function (r) { return r.keys }).join(","), primary[preset])
        check(preset + " enter label follows its actual action", preview.items[1].label,
              Keymap.lookupFor(preset, Qt.Key_Return, "", 0, "listing", "gui"))
        for (var p = 3; p < preview.items.length; p++) {
            var item = preview.items[p]
            check(preset + " preview chord exists: " + item.keys, table.some(function (r) {
                return r.keys === item.keys && Keymap.actionGroup(r.action) === item.label
            }), true)
        }
        check(preset + " section contains actual bindings", table.length > 20, true)
        for (var j = 0; j < table.length; j++) {
            var row = table[j]
            check(preset + " advertised " + row.keys + " binding", Keymap.lookupFor(preset, row.keycode, row.text, row.mask, "listing", "gui"), row.action)
            total++
        }
    }
    check("preset check denominator covers all effective bindings", total > 100, true)
    // A key the preset leaves unbound is named by no hint: the hint is left out and never drawn empty or with a hole in its sentence.
    var running = Keymap.preset, hollow = []
    try {
        for (var h = 0; h < Settings.PRESETS.length; h++) {
            Keymap.setPreset(Settings.PRESETS[h])
            var drawn = Settings.rows("display", displayState(TextSize.follow(), 14)).concat(Settings.rows("keys", { preset: Settings.PRESETS[h] }),
                Settings.rows("preview", { data: { preview: { loadOn: "manual" } } }))
            var bare = drawn.filter(function (r) { return r.kind === "hint" && (String(r.label).trim().length === 0 || /^ | {2}/.test(r.label)) })
            if (bare.length > 0 || /^ | {2}/.test(TextSize.announce({ mode: 16 }, 14))) hollow.push(Settings.PRESETS[h])
        }
    } finally {
        Keymap.setPreset(running)
    }
    check("no preset draws an empty hint row or a sentence with a key missing from it", hollow.join(","), "")
    // Every shipped preset binds the four actions, so the omitting branches only run on a table that leaves them unbound; the control first shows the bound rows draw both hints.
    var hintCount = function (test) { return Settings.rows("display", displayState(TextSize.follow(), 14)).concat(Settings.rows("preview", { data: { preview: { loadOn: "manual" } } })).filter(function (r) { return r.kind === "hint" && test.test(String(r.label)) }).length }
    check("with the keys bound both hints draw, so the zeros below are not vacuous", Math.min(hintCount(/step through them/), hintCount(/loads the current selection/)) >= 1, true)
    withoutKeys(["textSizeUp", "textSizeDown", "textSizeReset", "loadPreview"], function () {
        check("an unbound text size key leaves its hint out of Display", hintCount(/step through them/), 0)
        check("an unbound load key leaves its hint out of Preview", hintCount(/loads the current selection/), 0)
        check("no hint left on those panes is empty or holed", hintCount(/^\s*$|^ | {2}| [.,;]/), 0)
        check("an override with no reset key announces the size alone", TextSize.announce({ mode: 16 }, 14), "Text size 16px")
    })
    var menuRows = Settings.menuRows([], true)
    check("SettingsMenus contains exactly 36 action switches", menuRows.filter(function (r) { return r.kind === "check" && r.id !== "keyHints" }).length, 36)
    // The board's Tags row is one of those switches.
    check("the added switch is the Tags menu row", find(menuRows, "tags").label, "Tags")
    check("Delete permanently is visually destructive", find(menuRows, "delete").role, "error")
    check("Delete permanently explains its default", find(menuRows, "delete").value, "off by default")
}

function runCompletionRows(check) {
    var code = find(Settings.rows("view", {}), "langMarks")
    check("code icons have an on-by-default View checkbox", [code.kind, code.label, code.glyph, code.on].join("|"), "check|Show code icons|code|true")
    check("the View checkbox respects saved OFF", find(Settings.rows("view", { data: { langMarks: false } }), "langMarks").on, false)
    check("invalid code-icon values heal to the enabled checkbox", find(Settings.rows("view", { data: { langMarks: "false" } }), "langMarks").on, true)
    var places = Settings.rows("places", {})
    check("Places spells the manager group Favorites", places[0].label, "Favorites")
    // SettingsRest rules 3 and 4: Add rides the heading it governs, and nothing floats under the list.
    check("Add rides the Favorites heading and is a focus stop there",
          [places[0].action, places[0].value, Settings.focusable(places[0])].join("|"),
          "addFavourite|Add this folder|true")
    check("and the two buttons under the list are gone",
          places.filter(function (row) { return row.kind === "favouriteActions" }).length, 0)
    // Sidebar040: a favourite's path reads as the person writes it, while value keeps the stored absolute path.
    var homed = Settings.rows("places", { home: "/home/gm", data: { places: { favourites: [
        { label: "Projects", path: "/home/gm/Projects" }, { label: "flea", path: "~/Documents/claude/flea" },
        { label: "Archive", path: "/srv/archive" }, { label: "Sibling", path: "/home/gmx/s" }] } } })
    check("a favourite inside home shows ~/ and keeps the absolute value",
          [find(homed, "favourite:0").display, find(homed, "favourite:0").value].join("|"), "~/Projects|/home/gm/Projects")
    check("a stored tilde path shows the same tilde form", find(homed, "favourite:1").display, "~/Documents/claude/flea")
    check("a favourite outside home shows its absolute path", find(homed, "favourite:2").display, "/srv/archive")
    check("a sibling named like home is outside it", find(homed, "favourite:3").display, "/home/gmx/s")
    check("no home published shows the stored path", find(Settings.rows("places", { data: { places: { favourites: [{ label: "P", path: "/home/gm/P" }] } } }), "favourite:0").display, "/home/gm/P")
    check("optional rail details default off", [find(places, "places.driveSize").on, find(places, "places.trashCount").on].join(","), "false,false")
    // Decided in 0.3.3: unmounted drives ship on, and an off the file stored is still the operator's.
    check("Show unmounted drives defaults on and a stored off reads off", [find(places, "places.showUnmounted").on,
          find(Settings.rows("places", { data: { places: { showUnmounted: false } } }), "places.showUnmounted").on].join(","), "true,false")
    // Sidebar040: Recent is a Built in row like Home and Trash, and it ships off.
    check("Recent ships off and a stored on reads back on", [find(places, "places.showRecent").label + "|" + find(places, "places.showRecent").on,
          find(Settings.rows("places", { data: { places: { showRecent: true } } }), "places.showRecent").on].join(","), "Recent|false,true")
    check("the Rail controls follow the ruled order", places.slice(-7, -2).map(function (row) { return row.label }).join("|"), "Show unmounted drives|Show tags|Auto-hide sidebar|Show sidebar|Sidebar width")
    // Directive 74: two handles on one remembered state, so the row reads the word ctrl-b writes.
    check("Show sidebar is checked while the rail is shown", find(places, "places.rail").on, true)
    check("and auto-hide ships off, so nothing hides itself", find(places, "places.autoHide").on, false)
    // Directive 77: with auto-hide on the pointer governs the rail, so the remembered choice is
    // greyed and not a stop, the same treatment every other dependent row gets.
    var hiding = Settings.rows("places", { data: { places: { autoHide: true } } })
    check("Show sidebar is greyed while auto-hide is on",
          [find(hiding, "places.rail").available, Settings.focusable(find(hiding, "places.rail"))].join("|"), "false|false")
    check("and it is a control again with auto-hide off",
          [find(places, "places.rail").available, Settings.focusable(find(places, "places.rail"))].join("|"), "true|true")
    // The 30 day sweep's own row, at the foot of Places under its own eyebrow. Off unless ui.json says otherwise, which is the whole of the opt-in behavior as the panel sees it.
    check("Places ends with the Trash group and its one row",
          places.slice(-2).map(function (row) { return row.label }).join("|"),
          "Trash|Empty after 30 days")
    check("the sweep is off on a fresh install", find(places, "trashAutoEmpty").on, false)
    check("and says what it does and how often", find(places, "trashAutoEmpty").caption, "permanently")
    check("a ui.json that switched it on reads back on",
          find(Settings.rows("places", { data: { trashAutoEmpty: true } }), "trashAutoEmpty").on, true)
    var detailedPlaces = Settings.rows("places", { data: { places: { driveSize: true, trashCount: true } } })
    check("both rail detail controls reflect persisted on values", [find(detailedPlaces, "places.driveSize").on, find(detailedPlaces, "places.trashCount").on].join(","), "true,true")
    var state = { data: { view: "grid", density: "comfortable", columns: ["name", "kind"],
        preview: { column: false, loadOn: "manual", thumbnails: "off", thumbSize: "xlarge", ctrlZoom: false } } }
    var view = Settings.rows("view", state)
    check("View displays the persisted view", find(view, "view").selected, "grid")
    check("View preserves optional column choices", find(view, "columns").value, "Name, Kind")
    check("View density uses schema values", find(view, "density").selected, "comfortable")
    check("Row density defaults to compact", find(Settings.rows("view", { data: {} }), "density").selected, "compact")
    check("Folders first uses its distinct ordering mark", find(view, "foldersFirst").glyph, "folders-first")
    check("Preview rail mark differs from the three-column view", Settings.SECTIONS[Settings.sectionIndex("preview")].glyph, "preview")
    check("grouping explains the categories before it is enabled", find(view, "groupByKind").caption, "folders, photos, files")
    check("wrapping explains the boundary before it is enabled", find(view, "wrapAtEnds").caption, "arrow-up at the top")
    check("grid row wrap ships off beside it",
          find(view, "wrapGridRows").label + "|" + find(view, "wrapGridRows").on + "|" + find(view, "wrapGridRows").caption,
          "Continue onto the next row|false|right at a row's end")
    check("and a stored on reads back on",
          find(Settings.rows("view", { data: { wrapGridRows: true } }), "wrapGridRows").on, true)
    // Issue 29 and ClickAndRefresh: Escape stays put until switched on, one tap opens only in
    // single mode, and the slow click renames while double mode is on.
    check("Escape goes up a folder ships off", find(view, "escapeUp").label + "|" + find(view, "escapeUp").on,
          "Escape goes up a folder|false")
    check("and explains when it climbs", find(view, "escapeUp").caption, "with nothing to close")
    check("and a stored on reads back on",
          find(Settings.rows("view", { data: { escapeUp: true } }), "escapeUp").on, true)
    check("opening defaults to a double click", find(view, "openMode").selected, "double")
    check("and its two values are the board's own",
          find(view, "openMode").values.join(",") + "|" + find(view, "openMode").labels.join("|"),
          "double,single|Double click|Single click")
    check("the slow click ships on", find(view, "clickRename").label + "|" + find(view, "clickRename").on,
          "Click a selected name to rename|true")
    var single = Settings.rows("view", { data: { openMode: "single" } })
    check("single mode reads back", find(single, "openMode").selected, "single")
    check("and greys the slow click in place rather than hiding it",
          [find(single, "clickRename").available, Settings.focusable(find(single, "clickRename"))].join("|"),
          "false|false")
    check("while double mode keeps it a control",
          [find(view, "clickRename").available, Settings.focusable(find(view, "clickRename"))].join("|"),
          "true|true")
    // ClickAndRefresh draws the pointer mark on Open items with and leaves Click a selected name to rename's slot blank; KeyboardFlows leaves Escape's blank too.
    check("Open items with wears the board's pointer mark",
          find(view, "openMode").glyph, "pointer")
    check("Click a selected name to rename draws no mark", find(view, "clickRename").glyph === undefined, true)
    check("Escape goes up a folder draws no mark", find(view, "escapeUp").glyph === undefined, true)
    check("Highlight today's dates ships off", find(view, "highlightToday").label + "|" + find(view, "highlightToday").on, "Highlight today's dates|false")
    check("and a stored on reads back on", find(Settings.rows("view", { data: { highlightToday: true } }), "highlightToday").on, true)
    check("Columns view limit ships at 3", find(view, "columnsLimit").selected, 3)
    check("and a stored cap reads back", find(Settings.rows("view", { data: { columnsLimit: 4 } }), "columnsLimit").selected, 4)
    check("save feedback is a separate footer", view[view.length - 1].footer, true)

    // Settings > View > Opening, which is where a window and a new tab begin. ui/js/Startup.js turns the values into a path and tests/js/startup.js drives that; this is only what the panel draws.
    var opening = Settings.rows("view", {})
    check("Opening defaults to home", find(opening, "startIn").selected, "home")
    check("and its three values are the ones the schema allows",
          find(opening, "startIn").values.join(","), "home,last,folder")
    check("a chosen folder that was never chosen invites the operator to pick one",
          find(opening, "startFolder").value, "Use this folder")
    check("new tabs default to the folder the pane is on", find(opening, "newTab").selected, "current")
    check("and the tab values are the schema's own",
          find(opening, "newTab").values.join(","), "current,home,start")
    var opened = Settings.rows("view", { data: { startIn: "folder", startFolder: "/home/gm/Work", newTab: "home" } })
    check("a chosen folder is named by its own path", find(opened, "startFolder").value, "/home/gm/Work")
    check("and the mode beside it reads back", find(opened, "startIn").selected, "folder")
    check("the tab setting reads back too", find(opened, "newTab").selected, "home")
    // Tabs040 callout 2: the board's own sentence under "Flea opens in", only while Last folder
    // is chosen. A hint is read-only, so it is never the cursor; kinds() pins that separately.
    var hinted = Settings.rows("view", { data: { startIn: "last" } })
    var hints = hinted.filter(function (row) { return row.label === "Last folder reopens every tab you had." })
    check("Last folder carries the tab restore hint", hints.length, 1)
    check("as a hint and not a control", hints[0].kind, "hint")
    check("sitting under the control it explains",
          hinted.indexOf(hints[0]) - hinted.indexOf(find(hinted, "startIn")), 1)
    var unhinted = Settings.rows("view", { data: { startIn: "folder" } })
          .filter(function (row) { return row.label === "Last folder reopens every tab you had." })
    check("while Chosen folder shows no such hint", unhinted.length, 0)
    check("and neither does Home",
          Settings.rows("view", {}).filter(function (row) {
              return row.label === "Last folder reopens every tab you had."
          }).length, 0)
    check("the save failure keeps its message and error role",
          Settings.rows("view", { saveStatus: "Could not save settings" }).slice(-1).map(function (row) {
              return row.label + "|" + row.role + "|" + row.footer
          }).join(""), "Could not save settings|error|true")
    var preview = Settings.rows("preview", state)
    check("preview visibility is independent of loading", find(preview, "preview.column").on, false)
    check("manual preview reports the stored load mode", find(preview, "preview.loadOn").value, "Manual")
    check("all six thumbnail display stops remain available", find(preview, "preview.thumbSize").values.join(","), "small,medium,large,xlarge,huge,largest")
    check("thumbnail source policy remains separate", find(preview, "preview.thumbnails").selected, "off")
    check("ctrl zoom can be disabled", find(preview, "preview.ctrlZoom").on, false)
    // HANDOFF rule 3: nothing is indented, so no row carries the flag that stepped its mark right.
    check("no preview row is indented", preview.filter(function (row) { return row.indented }).length, 0)
    var sizes = ["small", "medium", "large", "xlarge", "huge", "largest"]
    check("the size row names the stop and no figure, one value per row", sizes.map(function (size) {
        var row = find(Settings.rows("preview", { data: { preview: { thumbSize: size } } }), "preview.thumbSize")
        return (row.caption === undefined ? "-" : row.caption) + "|" + row.value
    }).join(","), "-|Small,-|Medium,-|Large,-|Extra large,-|Huge,-|Largest")
    // Rule 3: the sentence explaining Load sits under Load, inside its own group, not in a footer two hairlines away.
    check("the load hint sits under the control it explains", preview[3].kind + "|" + preview[3].label + "|" + (preview[3].footer === undefined),
          "hint|Ctrl+Space loads the current selection.|true")
    check("and the automatic case names what automatic follows", Settings.rows("preview", {})[3].label, "Automatic follows the cursor.")
    // Rule 7: with the column off SelectionPreview.canRead is false and both load paths return, so Load is the one real dependent; grid zoom and the size feed the tile geometry with thumbnails off, so neither greys.
    var shown = Settings.rows("preview", { data: { preview: { column: true, thumbnails: "off" } } })
    check("Load greys with the preview column, steps out of the cursor with it, and comes back when the column does",
          [find(preview, "preview.loadOn").available, Settings.focusable(find(preview, "preview.loadOn")), find(shown, "preview.loadOn").available].join("|"), "false|false|true")
    check("grid zoom and thumbnail size depend on nothing, and zoom is named for what it resizes",
          [find(shown, "preview.ctrlZoom").available === undefined, find(shown, "preview.thumbSize").available === undefined, find(shown, "preview.ctrlZoom").label].join("|"),
          "true|true|Zoom the grid with ctrl and scroll")
    var about = Settings.rows("about", { about: { version: "0.1.6", handler: "com.thisisgm.flea.desktop" } })
    // Rules 5 and 6: a fact is muted whole, so no row writes "status only" on itself, and the handler keeps the tail that identifies it.
    check("the handler row states the handler and nothing else",
          about.filter(function (row) { return row.label === "File manager"; })
               .map(function (row) { return row.value + "|" + row.elide; }).join(""),
          "com.thisisgm.flea.desktop|head")
    check("About version comes from supplied binary facts", about[1].value, "0.1.6")
    check("unreported builds never repeat a specimen commit", about[2].value, "Not recorded in this build")
    check("passive About metadata takes no focus", Settings.focusable(about[1]), false)
    check("About support routes are keyboard actions", Settings.focusable(find(about, "support")), true)
    var columns = Settings.columnRows(state)
    check("Name cannot be removed", columns[1].kind, "lock")
    check("optional kind reflects persisted columns", find(columns, "column:kind").on, true)
}
