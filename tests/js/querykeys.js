.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap
.import "focus-lines.js" as Fixture

function run(check) {
    var saved = Keymap.preset
    for (var preset of Keymap.PRESETS) {
        Keymap.setPreset(preset)
        for (var kind of ["search", "filter"]) {
            for (var chord of [[Qt.Key_V, Qt.ControlModifier], [Qt.Key_V, Qt.MetaModifier], [Qt.Key_Insert, Qt.ShiftModifier]]) {
                var pane = Fixture.queryPane(), pasted = 0, acted = []
                pane.searchMode = kind === "search" ? "typing" : ""
                pane.filterTyping = kind === "filter"
                pane.queryClipboard = { paste: function () { pasted++ } }
                pane.act = function (action) { acted.push(action) }
                Focus.handleKey({key: chord[0], modifiers: chord[1], text: ""}, pane, Fixture.noRail())
                check(preset + " " + kind + " paste reaches the text clipboard " + chord, pasted, 1)
                check(preset + " " + kind + " paste cannot dispatch a file operation " + chord, acted.length, 0)
                check(preset + " paste never starts a subtree walk", pane.walked.length, 0)
            }
        }
    }
    Keymap.setPreset("nautilus")
    var listing = Fixture.queryPane(), actions = []
    listing.queryClipboard = { paste: function () { throw new Error("listing read the text clipboard") } }
    listing.act = function (action) { actions.push(action) }
    Focus.handleKey({key: Qt.Key_V, modifiers: Qt.ControlModifier, text: ""}, listing, Fixture.noRail())
    check("Ctrl+V outside a query keeps normal file paste", actions.join(","), "paste")
    Keymap.setPreset(saved)
}
