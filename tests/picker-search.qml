import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/Keymap.js" as Keymap

// No model or backend stub: names, marks and portal replies all come from the shipped picker.
ShellRoot {
    id: root
    property var pickerShell: null
    property var win: null
    property var keys: null
    property var listing: null
    property var workerBefore: null
    property int listingReplies: 0
    property int repliesBefore: 0
    property var rowsBefore: []
    property int heldBefore: 0
    property int totalBefore: 0
    property string scenario: Quickshell.env("FLEA_PICKER_SEARCH_CASE")
    property int stage: 0
    property double stamp: Date.now()
    property int failures: 0
    property int checks: 0
    property bool driving: false
    property bool finished: false
    property bool expectAnswer: false
    readonly property int settleMs: 180
    readonly property int deadlineMs: 8000
    readonly property var matches: ["a-pick.txt", "c-pick.txt", "e-pick.png"]
    readonly property string startFolder: JSON.parse(Quickshell.env("FLEA_PICKER")).folder

    function check(label, got, want) {
        checks++
        var ok = JSON.stringify(got) === JSON.stringify(want)
        if (!ok) failures++
        console.log("PICKER_SEARCH " + (ok ? "PASS " : "FAIL ") + label
            + " got=" + JSON.stringify(got) + " expected=" + JSON.stringify(want))
        return ok
    }
    function finish(exitNow) {
        if (finished) return
        finished = true
        console.log("PICKER_SEARCH DONE checks=" + checks + " failed=" + failures)
        if (exitNow !== false) Qt.exit(failures ? 1 : 0)
    }
    function advance(next) { stage = next; stamp = Date.now() }
    function descendants(item) {
        var out = [item]
        for (var i = 0; i < out.length; i++) {
            var kids = out[i].children || []
            for (var j = 0; j < kids.length; j++) out.push(kids[j])
        }
        return out
    }
    function typed(name) {
        return descendants(win.contentItem).filter(function(item) {
            return String(item).indexOf(name + "_QMLTYPE") === 0
        })[0] || null
    }
    function strip() { return typed("FilterStrip") }
    function names() { return win.rows.map(function(row) { return row.n }) }
    function indexOf(name) { return win.held + names().indexOf(name) }
    function press(key, modifiers) { keys.keyClick(key, modifiers || Qt.NoModifier, -1) }
    function typeText(text) {
        for (var i = 0; i < text.length; i++) press(Qt.Key_A + text.charCodeAt(i) - 97)
    }
    function nextKey() { return win.viewMode === "grid" ? Qt.Key_Right : Qt.Key_Down }
    function previousKey() { return win.viewMode === "grid" ? Qt.Key_Left : Qt.Key_Up }
    function openQuery(query) {
        press(Qt.Key_Slash)
        var ok = check("slash opens filename typing", win.filterTyping, true)
        ok = check("slash draws the shared query strip", !!strip() && strip().visible && strip().height > 0, true) && ok
        if (!ok) { finish(); return false }
        typeText(query)
        check("native text updates filename query", win.filterQuery, query)
        return true
    }
    function checkView(wanted) {
        check("query reports only matching immediate rows", win.shownTotal, wanted.length)
        check("visible model is narrowed", win.viewItem().model, wanted.length)
        var indices = wanted.map(function(name) { return root.indexOf(name) })
        check("shown keeps backend listing-row indices", win.shown, indices)
        for (var i = 0; i < wanted.length; i++) {
            var cell = win.viewItem().itemAtIndex(i)
            if (!check("matching delegate " + i + " exists", !!cell, true)) continue
            check("matching delegate " + i + " draws the intended file", cell.row ? cell.row.n : "", wanted[i])
            check("matching delegate " + i + " maps its listing index", cell.listingIndex, indices[i])
        }
    }
    function checkNoListing() {
        check("filename typing never replaces the listing worker", listing.current === workerBefore, true)
        check("filename typing fetches no listing or row window", listingReplies - repliesBefore, 0)
        check("filename typing keeps held names", names(), rowsBefore)
        check("filename typing keeps the original directory count and window", [win.total, win.held], [totalBefore, heldBefore])
    }
    function checkCleared(label) {
        check(label + " clears only filename state", [win.filterQuery, win.filterTyping, win.answered], ["", false, false])
        check(label + " collapses the strip", strip().visible, false)
        check(label + " restores the unfiltered model", win.viewItem().model, win.total)
    }
    function checkMarks(wanted) {
        var got = win.marks.map(function(mark) { return mark.path }).sort()
        var paths = wanted.map(function(name) { return root.startFolder + "/" + name }).sort()
        check("checked identities exclude every hidden and recursive row", got, paths)
    }
    function commit() {
        var query = win.filterQuery
        press(Qt.Key_Return)
        check("Enter commits without accepting a file", [win.filterQuery, win.filterTyping, win.answered], [query, false, false])
        check("Enter returns keyboard ownership to the listing", win.viewItem().activeFocus, true)
        check("committed query keeps its strip", strip().visible, true)
    }
    function accept() {
        expectAnswer = true
        advance(90)
        press(Qt.Key_Return)
    }
    function choosePictures() {
        var trigger = typed("PickerFilter")
        if (!check("the real file-type control remains available", !!trigger && trigger.visible && trigger.enabled, true)) {
            finish()
            return
        }
        keys.mouseClick(trigger, trigger.width / 2, trigger.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
        if (!check("file-type dropdown still opens beside filename filtering", !!trigger.menu && trigger.menu.opened, true)) {
            finish()
            return
        }
        press(Qt.Key_Down)
        press(Qt.Key_Return)
    }
    function step() {
        if (stage === 0) {
            if (!win || win.listingState !== "ready" || win.pendingListings || win.markRequest || Date.now() - stamp < settleMs) return
            if (scenario === "recent" && !win.recent) {
                win.open("flea:recent")
                stamp = Date.now()
                return
            }
            check("requested real picker view is loaded", win.viewMode, Quickshell.env("FLEA_PICKER_SEARCH_VIEW"))
            check("shared slash binding still resolves to filename filter", Keymap.lookup(Qt.Key_Slash, "/", Qt.NoModifier, "listing"), "filter")
            listing = typed("PickerListing")
            if (!check("real replaceable listing backend exists", !!listing && !!listing.current, true)) { finish(); return }
            workerBefore = listing.current
            repliesBefore = listingReplies
            rowsBefore = names()
            totalBefore = win.total
            heldBefore = win.held
            if (scenario === "scope") {
                check("scope fixture exceeds the loaded row window", win.total > win.rows.length, true)
                check("unloaded matching file is absent from held rows", names().indexOf("zz-unloaded-only.txt"), -1)
            } else if (scenario === "recent") {
                check("real XBEL Recent exceeds the held window", win.total > win.rows.length, true)
                check("Recent holds full paths with the parent-only query term", names().every(function(name) { return name.indexOf("/wide/") >= 0 }), true)
            } else {
                win.cursorIndex = indexOf("b-hidden.txt")
                // Pictures are absent initially under the Text chip; every other scenario starts All files.
                check("cursor begins on a row the query will hide", win.rowFor(win.cursorIndex).n, "b-hidden.txt")
            }
            win.focusView()
            advance(1)
            var query = scenario === "scope" ? "unloaded" : scenario === "recent" ? "wide"
                : scenario === "navigate" ? "folder" : scenario === "escape" ? "missing" : "pick"
            openQuery(query)
            return
        }
        if (Date.now() - stamp < settleMs) return
        if (stage === 1) {
            var wanted = scenario === "scope" || scenario === "escape" || scenario === "recent" ? []
                : scenario === "navigate" ? ["z-folder"] : scenario === "chips" ? matches.slice(0, 2) : matches
            checkView(wanted)
            checkNoListing()
            if (wanted.length) check("hidden cursor rebases onto the first matching listing row", win.cursorIndex, indexOf(wanted[0]))
            else check("no-match query cannot accept a hidden cursor file", [win.cursorIndex, win.canAccept], [-1, false])
            commit()
            if (finished) return
            if (scenario === "activate") {
                press(nextKey())
                check("arrow skips a hidden backend row", win.rowFor(win.cursorIndex).n, "c-pick.txt")
                check("arrow cursor remains in listing-row coordinates", win.cursorIndex, indexOf("c-pick.txt"))
                accept()
            } else if (scenario === "escape") {
                press(Qt.Key_Escape)
                checkCleared("Esc after a committed no-match query")
                check("clearing no-match leaves the listing focused", win.viewItem().activeFocus, true)
                if (!openQuery("pick")) return
                press(Qt.Key_Escape)
                checkCleared("Esc while typing")
                if (!openQuery("")) return
                press(Qt.Key_Escape)
                checkCleared("Esc in an empty query strip")
                check("Ctrl+L keeps its shared path-bar binding", Keymap.lookup(Qt.Key_L, "l", Qt.ControlModifier, "listing"), "pathBar")
                press(Qt.Key_L, Qt.ControlModifier)
                var pathEntry = typed("PathEntry")
                check("Ctrl+L still opens and focuses the real path field", !!pathEntry && pathEntry.editing && pathEntry.pathField.activeFocus, true)
                press(Qt.Key_Escape)
                check("Esc closes the path field without cancelling", [!!pathEntry && pathEntry.editing, win.answered], [false, false])
                win.focusView()
                expectAnswer = true
                advance(90)
                press(Qt.Key_Escape)
            } else if (scenario === "navigate") {
                advance(20)
                press(Qt.Key_Return)
            } else if (scenario === "chips") {
                check("Text chip is independent of filename query", win.filterIndex, 0)
                advance(30)
                choosePictures()
            } else if (scenario === "all") {
                check("shared select-all binding remains available", Keymap.lookup(Qt.Key_A, "a", Qt.ControlModifier, "listing"), "selectAll")
                advance(40)
                press(Qt.Key_A, Qt.ControlModifier)
            } else if (scenario === "range") {
                advance(50)
                press(nextKey(), Qt.ShiftModifier)
            } else if (scenario === "scope") advance(60)
            else if (scenario === "recent") {
                var recentScope = "0 of " + win.rows.length + " shown · of " + win.total + " in Recent"
                check("bounded Recent summary never calls history a folder", win.filterSummary, recentScope)
                check("Recent strip draws its actual scope", descendants(strip()).some(function(item) { return item.text === recentScope && item.visible }), true)
                press(Qt.Key_Escape)
                checkCleared("Clearing a parent-path-only Recent query")
                advance(70)
                openQuery("loaded")
            }
            return
        }
        if (stage === 20) {
            if (win.path !== startFolder + "/z-folder" || win.listingState !== "ready" || win.pendingListings) return
            checkCleared("Enter navigation into a matching directory")
            check("navigation loads the real child folder", names(), ["nested-pick.txt"])
            advance(21)
            press(Qt.Key_Backspace)
        } else if (stage === 21) {
            if (win.path !== startFolder || win.listingState !== "ready" || win.pendingListings) return
            checkCleared("Parent navigation")
            check("Parent returns to the actual original listing", names(), rowsBefore)
            finish()
        } else if (stage === 30) {
            if (win.filterIndex !== 1 || win.listingState !== "ready" || win.pendingListings) return
            checkCleared("Changing file-type chip")
            check("Pictures chip still narrows backend rows", names(), ["z-folder", "d-hidden.png", "e-pick.png"])
            win.focusView()
            advance(31)
            openQuery("pick")
        } else if (stage === 31) {
            checkView(["e-pick.png"])
            commit()
            press(Qt.Key_Escape)
            checkCleared("Esc with a Pictures chip")
            check("Esc preserves the selected type chip", win.filterIndex, 1)
            check("clearing name query never restores excluded Text files", names(), ["z-folder", "d-hidden.png", "e-pick.png"])
            finish()
        } else if (stage === 40) {
            if (win.markRequest) return
            checkMarks(matches)
            check("select-all keeps filename query committed", [win.filterQuery, win.filterTyping], ["pick", false])
            accept()
        } else if (stage === 50) {
            if (win.markRequest) return
            check("Shift movement skips the hidden intervening row", win.cursorIndex, indexOf("c-pick.txt"))
            checkMarks(matches.slice(0, 2))
            advance(51)
            press(previousKey(), Qt.ShiftModifier)
        } else if (stage === 51) {
            if (win.markRequest) return
            checkMarks(matches.slice(0, 1))
            var last = win.viewItem().itemAtIndex(2)
            if (!check("last matching delegate is available to Shift-click", !!last, true)) { finish(); return }
            advance(52)
            keys.mouseClick(last, last.width - 10, last.height / 2, Qt.LeftButton, Qt.ShiftModifier, -1)
        } else if (stage === 52) {
            if (win.markRequest) return
            check("Shift-click cursor maps the visible position back to its listing row", win.cursorIndex, indexOf("e-pick.png"))
            checkMarks(matches)
            accept()
        } else if (stage === 60) {
            checkNoListing()
            checkView([])
            var summary = "0 of " + win.rows.length + " shown · of " + win.total + " in this folder"
            check("scope summary distinguishes loaded rows from the whole folder", win.filterSummary, summary)
            check("shared query strip actually draws the scope caveat", descendants(strip()).some(function(item) { return item.text === summary && item.visible }), true)
            check("no-match Enter never answers or recursively finds an unloaded file", win.answered, false)
            finish()
        } else if (stage === 70) {
            var matching = rowsBefore.filter(function(name) { return name.split("/").pop().indexOf("loaded") >= 0 })
            var indices = matching.map(function(name) { return root.indexOf(name) })
            check("Recent filename query matches the held basenames", win.shown, indices)
            check("Recent filename query narrows the actual visible model", win.viewItem().model, matching.length)
            var first = win.viewItem().itemAtIndex(0)
            check("Recent delegate draws a basename rather than its parent path", first && first.shownRow ? first.shownRow.n : "", "a-loaded-000.txt")
            commit()
            press(nextKey())
            check("Recent cursor advances through matching listing identities", win.rowFor(win.cursorIndex).n.split("/").pop(), "a-loaded-001.txt")
            accept()
        }
    }

    Connections {
        target: root.win
        function onAnsweredChanged() {
            if (!root.win.answered) return
            root.check("only explicit final activation or cancellation answers", root.expectAnswer, true)
            root.finish(false)
        }
    }
    Connections {
        target: root.listing
        function onMessage(message) {
            if (message.t === "listed" || message.t === "rows") root.listingReplies++
        }
    }
    Component.onCompleted: {
        var comp = Qt.createComponent("flea/PickerWindow.qml")
        if (comp.status !== Component.Ready) {
            console.log("PICKER_SEARCH FAIL compile " + comp.errorString())
            Qt.exit(1)
            return
        }
        pickerShell = comp.createObject(root)
        if (!pickerShell) {
            console.log("PICKER_SEARCH FAIL create real picker")
            Qt.exit(1)
            return
        }
        win = pickerShell.pickerWin
        win.width = 800
        win.height = 410
        keys = Qt.createQmlObject("import QtTest; TestEvent {}", win.contentItem)
    }
    Timer {
        interval: 20
        running: !root.finished
        repeat: true
        onTriggered: {
            if (root.driving) return
            if (Date.now() - root.stamp > root.deadlineMs) {
                root.check("probe completes", "timeout at stage " + root.stage, "complete")
                root.finish()
                return
            }
            // QtTest key and mouse events pump Qt's loop; a nested timer must not drive the same stage twice.
            root.driving = true
            try { root.step() } finally { root.driving = false }
        }
    }
}
