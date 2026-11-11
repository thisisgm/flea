import QtQuick
import "Keymap.js" as Keymap

QtObject {
    Component.onCompleted: {
        var failures = 0
        function check(actual, expected, message) {
            if (actual === expected) return
            console.log("FAIL " + message + ": " + actual + " != " + expected)
            failures++
        }
        for (var p = 0; p < Keymap.PRESETS.length; p++) {
            var preset = Keymap.PRESETS[p]
            for (var f = 0; f < 2; f++) {
                var frontend = f === 0 ? "gui" : "tui"
                var rows = Keymap.sheetFor(preset, frontend)
                function row(action) {
                    return rows.filter(function (candidate) { return candidate.action === action })[0]
                }
                check(row("textSizeUp").keys, "ctrl-shift-plus", preset + " " + frontend + " larger")
                check(row("textSizeDown").keys, "ctrl-shift-minus", preset + " " + frontend + " smaller")
                check(row("textSizeReset").keys, "ctrl-shift-0", preset + " " + frontend + " reset")
                if (frontend === "gui") {
                    check(row("viewDual").keys.split(" / ").indexOf("f3") >= 0, true, preset + " dual F3 alias")
                    check(Keymap.lookupFor(preset, Qt.Key_F3, "", Qt.NoModifier, "listing", frontend), "viewDual", preset + " F3 listing")
                    check(Keymap.lookupFor(preset, Qt.Key_F3, "", Qt.NoModifier, "editor", frontend), "", preset + " F3 editor")
                    check(row("carry").keys, "X", preset + " carry key")
                    check(row("carry").label, "Carry to another app", preset + " carry label")
                    check(Keymap.SHEET_GROUPS.move.indexOf("carry") >= 0, true, "carry stays in MOVE")
                    check(Keymap.SHEET_GROUPS.change.indexOf("carry"), -1, "carry is not in CHANGE")
                } else {
                    check(Keymap.lookupFor(preset, Qt.Key_F3, "", Qt.NoModifier, "listing", frontend), "", preset + " terminal F3")
                    check(row("carry"), undefined, "terminal has no carry action")
                }
                for (var i = 0; i < Keymap.SHARED_KEYS.length; i++) {
                    var binding = Keymap.SHARED_KEYS[i]
                    if (["textSizeUp", "textSizeDown", "textSizeReset"].indexOf(binding.action) < 0) continue
                    check(Keymap.lookupFor(preset, binding.keycode, binding.text, binding.mask, "listing", frontend),
                          binding.action, preset + " alias " + binding.keys)
                }
            }
        }
        if (!failures) console.log("KEYSHEET PASS sheet chords, carry label/group and scale aliases in every preset")
        Qt.exit(failures ? 1 : 0)
    }
}
