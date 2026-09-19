import Quickshell
import Quickshell.Io
import QtQuick
import "js/Messages.js" as Messages

Item {
    id: root

    signal listed(int total, real readMs, real sortMs, string path)
    // The listing directory's filesystem, straight off the listed line: a drag compares it against
    // the dropped-on folder's own to tell a move within one volume from a copy across two.
    property var dirDev: 0
    signal rows(int start, var items, real ms, var kinds)
    property real firstRowsAt: 0
    onRows: function(start, items, ms, kinds) {
        if (root.firstRowsAt === 0 && items.length > 0) root.firstRowsAt = Date.now()
    }
    // mode rides only on a denied listing, the one failure a pane draws more than a sentence for.
    signal failed(string where, string input, string message, int mode)
    // Directive 71: LocalSend's own CLI answers on its own thread, so both legs arrive as their own line.
    signal localSendPeers(var peers, string reason)
    signal localSendSent(bool ok, string reason)
    signal thumbed(int row, string file)
    signal dirSized(int row, real bytes, bool partial)
    signal searching(int total, int scanned, real ms)
    signal searched(int total, int scanned, real ms, bool cancelled)
    // The write operations, see docs/protocol.md; every one of them is reversible with undo.
    signal transferStarted(int id, int n, bool moving)
    signal transferProgress(int id, int index, string name, real bytes, real total, real scanned)
    signal transferItem(int id, int index, string name, bool ok, string err)
    signal transferDone(int id, int ok, int failed, int skipped, bool cancelled, var retryPaths)
    signal trashed(int ok, int failed)
    signal renamed(bool ok, string path)
    signal made(bool ok, string path)
    signal duplicated(bool ok, string path)
    signal undone(string op, bool ok)
    signal paths(var list)
    signal located(var message)
    signal trashResult(var message)
    signal permissionsResult(var message)
    signal pickerResult(var message)
    signal menuResult(var message)
    signal formatsResult(var message)
    signal redone(string op, bool ok)
    signal redoStarted(int id, int n, string op)
    signal metaResult(var message)
    property int metaToken: 0
    signal meta(int row, int w, int h, real durationMs, int sampleRate, int entries, real unpacked, bool archiveFailed, var names, real lines, bool partial, bool linesFailed, string target, bool targetDir, string owner)
    signal fsInfo(string fs, real free, string path)
    // The one line no request asked for: the directory the current listing came from changed under
    // it. path is that directory, so a pane that has since moved can ignore it; see docs/protocol.md.
    signal changed(string path)
    // readFailed tells a zero-row answer apart from an empty directory; mode is that directory's own, 0 when the stat failed too.
    // hidden is the flag the request carried, echoed by the backend: two clients peek this wire, so path alone does not say whose reply this is.
    signal peeked(string path, bool hidden, int total, var rows, bool readFailed, int mode)
    signal archiveStarted(int id)
    signal archiveDone(int id, bool ok, bool verified, string err)
    signal convertChecked(var message)
    signal convertStarted(int id, int requestId, string source)
    signal convertDone(int id, bool ok, string path, string err, int requestId, string source, bool collision)
    // The shell's exit gate: the backend has drained and this process can end.
    signal quitReady()

    // Capabilities arrive at launch and refresh at explicit menu and provider-action entry.
    // The compress submenu is exactly this list, so a box with no 7zip never shows .7z.
    property var archiveFormats: []
    property bool canConvert: false
    property var extraction: ({archive: false, sevenZip: false})
    property var providers: ({})
    property int formatsToken: 0

    readonly property bool running: child.running

    // What order the current listing is actually in, which is what ui/Header.qml's mark draws.
    // Only two things move it: list re-sorts by name ascending below, and an accepted sort, which
    // ui/js/Sort.js records here because it is the one place that knows which keys are accepted.
    property string sortBy: "name"
    property bool sortDesc: false
    property bool preserveSort: false
    property bool hasListed: false
    readonly property string sortPreference: JSON.stringify(ViewState.state.sort || {})
    onSortPreferenceChanged: if (!root.preserveSort || !root.hasListed) root.resetSort()

    function resetSort() {
        root.sortBy = (ViewState.state.sort || {}).key || "name"
        if (root.sortBy === "date") root.sortBy = "mtime"
        root.sortDesc = (ViewState.state.sort || {}).reverse === true
    }

    // What the settle gate asserts: how many thumb requests this process has attempted; see AGENTS.md.
    property int thumbRequests: 0
    // Same gate, for dirsize: a fling must issue none of these either.
    property int dirSizeRequests: 0
    // Same idiom again, for the watched re-read: a debt owed by the directory the pane has left must
    // cost the one it arrived in no listing at all, which only a count can say; see tests/ui.sh watch.
    property int listRequests: 0

    // A write before the child is spawned is dropped silently, so an early request waits here.
    property var pending: []
    property bool queueing: true

    // A quit is in flight, so the child's exit is the end the shell asked for, not a failure.
    property bool quitting: false
    // Only the portal chooser opts in: it has no filesystem write operations to drain.
    property bool pickerOnly: false

    // Everything the UI sends goes through here, so the protocol has exactly one author.
    function send(object) {
        if (root.pickerOnly && object.c !== "picker" && object.c !== "formats") return
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

    // One composition, two senders: the chooser adds the caller's filter to it and counts its own
    // replies, and issue 134 was the chooser building this by hand without the saved order in it.
    // A fresh scan is always name ascending, so a refresh after a write puts the header's mark back.
    function listRequest(path, first, hidden) {
        root.listRequests += 1
        if (!root.preserveSort || !root.hasListed) root.resetSort()
        root.hasListed = true
        return { c: "list", path: path, first: first, hidden: hidden, by: root.sortBy, desc: root.sortDesc,
                 foldersFirst: ViewState.state.foldersFirst !== false, groupByKind: ViewState.state.groupByKind === true }
    }
    function list(path, first, hidden) { root.send(root.listRequest(path, first, hidden)) }

    // A listing built from the paths named here, in that order and never sorted; see
    // docs/protocol.md "listpaths". The header's sort mark is left where the caller set it, because
    // this listing is in neither of the orders that mark can describe.
    function listPaths(paths, first) {
        root.send({ c: "listpaths", paths: paths, first: first })
    }

    function window(start, count) {
        root.send({ c: "window", start: start, count: count })
    }

    function sort(by, desc) {
        root.send({ c: "sort", by: by, desc: desc,
                    foldersFirst: ViewState.state.foldersFirst !== false,
                    groupByKind: ViewState.state.groupByKind === true })
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

    function trash(rows, menuId) {
        if (rows.length === 0) {
            return
        }
        root.send({ c: "trash", rows: rows, menuId: menuId || 0 })
    }

    function rename(path, to, menuId) {
        root.send({ c: "rename", path: path, to: to, menuId: menuId || 0 })
    }

    function duplicate(path, menuId) {
        root.send({ c: "duplicate", path: path, menuId: menuId || 0 })
    }

    // No name field: omitting it is what makes the backend take the first free "New Folder", so the
    // client needs no retry loop and no collision handling; see docs/protocol.md "mkdir".
    function mkdir(path) {
        root.send({ c: "mkdir", path: path })
    }

    function undo() {
        root.send({ c: "undo" })
    }

    function redo() { root.send({ c: "redo" }) }

    // Resolves indices to absolute paths, so a clipboard can hold a selection wider than the window.
    function askPaths(rows) {
        root.send({ c: "paths", rows: rows })
    }

    // One row, only when a surface asks: the same no-sweep rule thumb and dirsize already follow.
    // media and archive each cost a subprocess in the backend, so each is only ever true for a row
    // whose kind actually names the facts it would answer.
    function askMeta(row, text, media, archive) {
        root.metaToken += 1
        root.send({ c: "meta", row: row, text: text, media: media, archive: archive, token: root.metaToken })
        return root.metaToken
    }

    function askFsInfo() {
        root.send({ c: "fsinfo" })
    }

    // A read-only look at a directory that is not the current listing; see docs/protocol.md "peek".
    function peek(path, first, hidden) {
        root.send({ c: "peek", path: path, first: first, hidden: hidden })
    }

    // op is "peers" for the flyout's list and "send" for the transfer it chooses; both answer late.
    function localSend(op, peer, paths) {
        root.send({ c: "localsend", op: op, peer: peer, paths: paths, id: ++root.formatsToken })
    }

    function askFormats() {
        root.send({ c: "formats", id: ++root.formatsToken })
        return root.formatsToken
    }

    // paths are absolute and share a parent, which is what a selection from one listing is.
    function compress(paths, dest, format, menuId) {
        root.send({ c: "archive", op: "compress", paths: paths, dest: dest, format: format, menuId: menuId || 0 })
    }

    function extract(path, dest, menuId) {
        root.send({ c: "archive", op: "extract", path: path, dest: dest, menuId: menuId || 0 })
    }

    // No format field: magick reads the codec off dest's own extension, see docs/protocol.md "convert".
    function convertImage(path, dest, strip, menuId, requestId, check) {
        root.send({ c: "convert", path: path, dest: dest, strip: strip, menuId: menuId || 0,
                    requestId: requestId || 0, check: check === true })
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
        if (root.pickerOnly) {
            root.pending = []
            if (child.running) child.signal(9)
            else root.quitReady()
            return
        }
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
    // Sample input: {"t":"changed","path":"/home/gm/Downloads"}
    // Sample input: {"t":"searching","n":812,"scanned":41200,"ms":300.114}
    // Sample input: {"t":"transferstarted","id":12,"n":2,"moving":true}
    // Sample input: {"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000,"scanned":8400000000}
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
        Messages.route(root, message)
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
            if (root.pickerOnly && root.quitting) { child.signal(9); return }
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
                // FailedToStart has no exited signal. A chooser already cancelling still owes its
                // lifecycle an answer; a started child clears queueing and must be reaped in exited.
                if (root.pickerOnly && root.quitting) root.quitReady()
                else root.failed("backend", "", "the backend could not be started", 0)
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
