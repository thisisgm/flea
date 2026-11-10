.import "../../ui/js/Keymap.js" as Keymap

function run(check) {
    var shift = Qt.ShiftModifier
    var ctrlShift = Qt.ControlModifier | shift
    var metaShift = Qt.MetaModifier | shift
    for (var i = 0; i < Keymap.PRESETS.length; i++) {
        var preset = Keymap.PRESETS[i]
        check(preset + " Shift+Left maps to horizontal extension",
              Keymap.lookupFor(preset, Qt.Key_Left, "", shift), "extendLeft")
        check(preset + " Shift+Right maps to horizontal extension",
              Keymap.lookupFor(preset, Qt.Key_Right, "", shift), "extendRight")
        check(preset + " Ctrl+Shift+Left remains unbound",
              Keymap.lookupFor(preset, Qt.Key_Left, "", ctrlShift), "")
        check(preset + " Ctrl+Shift+Right remains unbound",
              Keymap.lookupFor(preset, Qt.Key_Right, "", ctrlShift), "")
        check(preset + " Meta+Shift+Left remains unbound",
              Keymap.lookupFor(preset, Qt.Key_Left, "", metaShift), "")
        check(preset + " Meta+Shift+Right remains unbound",
              Keymap.lookupFor(preset, Qt.Key_Right, "", metaShift), "")
        check(preset + " horizontal extension is unavailable to TUI",
              Keymap.lookupFor(preset, Qt.Key_Left, "", shift, "listing", "tui"), "")
        check(preset + " right extension is unavailable to TUI",
              Keymap.lookupFor(preset, Qt.Key_Right, "", shift, "listing", "tui"), "")
    }
}
