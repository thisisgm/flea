.pragma library

.import "DropDev.js" as DropDev
.import "Menu.js" as Menu

// Every line the backend sends, turned into the signal the front end listens for. Split out of
// ui/Backend.qml, which owns the socket, the requests and the signals themselves: the routing is a
// long table and a table is not a socket's business. The sample lines live with the signals there.
function route(root, message) {
    if (message.t === "listed") {
        root.dirDev = message.v || 0
        root.dirWritable = message.w !== false
        root.listed(message.n, message.read, message.sort, message.path || "", message.changed, message.gen || 0)
    } else if (message.t === "rows") {
        var landed = sized(message)
        root.rows(message.start, landed, message.ms, message.kinds || [], message.listing || 0, message.gen || 0)
    } else if (message.t === "error") {
        // The backend names an empty clipboard by code, so the one sentence lives in Menu.EMPTY_CLIPBOARD.
        root.failed(message.where, message.path, message.code === "emptyclipboard" ? Menu.EMPTY_CLIPBOARD : message.msg, message.mode || 0)
    } else if (message.t === "localsendpeers") {
        root.localSendPeers(message.peers || [], message.reason || "")
    } else if (message.t === "localsendsent") {
        root.localSendSent(message.ok === true, message.reason || "")
    } else if (message.t === "thumbed") {
        root.thumbed(message.row, message.file)
    } else if (message.t === "dirsized") {
        root.dirSized(message.row, message.bytes, message.partial)
    } else if (message.t === "settagged") {
        root.settagged(message)
    } else if (message.t === "tagstates") {
        root.tagStatesResult(message)
    } else if (message.t === "searching") {
        root.searching(message.n, message.scanned, message.ms)
    } else if (message.t === "searched") {
        root.searched(message.n, message.scanned, message.ms, message.cancelled, message.gen || 0, message.walking === true)
    } else if (message.t === "transferstarted") {
        root.transferStarted(message.id, message.n, message.moving, false)
    } else if (message.t === "extractstarted") {
        root.transferStarted(message.id, 1, false, true)
    } else if (message.t === "transferprogress") {
        root.transferProgress(message.id, message.index, message.name, message.bytes, message.total, message.scanned || 0,
                              message.phase || "", message.drive || "")
    } else if (message.t === "transferitem") {
        // err rides only on a failure, so an ok item has no field to read here.
        root.transferItem(message.id, message.index, message.name, message.ok, message.err || "")
    } else if (message.t === "transferdone") {
        // durable stays unread until a "written to the drive" sentence exists; note already
        // has one, the verdict's own, which ui/js/Ops.js draws ahead of the undo hint.
        root.transferDone(message.id, message.ok, message.failed, message.skipped, message.cancelled, message.retryPaths || [],
                          message.durable === true, message.note || "", message.merged || 0, message.keptBoth || 0, message.into || "")
    } else if (message.t === "collisions") {
        root.collisions(message.id, message.total, message.names || [], message.mergeable === true)
    } else if (message.t === "mergecollisions") {
        root.mergeCollisions(message.id, message.total, message.names || [])
    } else if (message.t === "fatnames") {
        root.fatnames(message.id || 0, message.renames || [], message.fstype || "", message.volume || "")
    // MenuAdditions040: Paste as links answers one line per request, and one
    // journal entry, so one undo removes every link it created.
    } else if (message.t === "linked") {
        root.linked(message.ok || 0, message.failed || 0, message.skipped || 0, message.note || "")
    // MenuAdditions040: Show original reveals the link's target in its own folder.
    } else if (message.t === "linktarget") {
        root.linkTarget(message.path || "", message.directory || "", message.name || "", message.id || 0)
    } else if (message.t === "trashed") {
        root.trashed(message.ok, message.failed, message.err || "")
    } else if (message.t === "renamed") {
        root.renamed(message.ok, message.path)
    } else if (message.t === "slow") {
        // A remote write past its deadline: information first, the reply itself later.
        root.slowOp(message.op || "", message.path || "", message.msg || "")
    } else if (message.t === "bulkrenamed") {
        root.bulkRenamed(message.ok || 0, message.unchanged || 0)
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
    } else if (message.t === "pdfcopied") {
        root.pdfCopied(message.id || 0, message.path || "", message.err || "", message.stalled === true)
    } else if (message.t === "fontlooked") {
        root.fontLooked(message)
    } else if (message.t === "meta") {
        root.metaResult(message)
        root.meta(message.row, message.w, message.h, message.orient || 1, message.ms, message.rate, message.entries, message.unpacked, message.afailed, message.names, message.lines, message.partial, message.lfailed === true, message.target, message.targetdir, message.owner || "")
    } else if (message.t === "code") {
        root.codeResult(message)
    } else if (message.t === "html") {
        root.htmlResult(message)
    } else if (message.t === "compare") {
        root.compareResult(message)
    } else if (message.t === "shebang") {
        root.shebang(message.path || "", message.hasShebang === true, message.id || 0)
    } else if (message.t === "clip") {
        root.clipResult(message)
    } else if (message.t === "fsinfo") {
        root.fsInfo(message.fs, message.free, message.path || "", message.class || "", message.tagable === true, message.system === true)
    } else if (message.t === "changed") {
        root.changed(message.path || "")
    } else if (message.t === "unmounted") {
        root.unmounted(message.path || "", message.parent || "")
    } else if (message.t === "jumped") {
        root.jumped(message.id || 0, message.favourites || [], message.zoxide || [], message.recent || [], message.frecency || {})
    } else if (message.t === "devs") {
        if (message.id > 0) root.metaResult(message)
        else root.pathDevs = DropDev.merged(root.pathDevs, message.devs)
    } else if (message.t === "kind") {
        if (message.id > 0) root.metaResult(message)
    } else if (message.t === "peeked") {
        root.peeked(message.path, message.hidden === true, message.n, message.rows || [], message.failed === true, message.mode || 0, message.hiddenLast === true, message.first || 0, message.v || 0, message.w !== false, message.by || "", message.desc === true)
    } else if (message.t === "formats") {
        root.archiveFormats = message.archive || []
        root.canConvert = message.convert === true
        root.extraction = message.extract || ({archive: false, sevenZip: false})
        root.providers = message.providers || ({})
        root.formatsResult(message)
    } else if (message.t === "archivestarted") {
        root.archiveStarted(message.id)
    } else if (message.t === "archivedone") {
        root.archiveDone(message.id, message.ok, message.verified !== false, message.err || "", message.path || "", message.dest || "")
    } else if (message.t === "convertchecked") {
        root.convertChecked(message)
    } else if (message.t === "convertstarted") {
        root.convertStarted(message.id, message.requestId || 0, message.source || "")
    } else if (message.t === "convertdone") {
        root.convertDone(message.id, message.ok, message.path || "", message.err || "", message.requestId || 0,
                         message.source || "", message.collision === true)
    } else if (message.t === "flashimage") {
        root.flashImageResult(message)
    } else if (message.t === "flashtargets") {
        root.flashTargetsResult(message)
    } else if (message.t === "flashstarted") {
        root.flashStartedLine(message.id, message.total || 0, message.known === true)
    } else if (message.t === "flashprogress") {
        root.flashProgressLine(message.id, message.phase || "", message.bytes || 0,
                               message.total || 0, message.permille || 0)
    } else if (message.t === "flashdone") {
        root.flashDoneLine(message)
    } else if (message.t === "flasheject") {
        root.flashEjectLine(message)
    }
}

// A live answer's rows line names the folder sizes the backend already knows; each one rides on its own row.
// Sample input: {"t":"rows","start":20,"rows":[{"n":"c","d":true,"s":4096,"m":1787790423,"p":16877,"i":"folder","t":false,"k":0,"v":2049}],"kinds":["Folder"],"ms":0.410,"listing":5,"gen":3,"z":[[20,96]]} gives row "c" z 96.
function sized(message) {
    var z = message.z || []
    for (var i = 0; i < z.length; i++) {
        var row = message.rows[z[i][0] - message.start]
        if (row)
            row.z = z[i][1]
    }
    return message.rows
}
