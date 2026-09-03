import QtQuick
import Quickshell
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places

// The Network group's own Service, the OEM Dropbox panel's shape: this is the only thing here
// that touches gio or the filesystem. ui/Sidebar.qml only reads "entries" and renders it.
Item {
    id: root

    property string bookmarksText: ""
    property var entries: []

    signal opened(string path)
    signal message(string text, bool isError)
    // Client-side only, see "listShares" below: ui/ShareBrowser.qml renders these as pane rows.
    signal sharesListed(string baseUri, string baseLabel, var names)
    // Fired once the write below has actually landed, so a caller's reload reads it, not stale content.
    signal renamed()

    // Nothing filesystem-watchable tells us when a share appears or drops: a mount materialises a
    // directory under /run/user/1000/gvfs that inotify never reports an event for, measured on this box.
    readonly property int mountPollMs: 5000
    // A listing is bounded the same way a mount is below. "gio mount -l" against a share whose server
    // has stopped answering never returns, and an unbounded one froze the rail until the app restarted.
    readonly property int listTimeoutMs: 10000
    // Hyprland has no secret/auth portal here, so "gio mount" on a share that wants a
    // credential prompt hangs forever with no stdin to answer it; measured live against the real
    // NAS. This bounds that hang so the entry fails with a message instead of dying silently.
    readonly property int mountTimeoutMs: 15000
    property string _mountListing: ""
    property string _pendingUri: ""
    // A Process's own onExited can race its StdioCollector's text property, so both processes
    // below that need stdout also capture it via onStreamFinished as a fallback; see the OEM
    // Dropbox panel's Service.qml "statusStdout.text || root._statusOutput" for the same guard.
    property string _infoOutput: ""
    property string _mountListOutput: ""
    property string _listSharesOutput: ""
    property string _mountErrOutput: ""
    // Set right before mountTimeout terminates the process, so its own onExited does not also
    // report a second, redundant failure for the exact same mount attempt.
    property bool _mountTimedOut: false
    property string _pendingUnmountLabel: ""
    // A re-read asked for mid-listing used to be dropped, leaving a just-mounted share to wait out
    // the poll; this remembers it instead, and mountListProcess runs it the moment the listing ends.
    property bool _pollAgain: false
    // Cleared only when the next listing starts, never in onExited, so it still reads true while the
    // ended listing's own stream finishes and cannot overwrite the last good listing with nothing.
    property bool _listTimedOut: false
    // The entry's own label at activation time, carried through to the sharesListed signal.
    property string _pendingLabel: ""

    onBookmarksTextChanged: root.rebuild()

    // ~/Dropbox does not exist until the stock service is installed and authenticated; this
    // FileView never reads text(), it only watches the path's own existence flip.
    // The context menu's own gate for "Move to Dropbox": the same watch the sidebar row uses, because
    // ~/Dropbox does not exist until the stock service is installed and authenticated.
    readonly property bool dropboxReady: dropboxFile.loaded

    FileView {
        id: dropboxFile
        path: Quickshell.env("HOME") + "/Dropbox"
        watchChanges: true
        printErrors: false
        onLoaded: root.rebuild()
        onLoadFailed: root.rebuild()
    }

    // A second FileView on the same path as ui/Sidebar.qml's own read-only watch, the identical
    // split ui/NetworkDialog.qml already uses to write this file without fighting that watch.
    FileView {
        id: bookmarksWrite
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        printErrors: false
    }

    Timer {
        interval: root.mountPollMs
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.pollMounts()
    }

    // Three sources, deduped on the normalized uri (see ui/js/Mounts.js "normalize"): a live gio mount wins over a bookmark for the same share even when the trailing slash differs.
    function rebuild() {
        var home = Quickshell.env("HOME")
        var out = []
        var seen = {}
        var mounts = Mounts.parseMounts(root._mountListing)
        for (var i = 0; i < mounts.length; i++) {
            var mkey = Mounts.normalize(mounts[i].uri)
            if (seen[mkey]) continue
            seen[mkey] = true
            out.push({ path: "", label: mounts[i].label, group: "network", kind: "share", uri: mounts[i].uri, mounted: true, glyph: "server" })
        }
        var marks = Mounts.nonFileBookmarks(root.bookmarksText)
        for (var j = 0; j < marks.length; j++) {
            var bkey = Mounts.normalize(marks[j].uri)
            if (seen[bkey]) continue
            seen[bkey] = true
            out.push({ path: "", label: marks[j].label, group: "network", kind: "share", uri: marks[j].uri, mounted: false, glyph: "server" })
        }
        if (dropboxFile.loaded) {
            out.push({ path: home + "/Dropbox", label: "Dropbox", group: "network", kind: "dropbox", uri: "", mounted: true, glyph: "" })
        }
        // Every five seconds forever, so an unchanged poll must not assign: see Mounts.sameEntries.
        if (!Mounts.sameEntries(root.entries, out))
            root.entries = out
    }

    // A favourite's path is already real; a share needs mounting (if not live) then resolving.
    function activate(index) {
        var e = root.entries[index]
        if (!e) return
        if (e.kind === "share") {
            root.openShare(e.uri, e.mounted, e.label)
            return
        }
        root.opened(e.path)
    }

    function openShare(uri, alreadyMounted, label) {
        if (mountProcess.running || infoProcess.running) return
        root._pendingUri = uri
        root._pendingLabel = label || ""
        if (alreadyMounted) {
            root.runInfo(uri)
            return
        }
        mountProcess.command = ["gio", "mount", uri]
        mountProcess.running = true
        mountTimeout.restart()
    }

    function runInfo(uri) {
        infoProcess.command = ["gio", "info", uri]
        infoProcess.running = true
    }

    // A server root with no share segment has no FUSE path of its own; see AGENTS.md "A server
    // root with no share segment mounts, but GVFS gives it no FUSE path".
    function isBareRoot(uri) {
        return /^[a-z][a-z0-9+.-]*:\/\/[^\/]+\/?$/i.test(uri)
    }

    // Reads gio's own words rather than guessing from the uri's shape; see AGENTS.md "'Already
    // mounted' is not just a bare-root quirk" for why isBareRoot() alone was wrong here.
    function isAlreadyMountedQuirk(text) {
        return /already mounted/i.test(String(text || ""))
    }

    // Client-side only, never writes bookmarks; see AGENTS.md "The share browser overlay".
    function listShares(uri) {
        listSharesProcess.command = ["gio", "list", uri]
        listSharesProcess.running = true
    }

    // Right click unmounts directly, no confirmation popup: see AGENTS.md "A second ContextMenu
    // instance breaks the whole window's keyboard focus" for why one was tried and reverted.
    function unmount(index) {
        var e = root.entries[index]
        if (!e || e.kind !== "share" || !e.mounted || unmountProcess.running) return
        root._pendingUnmountLabel = e.label
        unmountProcess.command = ["gio", "mount", "-u", e.uri]
        unmountProcess.running = true
    }

    // Drops the bookmark line. A live mount is unmounted too, so the row leaves the rail instead
    // of turning into a mount-only leftover of the place the operator just deleted.
    function remove(index) {
        var e = root.entries[index]
        if (!e || e.kind !== "share") return
        var body = bookmarksWrite.text()
        bookmarksWrite.setText(Places.remove(body, e.uri))
        bookmarksWrite.waitForJob()
        root.renamed()
        if (e.mounted)
            root.unmount(index)
    }

    // Rewrites uri's own label, or appends a bookmark for it if it was only ever a live mount;
    // either way this is what makes the rename survive a reboot. waitForJob() blocks until the
    // write actually lands, the same fix AGENTS.md "A FileView write can race a reload" applies
    // to NetworkDialog.qml's own bookmark write.
    function rename(uri, name) {
        var body = bookmarksWrite.text()
        bookmarksWrite.setText(Places.relabel(body, uri, name))
        bookmarksWrite.waitForJob()
        root.renamed()
    }

    function pollMounts() {
        if (mountListProcess.running) {
            root._pollAgain = true
            return
        }
        root._listTimedOut = false
        mountListProcess.running = true
        listTimeout.restart()
    }

    Timer {
        id: listTimeout
        interval: root.listTimeoutMs
        repeat: false
        onTriggered: {
            if (!mountListProcess.running)
                return
            // Ending it is what lets the next poll run at all; a listing nobody can end froze the rail.
            root._listTimedOut = true
            mountListProcess.running = false
        }
    }

    Timer {
        id: mountTimeout
        interval: root.mountTimeoutMs
        repeat: false
        onTriggered: {
            if (!mountProcess.running) return
            root._mountTimedOut = true
            mountProcess.running = false
            root.message("That network location did not respond; check the address and try again.", true)
        }
    }

    Process {
        id: mountProcess
        stderr: StdioCollector { id: mountErr; waitForEnd: true; onStreamFinished: root._mountErrOutput = text }
        onExited: function (exitCode) {
            mountTimeout.stop()
            var timedOut = root._mountTimedOut
            root._mountTimedOut = false
            if (timedOut) return
            var errText = String(mountErr.text || root._mountErrOutput || "")
            // gio's own "already mounted" quirk is still worth listing; only a real failure is fatal.
            if (exitCode !== 0 && !root.isAlreadyMountedQuirk(errText)) {
                root.message("That network location could not be mounted; check the address and try again.", true)
                return
            }
            root.runInfo(root._pendingUri)
        }
    }

    // "gio info" prints a "local path: " line only for a location GVFS exposes through its FUSE mount.
    Process {
        id: infoProcess
        stdout: StdioCollector { id: infoOut; waitForEnd: true; onStreamFinished: root._infoOutput = text }
        onExited: function (exitCode) {
            root.pollMounts()
            var body = String(infoOut.text || root._infoOutput || "")
            var line = body.split("\n").find(function (l) { return l.indexOf("local path: ") === 0 })
            if (exitCode === 0 && line) {
                root.opened(line.substring("local path: ".length).trim())
                return
            }
            if (root.isBareRoot(root._pendingUri)) {
                root.listShares(root._pendingUri)
                return
            }
            root.message("This network location has no browsable folder; bookmark a specific share instead.", false)
        }
    }

    Process {
        id: listSharesProcess
        stdout: StdioCollector { id: listSharesOut; waitForEnd: true; onStreamFinished: root._listSharesOutput = text }
        onExited: function (exitCode) {
            var body = String(listSharesOut.text || root._listSharesOutput || "")
            var names = body.split("\n").map(function (s) { return s.trim() }).filter(function (s) { return s.length > 0 })
            if (exitCode !== 0 || names.length === 0) {
                root.message("This network location has no browsable folder; bookmark a specific share instead.", false)
                return
            }
            root.sharesListed(root._pendingUri, root._pendingLabel, names)
        }
    }

    Process {
        id: unmountProcess
        onExited: function (exitCode) {
            root.pollMounts()
            // A success message replaces the arm prompt, which would otherwise linger, stale,
            // for up to its own 4 s clear window with nothing on screen to say it already fired.
            root.message(exitCode === 0
                ? "Unmounted " + root._pendingUnmountLabel + "."
                : "That share could not be unmounted; it may still be in use.", exitCode !== 0)
        }
    }

    Process {
        id: mountListProcess
        command: ["gio", "mount", "-l"]
        stdout: StdioCollector { id: mountListOut; waitForEnd: true; onStreamFinished: if (!root._listTimedOut) root._mountListOutput = text }
        onExited: function () {
            listTimeout.stop()
            // A listing this timer ended collected nothing, and reading that as "no shares" would
            // empty the rail, taking the Unmount action with it exactly when a server is misbehaving.
            if (!root._listTimedOut) {
                root._mountListing = mountListOut.text || root._mountListOutput || ""
                root.rebuild()
            }
            if (root._pollAgain) {
                root._pollAgain = false
                root.pollMounts()
            }
        }
    }
}
