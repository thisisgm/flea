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
            taildropPeers: [{ id: "x", label: "Box" }], archiveFormats: ["zip"], canConvert: true,
            rowInDropbox: shapes[s].rowInDropbox, rowIsArchive: shapes[s].rowIsArchive,
            rowIsImage: shapes[s].rowIsImage, hiddenActions: []
        })
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].separator === true)
                continue
            built[rows[i].action] = rows[i].label
            builtMark[rows[i].action] = rows[i].glyph !== undefined ? rows[i].glyph : rows[i].mark
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
    // New folder is the one row this release's panel gives no switch, and the two locked ones are
    // drawn locked; anything else without a switch would be a row the section cannot reach.
    var reachable = switched.concat(Settings.LOCKED).concat(["newFolder"])
    check("and no row the menu builds is left without one",
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
          kinds(display), "group|choice|ruler|hint|group|fact|hint|group|fact")
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
    check("the monitor scale is drawn as the compositor reports it", display[5].value, "1x")
    check("a fractional one keeps its fraction",
          Settings.rows("display", displayState(TextSize.follow(), 14, 1.25))[5].value, "1.25x")
    check("and an unanswered query says so rather than claiming 1x",
          Settings.rows("display", displayState(TextSize.follow(), 14, 0))[5].value, "not reported")
    check("its hint is the board's own sentence, so no reader expects a control",
          display[6].label, "Flea follows the compositor value and does not step or cycle it.")
    check("the rounding Flea mirrors is drawn beside it", display[8].value, "rounding 8")

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
    check("every toggleable action the listing menu can build has a row, plus the hints row",
          menus.filter(function (r) { return r.kind === "check" }).length, 15)
    // The one check that is not a menu action: it says how every row is drawn, not whether it is.
    check("the hints row is a check of its own, off until it is switched on",
          find(menus, "keyHints").label + "|" + find(menus, "keyHints").on,
          "Show keyboard hints|false")
    check("and it reads the value it is given",
          find(Settings.rows("menus", { hidden: [], keyHints: true }), "keyHints").on, true)
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
          Settings.stepRow(display, 1, 1), 1)
    var pinned = Settings.rows("display", displayState({ mode: 16 }, 16))
    check("an override gives the cursor a second stop to walk to",
          Settings.stepRow(pinned, 1, 1), 2)
    check("and the compositor's rows still take none",
          Settings.stepRow(pinned, 2, 1), 2)
}

// The two-value toggle over the one key table. Each row the Keys section lists is resolved back
// through the generated overlay, so a listed chord cannot advertise a binding the preset lacks.
function runPresets(check) {
    check("there are exactly two presets, which is what the public list named",
          Settings.PRESETS.join(","), "mac,windows")
    check("both are named for the panel", Settings.PRESET_LABELS.mac + "," + Settings.PRESET_LABELS.windows,
          "Mac,Windows")
    var listed = 0
    for (var i = 0; i < Keymap.PRESET_KEYS.length; i++) {
        var row = Keymap.PRESET_KEYS[i]
        var mods = (row.ctrl ? Qt.ControlModifier : 0) | (row.shift ? Qt.ShiftModifier : 0)
        check("the Keys section's " + row.preset + " row " + row.keys + " is really bound",
              Keymap.lookupPreset(row.preset, Qt[row.code], "", mods), row.action)
        listed += 1
    }
    check("and the table is not empty, so the check above has a denominator", listed > 0, true)
    check("a mac chord is dead under the Windows preset",
          Keymap.lookupPreset("windows", Qt.Key_1, "", Qt.ControlModifier), "")
    check("and a Windows chord is dead under Mac",
          Keymap.lookupPreset("mac", Qt.Key_H, "", Qt.ControlModifier), "")
}
