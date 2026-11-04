//@ pragma ShellId flea-presskeep-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// A press held across a watched re-read of an unchanged listing, or past the press-and-hold interval, must still tap on release, in every view.
ShellRoot {
    id: root
    readonly property var pane: body.currentPane
    readonly property string fixture: Quickshell.env("FLEA_PATH")
    readonly property int fixtureRows: 4
    readonly property int pressedRow: 2
    readonly property int pollMs: 20
    readonly property int stageLimitMs: 5000
    readonly property int pointerEventMs: 1
    // Longer than the 400 ms watchMs burst timer plus a local round trip, so a re-read the press does not hold lands inside the press.
    readonly property int holdMs: 700
    // Past the platform's press-and-hold interval, where a TapHandler's default long press would swallow the release.
    readonly property int longHoldMarginMs: 200
    readonly property int liftSteps: 1
    readonly property int liftStepPx: 40
    readonly property var cases: {
        var out = []
        for (var view of ["list", "grid", "columns"])
            for (var button of [Qt.RightButton, Qt.LeftButton])
                for (var changed of [false, true]) out.push({view: view, button: button, changed: changed})
        for (var longView of ["list", "grid", "columns"])
            for (var longButton of [Qt.RightButton, Qt.LeftButton]) out.push({view: longView, button: longButton, changed: false, long: true})
        // A press that lifts a file drag: the platform drag takes the release, so the pane must clear the hold itself.
        for (var dragView of ["list", "grid", "columns"]) out.push({view: dragView, button: Qt.LeftButton, changed: true, drag: true})
        return out
    }
    property int caseIndex: 0
    property string stage: "view"
    property int checks: 0
    property int failures: 0
    property int beforeLists: 0
    property double started: Date.now()
    property bool executing: false
    property bool finished: false
    property var heldItem: null
    property var dragWatch: null
    property int dragsStarted: 0
    property int dragsFinished: 0
    property int dragsBefore: 0
    property int dragsFinishedBefore: 0
    property point heldPoint: Qt.point(0, 0)
    readonly property var current: cases[caseIndex]

    function label() { return caseIndex < cases.length ? current.view + "/" + (current.button === Qt.RightButton ? "right" : "left") + (current.drag ? "/drag" : current.long ? "/long" : current.changed ? "/changed" : "/quiet") : "end" }
    function check(name, actual, expected) {
        checks += 1
        var equal = actual === expected
        if (!equal) failures += 1
        console.log("PRESSKEEP " + (equal ? "ok " : "FAIL ") + label() + " " + name + ": got " + actual + ", expected " + expected)
    }
    function next(stage) { root.stage = stage; started = Date.now() }
    function finish() {
        finished = true
        console.log("PRESSKEEP DONE checks=" + checks + " failed=" + failures)
        body.quitBackends()
    }
    function advance() {
        if (executing || finished) return
        executing = true
        try { runStage() }
        catch (error) { check("harness error", String(error), "none"); finish() }
        executing = false
    }
    // The FileDrag a row's RowDrag speaks to, found under the delegate in whichever view drew it.
    function sessionIn(item) {
        for (var c = 0; c < item.children.length; c++) {
            var kid = item.children[c]
            if (kid.session !== undefined && kid.listingIndex !== undefined) return kid.session
            var found = sessionIn(kid)
            if (found) return found
        }
        return null
    }
    function settled() {
        return !pane.listInFlight && pane.listingState === "ready" && pane.path === fixture && pane.total === fixtureRows
            && !pane.wire.anchor && !pane.wire.stale && !!pane.visibleItemFor(pressedRow)
    }
    function runStage() {
        if (Date.now() - started > stageLimitMs) {
            check("stage " + stage + " completed", false, true)
            finish()
            return
        }
        if (stage === "view") {
            Flea.ViewState.changeKey("view", current.view)
            next("settle")
        } else if (stage === "settle") {
            body.focusPane(0)
            if (pane.path !== fixture && !pane.listInFlight) { pane.open(fixture); return }
            if (!settled()) return
            pane.contextMenu().close()
            pane.setCursor(0)
            pane.listArea.forceActiveFocus()
            heldItem = pane.visibleItemFor(pressedRow)
            if (current.drag === true) dragWatch = dragWatchComponent.createObject(root, {owner: root, target: sessionIn(heldItem).Drag})
            heldPoint = body.mapFromItem(heldItem, heldItem.width / 2, heldItem.height / 2)
            driver.mousePress(body, heldPoint.x, heldPoint.y, current.button, Qt.NoModifier, pointerEventMs)
            beforeLists = pane.backend.listRequests
            // The unsolicited line a foreign write to the folder sends, which PaneWire pays after its burst timer.
            if (current.changed) pane.backend.changed(pane.path)
            next(current.drag === true ? "lift" : "held")
        } else if (stage === "lift") {
            dragsBefore = dragsStarted
            dragsFinishedBefore = dragsFinished
            // One jump past the DragHandler threshold, so no move follows the drag: the row asks for paths, the reply starts the offscreen platform drag.
            for (var step = 1; step <= liftSteps; step++)
                driver.mouseMove(body, heldPoint.x + step * liftStepPx, heldPoint.y, pointerEventMs, current.button, Qt.NoModifier)
            next("lifted")
        } else if (stage === "lifted") {
            // The offscreen platform drag takes the release itself, so the handler never sees it.
            if (pane.dragActive) driver.mouseRelease(body, heldPoint.x + liftSteps * liftStepPx, heldPoint.y, current.button, Qt.NoModifier, pointerEventMs)
            if (dragsFinished <= dragsFinishedBefore) return
            check("the press became a platform drag", dragsStarted > dragsBefore, true)
            check("the drag's end clears the held button", pane.pointerHeld, false)
            dragWatch.destroy()
            // The offscreen platform sends no release when a drag ends; QtWayland sends one (QWaylandInputDevice::Pointer::releaseButtons), which this stands in for.
            driver.mouseRelease(body, heldPoint.x, heldPoint.y, current.button, Qt.NoModifier, pointerEventMs)
            // The press after a drag is held again and let go again.
            driver.mousePress(body, heldPoint.x, heldPoint.y, current.button, Qt.NoModifier, pointerEventMs)
            check("the press after the drag is held", pane.pointerHeld, true)
            driver.mouseRelease(body, heldPoint.x, heldPoint.y, current.button, Qt.NoModifier, pointerEventMs)
            check("and its release clears it", pane.pointerHeld, false)
            next("paid")
        } else if (stage === "held") {
            if (current.changed && Date.now() - started < holdMs) return
            if (current.long === true && Date.now() - started < Qt.styleHints.mousePressAndHoldInterval + longHoldMarginMs) return
            if (current.changed) check("the pressed delegate lasts the press", pane.visibleItemFor(pressedRow) === heldItem, true)
            if (current.long === true) check("the press is still held past the press-and-hold interval",
                                             Qt.styleHints.mousePressAndHoldInterval > 0 && pane.pointerHeld, true)
            driver.mouseRelease(body, heldPoint.x, heldPoint.y, current.button, Qt.NoModifier, pointerEventMs)
            if (current.button === Qt.RightButton) check("release opens the menu", pane.contextMenu().opened, true)
            else check("release moves the cursor to the pressed row", pane.cursorIndex, pressedRow)
            pane.contextMenu().close()
            next(current.changed ? "paid" : "advance")
        } else if (stage === "paid") {
            if (pane.listInFlight || pane.wire.stale || pane.wire.anchor || pane.backend.listRequests <= beforeLists) return
            check("the re-read is paid after the release", pane.backend.listRequests > beforeLists, true)
            next("advance")
        } else if (stage === "advance") {
            caseIndex += 1
            if (caseIndex === cases.length) finish()
            else next("view")
        }
    }

    // Counts the platform drags of the pressed row's FileDrag, built per drag case because its target is a delegate.
    Component {
        id: dragWatchComponent
        Connections {
            property var owner: null
            function onDragStarted() { owner.dragsStarted += 1 }
            function onDragFinished() { owner.dragsFinished += 1 }
        }
    }

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 800
        Flea.WindowBody { id: body; host: window }
        Item { anchors.fill: parent; TestEvent { id: driver } }
    }
    Timer { interval: root.pollMs; repeat: true; running: !root.finished; onTriggered: root.advance() }
}
