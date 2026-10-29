import QtQuick
import "Keymap.js" as Keymap

QtObject {
    Component.onCompleted: {
        var failures = 0
        var cases = [
            [Qt.Key_F, "F", Qt.ShiftModifier, "previewPageForward"],
            [Qt.Key_B, "B", Qt.ShiftModifier, "previewPageBack"],
            [Qt.Key_D, "", Qt.ControlModifier, "pageDown"],
            [Qt.Key_U, "", Qt.ControlModifier, "pageUp"],
            [Qt.Key_PageDown, "", 0, "pageDown"],
            [Qt.Key_PageUp, "", 0, "pageUp"]
        ]
        for (var p = 0; p < Keymap.PRESETS.length; p++) {
            var preset = Keymap.PRESETS[p]
            for (var c = 0; c < 2; c++) {
                var context = c === 0 ? "preview" : "pdf"
                for (var i = 0; i < cases.length; i++) {
                    var row = cases[i]
                    var actual = Keymap.lookupFor(preset, row[0], row[1], row[2], context, "gui")
                    if (actual === row[3]) continue
                    console.log("FAIL " + preset + " " + context + " " + row[3] + ": " + actual)
                    failures++
                }
            }
        }
        if (!failures) console.log("DOCUMENT KEYS PASS six paging keys in both contexts and every preset")
        Qt.exit(failures ? 1 : 0)
    }
}
