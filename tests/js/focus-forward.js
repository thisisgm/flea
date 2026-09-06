.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/PreviewKeys.js" as PreviewKeys

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

function pdfOpen() {
    return { active: true, isMedia: false, isPdf: true }
}

function mediaOpen() {
    return { active: true, isMedia: true, isPdf: false }
}

function lKey() {
    return { key: Qt.Key_L, text: "l", modifiers: Qt.NoModifier }
}

// rowFor records its only input so browse-forward cannot grow beyond Pane's held viewport.
function pane(row, view) {
    var p = {
        cursorIndex: 37,
        focusView: view ? view : "list",
        viewMode: "list",
        searchMode: "",
        preview: closed(),
        shareBrowser: { active: false },
        rowsRead: [],
        rowFor: function (index) { p.rowsRead.push(index); return row }
    }
    return p
}

function handlePane(row) {
    var p = pane(row)
    p.filterTyping = false
    p.inputAt = 0
    p.rowsAt = 0
    p.trashArmedAt = 0
    p.shown = null
    p.said = ""
    p.renameEditor = function () { return null }
    p.message = function (text) { p.said = text }
    return p
}

function sidebar() {
    return { renameEditor: function () { return null } }
}

function pdfPreview() {
    return {
        active: true,
        isMedia: false,
        isPdf: true,
        page: 0,
        revealStrip: function () {},
        turnPage: function (delta) { this.page += delta }
    }
}

function run(check) {
    var l = lKey()

    var directory = pane({ d: true, t: false, i: "folder" })
    check("l enters the directory under the cursor", Focus.lookup(l, directory), "open")
    check("directory lookup reads only the held cursor row", directory.rowsRead.join(","), "37")

    var extensionlessSymlinkDirectory = pane({ d: false, p: 0o120777, i: "folder", t: false })
    check("l enters an extensionless symlink to a directory", Focus.lookup(l, extensionlessSymlinkDirectory), "open")
    check("extensionless symlink lookup reads only the held cursor row", extensionlessSymlinkDirectory.rowsRead.join(","), "37")

    var thumbnailableSymlinkDirectory = pane({ d: false, p: 0o120777, i: "folder", t: true })
    check("l enters a thumbnailable symlink to a directory", Focus.lookup(l, thumbnailableSymlinkDirectory), "open")
    check("thumbnailable symlink lookup reads only the held cursor row", thumbnailableSymlinkDirectory.rowsRead.join(","), "37")

    var file = pane({ d: false, p: 0o100644, i: "folder", t: false })
    check("l previews an ordinary file even with a folder icon", Focus.lookup(l, file), "preview")
    check("file lookup reads only the held cursor row", file.rowsRead.join(","), "37")

    var empty = pane(null)
    check("l is silent without a held cursor row", Focus.lookup(l, empty), "")
    check("empty lookup still asks only for the held cursor row", empty.rowsRead.join(","), "37")

    var unloaded = handlePane(null)
    check("l on an unloaded row is consumed without a filter hint",
          Focus.handleKey(l, unloaded, sidebar()) + "|" + unloaded.said, "true|")
    check("unloaded handling still asks only for the held cursor row", unloaded.rowsRead.join(","), "37")

    var rail = pane(null, "rail")
    check("l activates the selected rail row", Focus.lookup(l, rail), "open")
    check("rail lookup does not inspect the hidden listing", rail.rowsRead.length, 0)

    var share = pane({ d: false })
    share.shareBrowser.active = true
    check("l activates the selected share", Focus.lookup(l, share), "open")
    check("share lookup does not inspect the hidden listing", share.rowsRead.length, 0)

    var pdf = pane(null)
    pdf.preview = pdfOpen()
    check("l pages an open PDF forward", Focus.lookup(l, pdf), "pageForward")
    check("PDF lookup does not inspect the hidden listing", pdf.rowsRead.length, 0)

    var reader = pdfPreview()
    PreviewKeys.act("pageForward", { preview: reader })
    PreviewKeys.act("pageForward", { preview: reader })
    PreviewKeys.act("parent", { preview: reader })
    check("l advances the PDF and h retreats it", reader.page, 1)

    var media = pane(null)
    media.preview = mediaOpen()
    check("l stays silent over media", Focus.lookup(l, media), "")
    check("media lookup does not inspect the hidden listing", media.rowsRead.length, 0)
}
