import QtQuick
import QtTest
import Quickshell
import qs.Commons
import "flea" as Flea
import "flea/js/Keymap.js" as Keymap

// Drive the real picker and its real backend, with native Qt key events.
ShellRoot {
    id: root
    property var pickerShell: null
    property var win: null
    property var keys: null
    property string scenario: Quickshell.env("FLEA_PICKER_HUNT_CASE")
    property int stage: 0
    property bool timerBusy: false
    property double stamp: Date.now()
    property int failures: 0
    property int checks: 0
    // The picker's path logic, whose backend counts every listing the typed lines ask for.
    property var logic: null
    property int listings: 0
    property int listingsBefore: 0
    // Rows lines that start past row 0, so a typed file that walked its folder window by window shows here.
    property int walked: 0
    readonly property int pollIntervalMs: 20
    readonly property int deadlineMs: 8000
    readonly property int settleWaitMs: 1000

    function check(label, got, want) {
        checks++
        var ok = JSON.stringify(got) === JSON.stringify(want)
        if (!ok) failures++
        console.log("PICKER_HUNT " + (ok ? "PASS " : "FAIL ") + label
            + " got=" + JSON.stringify(got) + " expected=" + JSON.stringify(want))
    }
    function verdict() {
        console.log("PICKER_HUNT DONE " + checks + " checks, " + failures + " failed")
    }
    function finish() {
        if (root.heldBackend && root.heldBackend.running) root.heldBackend.signal(9)
        root.verdict()
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
    function press(key, modifiers) { keys.keyClick(key, modifiers || Qt.NoModifier, -1) }
    // Types through native key events, so the field's own handlers read them.
    function typeText(s) {
        for (var i = 0; i < s.length; i++) {
            var ch = s.charAt(i)
            var code = s.charCodeAt(i)
            if (code >= 97 && code <= 122) press(Qt.Key_A + (code - 97), Qt.NoModifier)
            else if (code >= 65 && code <= 90) press(Qt.Key_A + (code - 65), Qt.ShiftModifier)
            else if (code >= 48 && code <= 57) press(Qt.Key_0 + (code - 48), Qt.NoModifier)
            else if (ch === "/") press(Qt.Key_Slash, Qt.NoModifier)
            else if (ch === ".") press(Qt.Key_Period, Qt.NoModifier)
            else if (ch === "-") press(Qt.Key_Minus, Qt.NoModifier)
            else root.check("typeable path char " + ch, false, true)
        }
    }
    function pathEntry() {
        return root.descendants(win.contentItem).filter(function(item) { return item.pathFrame !== undefined })[0]
    }
    function startFolder() {
        try { return JSON.parse(Quickshell.env("FLEA_PICKER")).folder } catch (e) { return "" }
    }
    function pickerLogic() {
        return root.descendants(win.contentItem).filter(function(item) { return item.leaf !== undefined && item.backend !== undefined })[0]
    }
    function openFields() {
        return root.descendants(win.contentItem).filter(function(item) { return item.activeFocus && typeof item.selectAll === "function" })
    }
    // The field is open, settled, and its Jump's ask has come back through the picker's worker.
    function fieldReady() {
        return Date.now() - root.stamp >= root.settleWaitMs && root.pathEntry().jump !== null && root.pathEntry().jump.answered
            && root.openFields().length === 1
    }
    function typeLine(text) {
        var field = root.openFields()[0]
        field.text = text
        field.cursorPosition = text.length
        root.listingsBefore = root.listings
        root.press(Qt.Key_Return)
    }
    // The topmost item at a window point that draws or takes the pointer, in paint order and past its parent's bounds.
    function topmostAt(item, x, y) {
        if (!item.visible || item.opacity === 0) return null
        var local = item.mapFromItem(null, x, y)
        var inside = local.x >= 0 && local.y >= 0 && local.x < item.width && local.y < item.height
        if (item.clip && !inside) return null
        var kids = Array.prototype.slice.call(item.children || [])
        kids = kids.map(function(kid, at) { return [kid, at] }).sort(function(a, b) { return a[0].z - b[0].z || a[1] - b[1] })
        for (var i = kids.length - 1; i >= 0; i--) { var hit = root.topmostAt(kids[i][0], x, y); if (hit) return hit }
        var paints = (item.color !== undefined && item.color.a > 0) || item.acceptedButtons !== undefined
        return inside && paints ? item : null
    }
    // The first item whose QML type is name, or null; the answer row and the filter are found by type.
    function typed(name) {
        return root.descendants(win.contentItem).filter(function(item) { return String(item).indexOf(name + "_QMLTYPE") === 0 })[0] || null
    }
    function named(name) {
        return root.descendants(win.contentItem).filter(function(item) { return item.objectName === name })[0] || null
    }
    // Children and resources, recursively, the way tests/columnscost.qml counts a subtree.
    function objectsUnder(item) {
        var n = 0
        var stack = [item]
        while (stack.length > 0) {
            var o = stack.pop()
            n += 1
            var kids = (o !== null && o.children !== undefined) ? o.children : []
            for (var i = 0; i < kids.length; i++) stack.push(kids[i])
            var res = (o !== null && o.resources !== undefined) ? o.resources : []
            for (var j = 0; j < res.length; j++) stack.push(res[j])
        }
        return n - 1
    }
    // Cancel or the answer, in the answer row only; the answer is destructive while a collision review stands.
    function answerButtons() {
        var bar = root.typed("PickerAnswers")
        return bar ? root.descendants(bar).filter(function(item) { return item.tabHandle !== undefined && item.label !== undefined }) : []
    }
    function cancelButton() { return root.answerButtons().filter(function(item) { return item.label === "Cancel" })[0] }
    function acceptButton() { return root.answerButtons().filter(function(item) { return item.label !== "Cancel" })[0] }
    function at(item, x, y) { return item.mapToItem(win.contentItem, x, y) }
    function near(a, b) { return Math.abs(a - b) <= 1 }
    function within(item, ancestor) {
        for (var at = item; at; at = at.parent) if (at === ancestor) return true
        return false
    }
    function parentOf(path) {
        var cut = String(path).lastIndexOf("/")
        return cut <= 0 ? "/" : String(path).substring(0, cut)
    }

    // The objects under the window at open on the tree before the 0.3.10 answer row, same request, per view; the row must not grow them.
    readonly property var objectCeiling: ({list: 423, grid: 405})
    readonly property int markedFiles: 3
    property int walkAt: 0
    function titleLine() {
        var line = root.named("pickerTitleLine")
        return line ? line.children.filter(function(item) { return item.visible }) : []
    }
    function rowIndex(name) {
        return win.rows.map(function(row) { return row.n }).indexOf(name) + win.held
    }
    function answerRow() {
        var bar = root.typed("PickerAnswers")
        var accept = root.acceptButton()
        var cancel = root.cancelButton()
        // Cancel and the answer sit right-aligned, 14 from the edge with a gap of 9, whether or not a filter stands at the left.
        root.check("the answer row sits under the listing", root.near(root.at(bar, 0, 0).y, root.at(win.viewItem(), 0, win.viewItem().height).y), true)
        root.check("the answer row stands on the footer", root.near(root.at(bar, 0, bar.height).y + Flea.Theme.chromeHeight, win.contentItem.height), true)
        root.check("the answer sits 14 from the right edge", root.near(root.at(accept, accept.width, 0).x, win.contentItem.width - Flea.Theme.spacing.rowPaddingX), true)
        root.check("Cancel sits one gap left of the answer", root.near(root.at(cancel, cancel.width, 0).x + Flea.Theme.spacing.gap, root.at(accept, 0, 0).x), true)
        root.check("the buttons are the 30 px body control", [cancel.height, accept.height], [30, 30])
    }
    readonly property var focusScenarios: ["fresh-save", "backend-save", "pointer-save", "list-folder", "list-file", "field-cancel", "field-keys"]
    property var heldBackend: null
    property string answerFolder: ""
    function focusStep() {
        var field = root.saveForm().fieldItem
        if (stage === 0) {
            if (!win.saveReady) return
            root.logic = root.pickerLogic()
            root.answerFolder = win.path
            root.listingsBefore = root.listings
            root.check("a fresh save starts with a folder row", win.rowFor(0).n, "big")
            root.check("a fresh save focuses Filename and selects only the stem", [field.activeFocus, field.selectedText, win.viewItem().activeFocus], [true, "report", false])
            root.stage = 40
            root.stamp = Date.now()
            if (scenario === "backend-save") {
                root.heldBackend = root.logic.backend.resources.filter(function(item) { return typeof item.signal === "function" && item.running })[0]
                root.check("the save owns a running checks backend", !!root.heldBackend, true)
                if (!root.heldBackend) { root.finish(); return }
                root.heldBackend.signal(19)
                root.press(Qt.Key_Return)
                root.check("the held save steps focus to Cancel", [win.submitting, root.cancelButton().activeFocus], [true, true])
                root.stage = 43
            } else if (scenario === "pointer-save") {
                var up = root.named("pickerUp")
                root.answerFolder = root.parentOf(win.path)
                keys.mouseClick(up, up.width / 2, up.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                root.stage = 41
            } else if (scenario === "list-folder" || scenario === "list-file") {
                root.press(Qt.Key_Backtab, Qt.ShiftModifier)
                root.check("Shift+Tab deliberately focuses the listing", win.viewItem().activeFocus, true)
                if (scenario === "list-file") win.cursorIndex = root.rowIndex("a.txt")
                else root.stage = 42
                root.expectAnswer = scenario === "list-file"
                root.press(Qt.Key_Return)
            } else if (scenario === "field-cancel") {
                root.expectAnswer = true
                root.press(Qt.Key_Escape)
            } else if (scenario === "field-keys") {
                root.press(Qt.Key_X)
                root.check("typing replaces the selected stem only", win.saveName, "x.pdf")
                root.press(Qt.Key_L, Qt.ControlModifier)
                root.check("an unhandled field key still opens the path bar", root.pathEntry().editing, true)
                root.finish()
            } else {
                root.expectAnswer = true
                root.press(Qt.Key_Return)
            }
            return
        }
        if (stage === 43) {
            root.check("the held backend leaves the save pending", [win.submitting, win.backendUnavailable, root.cancelButton().activeFocus], [true, false, true])
            root.heldBackend.signal(9)
            root.heldBackend = null
            root.stage = 44
            root.stamp = Date.now()
        } else if (stage === 44) {
            if (!win.backendUnavailable || Date.now() - root.stamp < root.settleWaitMs) return
            root.check("backend loss clears the save without answering", [win.submitting, win.answered, root.acceptButton().enabled], [false, false, false])
            root.check("backend loss preserves the enabled Filename draft", [field.enabled, win.saveName], [true, "report.pdf"])
            root.check("backend loss during a save keeps Cancel focused", [root.cancelButton().enabled, root.cancelButton().activeFocus, field.activeFocus], [true, true, false])
            root.finish()
        } else if (stage === 41) {
            if (win.path !== root.answerFolder || win.listingState === "loading" || !win.saveReady) return
            root.check("pointer Up returns focus to Filename", [field.activeFocus, win.viewItem().activeFocus], [true, false])
            root.listingsBefore = root.listings
            root.expectAnswer = true
            root.stage = 40
            root.press(Qt.Key_Return)
        } else if (stage === 42) {
            if (win.path !== root.startFolder() + "/big" || win.listingState !== "ready") return
            root.check("Enter on a deliberately focused folder opens it without answering", [win.answered, win.viewItem().activeFocus], [false, true])
            root.finish()
        } else if (stage === 40 && win.path !== root.answerFolder) {
            root.check("Enter saves in the shown folder without navigation", win.path, root.answerFolder)
            root.finish()
        }
    }

    readonly property var actionScenarios: ["listing-sort", "listing-view", "header-edit"]
    property string switchedView: ""
    function actionFocusStep() {
        var field = root.saveForm().fieldItem
        if (stage === 0) {
            if (!win.saveReady) return
            if (scenario === "header-edit" && win.viewMode === "grid") {
                win.setView("list")
                root.stamp = Date.now()
                return
            }
            root.check("the focus action starts on Filename", [field.activeFocus, win.saveName], [true, "report.pdf"])
            if (scenario === "header-edit") {
                field.forceActiveFocus()
                root.press(Qt.Key_A, Qt.ControlModifier)
                root.typeText("repo")
                root.check("the Filename draft has an end caret and no selection", [field.text, field.cursorPosition, field.selectedText], ["repo", 4, ""])
                var cell = root.typed("PickerHeader").cell("name")
                keys.mouseClick(cell, cell.width / 2, cell.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
            } else {
                root.press(Qt.Key_Backtab, Qt.ShiftModifier)
                root.check("Shift+Tab puts the focus action on the listing", win.viewItem().activeFocus, true)
                if (scenario === "listing-sort") {
                    root.check("the sort starts at Name ascending", [win.sortBy, win.sortDesc], ["name", false])
                    root.press(Qt.Key_S)
                } else {
                    root.switchedView = win.viewMode === "list" ? "grid" : "list"
                    root.press(root.switchedView === "grid" ? Qt.Key_3 : Qt.Key_1, Qt.ControlModifier)
                }
            }
            root.stage = 1
            root.stamp = Date.now()
            return
        }
        if (Date.now() - root.stamp < root.pollIntervalMs * 5) return
        if (scenario === "listing-sort") {
            if (stage === 1) {
                root.check("a listing sort key in a save keeps the listing focused", [win.viewItem().activeFocus, field.activeFocus, win.saveName, win.sortBy, win.sortDesc], [true, false, "report.pdf", "size", false])
                root.press(Qt.Key_S)
                root.stage = 2
                root.stamp = Date.now()
                return
            }
            root.check("a second listing sort key advances again without editing Filename", [win.viewItem().activeFocus, field.activeFocus, win.saveName, win.sortBy, win.sortDesc], [true, false, "report.pdf", "mtime", false])
        } else if (scenario === "listing-view") {
            root.check("a listing view key in a save keeps the listing focused", [win.viewMode, win.viewItem().activeFocus, field.activeFocus, win.saveName], [root.switchedView, true, false, "report.pdf"])
            root.press(win.viewMode === "grid" ? Qt.Key_3 : Qt.Key_1, Qt.ControlModifier)
            root.check("the current view key keeps the listing focused too", [win.viewItem().activeFocus, win.saveName], [true, "report.pdf"])
        } else {
            root.check("the header click reverses Name", [win.sortBy, win.sortDesc], ["name", true])
            root.check("a header sort keeps the Filename edit", [field.activeFocus, field.text, field.cursorPosition, field.selectedText], [true, "repo", 4, ""])
            root.press(Qt.Key_X)
            root.check("typing after a header sort appends", win.saveName, "repox")
            root.press(Qt.Key_3, Qt.ControlModifier)
            root.check("a view switch keeps the Filename caret", [win.viewMode, field.activeFocus, field.cursorPosition, field.selectedText, win.saveName], ["grid", true, 5, "", "repox"])
            field.select(1, 3)
            root.check("the Filename selection is established before switching", [field.selectionStart, field.selectionEnd, field.cursorPosition], [1, 3, 3])
            root.press(Qt.Key_1, Qt.ControlModifier)
            root.check("a view switch keeps the Filename selection", [win.viewMode, field.activeFocus, field.selectionStart, field.selectionEnd, field.cursorPosition], ["list", true, 1, 3, 3])
            root.press(Qt.Key_1, Qt.ControlModifier)
            root.check("the current view key keeps the Filename selection too", [field.activeFocus, field.selectionStart, field.selectionEnd, field.cursorPosition], [true, 1, 3, 3])
        }
        root.finish()
    }

    // The 0.3.10 save area: the column under the listing, the collision review, the marked rows and the Tab walk.
    property bool expectAnswer: false
    readonly property var saveScenarios: ["collision", "replace", "replace-button", "return-twice", "ends", "cancel", "tab", "marked"]
    property int endAt: 0
    property string rootPath: ""
    property real listingBefore: 0
    property int objectsBefore: 0
    property var walkOrder: []
    // The save area and the answer row, where a collision shows its line and its answer; the grid's thumbnails keep arriving elsewhere.
    function saveObjects(form) { return root.objectsUnder(form) + root.objectsUnder(root.typed("PickerAnswers")) }
    function saveForm() { return root.typed("PickerSave") }
    function replaceButton() { return root.answerButtons().filter(function(item) { return item.label === "Replace" })[0] || null }
    function visibleTexts(text) {
        return root.descendants(win.contentItem).filter(function(item) { return item.visible && item.text === text })
    }
    // Focus the field and press Enter on a name that exists: the review starts.
    function startReview(form) {
        form.fieldItem.forceActiveFocus(Qt.TabFocusReason)
        root.press(Qt.Key_Return)
    }
    function saveColumn(form) {
        var label = root.visibleTexts("Filename")[0]
        var frame = form.fieldItem.parent
        root.check("Filename is 9 above the field", root.near(root.at(frame, 0, 0).y - root.at(label, 0, label.height).y, Flea.Theme.spacing.gap), true)
        root.check("the field is the area less 14 each side", [frame.width, root.near(root.at(frame, 0, 0).x, Flea.Theme.spacing.rowPaddingX)], [form.width - 2 * Flea.Theme.spacing.rowPaddingX, true])
        root.check("the field is the control height with the hitMin floor", frame.height, Math.max(Flea.Theme.hitMin, root.acceptButton().height))
        root.check("no Output URI item exists", [form.uriItem === undefined, root.visibleTexts("Output URI").length], [true, 0])
        root.check("a save draws no error line at rest", form.statusItem.visible, false)
        root.check("a save answers Save in the primary role", [root.acceptButton().label, root.acceptButton().destructive], ["Save", false])
        root.check("a save answers under the save area", root.near(root.at(root.typed("PickerAnswers"), 0, 0).y, root.at(form, 0, form.height).y), true)
        // The board draws field, 9, the answer row in one column: no second rule, and 9 from the field to the buttons.
        var answers = root.typed("PickerAnswers")
        var rule = root.descendants(answers).filter(function(item) { return item.visible && item.color !== undefined && item.height === Flea.Theme.spacing.hairline && item.width === answers.width })
        root.check("a save has no rule between the field and the answers", rule.length, 0)
        root.check("a save puts the answers 9 under the field", root.near(root.at(root.cancelButton(), 0, 0).y - root.at(frame, 0, frame.height).y, Flea.Theme.spacing.gap), true)
        root.check("a save answer row is 9, the buttons and 14", root.near(answers.height, Flea.Theme.spacing.gap + root.cancelButton().height + Flea.Theme.spacing.rowPaddingX), true)
    }
    function reviewShown(form) {
        var field = form.fieldItem
        var frame = field.parent
        var line = form.statusItem
        var replace = root.replaceButton()
        root.check("the review keeps the picker open", [win.answered, win.submitting], [false, false])
        root.check("focus returns to Filename", field.activeFocus, true)
        root.check("exactly the stem is selected", [field.selectedText, field.selectionStart, field.selectionEnd, field.cursorPosition], ["a", 0, 1, 1])
        root.check("the error line names the file", line.text.replace(/\u200b/g, ""), "a.txt already exists here.")
        root.check("the error line is the surface error role at caption", [Qt.colorEqual(line.color, Flea.Theme.color.errorOnSurface), line.font.pixelSize], [true, Flea.Theme.font.caption])
        root.check("the error line is 9 under the field", root.near(root.at(line, 0, 0).y - root.at(frame, 0, frame.height).y, Flea.Theme.spacing.gap), true)
        root.check("the listing gave up the error line's row", root.near(root.listingBefore - win.viewItem().height, line.height + Flea.Theme.spacing.gap), true)
        root.check("the answer relabels to Replace in the error ink", replace !== null && [replace.destructive, replace.primary, Qt.colorEqual(replace.ink, Flea.Theme.color.error)], [true, false, true])
        root.check("Cancel stays the secondary answer", [root.cancelButton().label, root.cancelButton().destructive], ["Cancel", false])
        root.check("the answer row holds two buttons", root.answerButtons().length, 2)
        root.check("no second line and no Use this location", root.descendants(win.contentItem).filter(function(item) {
            return typeof item.text === "string" && item.text.indexOf("This confirms") >= 0 || item.label === "Use this location" }).length, 0)
        root.check("the review builds no new item", root.saveObjects(form), root.objectsBefore)
    }
    function saveStep() {
        var form = root.saveForm()
        var tabbing = scenario === "tab"
        if (scenario === "marked") { root.markedStep(); return }
        if (stage === 0) {
            if (!win.saveReady || (!tabbing && !win.saveCollision)) return
            if (tabbing) {
                win.open(win.path + "/sub")
                root.stage = 10
                root.stamp = Date.now()
                return
            }
            if (scenario === "ends") { root.rootPath = win.path; root.stage = 20; return }
            root.saveColumn(form)
            root.listingBefore = win.viewItem().height
            root.objectsBefore = root.saveObjects(form)
            root.startReview(form)
            root.stage = 1
            root.stamp = Date.now()
            return
        }
        if (stage === 1) {
            if (!form.statusItem.visible || Date.now() - root.stamp < root.pollIntervalMs * 5) return
            root.reviewShown(form)
            if (scenario === "collision") {
                root.press(Qt.Key_X)
                root.stage = 2
                root.stamp = Date.now()
                return
            }
            if (scenario === "return-twice") {
                // A held or bounced Return in the field never answers: the review stands and nothing is sent.
                root.press(Qt.Key_Return)
                root.press(Qt.Key_Return)
                root.stage = 5
                root.stamp = Date.now()
                return
            }
            root.expectAnswer = true
            root.stage = 3
            // Only Replace itself overwrites: Space or Enter with focus on it.
            if (scenario === "replace") { root.replaceButton().forceActiveFocus(Qt.TabFocusReason); root.press(Qt.Key_Space) }
            else if (scenario === "replace-button") { root.replaceButton().forceActiveFocus(Qt.TabFocusReason); root.press(Qt.Key_Return) }
            else { root.cancelButton().forceActiveFocus(Qt.TabFocusReason); root.press(Qt.Key_Return) }
            return
        }
        if (stage === 5) {
            // Past the backend's answer to a review, which a leaked second Return would have asked for.
            if (Date.now() - root.stamp < root.settleWaitMs) return
            root.check("a second Return in the field leaves the review standing and unanswered", [win.answered, win.submitting, win.collisionReview, form.statusItem.visible, root.acceptButton().label], [false, false, true, true, "Replace"])
            root.check("Return in the field keeps focus and the stem selected", [form.fieldItem.activeFocus, form.fieldItem.selectedText], [true, "a"])
            root.finish()
            return
        }
        if (stage === 2) {
            // The first poll after the keystroke: the review state is read one frame on, with no settle wait.
            if (win.saveName !== "x.txt") return
            root.check("typing ends the review in one frame", [form.statusItem.visible, form.statusItem.text, root.acceptButton().label, root.acceptButton().destructive], [false, "", "Save", false])
            root.stage = 4
            root.stamp = Date.now()
            return
        }
        if (stage === 4) {
            // The area settles over a few polls, so the geometry is read after them.
            if (Date.now() - root.stamp < root.pollIntervalMs * 5) return
            root.check("typing gives the row back to the listing", win.viewItem().height, root.listingBefore)
            root.finish()
            return
        }
        if (stage === 20 || stage === 21 || stage === 22) { root.endsStep(form); return }
        if (stage === 10) {
            if (win.path.indexOf("/sub") < 0 || win.listingState !== "ready" || !win.saveReady || !root.acceptButton().available) return
            var chrome = root.typed("PickerChrome").focusItems()
            root.walkOrder = chrome.concat([root.typed("PickerPlaces").focusItem, win.viewItem(), form.fieldItem, root.typed("PickerFilter"), root.cancelButton(), root.acceptButton()])
            chrome[0].forceActiveFocus(Qt.TabFocusReason)
            if (!chrome[0].activeFocus) return
            var names = ["Back", "Up", "Rail", "List", "Filename", "Filter", "Cancel", "Answer"]
            // A list or a rail hands the key focus to a row or a content item inside it, so a stop is the item or what holds the focused one.
            function stopName(item) { var at = root.walkOrder.findIndex(function(stop) { return root.within(item, stop) }); return at >= 0 ? names[at] : String(item) }
            var seen = []
            for (var i = 0; i < root.walkOrder.length; i++) {
                root.press(Qt.Key_Tab)
                seen.push(stopName(win.contentItem.Window.window.activeFocusItem))
            }
            root.check("Tab from Back visits Up, the rail, the list, Filename, the filter, Cancel and the answer once each, then Back", seen, names.slice(1).concat(names[0]))
            var back = []
            for (var j = 0; j < root.walkOrder.length; j++) {
                root.press(Qt.Key_Backtab, Qt.ShiftModifier)
                back.push(stopName(win.contentItem.Window.window.activeFocusItem))
            }
            root.check("Shift+Tab walks the same order backwards", back, names.slice().reverse().slice(0, 7).concat(names[0]))
            root.finish()
        }
    }
    // A new folder (walked into), a re-list (the watcher's own openWithoutHistory call) and a new place (Parent) each end a standing review.
    function endsStep(form) {
        if (stage === 20) {
            if (win.path !== root.rootPath || win.listingState !== "ready" || !win.saveReady || !win.saveCollision) return
            root.startReview(form)
            root.stage = 21
            return
        }
        if (stage === 21) {
            if (!win.collisionReview) return
            root.check("the review stands before " + ["a folder", "a re-list", "a new place"][root.endAt], [form.statusItem.visible, root.acceptButton().label], [true, "Replace"])
            if (root.endAt === 0) win.activate(root.rowIndex("sub"))
            else if (root.endAt === 1) win.openWithoutHistory(win.path)
            else win.goUp()
            // clearListing drops the state to loading in the same call, so stage 22 waiting on ready waits for the new listing to land.
            root.check("the " + ["folder", "re-list", "new place"][root.endAt] + " starts a listing", [win.listingState, win.pendingListings], ["loading", 1])
            root.stage = 22
            return
        }
        if (win.listingState !== "ready" || win.path === root.rootPath && root.endAt !== 1) return
        var what = ["a new folder", "a re-list", "a new place"][root.endAt]
        root.check(what + " ends the review", [win.collisionReview, win.reviewAsked, form.statusItem.visible, root.acceptButton().label, root.acceptButton().destructive], [false, false, false, "Save", false])
        root.endAt++
        if (root.endAt === 3) { root.finish(); return }
        if (win.path !== root.rootPath) win.open(root.rootPath)
        root.stage = 20
    }
    // Three files marked by the picker's own toggle: a marked row takes the selection fill and no accent bar.
    function markedStep() {
        if (win.markRequest) return
        if (win.marks.length < root.markedFiles) {
            win.toggleMark(root.rowIndex(["a.txt", "b.txt", "c.txt"][win.marks.length]))
            return
        }
        root.check("the answer reads Open 3", root.acceptButton().label, "Open 3")
        // The rows' fill is the list's own paint; the grid's tiles keep their marks in the count and the answer.
        if (win.viewMode !== "list") { root.check("the grid counts the three marks", win.marks.length, root.markedFiles); root.finish(); return }
        var list = win.viewItem()
        var marked = list.itemAtIndex(root.rowIndex("b.txt"))
        var plain = list.itemAtIndex(root.rowIndex("e.txt"))
        function fills(row, color) {
            return Array.prototype.slice.call(row.children).filter(function(item) { return item.visible && item.color !== undefined && Qt.colorEqual(item.color, color) && item.width === list.width }).length
        }
        function bars(row) {
            return Array.prototype.slice.call(row.children).filter(function(item) { return item.visible && item.color !== undefined && item.width === 2 * Flea.Theme.spacing.hairline && Qt.colorEqual(item.color, Flea.Theme.color.accent) }).length
        }
        root.check("a marked row's fill is the selection fill", fills(marked, Style.selectionFill), 1)
        root.check("a marked row has no accent wash", fills(marked, Style.selectedAccentFill), 0)
        root.check("a marked row draws no 2 px accent bar", bars(marked), 0)
        root.check("an unmarked row draws neither", [fills(plain, Style.selectionFill), bars(plain)], [0, 0])
        root.finish()
    }

    function answersStep() {
        var accept = root.acceptButton()
        var trigger = root.typed("PickerFilter")
        var menu = root.typed("ContextMenu")
        if (stage === 0) {
            root.logic = root.pickerLogic()
            var ask = root.named("pickerAsk")
            var texts = root.titleLine()
            var baseline = texts.map(function(item) { return root.at(item, 0, item.baselineOffset).y })
            // The line's height is the font's: 33 px with the board's face and one pixel more with the CI image's, so the strip is judged by its formula.
            root.check("the title strip is one body line and its padding", ask.height, Math.max(Flea.Theme.chromeHeight, texts[0].implicitHeight + 2 * Flea.Theme.spacing.rowPaddingY))
            root.check("the title strip holds no button", root.descendants(ask).filter(function(item) { return typeof item.activated === "function" }).length, 0)
            root.check("the title line stands on one baseline", baseline.every(function(y) { return root.near(y, baseline[0]) }), true)
            root.check("the title starts at the strip inset", root.near(root.at(texts[0], 0, 0).x, Flea.Theme.spacing.rowPaddingX), true)
            root.answerRow()
            if (scenario === "answers") {
                root.check("a portal caller is named after a middot on the title line",
                    texts.map(function(item) { return item.text }).join("").replace(/\u00a0/g, " "), "All Files \u00b7 Requested by org.chromium.Chromium")
                root.check("the requester is a Text of its own, apart from the caller's title", [texts[0].text, root.named("pickerRequester").text], ["All Files", "Requested by org.chromium.Chromium"])
                // A long identity elides inside the strip's right inset rather than running past it.
                var requester = root.named("pickerRequester")
                requester.text = "Requested by " + "org.example.".repeat(40)
                root.check("a long requester elides", requester.truncated, true)
                root.check("a long requester stays inside the strip's inset", root.at(requester, requester.width, 0).x <= ask.width - Flea.Theme.spacing.rowPaddingX + 1, true)
                root.check("the middot is muted caption and the requester is foreground caption", [Qt.colorEqual(texts[1].color, Flea.Theme.color.muted), Qt.colorEqual(texts[2].color, Flea.Theme.color.foreground), texts[1].font.pixelSize, texts[2].font.pixelSize], [true, true, Flea.Theme.font.caption, Flea.Theme.font.caption])
                root.check("the trigger is the menu's width", trigger.width, Flea.Theme.menuWidth)
                root.check("the trigger is the control height", trigger.height, accept.height)
                root.check("the trigger sits 14 from the left", root.near(root.at(trigger, 0, 0).x, Flea.Theme.spacing.rowPaddingX), true)
                root.check("the trigger names the current filter", root.descendants(trigger).filter(function(item) { return item.text === "Pictures" }).length, 1)
                root.check("no menu object exists before the first open", menu === null, true)
                console.log("PICKER_HUNT objects " + root.objectsUnder(win.contentItem))
                root.check("the picker's object count at open stays within the earlier tree's", root.objectsUnder(win.contentItem) <= root.objectCeiling[win.viewMode], true)
                win.focusView()
                root.stage = 50
                root.stamp = Date.now()
                return
            }
            // No filters: no trigger, no menu, and the buttons keep the places they have with one.
            root.check("a host caller's title is drawn as it came", texts[0].text, "X \u00b7 Firefox")
            root.check("a title that imitates a requester gets no requester text", [texts.length, root.named("pickerRequester").visible, root.named("pickerRequester").text], [1, false, ""])
            root.check("a request with no filters draws no trigger", trigger === null || !trigger.visible, true)
            root.check("the answer area is one row 14 padded with a hairline rule", root.near(root.typed("PickerAnswers").height, 1 + 2 * Flea.Theme.spacing.rowPaddingX + accept.height), true)
            // A folder or an empty listing drops Open to 0.55; a file keeps it live.
            win.cursorIndex = root.rowIndex("sub")
            root.check("Open drops to 0.55 on a folder", [accept.available, Math.abs(accept.opacity - Flea.Theme.disabledOpacity) < 0.001], [false, true])
            win.cursorIndex = root.rowIndex("a.txt")
            root.check("Open is live on a file in a one-file request", [accept.available, accept.opacity, accept.label], [true, 1, "Open"])
            root.finish()
            return
        }
        if (stage === 50) {
            if (!win.viewItem().activeFocus) { if (Date.now() - root.stamp > root.settleWaitMs) root.check("the listing holds focus before Tab", false, true); return }
            root.press(Qt.Key_Tab)
            root.stage = 51
            root.stamp = Date.now()
            return
        }
        if (stage === 51) {
            if (!trigger.activeFocus) { if (Date.now() - root.stamp > root.settleWaitMs) root.check("Tab from the listing reaches the trigger", trigger.activeFocus, true); return }
            root.press(Qt.Key_Space)
            root.stage = 52
            root.stamp = Date.now()
            return
        }
        if (stage === 52) {
            if (!menu || !menu.opened) {
                if (Date.now() - root.stamp > root.settleWaitMs) {
                    root.check("the trigger opens the menu on Space", "no menu", "menu")
                }
                return
            }
            if (Date.now() - root.stamp < root.pollIntervalMs * 10) return
            var frame = menu.frameItem
            root.check("the menu opens flush above the trigger", root.near(root.at(frame, 0, frame.height).y, root.at(trigger, 0, 0).y), true)
            root.check("the menu stands at the trigger's left", root.near(root.at(frame, 0, 0).x, root.at(trigger, 0, 0).x), true)
            root.check("the menu is the menu width", frame.width, Flea.Theme.menuWidth)
            root.check("the menu lists the caller's filters then All files", menu.entries.map(function(entry) { return entry.label }), ["Text", "Pictures", "Data", "All files"])
            root.check("the menu keeps the mark slot empty", menu.entries.every(function(entry) { return entry.mark === "" }), true)
            root.check("the menu rows are the 37 px row", menu.itemFor(0).height, Flea.Theme.rowHeight)
            root.check("the cursor starts on the current filter", menu.cursor, 1)
            root.listingsBefore = root.listings
            root.press(Qt.Key_Down)
            root.press(Qt.Key_Down)
            root.press(Qt.Key_Return)
            root.stage = 53
            root.stamp = Date.now()
            return
        }
        if (stage === 53) {
            if (Date.now() - root.stamp < root.settleWaitMs) return
            root.check("choosing All files sets the filter", win.filterIndex, -1)
            root.check("choosing All files relists once", root.listings - root.listingsBefore, 1)
            root.check("the menu closes on a choice", menu.opened, false)
            root.check("the trigger names All files", root.descendants(trigger).filter(function(item) { return item.text === "All files" }).length, 1)
            root.check("focus returns to the trigger", trigger.activeFocus, true)
            root.press(Qt.Key_Down)
            root.stage = 54
            root.stamp = Date.now()
            return
        }
        if (stage === 54) {
            if (!menu.opened) { if (Date.now() - root.stamp > root.settleWaitMs) root.check("Down opens the menu", "closed", "open"); return }
            root.check("the cursor starts on All files when it is current", menu.cursor, 3)
            // A menu already open when the backend goes away takes no choice, as the chips took none.
            win.backendUnavailable = true
            menu.railChosen("1", "filter")
            win.backendUnavailable = false
            root.check("a choice is refused while the backend is unavailable", win.filterIndex, -1)
            root.press(Qt.Key_Escape)
            root.stage = 55
            root.stamp = Date.now()
            return
        }
        if (stage === 55) {
            if (Date.now() - root.stamp < root.settleWaitMs) return
            root.check("Esc closes the menu only", [menu.opened, win.answered, win.filterIndex], [false, false, -1])
            root.stage = 56
            root.stamp = Date.now()
            return
        }
        if (stage === 56) {
            // One mark is asked at a time, so each file waits for the answer to the one before.
            if (win.markRequest) return
            if (win.marks.length < root.markedFiles) {
                win.toggleMark(root.rowIndex(["a.txt", "b.txt", "c.txt"][win.marks.length]))
                return
            }
            root.check("three marked files make the answer read Open 3", accept.label, "Open 3")
            root.check("the walk starts on the trigger", trigger.activeFocus, true)
            root.stage = 57
            root.stamp = Date.now()
            return
        }
        // Tab runs top to bottom, left to right: Filter, Cancel, Open, round to the chrome (Back is off with no history), the rail, the listing.
        if (stage === 57) {
            var stops = [{label: "Cancel", item: root.cancelButton()}, {label: "Open 3", item: accept}, {label: "Up", item: root.named("pickerUp")},
                {label: "the rail", item: root.typed("PickerPlaces").focusItem}, {label: "the listing", item: win.viewItem()}]
            if (root.walkAt > 0 && !stops[root.walkAt - 1].item.activeFocus) {
                if (Date.now() - root.stamp > root.settleWaitMs) { root.check("Tab reaches " + stops[root.walkAt - 1].label, false, true); root.finish() }
                return
            }
            if (root.walkAt > 0) root.check("Tab reaches " + stops[root.walkAt - 1].label, stops[root.walkAt - 1].item.activeFocus, true)
            if (root.walkAt === stops.length) { root.finish(); return }
            root.press(Qt.Key_Tab)
            root.walkAt++
            root.stamp = Date.now()
        }
    }

    Component.onCompleted: {
        var comp = Qt.createComponent("flea/PickerWindow.qml")
        if (comp.status !== Component.Ready) {
            console.log("PICKER_HUNT FAIL compile " + comp.errorString())
            Qt.exit(1)
            return
        }
        pickerShell = comp.createObject(root)
        win = pickerShell.pickerWin
        keys = Qt.createQmlObject("import QtTest; TestEvent {}", win.contentItem)
    }

    Connections {
        target: root.logic ? root.logic.backend : null
        function onListed() { root.listings++ }
        function onRows(start) { if (start > 0) root.walked++ }
    }

    // The answer writes its reply and then ends this process, so the verdict goes out on the flag, before the write.
    Connections {
        target: root.win
        enabled: root.stage === 9 || root.stage === 26
        function onAnsweredChanged() {
            if (!root.win.answered) return
            root.check("a typed file path answers", root.win.answered, true)
            if (root.stage === 9) root.check("a file lists its folder once", root.listings - root.listingsBefore, 1)
            else root.check("a file before the held rows answers with nothing listed", root.listings - root.listingsBefore, 0)
            root.verdict()
        }
    }

    // A save answer writes its reply and ends this process, so the verdict goes out on the flag; tests/picker-040.sh reads the reply.
    Connections {
        target: root.win
        enabled: root.expectAnswer
        function onAnsweredChanged() {
            if (!root.win.answered) return
            root.check("the picker answered", root.win.answered, true)
            if (root.focusScenarios.indexOf(scenario) >= 0 && scenario !== "field-cancel")
                root.check("Enter saves in the shown folder without navigation", [win.path, root.listings - root.listingsBefore], [root.answerFolder, 0])
            root.verdict()
        }
    }

    Timer {
        interval: root.pollIntervalMs
        running: true
        repeat: true
        onTriggered: {
            if (root.timerBusy) return
            root.timerBusy = true
            try {
            if (Date.now() - root.stamp > root.deadlineMs) {
                var fellows = win ? root.descendants(win.contentItem).filter(function(item) {
                    return item.activeFocus && typeof item.selectAll === "function"
                }) : []
                var lane = fellows.length === 1 ? fellows[0].text : "fields=" + fellows.length
                var note = win ? win.message : "?"
                root.check("probe completes", "timeout stage " + stage + " path=" + (win ? win.path : "?") + " field=" + lane + " msg=" + note, "complete")
                root.finish()
                return
            }
            if (!win || win.listingState !== "ready" || !win.rows.length) return
            if (root.actionScenarios.indexOf(scenario) >= 0) { root.actionFocusStep(); return }
            if (root.focusScenarios.indexOf(scenario) >= 0) { root.focusStep(); return }
            if (scenario === "answers" || scenario === "single") { root.answersStep(); return }
            if (root.saveScenarios.indexOf(scenario) >= 0) { root.saveStep(); return }
            if (stage === 0) {
                if (win.saving && !win.saveReady) return
                var rowIndex = win.rows.findIndex(function(row) { return row.n === "a.txt" })
                if (rowIndex < 0) {
                    root.check("missing row a.txt", rowIndex >= 0, true)
                    root.finish()
                    return
                }
                win.cursorIndex = rowIndex + win.held
                win.focusView()
                if (!win.viewItem().activeFocus) return
                root.check("real cursor is a file", win.rowFor(win.cursorIndex).n, "a.txt")
                if (scenario === "path") {
                    root.check("Ctrl+L maps to pathBar", Keymap.lookup(Qt.Key_L, "l", Qt.ControlModifier, "listing"), "pathBar")
                    root.press(Qt.Key_L, Qt.ControlModifier)
                } else {
                    root.press(Qt.Key_L, Qt.ControlModifier)
                }
                root.stage = scenario === "far" ? 21 : scenario === "probe" ? 31 : 1
                root.stamp = Date.now()
                return
            }
            if (stage === 1 && Date.now() - root.stamp > root.settleWaitMs && !win.markRequest) {
                if (scenario === "path") {
                    var fields = root.descendants(win.contentItem).filter(function(item) {
                        return item.activeFocus && typeof item.selectAll === "function"
                    })
                    root.check("Ctrl+L focuses path input", fields.length, 1)
                    // The field is the one ChromeBar draws: 23 px at base size 14, centred in whatever strip holds it.
                    var entry = root.pathEntry()
                    var frame = entry.pathFrame
                    root.check("the field is the chrome's own height", frame.height, Flea.Theme.chromeHeight - 4 * frame.border.width)
                    var above = entry.y + frame.y
                    var below = entry.parent.height - above - frame.height
                    root.check("the field is centred in the strip", Math.abs(above - below) <= 1, true)
                    // The board's numbers at base size 14: a 27 px strip holding the chrome's 23 px field.
                    root.check("base size 14 draws the chrome at 27 px", Flea.Theme.chromeHeight, 27)
                    root.check("the path strip is the board's 27 px", entry.parent.height, 27)
                    root.check("the entry takes the chrome's slot", [entry.y, entry.height], [0, 26])
                    root.check("the field is the board's 23 px", frame.height, 23)
                    root.check("the field sits 2 px under the strip's top", above, 2)
                    root.logic = root.pickerLogic()
                    // A relative climb typed through the same keys the board counts.
                    root.typeText("..")
                    root.press(Qt.Key_Return)
                }
                root.stage = 2
                root.stamp = Date.now()
                return
            }
            // A typed climb lands in the parent folder with the field closed.
            if (stage === 2 && scenario === "path") {
                var start = root.startFolder()
                if (win.path !== root.parentOf(start) || root.pathEntry().editing) return
                root.check("a typed climb lands in the parent", win.path, root.parentOf(start))
                root.check("a typed climb leaves Back the way home", win.history[win.history.length - 1], start)
                root.press(Qt.Key_L, Qt.ControlModifier)
                root.stage = 3
                root.stamp = Date.now()
                return
            }
            // An absolute path is set through the field's own text, since TestEvent drops Shift for letters; Enter goes by key.
            if (stage === 3 && scenario === "path") {
                if (Date.now() - root.stamp < root.settleWaitMs) return
                // The field is not typed into until the Jump's ask has gone through the picker's worker and come back.
                if (!root.pathEntry().jump.answered) return
                var open = root.openFields()
                if (open.length !== 1) return
                // The dropdown opens flush under the strip: its top is the strip's 27 px, never over the strip's rule.
                var dropdown = root.pathEntry().jump.mapToItem(null, 0, 0).y - root.pathEntry().parent.mapToItem(null, 0, 0).y
                root.check("the dropdown opens under the 27 px strip", dropdown, 27)
                // The fixture's one favourite answers a typed name, so the dropdown shows over the listing.
                open[0].text = "sub"
                root.stage = 41
                root.stamp = Date.now()
                return
            }
            // The dropdown hangs below the chrome, so it must stand over the places, header and listing drawn after it.
            if (stage === 41 && scenario === "path") {
                var jumpEntry = root.pathEntry()
                if (!jumpEntry.jumpShown || !jumpEntry.jump.dropItem) return
                var chromeItem = jumpEntry.parent.parent
                var dropItem = jumpEntry.jump.dropItem
                var spot = dropItem.mapToItem(null, dropItem.width / 2, dropItem.height / 2)
                root.check("the dropdown point lies below the chrome", spot.y > chromeItem.mapToItem(null, 0, chromeItem.height).y, true)
                var top = root.topmostAt(win.contentItem, spot.x, spot.y)
                root.check("the dropdown is drawn over the listing", root.within(top, dropItem) ? "dropdown" : String(top), "dropdown")
                root.check("the chrome rises over the listing while the dropdown shows", chromeItem.z > 0, true)
                root.typeLine(root.startFolder())
                root.stage = 4
                root.stamp = Date.now()
                return
            }
            if (stage === 4 && scenario === "path") {
                var home = root.startFolder()
                if (win.path !== home || root.pathEntry().editing) return
                root.check("a typed folder path navigates", win.path, home)
                root.check("a typed folder lists once", root.listings - root.listingsBefore, 1)
                root.check(": maps to pathBar", Keymap.lookup(Qt.Key_Colon, ":", Qt.NoModifier, "listing"), "pathBar")
                root.press(Qt.Key_Colon, Qt.NoModifier)
                root.stage = 5
                root.stamp = Date.now()
                return
            }
            // A path that names nothing lists nothing: the field stays open over the folder and the status line says why.
            if (stage === 5 && scenario === "path") {
                if (Date.now() - root.stamp < root.settleWaitMs) return
                var typing = root.openFields()
                if (typing.length !== 1) return
                typing[0].text = root.startFolder() + "/nothing-here.txt"
                root.listingsBefore = root.listings
                root.press(Qt.Key_Return)
                root.stage = 6
                root.stamp = Date.now()
                return
            }
            if (stage === 6 && scenario === "path") {
                if (Date.now() - root.stamp < root.settleWaitMs) return
                var again = root.openFields()
                root.check("a missing path keeps the field open", again.length, 1)
                root.check("a missing path lists nothing", root.listings - root.listingsBefore, 0)
                root.check("a missing path stays in its folder", win.path, root.startFolder())
                root.check("a missing path says why", win.message, root.startFolder() + "/nothing-here.txt does not exist or cannot be reached.")
                if (again.length !== 1) { root.finish(); return }
                // The dialog's filter is Text: a png in this very folder is refused with nothing listed.
                again[0].text = root.startFolder() + "/p.png"
                again[0].cursorPosition = again[0].text.length
                root.listingsBefore = root.listings
                root.press(Qt.Key_Return)
                root.stage = 7
                root.stamp = Date.now()
                return
            }
            if (stage === 7 && scenario === "path") {
                if (Date.now() - root.stamp < root.settleWaitMs) return
                var filtered = root.openFields()
                root.check("a filtered file keeps the field open", filtered.length, 1)
                root.check("a filtered file in this folder lists nothing", root.listings - root.listingsBefore, 0)
                root.check("a filtered file says why", win.message, "p.png is hidden by the file filter or the hidden files setting.")
                if (filtered.length !== 1) { root.finish(); return }
                // The same refusal one folder down: its folder lists once, then the filter judges.
                filtered[0].text = root.startFolder() + "/sub/c.png"
                filtered[0].cursorPosition = filtered[0].text.length
                root.listingsBefore = root.listings
                root.press(Qt.Key_Return)
                root.stage = 8
                root.stamp = Date.now()
                return
            }
            if (stage === 8 && scenario === "path") {
                if (Date.now() - root.stamp < root.settleWaitMs || win.path !== root.startFolder() + "/sub" || win.listingState !== "ready") return
                var sibling = root.openFields()
                root.check("a filtered file in a sibling folder keeps the field open", sibling.length, 1)
                root.check("a filtered file lists its folder once", root.listings - root.listingsBefore, 1)
                root.check("a filtered file in a sibling folder says why", win.message, "c.png is hidden by the file filter or the hidden files setting.")
                if (sibling.length !== 1) { root.finish(); return }
                // A file the filter accepts answers the open dialog, Enter by key, one listing away.
                sibling[0].text = root.startFolder() + "/a.txt"
                sibling[0].cursorPosition = sibling[0].text.length
                root.listingsBefore = root.listings
                root.stage = 9
                root.stamp = Date.now()
                root.press(Qt.Key_Return)
                return
            }
            // A file the filter refuses, in a folder of several windows: located by name, with no window walked and the view kept at its top.
            if (stage === 21) {
                if (!root.fieldReady()) return
                root.logic = root.pickerLogic()
                root.typeLine(root.startFolder() + "/big/zz.png")
                root.stage = 22
                root.stamp = Date.now()
                return
            }
            if (stage === 22) {
                if (Date.now() - root.stamp < root.settleWaitMs || win.path !== root.startFolder() + "/big" || win.listingState !== "ready") return
                root.check("the big folder holds more rows than one window", win.total > win.windowSize, true)
                root.check("a filtered file in a big folder lists its folder once", root.listings - root.listingsBefore, 1)
                root.check("a filtered file in a big folder says why", win.message, "zz.png is hidden by the file filter or the hidden files setting.")
                root.check("a filtered file walks no window", root.walked, 0)
                root.check("a filtered file leaves the view at its top", win.held, 0)
                root.press(Qt.Key_Escape)
                root.stage = 23
                root.stamp = Date.now()
                return
            }
            // End takes the held window to the tail, past the first row the next typed line names.
            if (stage === 23) {
                if (root.pathEntry().editing || !win.viewItem().activeFocus) return
                root.press(Qt.Key_End)
                root.stage = 24
                root.stamp = Date.now()
                return
            }
            if (stage === 24) {
                if (win.cursorIndex !== win.total - 1 || !win.rowFor(win.total - 1) || win.held === 0) return
                root.check("End holds the tail and not the first row", win.rowFor(0), null)
                root.press(Qt.Key_L, Qt.ControlModifier)
                root.stage = 25
                root.stamp = Date.now()
                return
            }
            if (stage === 25) {
                if (!root.fieldReady()) return
                root.typeLine(root.startFolder() + "/big/f000.txt")
                root.stage = 26
                root.stamp = Date.now()
                return
            }
            // A save dialog refuses a typed file through the backend's folder-type mark, and says the backend's sentence.
            if (stage === 31) {
                if (!root.fieldReady()) return
                root.logic = root.pickerLogic()
                root.typeLine(root.startFolder() + "/b.txt")
                root.stage = 32
                root.stamp = Date.now()
                return
            }
            if (stage === 32) {
                if (Date.now() - root.stamp < root.settleWaitMs) return
                root.check("a typed file in a save dialog says the backend's refusal", win.message, "This item is not the requested file type.")
                root.check("a refused file answers nothing", win.answered, false)
                root.check("the refusal ends the probe and leaves no mark request", [win.pathProbe, win.markRequest, win.acceptMarks], [0, 0, false])
                root.check("a refused file keeps the field open", root.openFields().length, 1)
                root.check("a refused file lists nothing", root.listings - root.listingsBefore, 0)
                root.finish()
            }
            } finally {
                root.timerBusy = false
            }
        }
    }
}
