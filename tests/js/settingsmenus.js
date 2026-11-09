.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Menu.js" as Menu

// The Menus section of the settings panel: the switch inventory it is built from, and the per-group masters SettingsMenus rule 3 puts on the headings. Split out of tests/js/settings.js when that suite outgrew its budget; the rest of the panel stays there.

function run(check) {
    runInventory(check)
    runMaster(check)
}

// No mock controls: every id the Menus section can switch is an action ui/js/Menu.js really builds, and every row it builds that is not locked or background-only has a switch. Two menus are unioned because Move to Dropbox and Copy share link cannot appear on one row and Extract needs an archive.
function runInventory(check) {
    var built = {}
    var builtMark = {}
    var shapes = [
        { rowInDropbox: false, rowIsArchive: true, rowIsImage: true, rowIsSymlink: true,
          selectionCount: 1, selectionModes: undefined, hasShebang: true, cursorIsTarget: true,
          tagSubmenu: [{ id: "Red", label: "Red" }] },
        { rowInDropbox: true, rowIsArchive: false, rowIsImage: false, rowIsSymlink: false,
          selectionCount: 1, selectionModes: undefined, hasShebang: false,
          tagSubmenu: [{ id: "Red", label: "Red" }] },
        // Permissions040: Permissions takes the whole selection, so one shape
        // selects three files and the row still builds.
        { rowInDropbox: false, rowIsArchive: false, rowIsImage: false, rowIsSymlink: false,
          selectionCount: 3, selectionModes: [0o100644, 0o100644, 0o040755], hasShebang: false,
          tagSubmenu: [{ id: "Red", label: "Red" }] }
    ]
    for (var s = 0; s < shapes.length; s++) {
        // Compare state-toggle wording while off, matching the customization labels.
        var rows = Menu.listingEntries({ langMarks: false,
            showHidden: false, hasRow: true, dropboxPath: "/home/jw/Dropbox",
            taildropPeers: [{ id: "x", label: "Box" }], taildropInstalled: true, dropboxInstalled: true,
            localSendInstalled: true, clipboardAvailable: true,
            archiveFormats: ["zip"], canConvert: true, canExtract: true, selectionCount: shapes[s].selectionCount,
            selectionModes: shapes[s].selectionModes, rowMode: 0o100644, rowIsSymlink: shapes[s].rowIsSymlink,
            rowName: "debian-13.iso", flashZipMember: "",
            rowInDropbox: shapes[s].rowInDropbox, rowIsArchive: shapes[s].rowIsArchive,
            rowIsImage: shapes[s].rowIsImage, hiddenActions: [], hasShebang: shapes[s].hasShebang,
            cursorIsTarget: shapes[s].cursorIsTarget,
            // The Tags row builds only where a tag flyout exists, the way a folder with tags in use builds it.
            tagSubmenu: shapes[s].tagSubmenu,
            // One script, so the Run script row is built here the way a box with a scripts directory builds it.
            scripts: [{ id: "one.sh", label: "one" }]
        })
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].separator === true)
                continue
            // The rename row names its count over several rows (pinned in menu.js); the switch carries the one-row wording.
            if (shapes[s].selectionCount > 1 && rows[i].action === "rename")
                continue
            built[rows[i].id || rows[i].action] = rows[i].label
            builtMark[rows[i].id || rows[i].action] = rows[i].glyph !== undefined ? rows[i].glyph : rows[i].mark
        }
    }
    var switched = []
    for (var g = 0; g < Settings.MENU_GROUPS.length; g++)
        switched = switched.concat(Settings.MENU_GROUPS[g].ids)
    // The background menu's rows count too where a switch governs them, as Update Flea's does once an update is known.
    // storageClass "usb" builds the ExtThumbs row the way a removable drive does, so its switch is covered too.
    var background = Menu.listingEntries({ hasRow: false, showHidden: false, langMarks: false, hiddenActions: [], updateVersion: "0.3.4", storageClass: "usb", thumbPreview: {} })
    for (var b = 0; b < background.length; b++) {
        if (switched.indexOf(background[b].id) >= 0) {
            built[background[b].id] = background[b].label
            builtMark[background[b].id] = background[b].glyph
        }
    }
    // Directive 38 added a second kind of switch: one that gates a whole surface a board draws rather
    // than one listing row, so those are named in Settings.FEATURES and only they are exempt here.
    var features = []
    for (var f = 0; f < Settings.MENU_GROUPS.length; f++)
        features = features.concat(Settings.MENU_GROUPS[f].features || [])
    var rowSwitches = switched.filter(function (id) { return features.indexOf(id) < 0 })
    check("every switch in the Menus section is over a row the menu really builds, or gates a surface",
          rowSwitches.filter(function (id) { return built[id] === undefined }).join(","), "")
    check("a feature switch still carries its own wording and mark",
          features.filter(function (id) {
              return !Settings.label(id) || Settings.GLYPHS[id] === undefined
          }).join(","), "")
    check("and each switch carries that row's own wording, so the two cannot drift",
          // extThumbs names its class and state in the menu ("Hide USB thumbnails") while the
          // switch keeps the board's one name ("Show thumbnails"), the shelf's own exemption.
          rowSwitches.filter(function (id) { return id !== "shelf" && id !== "extThumbs" && Settings.label(id) !== built[id] }).join(",")
          + "|" + Settings.label("shelf") + "|" + built["shelf"], "|Enable shelf|Add to shelf")
    // A switch wears the mark of the row it governs, which is the only way a reader can pair the two.
    check("and each row wears the mark the menu draws for that action",
          rowSwitches.concat(Settings.LOCKED).filter(function (id) {
              var mine = Settings.GLYPHS[id] !== undefined ? Settings.GLYPHS[id] : Settings.MARKS[id]
              return mine === undefined || mine !== builtMark[id]
          }).join(","), "")
    // SettingsPlaces section 02 adds the listing Favorite action; SettingsMenus keeps its 20 switches and Menus' New folder has none.
    var reachable = rowSwitches.concat(Settings.LOCKED).concat(["newFolder", "addFavourite"])
    check("no other menu action is omitted from the board's switch inventory",
          Object.keys(built).filter(function (id) { return reachable.indexOf(id) < 0 }).join(","), "")
}

