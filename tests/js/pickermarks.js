.import "../../ui/js/PickerMarks.js" as Marks
.import "../../ui/js/Picker.js" as Picker

function run(check) {
    var marks = Marks.toggle([], "/x/a.png", 10, true)
    marks = Marks.toggle(marks, "/x/b.png", 20, true)
    check("multiple keeps both marks", Picker.paths(marks).join(","), "/x/a.png,/x/b.png")
    check("both marks are weighed", Picker.totalBytes(marks), 30)
    check("a mark is found by its path", Marks.marked(marks, "/x/b.png"), true)
    marks = Marks.toggle(marks, "/x/a.png", 10, true)
    check("a second space unmarks", Picker.paths(marks).join(","), "/x/b.png")
    var single = Marks.toggle(Marks.toggle([], "/x/a.png", 10, false), "/x/b.png", 20, false)
    check("single mode replaces the prior check", Picker.paths(single).join(","), "/x/b.png")
    check("single mode unmarks its own", Picker.paths(Marks.toggle(single, "/x/b.png", 20, false)).length, 0)

    // A plain click is the row's check box, so it is toggle: it adds in multiple mode, replaces in
    // single mode, and takes the mark off a marked row in both.
    var one = Marks.toggle([], "/x/a.png", 10, false)
    check("click on an unmarked row replaces the mark in single mode", Picker.paths(Marks.toggle(one, "/x/b.png", 20, false)).join(","), "/x/b.png")
    check("click on the marked row removes it in single mode", Picker.paths(Marks.toggle(one, "/x/a.png", 10, false)).length, 0)
    var many = Marks.toggle(Marks.toggle([], "/x/a.png", 10, true), "/x/b.png", 20, true)
    check("click on an unmarked row adds in multiple mode", Picker.paths(many).join(","), "/x/a.png,/x/b.png")
    check("click on a marked row removes it in multiple mode", Picker.paths(Marks.toggle(many, "/x/a.png", 10, true)).join(","), "/x/b.png")

    // The second tap of a folder double click takes the mark off, whatever the first tap did.
    check("unmark takes a standing mark off", Picker.paths(Marks.unmark(many, "/x/a.png")).join(","), "/x/b.png")
    check("unmark leaves an unmarked path alone", Picker.paths(Marks.unmark(many, "/x/c.png")).join(","), "/x/a.png,/x/b.png")
    var folder = Marks.toggle(Marks.toggle([], "/x/sub", 0, false), "/x/sub", 0, false)
    check("a folder double tap ends with no mark on it", Picker.paths(Marks.unmark(folder, "/x/sub")).length, 0)
    check("a folder double tap on a marked folder ends with no mark on it",
          Picker.paths(Marks.unmark(Marks.toggle([{ path: "/x/sub", bytes: 0 }], "/x/sub", 0, false), "/x/sub")).length, 0)

    // Return prefers explicit marks. With none, a file under the cursor is the one answer.
    var cursorFile = { n: "c.png", d: false }
    check("marks win over the cursor fallback",
          Marks.answer(many, "/x", cursorFile).join(","), "/x/a.png,/x/b.png")
    check("an unmarked cursor file is the answer",
          Marks.answer([], "/x", cursorFile).join(","), "/x/c.png")
    check("an explicit Recent mark is still the answer",
          Marks.answer([{ path: "/home/gm/c.png", bytes: 1 }], Picker.RECENT,
                       { n: "home/gm/c.png", d: false }).join(","), "/home/gm/c.png")
    check("an unmarked Recent cursor file is not an answer",
          Marks.answer([], Picker.RECENT, { n: "home/gm/c.png", d: false }).length, 0)
    check("a cursor directory is not an answer", Marks.answer([], "/x", { n: "sub", d: true }).length, 0)
    check("no cursor row is not an answer", Marks.answer([], "/x", null).length, 0)

    // Ctrl+A hands every drawn row; a row already marked stays, and nothing comes off.
    var a = { path: "/x/a.png", bytes: 10 }
    var b = { path: "/x/b.png", bytes: 20 }
    var c = { path: "/x/c.png", bytes: 30 }
    var range = Marks.markRange([], [a, b])
    check("a range marks both rows", Picker.paths(range).join(","), "/x/a.png,/x/b.png")
    check("a range weighs both rows", Picker.totalBytes(range), 30)
    check("a row already marked is not marked twice", Picker.paths(Marks.markRange(range, [b, c])).join(","), "/x/a.png,/x/b.png,/x/c.png")
    check("markRange never unmarks", Picker.paths(Marks.markRange(range, [a])).length, 2)
    check("a range read downwards keeps its order", Picker.paths(Marks.markRange([], [c, b])).join(","), "/x/c.png,/x/b.png")
    check("the marks handed in are left alone", Picker.paths(range).length, 2)

    // Shift+click: a mixed range marks every row, a range all marked already is cleared.
    check("shift over a mixed range marks all of it", Picker.paths(Marks.toggleRange(range, [b, c], true)).join(","), "/x/a.png,/x/b.png,/x/c.png")
    check("shift over an unmarked range marks it", Picker.paths(Marks.toggleRange([], [c, b], true)).join(","), "/x/c.png,/x/b.png")
    check("shift over an all-marked range clears it", Picker.paths(Marks.toggleRange(range, [a, b], true)).length, 0)
    check("shift clears only the range", Picker.paths(Marks.toggleRange(Marks.markRange(range, [c]), [a, b], true)).join(","), "/x/c.png")
    check("an empty range changes nothing", Marks.toggleRange(range, [], true), range)
    check("a shift toggle leaves the marks handed in alone", Picker.paths(range).length, 2)

    // Single mode: the clicked row alone, toggled the way Space toggles it.
    check("single mode keeps one mark", Picker.paths(Marks.toggleRange([a], [a, b], false)).join(","), "/x/b.png")
    check("single mode unmarks the clicked row", Picker.paths(Marks.toggleRange([b], [a, b], false)).length, 0)
}
