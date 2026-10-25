import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "flea" as Flea

// The folder jump's wiring through the real ui/ChromeBar.qml and ui/PathJump.qml, keys delivered by
// QtTest's TestEvent into an offscreen window. tests/jump-ui.sh plays the backend: this file records what
// the bar asks for and answers it by hand, which is what lets a stale answer and a missing one be staged.
ShellRoot {
    id: root
    readonly property string home: Quickshell.env("HOME")
    readonly property string here: root.home + "/Documents"
    readonly property var sources: ({
        favourites: [root.home + "/Projects", root.home + "/Documents/claude/flea", root.home + "/Documents/claude/omarchy"],
        zoxide: [root.home + "/Documents", root.home + "/Downloads"],
        recent: [root.home + "/Pictures/screenshots", root.home + "/Work/field"]
    })
    property var asked: []
    property var entered: []
    property var peeked: []
    property var firstJump: null
    // Counted on the change, since the settle clears on the next frame and one can land inside keyClickChar.
    property int settleArms: 0
    property int checks: 0
    property int failures: 0
    property int stepIndex: 0
    property real stepStarted: 0
    property bool inStep: false

    function check(label, actual, expected) {
        root.checks++
        if (JSON.stringify(actual) === JSON.stringify(expected)) {
            console.log("ok   " + label)
            return
        }
        root.failures++
        console.log("FAIL " + label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    function rows() {
        return chrome.jump.entries.filter(function (e) { return e.separator !== true }).map(function (e) { return e.path })
    }
    // The real ui/Ipc.qml seam over this chrome, so pathSuggestionRows and pathSuggestionCursor are the shipped readers.
    function suggestionRows() { return ipc.seam.pathSuggestionRows() }
    function suggestionCursor() { return ipc.seam.pathSuggestionCursor() }
    function type(text) {
        for (var i = 0; i < text.length; i++)
            keys.keyClickChar(text.charAt(i), Qt.NoModifier, -1)
    }
    function press(key) { keys.keyClick(key, Qt.NoModifier, -1) }
    // The dropdown CardScroll's stepping handler, the wheel's own route into Jump.step.
    function stepHandler(item) {
        var stack = [item]
        while (stack.length > 0) {
            var o = stack.pop()
            if (o !== item && o.stepMode === true) return o
            var kids = (o && o.children !== undefined) ? o.children : []
            for (var i = 0; i < kids.length; i++) stack.push(kids[i])
        }
        return null
    }
    function answer(offset) {
        var last = root.asked[root.asked.length - 1]
        chrome.jump.take(last.id + offset, root.sources.favourites, root.sources.zoxide, root.sources.recent)
    }
    // Each step returns true when it is done; a step that waits returns false until its condition holds.
    readonly property var steps: [
        function () {
            root.check("a fresh bar builds no jump until editing starts", chrome.jump === null, true)
            root.check("pathSuggestionPointer reads none before the popup is built", ipc.seam.pathSuggestionPointer(), "none")
            chrome.startEdit()
            root.check("the first edit builds the jump and keeps it", chrome.jump !== null, true)
            root.firstJump = chrome.jump
            return true
        },
        function () { return root.asked.length === 1 },
        function () {
            root.check("a stale history still asks at once, with what is kept", root.asked[0].recent, [])
            root.check("with the favourites as the rail holds them", root.asked[0].favourites, root.sources.favourites)
            root.check("the provisional ask names no ranking", root.asked[0].ranking, 0)
            root.type("o")
            root.check("no answer yet, so no dropdown", chrome.jump.shown, false)
            root.press(Qt.Key_Return)
            root.check("Enter on a name before the answer is held, not resolved as a path", [root.entered, chrome.editing], [[], true])
            chrome.jump.take(0, root.sources.favourites, root.sources.zoxide, root.sources.recent)
            root.check("an answer carrying no ask's id is dropped", [chrome.jump.shown, root.entered], [false, []])
            chrome.jump.take(1, root.sources.favourites, root.sources.zoxide, [])
            root.check("the provisional answer draws but does not spend the held Enter", [root.entered, chrome.editing], [[], true])
            root.check("its rows are the kept sources alone", root.rows(), [
                root.home + "/Projects", root.home + "/Documents/claude/omarchy", root.home + "/Documents",
                root.home + "/Downloads", root.home + "/Documents/claude/flea"])
            root.check("the dropdown builds only while rows show", chrome.jump.dropBuilt, true)
            return true
        },
        // The whole ask waits for the provisional answer and the history read, in either order.
        function () { return root.asked.length === 2 },
        function () {
            root.check("the read history's files, newest first, on the whole ask", root.asked[1].recent,
                       [root.home + "/Pictures/screenshots/shot.png", root.home + "/Work/field/notes.md"])
            root.check("the whole ask names the provisional ask, so the backend runs zoxide once", root.asked[1].ranking, root.asked[0].id)
            root.answer(0)
            root.check("this open's whole answer takes the held Enter to the first row", root.entered, [root.home + "/Projects"])
            root.check("and the bar closes", chrome.editing, false)
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 3 },
        function () {
            root.answer(0)
            // The bar opens on the current path, which lists nothing, so the rows appear with the first name typed.
            root.check("the answer on a path line shows no rows and arms no settle", [chrome.jump.shown, chrome.jump.pointerSettling], [false, false])
            var armsBefore = root.settleArms
            root.type("o")
            root.check("the settle arms when rows appear", root.settleArms > armsBefore, true)
            // No frecency in this answer, so the own-name matches go favourites first, then zoxide, then recent.
            root.check("query o lists one ranked list", root.rows(), [
                root.home + "/Projects", root.home + "/Documents/claude/omarchy", root.home + "/Documents",
                root.home + "/Downloads", root.home + "/Pictures/screenshots", root.home + "/Documents/claude/flea", root.home + "/Work/field"])
            root.check("its rows stand seven delegates deep", chrome.jump.dropItem.liveCount, 7)
            var first = chrome.jump.dropItem.liveAt(0)
            root.answer(0)
            root.check("a second answer keeps delegates standing", chrome.jump.dropItem.liveAt(0) === first, true)
            root.check("the cursor opens on the first row", chrome.jump.cursor, 0)
            // One motion: narrow, widen, step and open with no answer in between.
            root.type("ma")
            root.check("a narrowing keystroke lists the one folder still matching", root.rows(), [root.home + "/Documents/claude/omarchy"])
            root.check("rebinding the standing delegates", chrome.jump.dropItem.liveAt(0) === first, true)
            root.check("with the cursor back on the first row", chrome.jump.cursor, 0)
            root.press(Qt.Key_Backspace); root.press(Qt.Key_Backspace)
            root.check("widening again rebinds the same delegates", [root.rows().length, chrome.jump.dropItem.liveAt(0) === first], [7, true])
            root.press(Qt.Key_Down); root.press(Qt.Key_Down); root.press(Qt.Key_Down)
            root.check("three downs move three rows", chrome.jump.cursor, 3)
            var dropH = root.stepHandler(chrome.jump.dropItem)
            root.check("the dropdown wheel handler steps", dropH !== null, true)
            if (dropH !== null) {
                dropH.handleWheel({ phase: 0, pixelDelta: { x: 0, y: 0 }, angleDelta: { x: 0, y: -120 }, accepted: false })
                root.check("one notch down steps like Down", chrome.jump.cursor, 4)
                dropH.handleWheel({ phase: 0, pixelDelta: { x: 0, y: 0 }, angleDelta: { x: 0, y: 120 }, accepted: false })
                root.check("one notch up steps back", chrome.jump.cursor, 3)
                dropH.handleWheel({ phase: 0, pixelDelta: { x: 0, y: -15 }, angleDelta: { x: 0, y: -120 }, accepted: false })
                root.check("a Qt 6.12 notch, pixels beside the angle, steps one row", chrome.jump.cursor, 4)
                dropH.handleWheel({ phase: 0, pixelDelta: { x: 0, y: 15 }, angleDelta: { x: 0, y: 120 }, accepted: false })
                root.check("and steps back", chrome.jump.cursor, 3)
            }
            root.press(Qt.Key_Down); root.press(Qt.Key_Up); root.press(Qt.Key_Down)
            root.press(Qt.Key_Return)
            root.check("Enter opens the row under the cursor", root.entered[1], root.home + "/Pictures/screenshots")
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 4 },
        function () {
            root.answer(0)
            root.type("Wo")
            root.press(Qt.Key_Tab)
            return root.peeked.length === 1
        },
        function () {
            chrome.completeWith(root.peeked[0].dir, root.peeked[0].hidden, [{ n: "Work", d: true }])
            root.check("Tab completes the one child the way it always has", chrome.editText, "Work/")
            root.check("and a line with a slash lists nothing", chrome.jump.shown, false)
            root.press(Qt.Key_Return)
            root.check("so Enter opens the completed child", root.entered[2], root.here + "/Work")
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 5 },
        function () {
            root.answer(0)
            root.type("zzz")
            root.press(Qt.Key_Return)
            root.check("a name that matches nothing is still a relative path", root.entered[3], root.here + "/zzz")
            root.check("with no rows the dropdown builds nothing", chrome.jump.dropBuilt, false)
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 6 },
        function () {
            root.type("o")
            root.press(Qt.Key_Return)
            root.check("a held Enter with no answer coming", [root.entered.length, chrome.editing], [4, true])
            return true
        },
        function () { return !chrome.editing || Date.now() - root.stepStarted > chrome.jump.answerLimitMs + 2000 },
        function () {
            root.check("resolves the line as a path once the answer limit passes", root.entered[4], root.here + "/o")
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 7 },
        function () {
            root.type("o")
            root.press(Qt.Key_Return)
            root.type("x")
            root.check("a key typed behind a held Enter is not typed", chrome.editText, "o")
            root.answer(0)
            root.check("so the answer opens what the line held when Enter went down", root.entered[5], root.home + "/Projects")
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 8 },
        function () {
            root.answer(0)
            root.type("Wo")
            root.check("Wo lists folders before Tab", chrome.jump.shown, true)
            root.press(Qt.Key_Tab)
            return root.peeked.length === 2
        },
        function () {
            chrome.completeWith(root.peeked[1].dir, root.peeked[1].hidden, [{ n: "Work", d: true }, { n: "Workshop", d: true }])
            root.check("two children stop Tab at their shared prefix, with no slash", chrome.editText, "Work")
            root.check("and a line Tab has touched lists nothing", chrome.jump.shown, false)
            root.press(Qt.Key_Return)
            root.check("so Enter opens ./Work as it always has", root.entered[6], root.here + "/Work")
            chrome.startEdit()
            return true
        },
        function () { return root.asked.length === 9 },
        function () {
            root.answer(0)
            root.type("o")
            return true
        },
        // The seam counts laid-out rows, so it is read once the dropdown has drawn.
        function () { return root.suggestionRows() > 0 },
        function () {
            root.check("pathSuggestionRows and pathSuggestionCursor read the drawn rows and the first row", [root.suggestionRows(), root.suggestionCursor()], [7, 0])
            chrome.jump.pointerGlobal = Qt.point(42, 64)
            root.check("pathSuggestionPointer reads the last point in popup coordinates", ipc.seam.pathSuggestionPointer(), "32 44")
            root.press(Qt.Key_Down)
            root.check("a Down moves pathSuggestionCursor one row", [root.suggestionRows(), root.suggestionCursor()], [7, 1])
            root.press(Qt.Key_Escape)
            root.check("esc closes the dropdown with the bar and opens nothing", [chrome.editing, chrome.jump.shown, root.entered.length], [false, false, 7])
            root.check("and the seam reads no rows and no cursor once it closes", [root.suggestionRows(), root.suggestionCursor()], [0, -1])
            root.check("pathSuggestionPointer reads none after the popup closes", ipc.seam.pathSuggestionPointer(), "none")
            // The recent history is read once and kept: every open so far read nothing more.
            var before = root.asked.length
            chrome.startEdit()
            root.check("an open with the history unchanged asks at once, from what it kept", root.asked.length, before + 1)
            root.check("and sends one ask that names no ranking", [root.asked[before].ranking, root.asked.length], [0, before + 1])
            root.check("so the history was read once, by the first open", chrome.jump.historyReads, 1)
            root.check("closing and reopening keeps the same jump", chrome.jump === root.firstJump, true)
            root.press(Qt.Key_Escape)
            history.command = ["sh", "-c", root.rewrite, "sh", root.xbel, root.home]
            history.running = true
            return true
        },
        // The desktop replaces the file, the way GTK writes it: a temp and a rename.
        function () { return !history.running && Date.now() - root.stepStarted > root.settleMs },
        function () { chrome.startEdit(); return true },
        function () { return root.asked.length === 11 },
        function () {
            // The provisional take is what sends the whole ask: the backend never answers here.
            root.answer(0)
            return true
        },
        function () { return root.asked.length === 12 },
        function () {
            root.check("the next open reads the replaced history, once", chrome.jump.historyReads, 2)
            root.check("and asks with its newest file first", root.asked[11].recent[0], root.home + "/Music/new.flac")
            root.check("naming the open's own provisional ask, and no older one", [root.asked[10].ranking, root.asked[11].ranking], [0, root.asked[10].id])
            root.press(Qt.Key_Escape)
            history.command = ["sh", "-c", "rm -f -- \"$1\"", "sh", root.xbel]
            history.running = true
            return true
        },
        function () { return !history.running && Date.now() - root.stepStarted > root.settleMs },
        function () { chrome.startEdit(); return true },
        function () { return root.asked.length === 13 },
        function () {
            root.answer(0)
            return true
        },
        function () { return root.asked.length === 14 },
        function () {
            root.check("a history that is gone is read as empty", [root.asked[13].recent, chrome.jump.historyReads], [[], 3])
            root.press(Qt.Key_Escape)
            return true
        }
    ]

    // Where the history lives for this run, and one newer bookmark written over it by a temp and a rename.
    readonly property string xbel: Quickshell.env("XDG_DATA_HOME") + "/recently-used.xbel"
    readonly property string rewrite: "printf '<?xml version=\"1.0\"?><xbel version=\"1.0\"><bookmark href=\"file://%s/Music/new.flac\" "
        + "visited=\"2026-09-24T10:00:00Z\"/></xbel>' \"$2\" > \"$1.new\" && mv -- \"$1.new\" \"$1\""
    // Long enough for the change to reach the watcher, which inotify delivers within milliseconds.
    readonly property int settleMs: 300
    Connections {
        target: chrome.jump
        function onPointerSettlingChanged() { if (chrome.jump.pointerSettling) root.settleArms += 1 }
    }

    Process { id: history }

    FloatingWindow {
        implicitWidth: 900
        implicitHeight: 600

        Item {
            anchors.fill: parent
            TestEvent { id: keys }
        }

        Flea.ChromeBar {
            id: chrome
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            path: root.here
            home: root.home
            onJumpRequested: function (id, ranking, favourites, recent) { root.asked = root.asked.concat([{ id: id, ranking: ranking, favourites: favourites, recent: recent }]) }
            onPathEntered: function (path) { root.entered = root.entered.concat([path]) }
            onCompleteRequested: function (dir, hidden) { root.peeked = root.peeked.concat([{ dir: dir, hidden: hidden }]) }
        }
    }

    Timer {
        interval: 10
        repeat: true
        running: root.stepIndex < root.steps.length
        onTriggered: {
            if (root.stepStarted === 0)
                root.stepStarted = Date.now()
            // QTest delivers a key through a nested event loop, which fires this timer again mid-step; it waits.
            if (root.inStep)
                return
            root.inStep = true
            var done = false
            try {
                done = root.steps[root.stepIndex]()
            } catch (error) {
                root.failures++
                console.log("FAIL step " + root.stepIndex + " threw: " + error)
                root.stepIndex = root.steps.length
            }
            if (done) {
                root.stepIndex++
                root.stepStarted = 0
            } else if (Date.now() - root.stepStarted > chrome.jump.answerLimitMs + 5000) {
                root.failures++
                console.log("FAIL step " + root.stepIndex + " never completed")
                root.stepIndex = root.steps.length
            }
            root.inStep = false
            if (root.stepIndex >= root.steps.length)
                console.log("jump-ui: " + root.checks + " checks, " + root.failures + " failed")
        }
    }

    Flea.Ipc {
        id: ipc
        chrome: chrome
        fleaWindow: QtObject {
            function rectOf(item) { return "10 20 300 400" }
        }
    }
}
