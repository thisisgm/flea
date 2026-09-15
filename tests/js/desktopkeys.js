.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/DesktopKeys.js" as DesktopKeys
.import "tabs.js" as Fixture

function run(check) {
    var ctrl = Qt.ControlModifier, shift = Qt.ShiftModifier, alt = Qt.AltModifier
    var bindings = [
        ["H", ctrl, "toggleHidden"], ["T", ctrl, "tabNew"], ["N", ctrl, "windowNew"],
        ["1", ctrl, "viewList"], ["2", ctrl, "viewGrid"], ["O", ctrl, "open"],
        ["I", ctrl, "properties"], ["Return", alt, "properties"], ["Return", ctrl, "openTab"],
        ["Return", shift, "openWindow"], ["Left", alt, "historyBack"], ["Right", alt, "historyForward"],
        ["Up", alt, "parent"], ["Down", alt, "open"], ["Home", alt, "home"],
        ["X", ctrl, "cut"], ["C", ctrl, "copy"], ["V", ctrl, "paste"], ["Z", ctrl, "undo"],
        ["Z", ctrl | shift, "redo"], ["Delete", 0, "trash"], ["Delete", shift, "deletePermanently"],
        ["F2", 0, "rename"], ["F5", 0, "refresh"], ["R", ctrl, "refresh"], ["A", ctrl, "selectAll"],
        ["D", ctrl, "bookmark"], ["I", ctrl | shift, "invertSelection"], ["Space", ctrl, "toggleSelect"],
        ["N", ctrl | shift, "newFolder"], ["Period", ctrl, "openTerminal"], ["L", ctrl, "pathBar"],
        ["F", ctrl, "searchCurrent"], ["F", ctrl | shift, "searchEverywhere"],
        ["W", ctrl, "closeTabOrWindow"], ["Q", ctrl, "closeWindow"],
        ["PageUp", ctrl, "tabPrevious"], ["PageDown", ctrl, "tabNext"],
        ["Plus", ctrl, "textSizeUp"], ["Minus", ctrl, "textSizeDown"], ["0", ctrl, "textSizeReset"],
        ["Question", ctrl | shift, "keymapSheet"], ["Comma", ctrl, "settings"], ["F10", shift, "menu"]
    ]
    for (var i = 0; i < bindings.length; i++) {
        var b = bindings[i]
        check("Nautilus " + b[0] + " modifiers " + b[1], Keymap.lookupFor("nautilus", Qt["Key_" + b[0]], "", b[1]), b[2])
        check("Nautilus listing shortcut does not consume editor input " + i,
              Keymap.lookupFor("nautilus", Qt["Key_" + b[0]], "", b[1], "editor"), "")
    }
    for (var tab = 1; tab <= 9; tab++)
        check("Nautilus Alt+" + tab, Keymap.lookupFor("nautilus", Qt["Key_" + tab], "", alt), "tab" + tab)
    check("typing d is not a destructive Vim binding", Keymap.lookupFor("nautilus", Qt.Key_D, "d", 0), "")
    check("slash enters an absolute location", Keymap.lookupFor("nautilus", Qt.Key_Slash, "/", 0), "locationRoot")
    check("tilde enters a home location", Keymap.lookupFor("nautilus", Qt.Key_AsciiTilde, "~", shift), "locationHome")
    var p = Fixture.pane("/tmp/current"), closed = 0, bookmarks = 0
    var host = { closeWindow: function () { closed++ }, bookmark: function () { bookmarks++ } }
    p.focusView = "list"; p.shown = [1, 3]; p.shownTotal = 2; p.selection.toggle(1)
    DesktopKeys.act("invertSelection", p, host)
    check("invert selection uses only the filtered visible rows", p.selectedIndices().join(","), "3")
    DesktopKeys.act("searchCurrent", p, host)
    check("Ctrl+F searches the current directory", p.searchHere, true)
    DesktopKeys.act("searchEverywhere", p, host)
    check("Ctrl+Shift+F selects the broader scope", p.searchHere, false)
    p.searchMode = ""
    check("typing starts a current-folder search", DesktopKeys.typeAhead({text:"d", modifiers:0}, p), true)
    check("type-ahead keeps the first character", p.searchQuery, "d")
    check("type-ahead never acts as a shortcut", DesktopKeys.typeAhead({text:"x", modifiers:ctrl}, p), false)
    p.searchMode = ""
    DesktopKeys.act("closeTabOrWindow", p, host)
    check("closing the last tab closes its window", closed, 1)
    p.rowFor = function () { return {n:"child", d:true} }
    p.join = function (base, name) { return base + "/" + name }
    DesktopKeys.act("openTab", p, host)
    check("Ctrl+Enter opens the selected directory", p.path, "/tmp/current/child")
    check("and preserves the original tab", p.tabs.items[0].path, "/tmp/current")
    DesktopKeys.act("closeTabOrWindow", p, host)
    check("closing one of two tabs keeps the window", closed, 1)
    check("and restores the original directory", p.path, "/tmp/current")
    DesktopKeys.act("bookmark", p, host)
    check("bookmark shortcut calls the normal writer", bookmarks, 1)
}