// The rule, and it is easy to get backwards: menu.hidden stores what is HIDDEN, and a master's count is of ENABLED actions, so one id in the set reads "5 of 6".
function runMaster(check) {
    var BASIC = Settings.BASIC
    check("nothing hidden is all six enabled", Settings.basicEnabled([], BASIC), 6)
    check("and the master reads all", Settings.masterState([], BASIC), "all")
    check("one hidden id is five enabled", Settings.basicEnabled(["paste"], BASIC), 5)
    check("and the master is partial", Settings.masterState(["paste"], BASIC), "some")
    check("all six hidden is none enabled", Settings.basicEnabled(BASIC, BASIC), 0)
    check("and the master is unchecked", Settings.masterState(BASIC, BASIC), "none")
    // An unrelated id in the set must not be counted as one of the six, in either direction.
    check("an unrelated hidden id does not change the count",
          Settings.basicEnabled(["showOriginal", "compress"], BASIC), 6)

    check("activating a checked master switches all six off",
          Settings.toggleMaster([], BASIC).sort().join(","), BASIC.slice().sort().join(","))
    check("activating a partial master switches all six on, which is the recovering keystroke",
          Settings.toggleMaster(["paste"], BASIC).length, 0)
    check("activating an unchecked master switches all six on too",
          Settings.toggleMaster(BASIC, BASIC).length, 0)
    check("switching all six on preserves an unrelated hidden id",
          Settings.toggleMaster(["paste", "showOriginal"], BASIC).join(","), "showOriginal")
    check("and switching all six off preserves it as well",
          Settings.toggleMaster(["showOriginal"], BASIC).indexOf("showOriginal") >= 0, true)

    check("an individual toggle adds its own id and nothing else",
          Settings.toggleId([], "paste").join(","), "paste")
    check("and toggling it again takes only that id back out",
          Settings.toggleId(["paste", "showOriginal"], "paste").join(","), "showOriginal")
    check("the master recomputes off the individual toggle at once",
          Settings.masterState(Settings.toggleId([], "cut"), BASIC) + " "
          + Settings.basicEnabled(Settings.toggleId([], "cut"), BASIC), "some 5")

    // menu.hidden is the sole state, so a master is a reading of that set and never a value beside it: there is no fold to apply here, and nothing a hand edit could leave the two disagreeing on.
    check("the model exports no stored master to read", typeof Settings.effectiveHidden, "undefined")
    check("a set with no master in it still draws one",
          Settings.rows("menus", { hidden: ["paste"] })[0].state, "some")
    check("and the master survives a round trip through the set it derives from",
          Settings.masterState(Settings.toggleMaster(Settings.toggleMaster([], BASIC), BASIC), BASIC), "all")

    // SettingsMenus rule 3: each group's own master, on its own heading, counting its own ids alone.
    var groups = Settings.rows("menus", { hidden: ["moveto", "copyto", "properties", "permissions"] })
        .filter(function (row) { return row.kind === "group" })
    check("every heading reports the group it governs, and a group of one row has no master at all",
          groups.map(function (row) { return row.label + "|" + (row.master ? row.value + "|" + row.state : "no master") }).join(", "),
          "Basic file actions|6 of 6|all, Destructive|no master, Open and inspect|8 of 12|some, "
          + "Extras|17 of 17|all, Shortcuts|no master, Always shown|no master")
    var inspect = groups[2]
    check("a heading with a master is a focus stop and one without is not",
          Settings.focusable(inspect) + "|" + Settings.focusable(groups[1]), "true|false")
    check("and it switches its own ids only, never another group's",
          Settings.toggleMaster(["moveto", "copyto", "properties", "permissions", "paste"], inspect.ids).sort().join(","),
          "paste")
}
