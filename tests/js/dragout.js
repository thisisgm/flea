.import "../../ui/js/DragOut.js" as DragOut
.import "../../ui/js/Ops.js" as Ops

function pane(sent) {
    return {
        path: "/d",
        clipPending: null,
        pathsPending: null,
        clipboard: null,
        message: function () {},
        selectedIndices: function () { return [0] },
        rowFor: function () { return { n: "a.txt" } },
        backend: { askPaths: function (rows) { sent.push({ c: "paths", rows: rows }) } }
    }
}

function run(check) {
    check("an empty authority is a local file",
          DragOut.filePath("file:///tmp/a.txt"), "/tmp/a.txt")
    check("localhost is a local file",
          DragOut.filePath("file://localhost/tmp/b.txt"), "/tmp/b.txt")
    check("another host is skipped",
          String(DragOut.filePath("file://other-host/tmp/a.txt")), "null")
    check("a bad percent-escape is skipped",
          String(DragOut.filePath("file:///tmp/a%zz.txt")), "null")
    check("one bad escape does not drop the good uri",
          DragOut.filePaths(["file:///tmp/a%zz.txt", "file:///tmp/ok.txt"]).join(","),
          "/tmp/ok.txt")

    var foreign = DragOut.sources(
        ["file:///tmp/a.txt", "file://localhost/tmp/b.txt"], "/tmp/c.txt", "", "")
    check("a foreign drop keeps both uri spellings and a plain path",
          foreign.join(","), "/tmp/a.txt,/tmp/b.txt,/tmp/c.txt")

    var marked = DragOut.sources(
        ["file:///tmp/one.txt"], "/tmp/one.txt\n/tmp/two.txt", "instance\n0\nmove\n/d\n1", "")
    check("an own drag does not read a newline in the name as a second file",
          marked.join(","), "/tmp/one.txt")

    var shelf = DragOut.sources(["file:///tmp/one.txt"], "/tmp/two.txt", "", "token\ncopy")
    check("a shelf drag does not read text/plain", shelf.join(","), "/tmp/one.txt")

    var both = ["/d/folder", "/d/folder/inside.txt"]
    check("a destination that is also inside the drag names the folder",
          DragOut.refusal(both, "/d/folder", "", ""), "That folder is inside the drag.")
    check("a file that already lives there says so",
          DragOut.refusal(["/d/a.txt"], "/d", "", ""), "Already in this folder.")
    check("nothing local says so",
          DragOut.refusal([], "/d", "", ""), "That drag has no local files.")
    check("an own drag with no public list stays quiet",
          DragOut.refusal([], "/d", "marker", ""), "")

    check("a paths ask waits for neither claim", DragOut.askAllowed(null, null), true)
    check("a drag claim blocks another paths ask", DragOut.askAllowed({ kind: "drag" }, null), false)
    check("a copy claim blocks a drag paths ask", DragOut.askAllowed(null, false), false)

    check("the floor refuses while a listing is out",
          DragOut.refuseLoading(true, false, false), true)
    check("a hovered tab keeps its stored folder",
          DragOut.refuseLoading(true, true, false), false)
    check("the tab being opened refuses",
          DragOut.refuseLoading(true, true, true), true)
    check("a settled listing accepts",
          DragOut.refuseLoading(false, false, false), false)
    check("the search tab refuses", DragOut.searchTabEnabled("results", true), false)
    check("another tab stays open during a search", DragOut.searchTabEnabled("results", false), true)
    check("a directory listing's own tab accepts", DragOut.searchTabEnabled("", true), true)

    var sent = []
    var dragPane = pane(sent)
    var delivered = []
    dragPane.pathsPending = {
        kind: "drag",
        deliver: function (list) { delivered.push(list.join(",")) }
    }
    Ops.pathsResolved(dragPane, ["/d/a", "/d/b"])
    check("a drag claim takes the paths reply", delivered.join(";"), "/d/a,/d/b")
    check("and that reply is not copied onto the clipboard",
          dragPane.clipboard === null ? "clear" : "copied", "clear")

    var busy = pane([])
    busy.pathsPending = { kind: "drag" }
    Ops.clip(busy, true)
    check("a cut does not ask paths while a drag claim is waiting",
          busy.clipPending === null ? "held" : "asked", "held")

    var cutting = pane(sent)
    cutting.clipPending = true
    Ops.compress(cutting, "zip")
    check("a compress does not ask paths while a cut is waiting",
          cutting.pathsPending === null ? "held" : "asked", "held")
}
