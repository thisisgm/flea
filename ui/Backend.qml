import Quickshell
import Quickshell.Io
import QtQuick
import "js/DropDev.js" as DropDev
import "js/Messages.js" as Messages
import "js/FolderSorts.js" as FolderSorts
import "js/Swap.js" as Swap

Item {
    id: root

    signal listed(int total, real readMs, real sortMs, string path, var changed, real gen)
    // The listing directory's filesystem, straight off the listed line: a drag compares it against
    // the dropped-on folder's own to tell a move within one volume from a copy across two.
    property var dirDev: 0
    // False when the listed directory cannot be written. A drag from it copies. Absent on an
    // older reply stays true, which is the move that reply always described.
    property bool dirWritable: true
    // The device of each ancestor of the listing by path, as the backend last answered; a path missing here is unknown and copies. See docs/protocol.md "devs".
    property var pathDevs: ({})
    // The listing the ancestors were last asked for, so a re-read or a live search answer of the same folder asks nothing.
    property string _devsFor: ""
    // The drag and the folder rows the last drop-target devs ask named, so one drag asks one set once.
    property string _dropAsked: ""
    // A drag entering a pane asks the devices of the column folders that name none, since a peek row carries none; a known answer stands until the new one lands, so a later drag never re-says Copy first.
    function askDropDevs(token, paths) {
        var key = token + "\n" + paths.join("\n")
        if (root.pickerOnly || paths.length === 0 || key === root._dropAsked) return
        root._dropAsked = key
        root.send({ c: "devs", paths: paths })
    }
    onListed: function(total, readMs, sortMs, path, changed, gen) {
        var ancestors = DropDev.askPaths(path)
        var asked = path + "\n" + root.dirDev
        if (!root.pickerOnly && ancestors.length > 0 && asked !== root._devsFor) {
            root.pathDevs = DropDev.without(root.pathDevs, ancestors)
            root.send({ c: "devs", paths: ancestors })
        }
        root._devsFor = asked
    }
    // listing is the numbering the rows are in, 0 from a backend that does not say; see docs/protocol.md "listing".
    signal rows(int start, var items, real ms, var kinds, real listing, real gen)
    // The filter's two answers, issue 315: the folder's matches as [start, length] runs, and one view window of their rows, each row carrying r, its listing row.
    signal filtered(int gen, int total, var runs, real ms, real listing)
    signal filterRows(int start, var items, var kinds, real listing, int gen)
    // The numbering of the rows the pane holds, which every row-indexed request names back; ui/PaneSwap.qml writes it.
    property real heldListing: 0
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
    signal tagStatesResult(var message)
    signal settagged(var message)
    signal searching(int total, int scanned, real ms)
    signal searched(int total, int scanned, real ms, bool cancelled, real gen, bool walking)
    // Copy/move support undo; extract uses the same activity card without an undo journal.
    signal transferStarted(int id, int n, bool moving, bool extract)
    signal transferProgress(int id, int index, string name, real bytes, real total, real scanned, string phase, string drive)
    signal transferItem(int id, int index, string name, bool ok, string err)
    // into rides only on a merge: the one merging folder, or the destination several folded into.
    signal transferDone(int id, int ok, int failed, int skipped, bool cancelled, var retryPaths, bool durable, string note, int merged, int keptBoth, string into)
    // The names a transfer would land on, asked first; ui/CollideHost.qml holds the transfer until it lands.
    signal collisions(int id, int total, var names, bool mergeable)
    // What files inside merging folders already exist, asked by one walk after Merge.
    signal mergeCollisions(int id, int total, var names)
    // The names a FAT destination cannot hold, answered in place of the collisions answer; see docs/protocol.md "fatnames".
    signal fatnames(int id, var renames, string fstype, string volume)
    // MenuAdditions040: Paste as links answers one line per request.
    signal linked(int ok, int failed, int skipped, string note)
    // MenuAdditions040: Show original reveals the link's target in its own folder.
    signal linkTarget(string path, string directory, string name, int id)
    // Linktarget ids rise forever, so a late reply never matches a new request.
    property int linkTargetSeq: 0
    function nextLinkTargetId() { return root.linkTargetSeq += 1 }
    signal trashed(int ok, int failed, string reason)
    signal renamed(bool ok, string path)
    // A remote write past its deadline answers this first and its own reply later; see docs/protocol.md "slow".
    signal slowOp(string op, string path, string msg)
    // Bulk rename answers one line for the whole batch, which one undo reverses.
    signal bulkRenamed(int ok, int unchanged)
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
    // The system clipboard for files, one line for set, get and clear alike.
    signal clipResult(var message)
    property int metaToken: 0
    // The coloured lines of one slot's file or text, answered late with the token askCode returned.
    signal codeResult(var message)
    signal htmlResult(var message)
    property int codeToken: 0
    // The unified diff of two files, or the reason there is none, answered late with the token askCompare returned.
    signal compareResult(var message)
    property int compareToken: 0
    // One counter numbers every PDF fetch, so two viewers never share an id.
    property int pdfCopySeq: 0
    signal meta(int row, int w, int h, int orient, real durationMs, int sampleRate, int entries, real unpacked, bool archiveFailed, var names, real lines, bool partial, bool linesFailed, string target, bool targetDir, string owner)
    signal shebang(string path, bool hasShebang, int id)
    // A PDF fetched into a session-private copy under a deadline; err names the refusal, stalled is the wait that ran out.
    signal pdfCopied(int id, string path, string err, bool stalled)
    // A font's facts and three sample files, or err; ui/FontPane.qml builds the request, see src/backend/fontinfo/render.rs.
    signal fontLooked(var message)
    signal fsInfo(string fs, real free, string path, string storageClass, bool tagable, bool systemVolume)
    // The one line no request asked for: the directory the current listing came from changed under
    // it. path is that directory, so a pane that has since moved can ignore it; see docs/protocol.md.
    signal changed(string path)
    // The open mount went away: parent is the nearest existing folder, so the pane moves there.
    signal unmounted(string path, string parent)
    // readFailed tells an unreadable directory from an empty one (mode 0 when its stat failed too); hidden, hiddenLast, first, by and desc echo the request, so each of the three peek clients knows its own reply.
    signal peeked(string path, bool hidden, int total, var rows, bool readFailed, int mode, bool hiddenLast, int first, var dev, var writable, string by, bool desc)
    // The path bar's folder jump, the existing folders of each source in its own order; see docs/protocol.md "jump".
    signal jumped(int id, var favourites, var zoxide, var recent, var frecency)
    signal archiveStarted(int id)
    signal archiveDone(int id, bool ok, bool verified, string err, string path, string dest)
    signal convertChecked(var message)
    signal convertStarted(int id, int requestId, string source)
    signal convertDone(int id, bool ok, string path, string err, int requestId, string source, bool collision)
    // Flash image: one image classified, every eligible drive, one flash at a time per backend.
    signal flashImageResult(var message)
    signal flashTargetsResult(var message)
    signal flashStartedLine(int id, real total, bool known)
    signal flashProgressLine(int id, string phase, real bytes, real total, real permille)
    signal flashDoneLine(var message)
    signal flashEjectLine(var message)
    // The shell's exit gate: the backend has drained and this process can end.
    signal quitReady()

    // Capabilities arrive at launch and refresh at explicit menu and provider-action entry.
    // The compress submenu is exactly this list, so a box with no 7zip never shows .7z.
    property var archiveFormats: []
    property bool canConvert: false
    property var extraction: ({archive: false, sevenZip: false, zip: false})
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
    // The path the last list request named, so a settings change re-reads that folder's own order.
    property string lastListedPath: ""
    readonly property string sortPreference: JSON.stringify(ViewState.state.sort || {})
    onSortPreferenceChanged: if (!root.preserveSort || !root.hasListed) root.resetSort(root.lastListedPath)

    function resetSort(path) {
        // Read once per listing, so browsing never writes; only Sort.resort does.
        var order = FolderSorts.orderFor(ViewState.state.folderSorts, path,
                                         ViewState.state.sort, ViewState.state.rememberSort !== false, UserDirs.newestFolders)
        root.sortBy = order.key === "date" ? "mtime" : order.key
        root.sortDesc = order.reverse === true
    }

    // Whether the Sort by flyout offers its forget row for this folder.
    function folderHasSort(path) {
        return ViewState.state.rememberSort !== false && FolderSorts.has(ViewState.state.folderSorts, path)
    }

    // A user sort writes its folder and moves it to the most recent end; past 500 the oldest goes.
    function rememberFolderSort(path, key, desc) {
        if (!FolderSorts.shouldRemember(ViewState.state.rememberSort, path))
            return
        var keyed = key === "mtime" ? "date" : key
        var leaf = {}
        leaf[path] = { key: keyed, reverse: desc === true }
        ViewState.changeMapEntries("folderSorts", leaf, FolderSorts.set(ViewState.state.folderSorts, path, keyed, desc === true))
    }

    function forgetFolderSort(path) {
        if (!path)
            return
        var leaf = {}
        leaf[path] = null
        ViewState.changeMapEntries("folderSorts", leaf, FolderSorts.forget(ViewState.state.folderSorts, path))
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

    // One of two writers: the chooser's listing worker builds its own list, listpaths and window lines.
    function send(object) {
        // The chooser opts out of writes, so a dropped command names the command it refused.
        if (root.pickerOnly && object.c !== "picker" && object.c !== "formats") { console.warn("Backend refused command " + object.c); return }
        // Every request that names rows names the numbering they were read in, its caller's if it read them earlier; src/backend/rowguard.rs refuses a stale one.
        var line = JSON.stringify(Swap.named(object, root.heldListing)) + "\n"
        if (root.queueing) {
            // A password is never parked while the child starts; it fails as its own operation, naming its archive, not a dead backend.
            if (object.password === undefined) root.pending.push(line)
            else root.failed("archive", object.path !== undefined ? object.path : object.dest, "the backend is not running", 0)
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
    function listRequest(path, first, hidden, wantChanged, select) {
        root.listRequests += 1
        root.lastListedPath = path
        if (!root.preserveSort || !root.hasListed) root.resetSort(path)
        root.hasListed = true
        return { c: "list", path: path, first: first, hidden: hidden, wantChanged: wantChanged === true, by: root.sortBy, desc: root.sortDesc,
                 foldersFirst: ViewState.state.foldersFirst !== false, groupByKind: ViewState.state.groupByKind === true,
                 hiddenLast: ViewState.state.hiddenLast === true, select: select || undefined }
    }
    function list(path, first, hidden, wantChanged, select) { root.send(root.listRequest(path, first, hidden, wantChanged, select)) }

    // A listing built from the paths named here, in that order and never sorted; see
    // docs/protocol.md "listpaths". The header's sort mark is left where the caller set it, because
    // this listing is in neither of the orders that mark can describe.
    function listPaths(paths, first) {
        root.send({ c: "listpaths", paths: paths, first: first })
    }

    function window(start, count) {
        root.send({ c: "window", start: start, count: count })
    }

    // The filter's wire, issue 315: the standing query with its gen and first view window, a later view window, and the close.
    function filter(query, gen, at, before, count) { root.send({ c: "filter", query: query, gen: gen, at: at, before: before, count: count }) }
    function filterWindow(start, count) { root.send({ c: "filterwindow", start: start, count: count }) }
    function filterClose() { root.send({ c: "filterclose" }) }

    function sort(by, desc) {
        root.send({ c: "sort", by: by, desc: desc,
                    foldersFirst: ViewState.state.foldersFirst !== false,
                    groupByKind: ViewState.state.groupByKind === true,
                    hiddenLast: ViewState.state.hiddenLast === true })
    }

    // searchopen starts the scope's walk with no rows touched; searchquery carries its gen and window count, searchclose drops the index.
    function searchOpen(scope, folder, hidden) {
        root.send({ c: "searchopen", path: scope, folder: folder, hidden: hidden })
    }

    function searchQuery(query, gen, count, sizes) {
        root.send({ c: "searchquery", query: query, gen: gen, count: count, sizes: sizes })
    }

    function searchClose() {
        root.send({ c: "searchclose" })
    }

    // Stops the session's walk and keeps its index; the standing query re-answers once.
    function searchCancel() {
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

    // Parallel paths and bare new names, validated whole before any rename.
    function bulkRename(paths, names) {
        if (paths.length === 0 || paths.length !== names.length) {
            return
        }
        root.send({ c: "bulkrename", paths: paths, names: names })
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
    // One document, only when a surface asks: the same no-sweep rule thumb and dirsize follow.
    // The id is minted here and returned, so the viewer matches the answer it asked for.
    function pdfCopy(path, slot) {
        root.pdfCopySeq += 1
        root.send({ c: "pdfcopy", id: root.pdfCopySeq, slot: slot || "", path: path })
        return root.pdfCopySeq
    }

    function askMeta(row, text, media, archive) {
        root.metaToken += 1
        root.send({ c: "meta", row: row, text: text, media: media, archive: archive, token: root.metaToken })
        return root.metaToken
    }

    // source is {path} for a file the backend opens or {text, name, lang} for bytes the UI holds; a newer ask on a slot ends the older one.
    function askCode(slot, source, first, count) {
        root.codeToken += 1
        root.send(Object.assign({ c: "code", token: root.codeToken, slot: slot, first: first, count: count }, source))
        return root.codeToken
    }

    function askHtml(slot, path, width, cancel, height) {
        root.codeToken += 1
        var message = { c: "html", token: root.codeToken, slot: slot, path: path, width: width, cancel: cancel }
        if (height > 0) message.height = height
        root.send(message)
        return root.codeToken
    }

    function askCompare(a, b) {
        root.compareToken += 1
        root.send({ c: "compare", token: root.compareToken, a: a, b: b })
        return root.compareToken
    }

    function askFsInfo() {
        root.send({ c: "fsinfo" })
    }

    // A read-only look elsewhere; Tab passes hiddenLast false, other callers keep the listing order, and only a column passes keep, the directories drawn, and its folder's own remembered order.
    function peek(path, first, hidden, hiddenLast, keep) {
        var last = hiddenLast === undefined ? ViewState.state.hiddenLast === true : hiddenLast === true
        var order = FolderSorts.peekOrder(ViewState.state.folderSorts, path, ViewState.state.rememberSort, keep, UserDirs.newestFolders, ViewState.state.sort)
        root.send({ c: "peek", path: path, first: first, hidden: hidden, hiddenLast: last, watch: Array.isArray(keep), keep: keep, by: order.by, desc: order.desc })
    }

    // op is "peers" for the flyout's list and "send" for the transfer it chooses; both answer late.
    // Once per open of the path bar: the client's favourites and recent files, joined there with zoxide; id comes back on the answer.
    function jump(id, ranking, favourites, recent) {
        var message = { c: "jump", id: id, favourites: favourites, recent: recent }
        // A whole ask names its provisional ask so the backend runs zoxide once for the open; any other ask names none.
        if (ranking !== 0) message.ranking = ranking
        root.send(message)
    }

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

    // The With-password compress: the secret rides this wire and leaves it only on 7z's stdin pipe.
    function compressPassword(paths, dest, format, password, encryptNames, menuId) {
        root.send({ c: "archive", op: "compress", paths: paths, dest: dest, format: format,
                    password: password, encryptNames: encryptNames === true, menuId: menuId || 0 })
    }

    function extract(path, dest, menuId) {
        root.send({ c: "archive", op: "extract", path: path, dest: dest, menuId: menuId || 0 })
    }

    // The extract card's retry with a password, over the same destination the first attempt named.
    function extractPassword(path, dest, password, menuId) {
        root.send({ c: "archive", op: "extract", path: path, dest: dest, password: password, menuId: menuId || 0 })
    }

    // No format field: magick reads the codec off dest's own extension, see docs/protocol.md "convert".
    function convertImage(path, dest, strip, menuId, requestId, check) {
        root.send({ c: "convert", path: path, dest: dest, strip: strip, menuId: menuId || 0,
                    requestId: requestId || 0, check: check === true })
    }

    // Flash image: the identity fields ride the flash request so the backend re-checks the drive it showed.
    function flashImage(path) {
        root.send({ c: "flashimage", path: path })
    }

    function flashTargets(image) {
        root.send({ c: "flashtargets", image: image || "" })
    }

    function flash(id, image, dev, seq, serial, bytes) {
        root.send({ c: "flash", id: id, image: image, dev: dev, seq: seq, serial: serial, bytes: bytes })
    }

    function flashCancel(id) {
        root.send({ c: "flashcancel", id: id })
    }

    function flashEject(dev, seq) {
        root.send({ c: "flasheject", dev: dev, seq: seq })
    }

    function thumb(rows, cacheOnly) {
        if (rows.length === 0) {
            return
        }
        root.thumbRequests += 1
        root.send({ c: "thumb", rows: rows, cacheOnly: cacheOnly === true })
    }

    // An empty rows cancels EVERYTHING queued, so an empty list is never sent; see docs/protocol.md.
    function thumbcancel(rows) {
        if (rows.length === 0) {
            return
        }
        root.send({ c: "thumbcancel", rows: rows })
    }

    // An eject releases the volume, so queued thumbnail and size work stops before the unmount.
    function quiesce() {
        root.send({ c: "thumbcancel", rows: [] })
        root.send({ c: "dirsizecancel" })
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
    // Sample input: {"t":"settagged","ok":2,"op":"remove"}
    // Sample input: {"t":"settagged","ok":2,"op":"add","skipped":["c.lnk"]}
    // Sample input: {"t":"tagstates","id":3,"applied":["Red"],"mixed":["Blue"],"listing":7}
    // Sample input: {"t":"searching","n":812,"scanned":41200,"ms":300.114}
    // Sample input: {"t":"changed","path":"/home/gm/Downloads"}
    // Sample input: {"t":"unmounted","path":"/media/stick","parent":"/media"}
    // Sample input: {"t":"transferstarted","id":12,"n":2,"moving":true}
    // Sample input: {"t":"transferstarted","id":12,"n":1,"moving":false,"extract":true}
    // Sample input: {"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000,"scanned":8400000000}
    // Sample input: {"t":"transferprogress","id":12,"index":0,"name":"","bytes":0,"total":0,"scanned":0,"phase":"writing","drive":"128GB"}
    // Sample input: {"t":"transferitem","id":12,"index":1,"name":"photos","ok":false,"err":"permission denied"}
    // Sample input: {"t":"transferdone","id":12,"ok":1,"failed":0,"skipped":0,"cancelled":false,"durable":false,"note":"","merged":212,"keptBoth":3,"into":"trip"}
    // Sample input: {"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false,"note":"rclone uploads them in the background"}
    // Sample input: {"t":"flashimage","path":"/home/gm/Downloads/raspios-arm64.img.xz","kind":"xz","bytes":1263537968,"unpacked":6280974336,"known":true,"member":""}
    // Sample input: {"t":"flashtargets","drives":[{"dev":"sda","seq":2,"serial":"2003007157171692","label":"128GB","model":"General USB Flash Disk","bytes":124623257600,"volumes":["128GB"],"refused":""}]}
    // Sample input: {"t":"flashstarted","id":31,"total":4294967296,"known":true}
    // Sample input: {"t":"flashprogress","id":31,"phase":"write","bytes":1283457024,"total":4294967296,"permille":299}
    // Sample input: {"t":"flashdone","id":31,"ok":true,"phase":"","at":0,"err":"","blank":false}
    // Sample input: {"t":"flasheject","ok":true,"err":""}
    // Sample input: {"t":"collisions","id":7,"total":1,"names":[{"n":"screenshot.png","d":false,"i":"image-x-generic"}],"mergeable":false}
    // Sample input: {"t":"mergecollisions","id":8,"total":3,"names":[{"n":"IMG_4120.HEIC","d":false,"i":"image-x-generic"}]}
    // Sample input: {"t":"linked","ok":1,"failed":1,"skipped":0,"note":"the link left at /home/gm/b.txt could not be removed (stale)"}
    // Sample input: {"t":"fatnames","id":7,"fstype":"exfat","volume":"128GB","renames":[{"old":"clip: x.mp4","new":"clip_ x.mp4","d":false,"i":"video-x-generic"}]}
    // Sample input: {"t":"trashed","ok":1,"failed":0}
    // Sample input: {"t":"bulkrenamed","ok":5,"unchanged":1}
    // Sample input: {"t":"made","ok":true,"path":"/home/gm/Pictures/New Folder"}
    // Sample input: {"t":"undone","op":"move","ok":true}
    // Sample input: {"t":"devs","devs":{"/":66305,"/home":66306,"/home/gm/gone":0}}
    // Sample input: {"t":"jumped","id":3,"favourites":["/home/gm/Projects"],"zoxide":["/home/gm/Documents"],"recent":[],"frecency":{"/home/gm/Documents":80},"ms":4.210}
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
            if (!root.pickerOnly) root.send({ c: "clipWatch" })
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
