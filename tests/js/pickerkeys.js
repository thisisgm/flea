.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/Sort.js" as Sort
.import "../../ui/js/Filter.js" as Filter
.import "sourcefixture.js" as Source

// Runs the shipped list handler, grid dispatch and activation under Qt's JS engine, never native key delivery or a portal round trip.
function run(check) {
    var handler = Source.slice(Source.source("ui/PickerList.qml"), "Keys.onPressed:",
                               "// The listing is a window").replace(/^Keys.onPressed:\s*/, "").trim()
    var gridSource = Source.source("ui/PickerGrid.qml")
    var dispatch = Source.slice(gridSource, "function handleAction(action, key, modifiers)",
                                "Keys.onPressed:").trim()
    var gridHandler = Source.slice(gridSource, "Keys.onPressed:", "// A scroll refetches")
        .replace(/^Keys.onPressed:\s*/, "").trim()
    var windowSource = Source.source("ui/PickerWindow.qml")
    var activate = Source.slice(windowSource, "function activate(index)",
                                "// File double clicks").trim()
    var savedPreset = Keymap.preset
    try {
        for (var preset of ["default", "vim", "mac", "windows"]) for (var view of ["list", "grid"]) {
            Keymap.setPreset(preset)
            var label = preset + " " + view
            var row = { n: "file.txt", d: false }
            var win = { cursorIndex: 0, path: "/fixture", marks: ["/fixture/file.txt"], shown: null,
                accepted: [], opened: [], toggled: [], cancelled: false, filterOpened: false,
                rowFor: function(index) { return index === 0 ? row : null },
                accept: function() { this.accepted.push(this.marks.slice()) },
                open: function(path) { this.opened.push(path) },
                toggleMark: function(index) { this.toggled.push(index) },
                cancel: function() { this.cancelled = true },
                openFilter: function() { this.filterOpened = true },
                escapeFilter: function() { return false } }
            win.activate = new Function("win", "Picker", "Filter", "return (" + activate + ")")(win, Picker, Filter)
            var root = { picker: win, firstArmed: false }
            var press = view === "list"
                ? new Function("root", "Keymap", "Qt", "Sort", "Picker", "Filter", "return (" + handler + ")")(root, Keymap, Qt, Sort, Picker, Filter)
                : gridPress(dispatch, gridHandler, root)
            for (var key of [Qt.Key_Return, Qt.Key_Enter]) {
                for (var mods of [Qt.NoModifier, Qt.KeypadModifier]) {
                    row = { n: "file.txt", d: false }
                    var event = { key: key, text: "\r", modifiers: mods, accepted: false }
                    win.accepted = []; win.opened = []
                    root.firstArmed = true
                    press(event)
                    check(label + " Enter accepts the checked paths", JSON.stringify(win.accepted),
                          JSON.stringify([["/fixture/file.txt"]]))
                    check(label + " Enter consumes the event", event.accepted, true)
                    check(label + " Enter disarms the first-row chord", root.firstArmed, false)
                    row = { n: "folder", d: true }
                    win.accepted = []
                    press(event)
                    check(label + " Enter walks into a directory", win.opened.join(), "/fixture/folder")
                    check(label + " directory navigation submits nothing", win.accepted.length, 0)
                }
            }
            win.accepted = []; win.opened = []; row = null
            press({ key: Qt.Key_Return, text: "\r", modifiers: Qt.NoModifier })
            check(label + " no row means no submission", win.accepted.length, 0)
            row = { n: "file.txt", d: false }
            press({ key: Qt.Key_Space, text: " ", modifiers: Qt.NoModifier })
            check(label + " Space still marks the cursor", win.toggled.join(), "0")
            for (var modifier of [Qt.ControlModifier, Qt.AltModifier, Qt.ShiftModifier, Qt.MetaModifier]) {
                var action = Keymap.lookup(Qt.Key_Return, "\r", modifier, "listing")
                win.accepted = []
                press({ key: Qt.Key_Return, text: "\r", modifiers: modifier })
                check(label + " modified Enter keeps its binding " + modifier, win.accepted.length,
                      action === "open" || action === "pageForward" ? 1 : 0)
            }
            if (preset === "mac") {
                check("Mac listing Enter remains Rename", Keymap.lookup(Qt.Key_Return, "\r", Qt.NoModifier, "listing"), "rename")
                win.accepted = []
                press({ key: Qt.Key_Down, text: "", modifiers: Qt.ControlModifier })
                check("Mac Ctrl+Down still opens in the picker", win.accepted.length, 1)
            }
            win.accepted = []
            var slash = { key: Qt.Key_Slash, text: "/", modifiers: Qt.NoModifier, accepted: false }
            check(label + " slash resolves to filter", Keymap.lookup(slash.key, slash.text, slash.modifiers, "listing"), "filter")
            press(slash)
            check(label + " slash dispatch opens the filename filter", win.filterOpened, true)
            check(label + " slash consumes the key", slash.accepted, true)
            check(label + " slash never activates the cursor file", win.accepted.length, 0)
            press({ key: Qt.Key_Escape, text: "", modifiers: Qt.NoModifier })
            check(label + " Escape still cancels", win.cancelled, true)
        }
    } finally { Keymap.setPreset(savedPreset) }
}

// Both view handlers are extracted, so their filename-typing gate is exercised before action dispatch.
function gridPress(dispatch, handler, root) {
    root.handleAction = new Function("root", "Picker", "Sort", "Qt", "Filter", "return (" + dispatch + ")")(root, Picker, Sort, Qt, Filter)
    return new Function("root", "Keymap", "Qt", "return (" + handler + ")")(root, Keymap, Qt)
}
