.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Menu.js" as Menu
.import "../../ui/js/TextSize.js" as TextSize

// The settings panel's model. ui/SettingsPanel.qml only paints what rows() returns, so every row a
// section can draw, and every value a control can hold, is assertable here without a window.

function run(check) {
    runMaster(check)
    runRows(check)
    runCursor(check)
    runPresets(check)
    runInventory(check)
    runCompletionRows(check)
}

// No mock controls: every id the Menus section can switch is an action ui/js/Menu.js really builds,
// and every row it builds that is not locked or background-only has a switch. Two menus are unioned
// because Move to Dropbox and Copy share link cannot appear on one row and Extract needs an archive.
function runInventory(check) {
    var built = {}
    var builtMark = {}
    var shapes = [
        { rowInDropbox: false, rowIsArchive: true, rowIsImage: true },
        { rowInDropbox: true, rowIsArchive: false, rowIsImage: false }
    ]
    for (var s = 0; s < shapes.length; s++) {
        var rows = Menu.listingEntries({
            showHidden: false, hasRow: true, dropboxPath: "/home/jw/Dropbox",
            taildropPeers: [{ id: "x", label: "Box" }], taildropInstalled: true, dropboxInstalled: true,
            archiveFormats: ["zip"], canConvert: true, canExtract: true, selectionCount: 1, rowMode: 0o100644,
            rowInDropbox: shapes[s].rowInDropbox, rowIsArchive: shapes[s].rowIsArchive,
            rowIsImage: shapes[s].rowIsImage, hiddenActions: []
        })
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].separator === true)
                continue
            built[rows[i].id || rows[i].action] = rows[i].label
            builtMark[rows[i].id || rows[i].action] = rows[i].glyph !== undefined ? rows[i].glyph : rows[i].mark
        }
    }
    var switched = []
    for (var g = 0; g < Settings.MENU_GROUPS.length; g++)
        switched = switched.concat(Settings.MENU_GROUPS[g].ids)
    check("every switch in the Menus section is over a row the menu really builds",
          switched.filter(function (id) { return built[id] === undefined }).join(","), "")
    check("and each switch carries that row's own wording, so the two cannot drift",
          switched.filter(function (id) { return Settings.label(id) !== built[id] }).join(","), "")
    // A switch wears the mark of the row it governs, which is the only way a reader can pair the two.
    check("and each row wears the mark the menu draws for that action",
          switched.concat(Settings.LOCKED).filter(function (id) {
              var mine = Settings.GLYPHS[id] !== undefined ? Settings.GLYPHS[id] : Settings.MARKS[id]
              return mine === undefined || mine !== builtMark[id]
          }).join(","), "")
    // SettingsPlaces section 02 adds the listing Favorite action; SettingsMenus keeps its 20 switches and Menus' New folder has none.
    var reachable = switched.concat(Settings.LOCKED).concat(["newFolder", "addFavourite"])
    check("no other menu action is omitted from the board's switch inventory",
          Object.keys(built).filter(function (id) { return reachable.indexOf(id) < 0 }).join(","), "")
}

