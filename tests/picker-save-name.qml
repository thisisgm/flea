import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// Real picker delegates receive Qt pointer events; controlled replies exercise only the stale-review boundary.
ShellRoot {
    id: root
    property var shell: null
    property var win: null
    property var driver: null
    property var save: null
    property var backend: null
    property var steps: []
    property int at: 0
    property bool entered: false
    property bool driving: false
    property int checks: 0
    property int failures: 0
    property double stamp: Date.now()
    property int staleId: 0
    property var staleReview: null
    readonly property string scenario: Quickshell.env("FLEA_SAVE_NAME_CASE")
    readonly property int pollMs: 20
    readonly property int deadlineMs: 5000
    readonly property int inputDelayMs: 1
    readonly property string typedName: "x.txt"
    readonly property string firstName: "a-first.txt"
    readonly property string otherName: "archive.tar.gz"
    readonly property string unicodeName: "日本語 résumé.txt"
    readonly property string folderName: "z-folder"

    function check(label, got, want) {
        checks++
        var ok = JSON.stringify(got) === JSON.stringify(want)
        if (!ok) failures++
        console.log("SAVE_NAME " + (ok ? "PASS " : "FAIL ") + label
            + " got=" + JSON.stringify(got) + " expected=" + JSON.stringify(want))
    }
    function done() {
        console.log("SAVE_NAME DONE " + checks + " checks, " + failures + " failed")
        Qt.exit(failures ? 1 : 0)
    }
    function descendants(item) {
        var out = [item]
        for (var i = 0; i < out.length; i++) {
            var kids = out[i].children || []
            for (var j = 0; j < kids.length; j++) out.push(kids[j])
        }
        return out
    }
    function rowIndex(name) {
        for (var i = 0; i < win.total; i++) {
            var row = win.rowFor(i)
            if (row && row.n === name) return i
        }
        throw new Error("missing fixture row " + name)
    }
    function click(name, twice) {
        var view = win.viewItem()
        var cell = view.itemAtIndex(rowIndex(name))
        check("pointer target exists: " + name, !!cell, true)
        if (!cell) return
        if (twice) driver.mouseDoubleClickSequence(cell, cell.width / 2, cell.height / 2, Qt.LeftButton, Qt.NoModifier, inputDelayMs)
        else driver.mouseClick(cell, cell.width / 2, cell.height / 2, Qt.LeftButton, Qt.NoModifier, inputDelayMs)
    }
    function key(key) { driver.keyClick(key, Qt.NoModifier, inputDelayMs) }
    function typeName() {
        win.saveName = "draft.txt"
        save.selectStem()
        key(Qt.Key_X)
        check("real editing replaces the selected stem", win.saveName, typedName)
    }
    function adopted(name, stem) {
        check("exact clicked basename", win.saveName, name)
        check("Filename receives focus", save.fieldItem.activeFocus, true)
        check("only the stem is selected", save.fieldItem.selectedText, stem)
        check("selection never submits", [win.answered, win.reviewAsked, win.submitting], [false, false, false])
    }
    function step(run, wait) { steps.push({run: run, wait: wait || function() { return true }}) }
    function ready() { return win.listingState !== "loading" && !win.listingFailed && !win.pendingListings && win.rows.length === win.total }
    function setup() {
        check("caller name survives startup", win.saveName, "caller.txt")
        if (win.saving) {
            check("Save starts on Filename", save.fieldItem.activeFocus, true)
            check("the first file is already current", [win.cursorIndex, win.rowFor(0).n], [0, firstName])
        }
        if (scenario === "save") saveSteps()
        else if (scenario === "collision") collisionSteps()
        else if (scenario === "stale") staleSteps()
        else {
            step(function() {
                click(firstName)
                check("non-Save click preserves name", win.saveName, "caller.txt")
                check("non-Save click keeps listing focus", win.viewItem().activeFocus, true)
                check("non-Save click never submits", win.answered, false)
                if (win.marksAllowed) check("ordinary row click does not mark", win.marks.length, 0)
            })
        }
        if (scenario !== "collision") step(done)
    }
    function saveSteps() {
        step(function() { click(firstName); adopted(firstName, "a-first") })
        step(function() { typeName(); click(otherName); adopted(otherName, "archive.tar") })
        step(function() { typeName(); click(folderName); check("folder click preserves typed name", win.saveName, typedName) })
        step(function() { click(unicodeName); adopted(unicodeName, "日本語 résumé") })
        step(function() { click("plain"); adopted("plain", "plain") })
        step(function() {
            typeName()
            win.focusView()
            key(Qt.Key_Down)
            check("arrow preserves typed name and listing focus", [win.saveName, win.viewItem().activeFocus], [typedName, true])
            win.cursorIndex = 0
            check("implicit cursor assignment preserves typed name", win.saveName, typedName)
            key(Qt.Key_Period)
        }, ready)
        step(function() {
            check("hidden toggle preserves typed name and focus", [win.showHidden, win.saveName, win.viewItem().activeFocus], [true, typedName, true])
            click(".env"); adopted(".env", ".env")
            typeName()
            win.requestSort({key: "size", desc: true})
        }, ready)
        step(function() {
            check("sort preserves typed name and Filename focus", [win.saveName, save.fieldItem.activeFocus], [typedName, true])
            win.openWithoutHistory(win.path)
        }, ready)
        step(function() {
            check("relist preserves typed name and Filename focus", [win.saveName, save.fieldItem.activeFocus], [typedName, true])
            var states = ["loading", "pending", "failed", "missing", "unavailable", "submitting"]
            for (var i = 0; i < states.length; i++) {
                var state = states[i]
                var held = win.held
                if (state === "loading") win.listingState = "loading"
                if (state === "pending") win.pendingListings = 1
                if (state === "failed") win.listingFailed = true
                if (state === "missing") win.held = win.total
                if (state === "unavailable") win.backendUnavailable = true
                if (state === "submitting") win.acceptMarks = true
                // The held-row gap still has a delegate; its pointer dispatch must not adopt a missing row.
                var cell = win.viewItem().itemAtIndex(0)
                driver.mouseClick(cell, cell.width / 2, cell.height / 2, Qt.LeftButton, Qt.NoModifier, inputDelayMs)
                check(state + " pointer preserves typed name", win.saveName, typedName)
                check(state + " pointer never answers", win.answered, false)
                win.listingState = "ready"; win.pendingListings = 0; win.listingFailed = false
                win.held = held; win.backendUnavailable = false; win.acceptMarks = false
            }
        })
        step(function() {
            click(firstName, true); adopted(firstName, "a-first")
            var file = win.path + "/" + firstName
            win.doubleActivate(rowIndex(firstName), file, file)
            check("file double-activation route never accepts in Save", [win.answered, win.reviewAsked], [false, false])
        })
        step(function() {
            var before = win.path
            // QtTest's synthetic sequence yields two single taps here; test the existing folder route separately, as picker-hunt does.
            var folder = before + "/" + folderName
            win.doubleActivate(rowIndex(folderName), folder, folder)
            check("folder double-activation route still navigates", win.path, folder)
            check("folder navigation preserves name", win.saveName, firstName)
        }, ready)
    }
    function collisionSteps() {
        step(function() { click(firstName, true); adopted(firstName, "a-first") }, function() { return win.saveReady })
        step(function() {
            check("existing clicked file collides without an automatic review", [win.saveCollision, win.reviewAsked, win.answered], [true, false, false])
            key(Qt.Key_Return)
            check("Enter asks for explicit Replace", [win.collisionReview, win.answered], [true, false])
            key(Qt.Key_Return)
            check("a second Enter cannot replace", [win.collisionReview, win.answered], [true, false])
            click(otherName)
            check("a different click invalidates the asked review", [win.reviewAsked, win.saveReady, win.answered], [false, false, false])
        }, function() { return win.saveReady })
        step(function() {
            key(Qt.Key_Return)
            check("new target needs its own explicit Replace", [win.saveName, win.collisionReview, win.answered], [otherName, true, false])
            var button = descendants(win.contentItem).filter(function(item) { return item.label === "Replace" && item.tabHandle === true })[0]
            check("real Replace button is available", !!button && button.available, true)
            if (!button) { done(); return }
            driver.mouseClick(button, button.width / 2, button.height / 2, Qt.LeftButton, Qt.NoModifier, inputDelayMs)
        }, function() { return win.answered })
    }
    function staleSteps() {
        step(function() {}, function() { return win.saveReady && !win.saveRequest })
        step(function() {
            staleId = ++win.nextCheck
            win.saveRequest = staleId
            win.probeKey = win.saveKey
            click(firstName)
            check("click invalidates previous save result", win.saveReady, false)
            backend.pickerResult({id: staleId, ok: true, review: 999999, collision: false, path: "/not-the-clicked-name"})
            check("late old-name result cannot become ready or answer", [win.saveReady, win.answered], [false, false])
        }, function() { return win.saveReady && !win.saveRequest })
        step(function() {
            check("fresh check belongs to clicked name", [win.saveName, win.saveReview.path, win.saveCollision], [firstName, win.path + "/" + firstName, true])
            staleReview = win.saveReview
            staleId = ++win.nextCheck
            win.reviewRequest = staleId
            click(otherName)
            check("submitting disables row adoption", win.saveName, firstName)
            win.adoptSaveName(rowIndex(otherName))
            check("submitting guard refuses a direct adoption", win.saveName, firstName)
            win.saveName = "edited-after-review.txt"
            backend.pickerResult({id: staleId, ok: true, path: staleReview.path})
            check("stale reviewed target cannot answer", [win.answered, win.reviewRequest, win.saveReady], [false, 0, false])
        })
    }
    Component.onCompleted: {
        var comp = Qt.createComponent("flea/PickerWindow.qml")
        if (comp.status !== Component.Ready) { console.log("SAVE_NAME FAIL " + comp.errorString()); Qt.exit(1); return }
        shell = comp.createObject(root)
        win = shell.pickerWin
        var items = descendants(win.contentItem)
        save = items.filter(function(item) { return typeof item.selectStem === "function" })[0]
        backend = items.filter(function(item) { return item.pickerOnly === true })[0]
        driver = Qt.createQmlObject("import QtTest; TestEvent {}", win.contentItem)
    }
    Connections {
        target: root.win
        function onAnsweredChanged() {
            if (!root.win.answered) return
            root.check("only the collision leg explicitly answers", root.scenario, "collision")
            console.log("SAVE_NAME DONE " + root.checks + " checks, " + root.failures + " failed")
        }
    }
    Timer {
        interval: root.pollMs
        running: !root.win || !root.win.answered
        repeat: true
        onTriggered: {
            if (root.driving) return
            root.driving = true
            try {
                if (Date.now() - root.stamp > root.deadlineMs) {
                    root.check("step " + root.at + " completes before deadline", false, true); root.done(); return
                }
                if (!root.steps.length) {
                    if (!root.win || !root.ready()) return
                    root.setup()
                }
                var step = root.steps[root.at]
                if (!root.entered) { root.entered = true; step.run() }
                if (step.wait()) { root.at++; root.entered = false; root.stamp = Date.now() }
            } catch (error) {
                root.check("step " + root.at + " throws no exception", String(error), "none"); root.done()
            } finally { root.driving = false }
        }
    }
}
