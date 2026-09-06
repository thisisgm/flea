.import "../../ui/js/Menu.js" as Menu
.import "../../ui/js/Archive.js" as Archive

// ui/MenuRow.qml sizes its disclosure slot from this one predicate, and it used to test
// submenu === true. Every row ui/ContextMenu.qml builds puts the flyout's own entries in that
// field instead, so the slot was zero wide on every submenu row and no chevron was ever drawn.
// The bug survived because the menu's sibling test at ui/ContextMenu.qml "onActivated" asks
// !== undefined, so the flyout opened correctly and only the affordance was missing.

function run(check) {
    runMenu(check)
    check("a Compress row carrying the probed formats is a submenu row",
          Menu.hasSubmenu({ label: "Compress", action: "compress",
                            submenu: Archive.formatEntries(["zip", "7z"]) }),
          true)
    check("a Taildrop row with no peer yet is still a submenu row, so the disclosure stays",
          Menu.hasSubmenu({ label: "Send with Taildrop", action: "taildrop", submenu: [] }),
          true)
    check("a plain action row has no submenu",
          Menu.hasSubmenu({ label: "Rename", action: "rename", glyph: "rename" }),
          false)
    check("a separator has no submenu",
          Menu.hasSubmenu({ separator: true }), false)
    check("ui/MenuRow.qml's own default entry has no submenu",
          Menu.hasSubmenu({}), false)
    check("a missing entry answers false rather than throwing",
          Menu.hasSubmenu(undefined) + "|" + Menu.hasSubmenu(null), "false|false")

    // ui/ContextMenu.qml used to clamp against frame.height inside place(), and a Column hands its
    // implicitHeight to the frame one polish after the model changes. So every menu was placed
    // against the height of the menu that was open before it. The two numbers below are what an
    // offscreen model of that file measured, at a pane 800 tall with rows of 37 and an inset of 7:
    // a listing menu is 310, the rail's single Eject row is 51.
    var pane = 800
    var listingMenu = 310
    var railMenu = 51

    check("a menu that fits under the row opens exactly there",
          Menu.clamp(100, listingMenu, pane), 100)
    check("a menu that would run out through the bottom is pulled back to sit against it",
          Menu.clamp(700, listingMenu, pane) + "|" + (Menu.clamp(700, listingMenu, pane) + listingMenu),
          "490|800")
    check("the rail menu low in the sidebar sits against the bottom, not 280 px above the row",
          Menu.clamp(770, railMenu, pane), 749)
    check("clamping the rail menu against the listing menu still resident is the defect itself",
          Menu.clamp(770, listingMenu, pane), 490)
    check("a point off the near edge pins to it rather than going negative",
          Menu.clamp(-40, railMenu, pane), 0)
    check("a frame taller than the pane pins to the near edge, which is where its tail is cut",
          Menu.clamp(700, 1200, pane), 0)
}

// ui/js/Menu.js listingEntries: the listing's rows, built from the pane's state in one object.
// The row order below is the canvas's own, and the Open row now carries the resolved application
// beside a Copy path row.

function labels(entries) {
    var out = []
    for (var i = 0; i < entries.length; i++)
        out.push(entries[i].separator === true ? "-" : entries[i].label)
    return out.join("|")
}

function findEntry(entries, action) {
    for (var i = 0; i < entries.length; i++)
        if (entries[i].action === action)
            return entries[i]
    return {}
}