// The Display state ui/SettingsPanel.qml passes in: the stored mode, the size ui/Theme.qml resolved
// from it, and the two numbers Flea reads off the compositor and never writes.
function displayState(textSize, baseSize, monitorScale) {
    return { textSize: textSize, baseSize: baseSize,
             monitorScale: monitorScale === undefined ? 1 : monitorScale, cornerRadius: 8 }
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

// GM's ruling, and it is easy to get backwards: menu.hidden stores what is HIDDEN, and the master's
// count is of ENABLED actions, so one id in the set reads "5 of 6".
function runMaster(check) {
    check("nothing hidden is all six enabled", Settings.basicEnabled([]), 6)
    check("and the master reads all", Settings.masterState([]), "all")
    check("one hidden id is five enabled", Settings.basicEnabled(["paste"]), 5)
    check("and the master is partial", Settings.masterState(["paste"]), "some")
    check("all six hidden is none enabled", Settings.basicEnabled(Settings.BASIC), 0)
    check("and the master is unchecked", Settings.masterState(Settings.BASIC), "none")
    // An unrelated id in the set must not be counted as one of the six, in either direction.
    check("an unrelated hidden id does not change the count",
          Settings.basicEnabled(["copypath", "compress"]), 6)

    check("activating a checked master switches all six off",
          Settings.toggleMaster([]).sort().join(","), Settings.BASIC.slice().sort().join(","))
    check("activating a partial master switches all six on, which is the recovering keystroke",
          Settings.toggleMaster(["paste"]).length, 0)
    check("activating an unchecked master switches all six on too",
          Settings.toggleMaster(Settings.BASIC).length, 0)
    check("switching all six on preserves an unrelated hidden id",
          Settings.toggleMaster(["paste", "copypath"]).join(","), "copypath")
    check("and switching all six off preserves it as well",
          Settings.toggleMaster(["copypath"]).indexOf("copypath") >= 0, true)

    check("an individual toggle adds its own id and nothing else",
          Settings.toggleId([], "paste").join(","), "paste")
    check("and toggling it again takes only that id back out",
          Settings.toggleId(["paste", "copypath"], "paste").join(","), "copypath")
    check("the master recomputes off the individual toggle at once",
          Settings.masterState(Settings.toggleId([], "cut")) + " "
          + Settings.basicEnabled(Settings.toggleId([], "cut")), "some 5")

    // menu.hidden is the sole state, so the master is a reading of that set and never a value beside
    // it: there is no fold to apply here, and nothing a hand edit could leave the two disagreeing on.
    check("the model exports no stored master to read", typeof Settings.effectiveHidden, "undefined")
    check("a set with no master in it still draws one",
          Settings.rows("menus", { hidden: ["paste"] })[1].state, "some")
    check("and the master survives a round trip through the set it derives from",
          Settings.masterState(Settings.toggleMaster(Settings.toggleMaster([]))), "all")
}

function runRows(check) {
    var display = Settings.rows("display", displayState(TextSize.follow(), 14))
    // The board's Display card: the text-size mode over its effective size, then the compositor's
    // two read-only facts. No monitor-scale control, because Flea does not step or cycle that one.
    check("the Display section is text size, then Scale, then Appearance",
          kinds(display), "group|choice|ruler|hint|group|fact|hint|group|check")
    check("its one control opens on Follow Omarchy", find(display, "textMode").value,
          "Follow Omarchy")
    // The board draws the mode as both names side by side, so the row names them rather than
    // leaving ui/SettingsRow.qml to invent a second list that could disagree with the writer.
    check("and it names both its values, in the board's own order",
          find(display, "textMode").options.join("|"), "Follow Omarchy|Override")
    check("the ruler reports Omarchy's own size", display[2].value, "14px")
    check("and fills to that stop, five of the seven", display[2].index, 4)
    // An Omarchy size that is not one of the seven still fills the ruler, at the nearest stop below.
    check("a size between two stops fills to the nearer one",
          Settings.rows("display", displayState(TextSize.follow(), 13))[2].index, 3)
    check("the hint names every stop the override can take",
          display[3].label.indexOf("9, 10, 11, 12, 14, 16, 20 px") >= 0, true)
    check("the monitor scale is drawn read-only, as the compositor reports it", display[5].value, "Read-only 1x")
    check("a fractional one keeps its fraction",
          Settings.rows("display", displayState(TextSize.follow(), 14, 1.25))[5].value, "Read-only 1.25x")
    check("and an unanswered query says so rather than claiming 1x",
          Settings.rows("display", displayState(TextSize.follow(), 14, 0))[5].value, "not reported")
    check("its hint is the board's own sentence, so no reader expects a control",
          display[6].label, "Flea follows the compositor value and does not step or cycle it.")
    check("the board icon override defaults off", display[8].id + "|" + display[8].on, "display.hyprlandIcons|false")

    // Switching to Override adds the stop row, and nothing else about the section moves.
    var pinned = Settings.rows("display", displayState({ mode: 16 }, 16))
    check("an override adds no row, it turns the ruler into the control", pinned.length,
          display.length)
    check("the mode row says which mode it is in", find(pinned, "textMode").value, "Override")
    check("the ruler carries the pinned size", find(pinned, "textStop").value, "16px")
    check("and fills one stop further along", pinned[2].index, 5)
    check("the ruler is live only while the override is", find(pinned, "textStop").on, true)
    check("and while following it reports rather than sets",
          find(display, "textStop").on, false)

    var menus = Settings.rows("menus", { hidden: ["paste"] })
    check("the Menus section leads with the master row under its own heading",
          kinds(menus).indexOf("group|master|check") === 0, true)
    check("the master's count is drawn beside it", menus[1].value, "5 of 6")
    // The mirror of the same ruling: five ids in the set is one action left ON, never "5 of 6".
    check("and five hidden ids read as one enabled, not five",
          Settings.rows("menus", { hidden: ["cut", "copy", "paste", "duplicate", "rename"] })[1].value,
          "1 of 6")
    check("a hidden action's row is drawn unchecked, not dropped",
          find(menus, "paste").on, false)
    check("and an enabled one is checked", find(menus, "copy").on, true)
    check("the current menu controls include Permissions and the retained hints preference",
          menus.filter(function (r) { return r.kind === "check" })
               .map(function (r) { return r.id }).join(","),
          "cut,copy,paste,duplicate,rename,trash,delete,openwith,openTerminal,moveto,copyto,properties,permissions,copypath,compress,extract,convert,taildrop,dropbox,sharelink,keyHints")
    // The one check that is not a menu action: it says how every row is drawn, not whether it is.
    // GM's ruling of 2026-09-10 turns it off by default, with the rail's own detail rows, and
    // src/uischema.rs stores that default, so an absent preference reads off and not on.
    check("the hints row defaults off, as GM ruled over the boards",
          find(menus, "keyHints").label + "|" + find(menus, "keyHints").on,
          "Show keyboard hints|false")
    check("and it reads the value it is given",
          find(Settings.rows("menus", { hidden: [], keyHints: true }), "keyHints").on, true)
    check("an explicitly disabled hints preference stays off",
          find(Settings.rows("menus", { hidden: [], keyHints: false }), "keyHints").on, false)
    // Open in terminal was drawn by every menu with no way to switch it off, because the shipped
    // hidden set named it "terminal" and ui/js/Menu.js builds the row as "openTerminal".
    check("Open in terminal is a switch like any other action row",
          find(menus, "openTerminal").label + "|" + find(menus, "openTerminal").glyph,
          "Open in terminal|terminal")
    // The board shows the two locked rows so the section is a complete list of what a menu can hold.
    check("Open and Show hidden files are listed, locked rather than omitted",
          menus.filter(function (r) { return r.kind === "lock" })
               .map(function (r) { return r.id }).join(","), "open,toggleHidden")
    check("and no locked row is a checkbox", find(menus, "open").kind, "lock")

    var keys = Settings.rows("keys", { preset: "mac", presetKeys: Keymap.PRESET_KEYS })
    check("the Keys section leads with the preset choice", keys[1].kind, "choice")
    check("and shows the selected preset by name", keys[1].value, "Mac")
    // The board draws all four on the control, and SettingsRow needs options.length > 1 to draw a
    // segment at all, so a chevron here is the defect: it names one value and hides the other three.
    check("the preset row draws all four as a segment, in the chooser's own order",
          (keys[1].options || []).join("|"), "Default|Vim|Mac|Windows")
    check("the Windows preset is shown by name too",
          Settings.rows("keys", { preset: "windows", presetKeys: Keymap.PRESET_KEYS })[1].value,
          "Windows")
}

function runCursor(check) {
    var menus = Settings.rows("menus", { hidden: [] })
    check("a heading is never a focus stop", Settings.focusable(menus[0]), false)
    check("so the opening cursor lands on the master row below it", Settings.firstRow(menus), 1)
    check("a check row is a focus stop", Settings.focusable(menus[2]), true)
    check("a locked row is not", Settings.focusable(menus[menus.length - 1]), false)
    // Stepping past the last focus stop keeps the cursor where it is, the way the context menu's own
    // stepCursor does, so the two locked rows at the bottom cannot swallow it.
    check("stepping down off the end holds the cursor on the last control",
          Settings.stepRow(menus, menus.length - 3, 1), menus.length - 3)
    check("stepping up off the top holds it on the first", Settings.stepRow(menus, 1, -1), 1)
    check("a step down crosses the heading between two groups",
          Settings.focusable(menus[Settings.stepRow(menus, 7, 1)]), true)

    var display = Settings.rows("display", displayState(TextSize.follow(), 14))
    check("the Display section's only control is where its cursor opens",
          Settings.firstRow(display), 1)
    check("and no read-only fact below it takes the cursor",
          Settings.stepRow(display, 1, 1), 8)
    var pinned = Settings.rows("display", displayState({ mode: 16 }, 16))
    check("an override gives the cursor a second stop to walk to",
          Settings.stepRow(pinned, 1, 1), 2)
    check("and the compositor's rows still take none",
          Settings.stepRow(pinned, 2, 1), 8)
}

// SettingsKeys.html's four-value chooser over the one key table. Each row the Keys section lists is
// resolved back through the generated overlay, so a listed chord cannot advertise a binding the
// preset lacks, and every one of the four claims a chord rather than drawing a heading over nothing.
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
    var menuRows = Settings.menuRows([], true)
    check("SettingsMenus contains exactly 20 action switches", menuRows.filter(function (r) { return r.kind === "check" && r.id !== "keyHints" }).length, 20)
    check("Delete permanently is visually destructive", find(menuRows, "delete").role, "error")
    check("Delete permanently explains its default", find(menuRows, "delete").value, "off by default")
}

