.import "../../ui/js/Drag.js" as Drag

// The gesture's decisions, with the pane stubbed the way tests/js/ops.js stubs it: what a drag
// carries, what may take it, what the row and the bar say, and the one request a drop sends.
function pane(sent, picked, rows) {
    return {
        path: "/d",
        rows: rows,
        clipboard: "untouched",
        selectedIndices: function () { return picked },
        rowFor: function (i) { return (i < 0 || i >= rows.length) ? null : rows[i] },
        join: function (a, b) { return a + "/" + b },
        backend: { send: function (msg) { sent.push(msg) } }
    }
}

function run(check) {
    var rows = [{ n: "omarchy", d: true }, { n: "flea", d: true }, { n: "a.txt", d: false }, { n: "b.txt", d: false }]

    // The selection when the pressed row is in it, the row alone when it is not.
    check("a drag from a selected row carries the whole selection",
          String(Drag.carried(pane([], [1, 2, 3], rows), 2)), "1,2,3")
    check("a drag from a row outside the selection carries that row alone",
          String(Drag.carried(pane([], [1, 2], rows), 3)), "3")
    check("with nothing selected the pressed row is the drag",
          String(Drag.carried(pane([], [], rows), 0)), "0")

    check("ctrl makes it a copy", Drag.copying(Qt.ControlModifier), true)
    check("plain is a move", Drag.copying(Qt.NoModifier), false)
    check("shift alone is still a move", Drag.copying(Qt.ShiftModifier), false)
    check("ctrl with shift is still a copy", Drag.copying(Qt.ControlModifier | Qt.ShiftModifier), true)

    // What takes a drop: a directory row that the drag is not itself carrying.
    check("a folder takes a drop", Drag.canDrop([2], 0, rows[0]), true)
    check("a file does not", Drag.canDrop([2], 3, rows[3]), false)
    check("a folder cannot take itself", Drag.canDrop([0], 0, rows[0]), false)
    check("nor a selection it is part of", Drag.canDrop([0, 2], 0, rows[0]), false)
    check("a row not loaded takes nothing", Drag.canDrop([2], 9, null), false)

    // The board's own words on the hovered folder.
    check("the row says move here", Drag.label(false), "move here")
    check("and copy here under ctrl", Drag.label(true), "copy here")

    // The status bar's half of the board's caption, "copy vs move reads in the status bar".
    check("the bar names the verb, the count and the folder",
          Drag.line(2, "omarchy", false), "Move 2 items to omarchy · ctrl at lift copies")
    check("a copy line drops the hint", Drag.line(1, "omarchy", true), "Copy 1 item to omarchy")
    check("with no folder under the pointer it says where one would go",
          Drag.line(3, "", false), "Move 3 items to a folder · ctrl at lift copies")

    // The drop is the transfer request, rows and not paths, the shape Ops.moveToDropbox sends.
    var sent = []
    var mover = pane(sent, [], rows)
    check("a drop on a folder sends one transfer", Drag.drop(mover, [2, 3], 0, false), true)
    check("and it is a move of those rows into that folder",
          JSON.stringify(sent), JSON.stringify([{ c: "transfer", op: "move", rows: [2, 3], dest: "/d/omarchy" }]))
    check("and the clipboard was never part of it", mover.clipboard, "untouched")
    var copied = []
    Drag.drop(pane(copied, [], rows), [2], 1, true)
    check("under ctrl it is a copy", copied.length === 1 ? copied[0].op + " " + copied[0].dest : "nothing sent", "copy /d/flea")

    // The marker carries where the rows were lifted from, so a drop after the listing changed can
    // resolve by path, and the wire carries the same paths as plain text for a terminal.
    var lifted = pane([], [], rows)
    lifted.backend.dirDev = 42
    var wire = Drag.mimeFor(lifted, [2, 3], false)
    check("the marker names the source directory", Drag.markerSource(wire[Drag.ROWS_MIME]), "/d")
    check("and its filesystem", Drag.markerDev(wire[Drag.ROWS_MIME]), 42)
    check("a marker from before this shape reads as no source", Drag.markerSource("x\n1\nmove"), "")
    check("and as an unknown filesystem", Drag.markerDev("x\n1\nmove"), 0)
    check("the same listing is the by-index case", Drag.sameListing(wire[Drag.ROWS_MIME], "/d"), true)
    check("another directory is not", Drag.sameListing(wire[Drag.ROWS_MIME], "/d/flea"), false)
    check("nor is a foreign drag, whatever its source", Drag.sameListing("other\n1\nmove\n/d\n42", "/d"), false)
    check("plain text is the absolute paths, one a line", wire["text/plain"], "/d/a.txt\n/d/b.txt")
    check("a wide selection offers no plain text either", Drag.mimeFor(lifted, [2, 9], false)["text/plain"], undefined)

    // A drop by path: into the floor of another tab's listing, or a folder reached after a switch.
    var urls = ["file:///d/a.txt", "file:///d/b.txt"]
    check("rows dropped where they already live are refused", Drag.canDropInto(wire[Drag.ROWS_MIME], urls, "/d"), false)
    check("and taken into another directory", Drag.canDropInto(wire[Drag.ROWS_MIME], urls, "/e"), true)
    check("a drag carrying no paths is refused", Drag.canDropInto(wire[Drag.ROWS_MIME], [], "/e"), false)
    var moved = []
    check("same filesystem, no ctrl: a move", Drag.dropInto(pane(moved, [], rows), wire[Drag.ROWS_MIME], urls, "/e", 42), true)
    check("of those paths into that directory", JSON.stringify(moved),
          JSON.stringify([{ c: "transfer", op: "move", paths: ["/d/a.txt", "/d/b.txt"], dest: "/e" }]))
    var crossed = []
    Drag.dropInto(pane(crossed, [], rows), wire[Drag.ROWS_MIME], urls, "/e", 7)
    check("another filesystem copies", crossed[0].op, "copy")
    var unknown = []
    Drag.dropInto(pane(unknown, [], rows), wire[Drag.ROWS_MIME], urls, "/e", 0)
    check("and so does a destination whose filesystem is unknown", unknown[0].op, "copy")
    var foreign = []
    Drag.dropInto(pane(foreign, [], rows), "", urls, "/e", 42)
    check("a foreign drag copies whatever the devices say", foreign[0].op, "copy")
    var refused = []
    check("a refused drop sends nothing", Drag.dropInto(pane(refused, [], rows), wire[Drag.ROWS_MIME], urls, "/d", 42), false)
    check("and nothing reached the backend", refused.length, 0)
    var folded = []
    check("a drop of a folder onto itself sends nothing", Drag.drop(pane(folded, [], rows), [0, 2], 0, false), false)
    check("and nothing went out", folded.length, 0)
    // A folder into itself or its own subtree is refused by path too, the gate the floor, the tabs and
    // a folder row all share; a sibling whose name merely starts the same is not the subtree.
    var folderUrls = ["file:///d/omarchy"]
    check("a folder cannot land on itself by path", Drag.canDropInto("", folderUrls, "/d/omarchy"), false)
    check("nor inside its own subtree", Drag.canDropInto("", folderUrls, "/d/omarchy/deep"), false)
    check("a sibling that starts with the same name is fine", Drag.canDropInto("", folderUrls, "/d/omarchy2"), true)
    var inside = []
    check("and the transfer is refused before it is sent", Drag.dropInto(pane(inside, [], rows), "", folderUrls, "/d/omarchy/deep", 3), false)
    check("so nothing reached the backend", inside.length, 0)
    // A foreign drag of a file into the folder it already lives in would copy it onto itself.
    check("a file from another window cannot land in its own folder", Drag.canDropInto("", ["file:///d/a.txt"], "/d"), false)
    check("but the same file can land one folder down", Drag.canDropInto("", ["file:///d/a.txt"], "/d/omarchy"), true)
    check("and a file straight under the root cannot land in the root", Drag.canDropInto("", ["file:///a.txt"], "/"), false)
    // Which route a drop takes: paths whenever the drag carries them, the row index only for a
    // selection too wide to carry paths, and then only on the listing it was lifted from.
    check("hasPaths reads the uri-list", Drag.hasPaths(urls), true)
    check("and answers false for a drag carrying none", Drag.hasPaths([]), false)
    check("a wide drag drops by index on its own listing", Drag.canDropByIndex(wire[Drag.ROWS_MIME], "/d", [0, 2], 1), true)
    check("but not onto a folder it carries", Drag.canDropByIndex(wire[Drag.ROWS_MIME], "/d", [0, 2], 0), false)
    check("and never on another listing", Drag.canDropByIndex(wire[Drag.ROWS_MIME], "/e", [0, 2], 1), false)
    check("nor in a view that does not own the lifted indices", Drag.canDropByIndex(wire[Drag.ROWS_MIME], "/d", [], 1), false)
    var onFile = []
    check("a drop on a file sends nothing", Drag.drop(pane(onFile, [], rows), [2], 3, false), false)
    check("a drop on a row that is not loaded sends nothing", Drag.drop(pane(onFile, [], rows), [2], 9, false), false)
    check("and nothing went out either way", onFile.length, 0)

    // A drop from another application: file:// URIs in, one transfer naming paths out.
    check("a file URI becomes a path",
          String(Drag.pathsFromUrls(["file:///d/a.txt"])), "/d/a.txt")
    check("percent escapes are decoded",
          String(Drag.pathsFromUrls(["file:///d/a%20b.txt"])), "/d/a b.txt")
    check("several URIs keep their order",
          String(Drag.pathsFromUrls(["file:///d/a.txt", "file:///d/b.txt"])), "/d/a.txt,/d/b.txt")
    check("a non-file URI is left behind rather than guessed at",
          String(Drag.pathsFromUrls(["https://example.com/a.txt"])), "")
    check("and it does not take the file ones with it",
          String(Drag.pathsFromUrls(["https://example.com/a.txt", "file:///d/a.txt"])), "/d/a.txt")
    check("no urls at all is no paths", Drag.pathsFromUrls(null).length, 0)

    var external = []
    check("an external drop on a folder sends one transfer",
          Drag.dropInto(pane(external, [], rows), "", ["file:///x/a.txt", "file:///x/b.txt"], "/d/omarchy", 0), true)
    check("and it is a copy of those paths into that folder",
          JSON.stringify(external),
          JSON.stringify([{ c: "transfer", op: "copy", paths: ["/x/a.txt", "/x/b.txt"], dest: "/d/omarchy" }]))
    // A file row and a row not loaded are refused before any drop by ui/List.qml's own canDrop above.
    var extRefused = []
    check("an external drop carrying no local file sends nothing",
          Drag.dropInto(pane(extRefused, [], rows), "", ["https://example.com/a.txt"], "/d/omarchy", 0), false)
    check("and nothing went out from any of them", extRefused.length, 0)

    // What the drag puts on the wire, and the marker that tells Flea's own drag from a foreign one.
    check("a path becomes a file URI", Drag.uriFor("/d/a.txt"), "file:///d/a.txt")
    check("a space is percent encoded", Drag.uriFor("/d/a b.txt"), "file:///d/a%20b.txt")
    check("and so is a hash, which encodeURI would leave alone", Drag.uriFor("/d/a#b.txt"), "file:///d/a%23b.txt")
    check("the separators survive the encoding", Drag.uriFor("/d/x/y.txt"), "file:///d/x/y.txt")
    check("a URI this side writes round trips back to its path",
          String(Drag.pathsFromUrls([Drag.uriFor("/d/a b#c.txt")])), "/d/a b#c.txt")

    var mime = Drag.mimeFor(pane([], [], rows), [0, 2], false)
    check("the wire carries the marker, sender first then the rows it holds",
          mime[Drag.ROWS_MIME].split("\n")[1], "0,2")
    check("and a CRLF separated uri-list of the carried rows",
          mime["text/uri-list"], "file:///d/omarchy\r\nfile:///d/a.txt\r\n")
    check("a drag carrying nothing offers no list either, for the same reason",
          Drag.mimeFor(pane([], [], rows), [], false).hasOwnProperty("text/uri-list"), false)
    check("the bar says nothing extra when the drag can leave", Drag.reachNote(true), "")
    check("and names the limit when it cannot", Drag.reachNote(false), " · too wide to drag out")
    // A selection reaches past the window the client holds, and dropping the rest in silence is how
    // Ops.js "abandoned the rest" on a wide move. The payload and the count the bar says must agree,
    // so an unresolvable selection offers no uri-list at all rather than a subset of one.
    var wide = Drag.mimeFor(pane([], [], rows), [0, 9], false)
    check("a selection reaching past the held window offers no uri-list at all",
          wide.hasOwnProperty("text/uri-list"), false)
    check("and the marker still carries the whole selection, so an internal drop is complete",
          wide[Drag.ROWS_MIME].split("\n")[1], "0,9")
    check("a fully resolvable selection still offers both",
          Drag.mimeFor(pane([], [], rows), [0, 2], false).hasOwnProperty("text/uri-list"), true)

    // The marker names the application; the instance mime names this process. Another Flea window is
    // a different process whose row indices mean nothing here, so it must not take the internal path.
    check("a drag from this window is recognised as its own",
          Drag.isOwnDrag(Drag.markerPayload([0, 2], false)), true)
    check("a drag from another Flea window is not",
          Drag.isOwnDrag("some-other-flea\n0,2"), false)
    check("and neither is something carrying no marker at all",
          Drag.isOwnDrag(""), false)
    check("the marker names the sender before the rows",
          Drag.markerPayload([0, 2], false).split("\n")[1], "0,2")
    // The wire's marker carries the pane's directory and its device, 0 when the stub backend has none.
    check("and the whole marker is what goes on the wire",
          Drag.mimeFor(pane([], [], rows), [0, 2], false)[Drag.ROWS_MIME], Drag.markerPayload([0, 2], false, "/d", 0))

    // One function decides the verb, and the label and the transfer both read it: a line promising a
    // copy while a move happens is the shape this branch has already produced twice.
    check("within one volume this window's own drag moves", Drag.verbFor(true, false, 56, 56), "move")
    check("across two volumes it copies, so the original survives the crossing",
          Drag.verbFor(true, false, 56, 32), "copy")
    check("ctrl forces a copy within one volume", Drag.verbFor(true, true, 56, 56), "copy")
    check("and across two it is a copy either way", Drag.verbFor(true, true, 56, 32), "copy")
    check("anything from elsewhere copies", Drag.verbFor(false, false, 56, 56), "copy")
    check("and ctrl cannot turn that into a move", Drag.verbFor(false, true, 56, 56), "copy")
    check("a source device that could not be read copies rather than risk a move",
          Drag.verbFor(true, false, 0, 56), "copy")
    check("and a destination that could not be read does the same",
          Drag.verbFor(true, false, 56, 0), "copy")
    check("the row label reads the verb the same function gave",
          Drag.label(Drag.verbFor(true, false, 56, 32) === "copy"), "copy here")
    check("and so does the bar line",
          Drag.line(1, "omarchy", Drag.verbFor(true, false, 56, 56) === "copy"),
          "Move 1 item to omarchy · ctrl at lift copies")

    // The lift's ctrl rides the marker because the drop event cannot carry it any more: Flea now
    // advertises Qt.CopyAction alone, so Chromium stops reporting dropEffect move, and Qt clamps
    // a DragEvent's proposedAction to what the source advertised. Measured on Qt 6.11.2: the
    // receiver read proposedAction 2 of supported 3 under copy|move and 1 of 1 under copy alone.
    check("the marker's last field is the modifier the lift read",
          Drag.markerPayload([0, 2], true).split("\n")[2], "copy")
    check("and a plain lift says move in that same field",
          Drag.markerPayload([0, 2], false).split("\n")[2], "move")
    check("a marked copy reads back as one", Drag.markerCopying(Drag.markerPayload([2], true)), true)
    check("a marked move reads back as one", Drag.markerCopying(Drag.markerPayload([2], false)), false)
    check("a marker carrying no modifier field is not a copy",
          Drag.markerCopying("some-other-flea\n0,2"), false)
    check("and neither is a drag carrying no marker at all", Drag.markerCopying(""), false)

    // The round trip the DropArea makes: what mimeFor put on the wire is what verbFor reads back.
    var plainWire = Drag.mimeFor(pane([], [], rows), [0, 2], false)[Drag.ROWS_MIME]
    var heldWire = Drag.mimeFor(pane([], [], rows), [0, 2], true)[Drag.ROWS_MIME]
    check("the wire carries the modifier the lift read", Drag.markerCopying(heldWire), true)
    check("and a plain lift puts a move on it", Drag.markerCopying(plainWire), false)
    check("so a plain drag within one volume still moves",
          Drag.verbFor(Drag.isOwnDrag(plainWire), Drag.markerCopying(plainWire), 56, 56), "move")
    check("ctrl at the lift still forces a copy",
          Drag.verbFor(Drag.isOwnDrag(heldWire), Drag.markerCopying(heldWire), 56, 56), "copy")
    check("and across two volumes the marker cannot make it a move",
          Drag.verbFor(Drag.isOwnDrag(plainWire), Drag.markerCopying(plainWire), 56, 32), "copy")
    check("a marked drag from another Flea window still copies, whatever its marker says",
          Drag.verbFor(Drag.isOwnDrag("some-other-flea\n0,2\nmove"),
                       Drag.markerCopying("some-other-flea\n0,2\nmove"), 56, 56), "copy")

    var fromOtherFlea = []
    Drag.dropInto(pane(fromOtherFlea, [], rows), "some-other-flea\n0\nmove\n/x\n56", ["file:///x/a.txt"], "/d/omarchy", 56)
    check("a drop from another Flea window copies, like any other foreign source",
          fromOtherFlea.length === 1 ? fromOtherFlea[0].op : "nothing sent", "copy")

    var feedback = Drag.feedbackFor(Drag.markerPayload([1, 3], false, "/source", 56),
        ["file:///source/a.txt", "file:///source/link"])
    check("a different view reads the full carried count from the marker", feedback.count, 2)
    check("target feedback follows same-device move", Drag.feedbackLine(feedback, "folder", 56),
        "Move 2 items to folder · ctrl at lift copies")
    check("target feedback follows cross-device copy", Drag.feedbackLine(feedback, "folder", 32),
        "Copy 2 items to folder")
    check("an unknown destination is described as copy", Drag.feedbackLine(feedback, "folder", 0),
        "Copy 2 items to folder")
    check("leaving a target restores the source gesture's generic line", Drag.feedbackLine(feedback, "", feedback.dev),
        "Move 2 items to a folder · ctrl at lift copies")
    var wideFeedback = Drag.feedbackFor(Drag.markerPayload([1, 3, 5], false, "/source", 56), [])
    check("a wide payload keeps its whole count in another view", wideFeedback.count, 3)
    check("wide feedback never promises external reach", Drag.feedbackLine(wideFeedback, "folder", 56),
        "Move 3 items to folder · ctrl at lift copies · too wide to drag out")
    var foreignFeedback = Drag.feedbackFor("other\n0,1,2\nmove\n/source\n56", ["file:///source/a.txt"])
    check("foreign feedback counts actual paths, never foreign row indices", foreignFeedback.count, 1)
    check("foreign feedback copies on the same device", Drag.feedbackLine(foreignFeedback, "folder", 56),
        "Copy 1 item to folder")
    check("a pathless foreign payload has no live feedback", Drag.feedbackLine(Drag.feedbackFor("", []), "folder", 56), "")
    check("ctrl survives the feedback handoff", Drag.feedbackLine(Drag.feedbackFor(
        Drag.markerPayload([1], true, "/source", 56), ["file:///source/a.txt"]), "folder", 56), "Copy 1 item to folder")
}
