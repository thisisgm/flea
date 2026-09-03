import Quickshell
import Quickshell.Io
import QtQuick

Item {
    id: root

    signal listed(int total, real readMs, real sortMs)
    // The listing directory's filesystem, straight off the listed line: a drag compares it against
    // the dropped-on folder's own to tell a move within one volume from a copy across two.
    property var dirDev: 0
    signal rows(int start, var items, real ms, var kinds)
    // mode rides only on a denied listing, the one failure a pane draws more than a sentence for.
    signal failed(string where, string input, string message, int mode)
    signal thumbed(int row, string file)
    signal dirSized(int row, real bytes, bool partial)
    signal searching(int total, int scanned, real ms)
    signal searched(int total, int scanned, real ms, bool cancelled)
    // The write operations, see docs/protocol.md; every one of them is reversible with undo.
    signal transferStarted(int id, int n, bool moving)
    signal transferProgress(int id, int index, string name, real bytes, real total)
    signal transferItem(int id, int index, string name, bool ok, string err)
    signal transferDone(int id, int ok, int failed, int skipped, bool cancelled)
    signal trashed(int ok, int failed)
    signal renamed(bool ok, string path)
    signal made(bool ok, string path)
    signal duplicated(bool ok, string path)
    signal undone(string op, bool ok)
    signal paths(var list)
    signal meta(int row, int w, int h, real durationMs, int sampleRate, int entries, real unpacked, bool archiveFailed, var names, real lines, bool partial, bool linesFailed, string target, bool targetDir, string owner)
    signal fsInfo(string fs, real free)
    // readFailed tells a zero-row answer apart from an empty directory; mode is that directory's own, 0 when the stat failed too.
    // hidden is the flag the request carried, echoed by the backend: two clients peek this wire, so path alone does not say whose reply this is.
    signal peeked(string path, bool hidden, int total, var rows, bool readFailed, int mode)
    signal archiveStarted(int id)
    signal archiveDone(int id, bool ok, bool verified, string err)
    signal convertStarted(int id)
    signal convertDone(int id, bool ok, string path, string err)
    // The shell's exit gate: the backend has drained and this process can end.
    signal quitReady()

    // What this box actually offers, probed by the backend at startup and asked for once at launch.
    // The compress submenu is exactly this list, so a box with no 7zip never shows .7z.
    property var archiveFormats: []
    property bool canConvert: false

    readonly property bool running: child.running

    // What order the current listing is actually in, which is what ui/Header.qml's mark draws.
    // Only two things move it: list re-sorts by name ascending below, and an accepted sort, which
    // ui/js/Sort.js records here because it is the one place that knows which keys are accepted.
    property string sortBy: "name"
    property bool sortDesc: false

    // What the settle gate asserts: how many thumb requests this process has attempted; see AGENTS.md.
    property int thumbRequests: 0
    // Same gate, for dirsize: a fling must issue none of these either.
    property int dirSizeRequests: 0

    // A write before the child is spawned is dropped silently, so an early request waits here.
    property var pending: []
    property bool queueing: true

    // A quit is in flight, so the child's exit is the end the shell asked for, not a failure.
    property bool quitting: false

    // Everything the UI sends goes through here, so the protocol has exactly one author.
    function send(object) {
        var line = JSON.stringify(object) + "\n"
        if (root.queueing) {
            root.pending.push(line)
            return
        }
        if (!child.running) {
            root.failed("backend", "", "the backend is not running", 0)
            return
        }
        child.write(line)
    }

    function list(path, first, hidden) {
        // A fresh scan is always name ascending, so every refresh after a write operation puts the
        // header's mark back rather than leaving it describing the order before the refresh.
        root.sortBy = "name"
        root.sortDesc = false
        root.send({ c: "list", path: path, first: first, hidden: hidden })
    }

    function window(start, count) {
        root.send({ c: "window", start: start, count: count })
    }

    function sort(by, desc) {
        root.send({ c: "sort", by: by, desc: desc })
    }

    // The walk replaces the current listing with its matches, each named relative to path; see docs/protocol.md "search".
    function search(path, query, hidden) {
        root.send({ c: "search", path: path, query: query, hidden: hidden })
    }

    // No rows form, unlike thumbcancel: one walk runs at a time, so a cancel can only mean that one.
    function searchcancel() {
        root.send({ c: "searchcancel" })
    }

    // rows, not paths: a client can only build a path for a row inside the window it holds, and a
    // selection can be wider than that; the backend resolves them at request time, see docs/protocol.md.
    function transfer(op, rows, dest) {
        if (rows.length === 0) {
            return
        }
        root.send({ c: "transfer", op: op, rows: rows, dest: dest })
    }

    function transfercancel(id) {
        root.send({ c: "transfercancel", id: id })
    }

    function trash(rows) {
        if (rows.length === 0) {
            return
        }
        root.send({ c: "trash", rows: rows })
    }

    function rename(path, to) {
        root.send({ c: "rename", path: path, to: to })
    }

    function duplicate(path) {
        root.send({ c: "duplicate", path: path })
    }

    // No name field: omitting it is what makes the backend take the first free "New Folder", so the
    // client needs no retry loop and no collision handling; see docs/protocol.md "mkdir".
    function mkdir(path) {
        root.send({ c: "mkdir", path: path })
    }

    function undo() {
        root.send({ c: "undo" })
    }

    // Resolves indices to absolute paths, so a clipboard can hold a selection wider than the window.
    function askPaths(rows) {
        root.send({ c: "paths", rows: rows })
    }

    // One row, only when a surface asks: the same no-sweep rule thumb and dirsize already follow.
    // media and archive each cost a subprocess in the backend, so each is only ever true for a row
    // whose kind actually names the facts it would answer.
    function askMeta(row, text, media, archive) {
        root.send({ c: "meta", row: row, text: text, media: media, archive: archive })
    }

    function askFsInfo() {
        root.send({ c: "fsinfo" })
    }

    // A read-only look at a directory that is not the current listing; see docs/protocol.md "peek".
    function peek(path, first, hidden) {
        root.send({ c: "peek", path: path, first: first, hidden: hidden })
    }

    function askFormats() {
        root.send({ c: "formats" })
    }

    // paths are absolute and share a parent, which is what a selection from one listing is.
    function compress(paths, dest, format) {
        root.send({ c: "archive", op: "compress", paths: paths, dest: dest, format: format })
    }

    function extract(path, dest) {
        root.send({ c: "archive", op: "extract", path: path, dest: dest })
    }

    // No format field: magick reads the codec off dest's own extension, see docs/protocol.md "convert".
    function convertImage(path, dest, strip) {
        root.send({ c: "convert", path: path, dest: dest, strip: strip })
    }

    function thumb(rows) {
        if (rows.length === 0) {
            return
        }
        root.thumbRequests += 1
        root.send({ c: "thumb", rows: rows })
    }

    // An empty rows cancels EVERYTHING queued, so an empty list is never sent; see docs/protocol.md.
    function thumbcancel(rows) {
        if (rows.length === 0) {
            return
        }
        root.send({ c: "thumbcancel", rows: rows })
    }

    function dirsize(rows) {
        if (rows.length === 0) {
            return
        }
        root.dirSizeRequests += 1
        root.send({ c: "dirsize", rows: rows })
    }

    // Unlike thumbcancel, dirsizecancel carries no rows: it always means everything queued, see docs/protocol.md.
    function dirsizecancel() {
        root.send({ c: "dirsizecancel" })
    }

    // The shell's last request, see docs/protocol.md "quit". The backend cancels the running
    // operation as it drains, and a cancelled copy removes its own partial destination, so waiting
    // for quitReady is what keeps a close from leaving a half-written file under the final name.
    function quit() {
        if (root.quitting) {
            return
        }
        root.quitting = true
        // Before the child is up a quit would only join the pending queue and never be written,
        // and nothing can be in flight yet, so there is nothing to drain and the shell goes now.
        if (root.queueing || !child.running) {
            root.quitReady()
            return
        }
        root.send({ c: "quit" })
        quitDeadline.start()
    }

    // Sample input: {"t":"rows","start":0,"rows":[{"n":"a.txt","d":false,"s":3,"m":1787790423,"p":33188,"i":"text-x-generic","t":false,"k":0}],"kinds":["Plain text document"],"ms":1.250}
    // Sample input: {"t":"thumbed","row":2,"file":"/home/gm/.cache/thumbnails/large/b98fa4.png","ms":75.823}
    // Sample input: {"t":"dirsized","row":4,"bytes":1048576,"partial":false,"ms":12.500}
    // Sample input: {"t":"searching","n":812,"scanned":41200,"ms":300.114}
    // Sample input: {"t":"transferstarted","id":12,"n":2,"moving":true}
    // Sample input: {"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000}
    // Sample input: {"t":"transferitem","id":12,"index":1,"name":"photos","ok":false,"err":"permission denied"}
    // Sample input: {"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false}
    // Sample input: {"t":"trashed","ok":1,"failed":0}
    // Sample input: {"t":"made","ok":true,"path":"/home/gm/Pictures/New Folder"}
    // Sample input: {"t":"undone","op":"move","ok":true}
    function receive(line) {
        if (!line || line.length === 0) {
            return
        }
        var message = null
        try {
            message = JSON.parse(line)
        } catch (e) {
            root.failed("parse", "", "the backend sent a line this build cannot read", 0)
            return
        }
        if (message.t === "listed") {
            root.dirDev = message.v || 0
            root.listed(message.n, message.read, message.sort)
        } else if (message.t === "rows") {
            root.rows(message.start, message.rows, message.ms, message.kinds || [])
        } else if (message.t === "error") {
            root.failed(message.where, message.path, message.msg, message.mode || 0)
        } else if (message.t === "thumbed") {
            root.thumbed(message.row, message.file)
        } else if (message.t === "dirsized") {
            root.dirSized(message.row, message.bytes, message.partial)
        } else if (message.t === "searching") {
            root.searching(message.n, message.scanned, message.ms)
        } else if (message.t === "searched") {
            root.searched(message.n, message.scanned, message.ms, message.cancelled)
        } else if (message.t === "transferstarted") {
            root.transferStarted(message.id, message.n, message.moving)
        } else if (message.t === "transferprogress") {
            root.transferProgress(message.id, message.index, message.name, message.bytes, message.total)
        } else if (message.t === "transferitem") {
            // err rides only on a failure, so an ok item has no field to read here.
            root.transferItem(message.id, message.index, message.name, message.ok, message.err || "")
        } else if (message.t === "transferdone") {
            root.transferDone(message.id, message.ok, message.failed, message.skipped, message.cancelled)
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
        } else if (message.t === "paths") {
            root.paths(message.paths || [])
        } else if (message.t === "meta") {
            root.meta(message.row, message.w, message.h, message.ms, message.rate, message.entries, message.unpacked, message.afailed, message.names, message.lines, message.partial, message.lfailed === true, message.target, message.targetdir, message.owner || "")
        } else if (message.t === "fsinfo") {
            root.fsInfo(message.fs, message.free)
        } else if (message.t === "peeked") {
            root.peeked(message.path, message.hidden === true, message.n, message.rows || [], message.failed === true, message.mode || 0)
        } else if (message.t === "formats") {
            root.archiveFormats = message.archive || []
            root.canConvert = message.convert === true
        } else if (message.t === "archivestarted") {
            root.archiveStarted(message.id)
        } else if (message.t === "archivedone") {
            root.archiveDone(message.id, message.ok, message.verified !== false, message.err || "")
        } else if (message.t === "convertstarted") {
            root.convertStarted(message.id)
        } else if (message.t === "convertdone") {
            root.convertDone(message.id, message.ok, message.path || "", message.err || "")
        }
    }

    // Longer than the backend's own 25 s drain limit, so this only fires for a child that never
    // answers a quit at all; without it such a child would leave a shell resident with no window.
    Timer {
        id: quitDeadline
        interval: 30000
        onTriggered: root.quitReady()
    }

    Process {
        id: child
        // FLEA_BIN is the dev seam, see AGENTS.md "Where the backend binary comes from".
        command: [Quickshell.env("FLEA_BIN") || "flea", "--backend"]
        running: true
        stdinEnabled: true

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: function (data) { root.receive(data) }
        }

        onStarted: {
            root.queueing = false
            // Asked once per process: which formats exist cannot change while the backend runs.
            root.askFormats()
            for (var i = 0; i < root.pending.length; i++) {
                child.write(root.pending[i])
            }
            root.pending = []
        }

        // A spawn that fails raises runningChanged and never exited, measured, so it reports here.
        onRunningChanged: {
            if (root.queueing && !child.running) {
                root.queueing = false
                root.pending = []
                root.failed("backend", "", "the backend could not be started", 0)
            }
        }

        // A dead backend is a message, not a crash, so the window can say what happened.
        onExited: function (exitCode, exitStatus) {
            if (root.quitting) {
                quitDeadline.stop()
                root.quitReady()
                return
            }
            root.failed("backend", "", "the backend exited with code " + exitCode, 0)
        }
    }
}