function runCompletionRows(check) {
    var places = Settings.rows("places", {})
    check("Places spells the manager group Favorites", places[0].label, "Favorites")
    check("optional rail details default off", [find(places, "places.driveSize").on, find(places, "places.trashCount").on].join(","), "false,false")
    check("the Rail controls follow the ruled order", places.slice(-5, -2).map(function (row) { return row.label }).join("|"), "Show drive size|Show Trash count|Sidebar width")
    // The 30 day sweep's own row, at the foot of Places under its own eyebrow. Off unless ui.json
    // says otherwise, which is the whole of GM's opt-in ruling as the panel sees it.
    check("Places ends with the Trash group and its one row",
          places.slice(-2).map(function (row) { return row.label }).join("|"),
          "Trash|Empty after 30 days")
    check("the sweep is off on a fresh install", find(places, "trashAutoEmpty").on, false)
    check("and says what it does and how often", find(places, "trashAutoEmpty").caption, "permanently")
    check("a ui.json that switched it on reads back on",
          find(Settings.rows("places", { data: { trashAutoEmpty: true } }), "trashAutoEmpty").on, true)
    var detailedPlaces = Settings.rows("places", { data: { places: { driveSize: true, trashCount: true } } })
    check("both rail detail controls reflect persisted on values", [find(detailedPlaces, "places.driveSize").on, find(detailedPlaces, "places.trashCount").on].join(","), "true,true")
    var state = { data: { view: "grid", density: "compact", columns: ["name", "kind"],
        preview: { column: false, loadOn: "manual", thumbnails: "off", thumbSize: "xlarge", ctrlZoom: false } } }
    var view = Settings.rows("view", state)
    check("View displays the persisted view", find(view, "view").selected, "grid")
    check("View preserves optional column choices", find(view, "columns").value, "Name, Kind")
    check("View density uses schema values", find(view, "density").selected, "compact")
    check("Folders first uses its distinct ordering mark", find(view, "foldersFirst").glyph, "folders-first")
    check("Preview rail mark differs from the three-column view", Settings.SECTIONS[Settings.sectionIndex("preview")].glyph, "preview")
    check("grouping explains the categories before it is enabled", find(view, "groupByKind").caption, "folders, photos, files")
    check("wrapping explains the boundary before it is enabled", find(view, "wrapAtEnds").caption, "arrow-up at the top")
    check("save feedback is a separate footer", view[view.length - 1].footer, true)

    // Settings > View > Opening, which is where a window and a new tab begin. ui/js/Startup.js turns
    // the values into a path and tests/js/startup.js drives that; this is only what the panel draws.
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
    check("the save failure keeps its message and error role",
          Settings.rows("view", { saveStatus: "Could not save settings" }).slice(-1).map(function (row) {
              return row.label + "|" + row.role + "|" + row.footer
          }).join(""), "Could not save settings|error|true")
    var preview = Settings.rows("preview", state)
    check("preview visibility is independent of loading", find(preview, "preview.column").on, false)
    check("manual preview reports the stored load mode", find(preview, "preview.loadOn").value, "Manual")
    check("all four thumbnail display stops remain available", find(preview, "preview.thumbSize").values.join(","), "small,medium,large,xlarge")
    check("thumbnail source policy remains separate", find(preview, "preview.thumbnails").selected, "off")
    check("ctrl zoom can be disabled", find(preview, "preview.ctrlZoom").on, false)
    check("only dependent preview controls are indented", preview.filter(function (row) { return row.indented }).map(function (row) {
        return row.id
    }).join(","), "preview.loadOn,preview.thumbSize,preview.ctrlZoom")
    var sizes = ["small", "medium", "large", "xlarge"]
    check("thumbnail pixels are live captions separate from each named stop", sizes.map(function (size) {
        var row = find(Settings.rows("preview", { data: { preview: { thumbSize: size } } }), "preview.thumbSize")
        return row.caption + "|" + row.value
    }).join(","), "48 px|Small,64 px|Medium,96 px|Large,128 px|Extra large")
    check("preview footer explains the active loading mode", preview[preview.length - 1].label + "|" + preview[preview.length - 1].footer,
          "Ctrl+Space loads the current selection.|true")
    var about = Settings.rows("about", { about: { version: "0.1.6", handler: "flea.desktop" } })
    check("About version comes from supplied binary facts", about[1].value, "0.1.6")
    check("unreported builds never repeat a specimen commit", about[2].value, "Not recorded in this build")
    check("passive About metadata takes no focus", Settings.focusable(about[1]), false)
    check("About support routes are keyboard actions", Settings.focusable(find(about, "support")), true)
    var columns = Settings.columnRows(state)
    check("Name cannot be removed", columns[1].kind, "lock")
    check("optional kind reflects persisted columns", find(columns, "column:kind").on, true)
}
