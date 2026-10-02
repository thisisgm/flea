.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/Sort.js" as Sort
.import "sourcefixture.js" as Source

// Execute the shipped handler and activation function under Qt's JS engine. This tests dispatch,
// including the real preset table; it does not simulate native key delivery or a portal round trip.
function run(check) {
    var handler = Source.slice(Source.source("ui/PickerList.qml"), "Keys.onPressed:",
                               "// The listing is a window").replace(/^Keys.onPressed:\s*/, "").trim()
    var activate = Source.slice(Source.source("ui/PickerWindow.qml"), "function activate(index)",
                                "// A file double click").trim()
    var savedPreset = Keymap.preset
    try {
        for (var preset of ["default", "vim", "mac", "windows"]) {
            Keymap.setPreset(preset)
            var row = { n: "file.txt", d: false }
            var win = { cursorIndex: 0, path: "/fixture", marks: ["/fixture/file.txt"],
                accepted: [], opened: [], toggled: [], cancelled: false,
                rowFor: function() { return row },
                accept: function() { this.accepted.push(this.marks.slice()) },
                open: function(path) { this.opened.push(path) },
                toggleMark: function(index) { this.toggled.push(index) },
                cancel: function() { this.cancelled = true } }
            win.activate = new Function("win", "Picker", "return (" + activate + ")")(win, Picker)
            var root = { picker: win, firstArmed: false }
            var press = new Function("root", "Keymap", "Qt", "Sort", "Picker", "return (" + handler + ")")(
                root, Keymap, Qt, Sort, Picker)
            for (var key of [Qt.Key_Return, Qt.Key_Enter]) {
                for (var mods of [Qt.NoModifier, Qt.KeypadModifier]) {
                    row = { n: "file.txt", d: false }
                    var event = { key: key, text: "\r", modifiers: mods, accepted: false }
                    win.accepted = []; win.opened = []
                    root.firstArmed = true
                    press(event)
                    check(preset + " Enter accepts the checked paths", JSON.stringify(win.accepted),
                          JSON.stringify([["/fixture/file.txt"]]))
                    check(preset + " Enter consumes the event", event.accepted, true)
                    check(preset + " Enter disarms the first-row chord", root.firstArmed, false)
                    row = { n: "folder", d: true }
                    win.accepted = []
                    press(event)
                    check(preset + " Enter walks into a directory", win.opened.join(), "/fixture/folder")
                    check(preset + " directory navigation submits nothing", win.accepted.length, 0)
                }
            }
            win.accepted = []; win.opened = []; row = null
            press({ key: Qt.Key_Return, text: "\r", modifiers: Qt.NoModifier })
            check(preset + " no row means no submission", win.accepted.length, 0)
            row = { n: "file.txt", d: false }
            press({ key: Qt.Key_Space, text: " ", modifiers: Qt.NoModifier })
            check(preset + " Space still marks the cursor", win.toggled.join(), "0")
            for (var modifier of [Qt.ControlModifier, Qt.AltModifier, Qt.ShiftModifier, Qt.MetaModifier]) {
                var action = Keymap.lookup(Qt.Key_Return, "\r", modifier, "listing")
                win.accepted = []
                press({ key: Qt.Key_Return, text: "\r", modifiers: modifier })
                check(preset + " modified Enter keeps its binding " + modifier, win.accepted.length,
                      action === "open" || action === "pageForward" ? 1 : 0)
            }
            if (preset === "mac") {
                check("Mac listing Enter remains Rename", Keymap.lookup(Qt.Key_Return, "\r", Qt.NoModifier, "listing"), "rename")
                win.accepted = []
                press({ key: Qt.Key_Down, text: "", modifiers: Qt.ControlModifier })
                check("Mac Ctrl+Down still opens in the picker", win.accepted.length, 1)
            }
            press({ key: Qt.Key_Escape, text: "", modifiers: Qt.NoModifier })
            check(preset + " Escape still cancels", win.cancelled, true)
        }
    } finally { Keymap.setPreset(savedPreset) }
}
