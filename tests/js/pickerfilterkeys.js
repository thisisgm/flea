.import "../../ui/js/PickerKeys.js" as PickerKeys
.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/Filter.js" as Filter
.import "pickerkeys.js" as Keys

// The filter's keys through ui/js/PickerKeys.js handle: "/" opens the query line, the line owns
// every key while it has the caret, Enter and the cursor keys hand the keyboard back with the
// filter standing, and Escape unwinds it before the dialog. The stubs are tests/js/pickerkeys.js's.
function run(check) {
    var shift = Qt.ShiftModifier
    var press = Keys.press
    var stubState = Keys.stubState
    var stubOps = Keys.stubOps
    // ui/PickerState.qml's listing surface, with shown computed the way the state binds it: the
    // chip's rows met with the query's. A mark is a path, so the marks stand whatever is typed.
    var listing = [{ n: "docs", d: true, s: 0, i: "folder" }, { n: "shots", d: true, s: 0, i: "folder" },
                   { n: "a.png", d: false, s: 1, i: "image-x-generic" }, { n: "b.md", d: false, s: 2, i: "text-x-generic" },
                   { n: "cat.png", d: false, s: 3, i: "image-x-generic" }, { n: "dog.txt", d: false, s: 4, i: "text-x-generic" }]
    function filterState(chip) {
        var s = stubState({ rows: listing, held: 0, total: listing.length, cursorIndex: 0, shown: null,
                            marks: [{ path: "/f/b.md", bytes: 2 }], scrolled: [], seated: [] })
        s.selectedIndices = function () { return s.filterTyping ? [] : [3] }
        s.showRow = function (view) { s.scrolled.push(view) }
        s.setCursor = function (index) { s.cursorIndex = index; s.seated.push(index) }
        Object.defineProperty(s, "shown", { get: function () {
            return Picker.narrow(Picker.shownRows(s.rows, s.held, chip || null), Filter.shown(s.rows, s.held, s.filterQuery))
        } })
        Object.defineProperty(s, "shownTotal", { get: function () { return s.shown === null ? s.total : s.shown.length } })
        return s
    }
    var typing = filterState()
    var typeOps = stubOps()
    check("slash opens the query line", PickerKeys.handle(press(Qt.Key_Slash, "/"), typing, typeOps), "filter")
    check("and the line has the caret", typing.filterTyping, true)
    check("with nothing typed yet nothing narrows", typing.shown, null)
    var typedKey = press(Qt.Key_O, "o")
    check("a character is typed into the line, not looked up", PickerKeys.handle(typedKey, typing, typeOps), "filter")
    check("and the event is taken", typedKey.accepted, true)
    check("the query narrows the rows on the keystroke", typing.filterQuery + "|" + typing.shown.join(","), "o|0,1,5")
    check("and the count follows", typing.shownTotal, 3)
    check("a cursor still standing does not move the view", typing.cursorIndex + "|" + typing.scrolled.length, "0|0")
    PickerKeys.handle(press(Qt.Key_G, "g"), typing, typeOps)
    check("a second character narrows again", typing.shown.join(","), "5")
    check("the cursor the query hid moved to the first row standing", typing.cursorIndex, 5)
    check("and the view dropped back to its first row", typing.scrolled.join(","), "0")
    check("the marks stand through the typing", typing.marks.length, 1)
    check("and no state verb ran", typing.calls.length, 0)
    var jKey = press(Qt.Key_J, "j")
    PickerKeys.handle(jKey, typing, typeOps)
    check("j is typed into the line and never moves the cursor", typing.filterQuery + "|" + typeOps.moved.length, "ogj|0")
    PickerKeys.handle(press(Qt.Key_Backspace), typing, typeOps)
    check("backspace shortens the query instead of climbing", typing.filterQuery + "|" + typing.calls.length, "og|0")
    check("a colon is typed, not the field", PickerKeys.handle(press(Qt.Key_Colon, ":", shift), typing, typeOps) + "|" + typeOps.focused, "filter|0")
    PickerKeys.handle(press(Qt.Key_Backspace), typing, typeOps)
    var tab = press(Qt.Key_Tab)
    PickerKeys.handle(tab, typing, typeOps)
    check("tab is swallowed by the line", tab.accepted + "|" + typing.focusView, "true|undefined")
    var space = press(Qt.Key_Space, " ")
    PickerKeys.handle(space, typing, typeOps)
    check("space is a character in the line, not a mark", typing.filterQuery + "|" + typing.calls.length, "og |0")
    PickerKeys.handle(press(Qt.Key_Backspace), typing, typeOps)
    // Enter hands the keyboard back with the filter standing; the narrowed rows stay narrowed.
    check("return commits the line", PickerKeys.handle(press(Qt.Key_Return), typing, typeOps), "filter")
    check("the caret is gone and the filter stands", typing.filterTyping + "|" + typing.filterQuery, "false|og")
    check("and the rows stay narrowed", typing.shown.join(","), "5")
    check("no activate ran on the commit", typing.calls.length, 0)
    check("space now marks the cursor row", PickerKeys.handle(press(Qt.Key_Space, " "), typing, typeOps) + "|" + typing.calls.join(","), "mark|mark 5")
    // Escape unwinds the standing filter first, and only a second Escape refuses the dialog.
    check("escape clears a standing filter", PickerKeys.handle(press(Qt.Key_Escape), typing, typeOps), "filterClose")
    check("the query is gone and nothing narrows", typing.filterQuery + "|" + typing.shown, "|null")
    check("the dialog still stands", typing.calls.indexOf("cancel"), -1)
    check("a second escape refuses the dialog", PickerKeys.handle(press(Qt.Key_Escape), typing, typeOps), "cancel")
    check("a fetch in flight still takes escape before a standing filter",
          PickerKeys.lookup(press(Qt.Key_Escape), stubState({ fetching: true, filterQuery: "x" })), "stopFetch")

    // A cursor key commits the line the way Enter does and then moves, ui/js/Focus.js's LEAVES_LINE.
    var leaving = filterState()
    var leaveOps = stubOps()
    PickerKeys.handle(press(Qt.Key_Slash, "/"), leaving, leaveOps)
    PickerKeys.handle(press(Qt.Key_P, "p"), leaving, leaveOps)
    check("down commits and moves", PickerKeys.handle(press(Qt.Key_Down), leaving, leaveOps) + "|" + leaving.filterTyping + "|" + leaveOps.moved.join(","), "cursorDown|false|1")
    check("the filter is left standing over the narrowed rows", leaving.shown.join(","), "2,4")
    PickerKeys.handle(press(Qt.Key_Slash, "/"), leaving, leaveOps)
    check("slash reopens the line over the standing query", leaving.filterTyping + "|" + leaving.filterQuery, "true|p")
    PickerKeys.handle(press(Qt.Key_End), leaving, leaveOps)
    check("end commits and goes last", leaving.filterTyping + "|" + leaveOps.moved.join(","), "false|1,2")
    PickerKeys.handle(press(Qt.Key_Slash, "/"), leaving, leaveOps)
    var escaped = press(Qt.Key_Escape)
    check("escape while typing closes the line", PickerKeys.handle(escaped, leaving, leaveOps) + "|" + leaving.filterQuery + "|" + leaving.filterTyping, "filter||false")
    check("and the dialog stands", leaving.calls.length, 0)
    PickerKeys.handle(press(Qt.Key_Slash, "/"), leaving, leaveOps)
    PickerKeys.handle(press(Qt.Key_Return), leaving, leaveOps)
    check("return on an empty line closes it rather than standing nothing", leaving.filterTyping + "|" + leaving.calls.length, "false|0")

    // The chip and the query meet: a row the query keeps but the chip hides is not shown, and the
    // cursor lands on the first row both leave standing rather than on the query's own first match.
    var chipped = filterState({ label: "Images", globs: ["*.png"], mimes: [] })
    var chipOps = stubOps()
    chipped.cursorIndex = 3
    check("the chip alone keeps the directories and its rows", chipped.shown.join(","), "0,1,2,4")
    PickerKeys.handle(press(Qt.Key_Slash, "/"), chipped, chipOps)
    PickerKeys.handle(press(Qt.Key_D, "d"), chipped, chipOps)
    check("the query's own matches would be docs, b.md and dog.txt", Filter.shown(listing, 0, "d").join(","), "0,3,5")
    check("but the chip hides both files, so only docs stands", chipped.shown.join(","), "0")
    check("and the cursor is seated on it", chipped.cursorIndex + "|" + chipped.seated.join(","), "0|0")
}
