.import "../../ui/js/LocalSend.js" as LocalSend
.import "../../ui/js/Ops.js" as Ops

function run(check) {
    var asked = [], launched = []
    var pane = {
        cursorIndex: 3, pathsPending: null, clipPending: null,
        message: function () {},
        selectedIndices: function () { return [3, 900] },
        backend: { askPaths: function (indices) { asked = indices } }
    }
    LocalSend.request(pane)
    check("the backend resolves the whole selection, including offscreen rows", asked.join(","), "3,900")
    var paths = ["/tmp/a file.txt", "/tmp/$(touch nope);'folder"]
    check("the LocalSend reply is consumed", LocalSend.resolved(pane, paths, function (list) { launched = list }), true)
    check("paths retain spaces and shell metacharacters", JSON.stringify(launched), JSON.stringify(paths))
    check("the pending action is cleared", pane.pathsPending, null)
    pane.selectedIndices = function () { return [] }
    LocalSend.request(pane)
    check("no selection falls back to the cursor", asked.join(","), "3")
    launched = []
    LocalSend.resolved(pane, [], function () { launched.push("unexpected") })
    check("an empty reply does not open LocalSend", launched.length, 0)
    pane.pathsPending = { kind: "compress", format: "zip" }
    check("other path consumers are untouched", LocalSend.resolved(pane, paths, function () { launched.push("unexpected") }), false)
    check("the other action keeps its pending state", pane.pathsPending.kind, "compress")
    check("no other reply launches LocalSend", launched.length, 0)
    pane.pathsPending = null
    Ops.clip(pane, false)
    pane.cursorIndex = 9
    LocalSend.request(pane)
    check("LocalSend cannot steal an outstanding clipboard reply", pane.pathsPending, null)
    check("the original clipboard request stays intact", asked.join(","), "3")
    Ops.pathsResolved(pane, ["/tmp/copied"])
    check("the clipboard receives its own files", pane.clipboard.paths.join(","), "/tmp/copied")
    LocalSend.request(pane)
    pane.cursorIndex = 10
    Ops.clip(pane, true)
    Ops.compress(pane, "zip")
    LocalSend.request(pane)
    check("copy, compress and repeated sends wait for LocalSend's reply", asked.join(","), "9")
    check("the original send retains reply ownership", pane.pathsPending.kind, "localsend")
    check("no clipboard request was armed", pane.clipPending, null)
    LocalSend.resolved(pane, ["/tmp/sent"], function (list) { launched = list })
    Ops.compress(pane, "zip")
    LocalSend.request(pane)
    check("LocalSend cannot steal a compression reply", pane.pathsPending.kind, "compress")
}
