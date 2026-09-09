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

    // Shift+click hands the range of drawn rows from the anchor to the click, anchor first.
    var a = { path: "/x/a.png", bytes: 10 }
    var b = { path: "/x/b.png", bytes: 20 }
    var c = { path: "/x/c.png", bytes: 30 }
    var range = Marks.markRange([], [a, b], true)
    check("a range marks both rows", Picker.paths(range).join(","), "/x/a.png,/x/b.png")
    check("a range weighs both rows", Picker.totalBytes(range), 30)
    check("a row already marked is not marked twice", Picker.paths(Marks.markRange(range, [b, c], true)).join(","), "/x/a.png,/x/b.png,/x/c.png")
    check("a range never unmarks", Picker.paths(Marks.markRange(range, [a], true)).length, 2)
    check("a range read downwards keeps its order", Picker.paths(Marks.markRange([], [c, b], true)).join(","), "/x/c.png,/x/b.png")
    check("an empty range changes nothing", Marks.markRange(range, [], true), range)
    check("the marks handed in are left alone", Picker.paths(range).length, 2)

    // Single mode: the clicked row alone, toggled the way Space toggles it.
    check("single mode keeps one mark", Picker.paths(Marks.markRange([a], [a, b], false)).join(","), "/x/b.png")
    check("single mode unmarks the clicked row", Picker.paths(Marks.markRange([b], [a, b], false)).length, 0)
}