function runMenu(check) {
    var full = Menu.listingEntries({
        showHidden: false, hasRow: true, rowInDropbox: false,
        dropboxPath: "/home/jw/Dropbox", taildropPeers: [{ id: "x", label: "Box" }],
        archiveFormats: ["zip"], rowIsArchive: false, rowIsImage: false, canConvert: true
    })
    check("the listing menu opens with the row's own Open", full[0].label, "Open")
    check("Copy path sits beside Open", findEntry(full, "copypath").label, "Copy path")
    // SettingsMenus.html's six basic rows, in its own order. Cut, Copy and Paste were keyboard-only
    // until the Menus section grew a switch for each, and a switch over a row no menu draws is a mock.
    check("the six basic rows are drawn in the board's order",
          labels(full).indexOf("Cut|Copy|Paste|Duplicate|Rename") >= 0, true)
    check("each of the three new rows carries its own cut mark",
          findEntry(full, "cut").glyph + "|" + findEntry(full, "copy").glyph + "|"
          + findEntry(full, "paste").glyph, "scissors|copy|clipboard")
    // Top level, not behind a submenu: it is the menu's most used row and tests/ui.sh pins it there.
    check("the hidden toggle is a top-level row",
          findEntry(full, "toggleHidden").label, "Show hidden files")
    check("and its label flips with the state",
          Menu.hiddenRow(false).label + "|" + Menu.hiddenRow(true).label,
          "Show hidden files|Hide hidden files")
    // SettingsMenus.html carries Open in terminal in all three menus. It acts on the directory being
    // shown, not the row, so it sits with New folder and appears with no row under the cursor too.
    check("Open in terminal is a menu row in its own right",
          findEntry(full, "openTerminal").label + "|" + findEntry(full, "openTerminal").glyph,
          "Open in terminal|terminal")
    // Settings is a background row and only a background row: SettingsMenus.html's table marks it
    // shown in that column alone, so a row menu offering it would be a fourth door the board denies.
    check("no row menu offers a Settings row, because the board gives it to the background alone",
          findEntry(full, "settings").label, undefined)

    runBackground(check)
    runHidden(check, full)

    // ui/Header.qml's own rows, on a right click over the column titles. Four toggles, flipping
    // labels, each answering "col:<key>"; Name is absent because it never hides.
    var head = Menu.headerEntries([], false)
    check("the header menu offers the four optional columns, flipping labels when hidden",
          labels(Menu.headerEntries(["size"], false)),
          "Hide Mode|Show Size|Hide Date Modified|Hide Kind|-|Show hidden files")
    check("every column row answers col:<key>",
          findEntry(head, "col:size").action + "|" + findEntry(head, "col:kind").action,
          "col:size|col:kind")
    check("the header menu carries the hidden toggle too, below its own rule",
          findEntry(head, "toggleHidden").action, "toggleHidden")

    // The keyboard's own entrance, lifted out of ui/Pane.qml: the cursor row is scrolled into view
    // first, because a wheel scroll in the grid can leave it off screen, then the frame opens at
    // that delegate's bottom-left. With nothing under the cursor it opens nothing and answers false.
    var placed = []
    var scrolled = []
    var delegate = { height: 20, mapToItem: function (item, x, y) { return String(item) + ":" + x + "," + y } }
    var withRow = { cursorIndex: 4,
                    setCursor: function (index) { scrolled.push(index) },
                    visibleItemFor: function (index) { return delegate } }
    check("m scrolls the cursor row into view and opens the frame at its bottom-left",
          Menu.openAtCursor(withRow, { openAt: function (point) { placed.push(point) } }, 8)
              + "|" + scrolled.join(",") + "|" + placed.join(","),
          "true|4|null:8,20")
    var withoutRow = { cursorIndex: 4,
                       setCursor: function (index) { scrolled.push(index) },
                       visibleItemFor: function (index) { return null } }
    check("and with no delegate under the cursor it opens nothing and answers false",
          Menu.openAtCursor(withoutRow, { openAt: function (point) { placed.push(point) } }, 8)
              + "|" + placed.length, "false|1")
}

// Menus.html's background column, on a right click that landed on no row. Its rows, its order and
// its three rules are the board's; New File is the one row it draws that this release does not
// build, because the backend has mkdir and no create-empty-file command of any kind.
function runBackground(check) {
    function background(hiddenActions) {
        return Menu.listingEntries({ showHidden: false, hasRow: false, rowInDropbox: false,
                                     dropboxPath: "", taildropPeers: [], archiveFormats: [],
                                     rowIsArchive: false, rowIsImage: false, canConvert: false,
                                     hiddenActions: hiddenActions })
    }
    // src/uischema.rs ships Open in terminal switched off, which is the state the board draws.
    check("the background menu at the shipped defaults is the board's own column",
          labels(background(["delete", "openwith", "openTerminal", "moveto", "copyto",
                             "properties", "permissions", "copypath"])),
          "New folder|-|Paste|Select all|-|Sort by|Show hidden files|-|Settings")
    check("and switching Open in terminal on puts it back beside the hidden toggle",
          labels(background([])),
          "New folder|-|Paste|Select all|-|Sort by|Open in terminal|Show hidden files|-|Settings")
    // Every row is marked, the rule ui/MenuRow.qml enforces for the row menu; a background row that
    // drew no mark would be the one unmarked row in the product.
    var marks = []
    var rows = background([])
    for (var i = 0; i < rows.length; i++)
        marks.push(rows[i].separator === true ? "-" : (rows[i].mark || rows[i].glyph || ""))
    check("and every background row carries its own mark",
          marks.join("|"),
          "folder-plus|-|clipboard|check|-|sort|terminal|eye|-|sliders")
    // The flyout can only offer an order ui/js/Sort.js will really ask the backend for.
    check("Sort by is a submenu row over the three orders the backend can produce",
          Menu.hasSubmenu(findEntry(rows, "sort")) + "|"
          + findEntry(rows, "sort").submenu.map(function (e) { return e.id + "=" + e.label }).join("|"),
          "true|name=Name|size=Size|mtime=Date Modified")
    check("and its flyout takes the sort mark, not the archive one the other flyouts default to",
          Menu.submenuGlyph("sort") + "|" + Menu.submenuGlyph("taildrop") + "|"
          + Menu.submenuGlyph("compress"), "sort|server|archive")
    // The hidden toggle is locked in both menus, so the background column can never be emptied of it.
    check("the locked hidden toggle survives a hidden set that names it",
          labels(background(["newFolder", "paste", "selectAll", "sort", "openTerminal",
                             "toggleHidden", "settings"])),
          "Show hidden files")
}

