.import "../../ui/js/CutMarks.js" as CutMarks

// A cut row draws the scissors until its paste lands; a copied one draws nothing, because it stays.

function pane(path, clipboard) {
    return { path: path, clipboard: clipboard, join: function (base, name) { return base === "/" ? "/" + name : base + "/" + name } }
}

function run(check) {
    var cut = { paths: ["/home/gm/a.txt", "/home/gm/sub"], moving: true }
    check("a cut file is marked", CutMarks.rowIsCut(pane("/home/gm", cut), { n: "a.txt" }), true)
    check("and so is a cut folder", CutMarks.rowIsCut(pane("/home/gm", cut), { n: "sub", d: true }), true)
    check("a row the cut did not take is not", CutMarks.rowIsCut(pane("/home/gm", cut), { n: "b.txt" }), false)
    check("the same name in another directory is not", CutMarks.rowIsCut(pane("/tmp", cut), { n: "a.txt" }), false)
    check("a search result resolves against the search root",
          CutMarks.rowIsCut(pane("/home", cut), { n: "gm/a.txt" }), true)
    check("a row at the root joins without a double slash",
          CutMarks.rowIsCut(pane("/", { paths: ["/etc"], moving: true }), { n: "etc" }), true)
    check("a copy marks nothing", CutMarks.rowIsCut(pane("/home/gm", { paths: ["/home/gm/a.txt"], moving: false }), { n: "a.txt" }), false)
    check("an empty clipboard marks nothing", CutMarks.rowIsCut(pane("/home/gm", { paths: [], moving: false }), { n: "a.txt" }), false)
    check("a row that has not loaded is not marked", CutMarks.rowIsCut(pane("/home/gm", cut), null), false)
    // The set is memoised per clipboard object, so a new cut has to replace it rather than read the old one.
    var next = { paths: ["/home/gm/b.txt"], moving: true }
    check("a new cut replaces the old marks", CutMarks.rowIsCut(pane("/home/gm", next), { n: "a.txt" }), false)
    check("and marks its own", CutMarks.rowIsCut(pane("/home/gm", next), { n: "b.txt" }), true)

    // Escape: a cut is taken back and its rows restored; a copy and an empty clipboard are not
    // Escape's to stop at, so it falls through to the status line and the marks.
    var escaping = pane("/home/gm", next)
    escaping.said = ""
    escaping.message = function (text) { this.said = text }
    check("escape cancels a cut", CutMarks.cancel(escaping), true)
    check("and says so", escaping.said, "Cut cancelled.")
    check("and the row it marked is restored", CutMarks.rowIsCut(escaping, { n: "b.txt" }), false)
    check("a second escape has no cut to cancel", CutMarks.cancel(escaping), false)
    var copying = pane("/home/gm", { paths: ["/home/gm/a.txt"], moving: false })
    check("escape leaves a copy alone", CutMarks.cancel(copying) + "|" + copying.clipboard.paths.length, "false|1")
}
