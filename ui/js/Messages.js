.pragma library

// Every line the backend sends, turned into the signal the front end listens for. Split out of
// ui/Backend.qml, which owns the socket, the requests and the signals themselves: the routing is a
// long table and a table is not a socket's business. The sample lines live with the signals there.
function route(root, message) {
    if (message.t === "listed") {
        root.dirDev = message.v || 0
        root.listed(message.n, message.read, message.sort, message.path || "")
    } else if (message.t === "rows") {
        root.rows(message.start, message.rows, message.ms, message.kinds || [], message.listing || 0)
    } else if (message.t === "error") {
        root.failed(message.where, message.path, message.msg, message.mode || 0)
    } else if (message.t === "localsendpeers") {
        root.localSendPeers(message.peers || [], message.reason || "")
    } else if (message.t === "localsendsent") {
        root.localSendSent(message.ok === true, message.reason || "")
    } else if (message.t === "thumbed") {
        root.thumbed(message.row, message.file)
    } else if (message.t === "dirsized") {
        root.dirSized(message.row, message.bytes, message.partial)
    } else if (message.t === "searching") {
        root.searching(message.n, message.scanned, message.ms)
    } else if (message.t === "searched") {
        root.searched(message.n, message.scanned, message.ms, message.cancelled)
    } else if (message.t === "transferstarted") {
        root.transferStarted(message.id, message.n, message.moving, false)
    } else if (message.t === "extractstarted") {
        root.transferStarted(message.id, 1, false, true)
    } else if (message.t === "transferprogress") {
        root.transferProgress(message.id, message.index, message.name, message.bytes, message.total, message.scanned || 0)
    } else if (message.t === "transferitem") {
        // err rides only on a failure, so an ok item has no field to read here.
        root.transferItem(message.id, message.index, message.name, message.ok, message.err || "")
    } else if (message.t === "transferdone") {
        root.transferDone(message.id, message.ok, message.failed, message.skipped, message.cancelled, message.retryPaths || [])
    } else if (message.t === "collisions") {
        root.collisions(message.id, message.total, message.names || [])
    } else if (message.t === "trashed") {
        root.trashed(message.ok, message.failed)
    } else if (message.t === "renamed") {
        root.renamed(message.ok, message.path)
    } else if (message.t === "made") {
        root.made(message.ok, message.path)
    } else if (message.t === "duplicated") {
        root.duplicated(message.ok, message.path)
    } else if (message.t === "undone") {
        root.undone(message.op, message.ok)
    } else if (message.t === "redone") {
        root.redone(message.op, message.ok)
    } else if (message.t === "redostarted") {
        root.redoStarted(message.id, message.n, message.op)
    } else if (message.t === "paths") {
        root.paths(message.paths || [])
    } else if (message.t === "located") {
        root.located(message)
    } else if (message.t === "trashbrowse") {
        root.trashResult(message)
    } else if (message.t === "permissions") {
        root.permissionsResult(message)
    } else if (message.t === "picker") {
        root.pickerResult(message)
    } else if (message.t === "menuaction") {
        root.menuResult(message)
    } else if (message.t === "meta") {
        root.metaResult(message)
        root.meta(message.row, message.w, message.h, message.ms, message.rate, message.fps || 0, message.bitrate || 0, message.entries, message.unpacked, message.afailed, message.names, message.lines, message.partial, message.lfailed === true, message.target, message.targetdir, message.owner || "")
    } else if (message.t === "fsinfo") {
        root.fsInfo(message.fs, message.free, message.path || "")
    } else if (message.t === "changed") {
        root.changed(message.path || "")
    } else if (message.t === "peeked") {
        root.peeked(message.path, message.hidden === true, message.n, message.rows || [], message.failed === true, message.mode || 0)
    } else if (message.t === "formats") {
        root.archiveFormats = message.archive || []
        root.canConvert = message.convert === true
        root.extraction = message.extract || ({archive: false, sevenZip: false})
        root.providers = message.providers || ({})
        root.formatsResult(message)
    } else if (message.t === "archivestarted") {
        root.archiveStarted(message.id)
    } else if (message.t === "archivedone") {
        root.archiveDone(message.id, message.ok, message.verified !== false, message.err || "")
    } else if (message.t === "convertchecked") {
        root.convertChecked(message)
    } else if (message.t === "convertstarted") {
        root.convertStarted(message.id, message.requestId || 0, message.source || "")
    } else if (message.t === "convertdone") {
        root.convertDone(message.id, message.ok, message.path || "", message.err || "", message.requestId || 0,
                         message.source || "", message.collision === true)
    }
}