// The Menus section's consumer. menu.hidden stores what is HIDDEN, so a row named there leaves the
// menu; the rules it divided leave with it, and neither of the two locked rows can be taken out.
function runHidden(check, full) {
    function menu(hiddenActions) {
        return labels(Menu.listingEntries({
            showHidden: false, hasRow: true, rowInDropbox: false,
            dropboxPath: "/home/jw/Dropbox", taildropPeers: [{ id: "x", label: "Box" }],
            archiveFormats: ["zip"], rowIsArchive: false, rowIsImage: false, canConvert: true,
            hiddenActions: hiddenActions
        }))
    }
    check("no hidden set at all draws the whole menu", menu([]), labels(full))
    check("an undefined set is the same as an empty one", menu(undefined), labels(full))
    check("one hidden action loses its row and nothing else",
          menu(["paste"]),
          "Open|Copy path|-|Cut|Copy|Duplicate|Rename|-|Compress|-|Send with Taildrop|"
          + "Move to Dropbox|-|Move to Trash|-|Open in terminal|New folder|Show hidden files")
    // A group that loses every member loses its separator too, which is the board's own rule and
    // the reason the answer below has three rules and not six.
    check("a group emptied by the settings takes its rule with it",
          menu(["cut", "copy", "paste", "duplicate", "rename", "trash", "copypath"]),
          "Open|-|Compress|-|Send with Taildrop|Move to Dropbox|-|Open in terminal|New folder|Show hidden files")
    check("hiding everything hideable still leaves the two locked rows and New folder",
          menu(["cut", "copy", "paste", "duplicate", "rename", "trash", "copypath", "openTerminal",
                "compress", "taildrop", "dropbox", "open", "toggleHidden"]),
          "Open|-|New folder|Show hidden files")
    // The shipped set named this row "terminal" while the menu built it as "openTerminal", so the
    // switch missed it and every menu drew it. The id the panel stores is the action id, as it is
    // for every other row.
    check("the shipped hidden id for Open in terminal is the action the menu really builds",
          menu(["openTerminal"]).indexOf("Open in terminal"), -1)
    check("Open and the hidden toggle are refused by the filter itself, not only by the panel",
          Menu.isHidden(["open", "toggleHidden"], "open") + "|"
          + Menu.isHidden(["open", "toggleHidden"], "toggleHidden"), "false|false")
    // applyHidden is called on a built list, so it is checked on one too: a leading rule would be
    // drawn against the top of the frame, and a trailing one against nothing at all.
    check("a leading rule left by a hidden first row is dropped",
          Menu.applyHidden([{ action: "a", label: "A" }, { separator: true },
                            { action: "b", label: "B" }], ["a"]).length, 1)
    check("a trailing rule is dropped as well",
          Menu.applyHidden([{ action: "a", label: "A" }, { separator: true },
                            { action: "b", label: "B" }], ["b"]).length, 1)
    check("two rules never end up beside each other",
          Menu.applyHidden([{ action: "a", label: "A" }, { separator: true },
                            { action: "b", label: "B" }, { separator: true },
                            { action: "c", label: "C" }], ["b"]).length, 3)
}
