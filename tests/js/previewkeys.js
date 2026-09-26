.import "../../ui/js/PreviewKeys.js" as PreviewKeys
.import "../../ui/js/PreviewPaths.js" as PreviewPaths

function run(check) {
    var activated = []
    var viewer = { pdfControlIndex: -1, pdfControls: [], turnPage: function() {},
        zoomBy: function() {}, toggleExpand: function() {}, scrollPage: function() {} }
    for (var i = 0; i < 6; i++) {
        (function(index) {
            viewer.pdfControls.push({enabled: index !== 0 && index !== 2, visible: true,
                activated: function() { activated.push(index) }})
        })(i)
    }
    PreviewKeys.pdfAction("focusNext", viewer)
    check("PDF focus enters first enabled control", viewer.pdfControlIndex, 1)
    PreviewKeys.pdfAction("preview", viewer)
    PreviewKeys.pdfAction("focusPrevious", viewer)
    check("PDF reverse focus wraps to Close", viewer.pdfControlIndex, 5)
    PreviewKeys.pdfAction("open", viewer)
    check("Space and Enter activate the focused PDF controls", activated.join(","), "1,5")
    viewer.pdfControls[5].enabled = false
    PreviewKeys.pdfAction("preview", viewer)
    check("a control disabled after focus never activates", activated.join(","), "1,5")
    viewer.pdfControls[5].visible = false
    PreviewKeys.pdfAction("focusNext", viewer)
    check("forward wrap skips disabled Previous", viewer.pdfControlIndex, 1)
    PreviewKeys.pdfAction("trash", viewer)
    check("listing actions do nothing in PDF context", activated.join(","), "1,5")

    function previewPane(kind) {
        var pane = { closed: 0, played: 0 }
        pane.preview = {
            isMedia: kind === "audio" || kind === "video",
            isPdf: kind === "pdf",
            revealStrip: function () {},
            close: function () { pane.closed += 1 },
            togglePlay: function () { pane.played += 1 }
        }
        return pane
    }
    for (var kind of ["text", "image", "pdf", "archive"]) {
        var open = previewPane(kind)
        PreviewKeys.act("preview", open)
        check("space closes a " + kind + " preview", open.closed, 1)
        check("and plays nothing on a " + kind + " preview", open.played, 0)
    }
    for (var kind of ["audio", "video"]) {
        var open = previewPane(kind)
        PreviewKeys.act("preview", open)
        check("space plays a " + kind + " preview", open.played, 1)
        check("space does not close a " + kind + " preview", open.closed, 0)
    }
    // Escape still closes, because a preview must never need a particular key to leave it.
    var escaped = previewPane("video")
    PreviewKeys.act("escape", escaped)
    check("escape still closes a media preview", escaped.closed, 1)

    // P also plays, and only where there is something to play.
    var tune = previewPane("audio")
    PreviewKeys.act("playPause", tune)
    check("p plays and pauses a media preview", tune.played, 1)
    check("and closes nothing", tune.closed, 0)
    var still = previewPane("image")
    PreviewKeys.act("playPause", still)
    check("p does nothing to a still preview", still.played + still.closed, 0)

    // Rows can arrive before pane.path is updated. The preview must keep the directory from the
    // request or fall back to home instead of opening /filename.
    function pathPane(path, listingPath, home, row) {
        return {path: path, listingPath: listingPath, home: home,
            join: function(base, name) { return base === "/" ? "/" + name : base + "/" + name },
            rowFor: function() { return row }, cursorIndex: 0,
            kindNames: ["image"], preview: {}}
    }
    var pending = pathPane("", "/home/adam/Downloads", "/home/adam", {n: "photo.jpg", d: false, i: "image", s: 12, k: 0})
    check("preview path uses the pending listing directory", PreviewPaths.rowPath(pending, pending.rowFor()), "/home/adam/Downloads/photo.jpg")
    var home = pathPane("", "", "/home/adam", {n: "photo.jpg", d: false, i: "image", s: 12, k: 0})
    check("preview path falls back to home", PreviewPaths.rowPath(home, home.rowFor()), "/home/adam/photo.jpg")

    var opened = pathPane("", "/home/adam/Downloads", "/home/adam", {n: "photo.jpg", d: false, i: "image", s: 12, k: 0})
    opened.preview.opened = ""
    opened.preview.open = function(path) { opened.preview.opened = path }
    PreviewKeys.open(opened)
    check("file preview uses the safe row path", opened.preview.opened, "/home/adam/Downloads/photo.jpg")

    var folder = pathPane("", "/home/adam/Downloads", "/home/adam", {n: "Pictures", d: true, i: "folder", s: 0, k: 0})
    folder.preview.folder = ""
    folder.preview.openFolder = function(path) { folder.preview.folder = path }
    PreviewKeys.open(folder)
    check("directory preview uses the safe row path", folder.preview.folder, "/home/adam/Downloads/Pictures")

    var moves = []
    var moved = {preview: {movePreview: function(delta) { moves.push(delta) },
        revealStrip: function() {}}}
    PreviewKeys.act("cursorUp", moved)
    PreviewKeys.act("cursorDown", moved)
    check("preview cursor actions move between files", moves.join(","), "-1,1")
}
