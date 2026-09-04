import QtQuick
import Quickshell
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/Places.js" as Places

// OEM-shaped Network service: only this file touches gio; Sidebar renders its entries.
Item {
    id: root

    property string bookmarksText: ""
    property var entries: []
    // Secrets live only here for this QML process lifetime; the map is never serialized or exposed.
    property var _passwords: ({})
    property string result: "idle"

    signal opened(string path)
    signal message(string text, bool isError)
    // Client-side only, see "listShares" below: ui/ShareBrowser.qml renders these as pane rows.
    signal sharesListed(string baseUri, string baseLabel, var names)
    // Fired once the write below has actually landed, so a caller's reload reads it, not stale content.
    signal renamed()
    signal retryRequested(string uri, string label, string password, string reason, bool failedConnect)

    // GVFS mount directories emit no useful inotify event here, so the rail polls.
    readonly property int mountPollMs: 5000
    // A wedged gio mount listing once froze the rail until restart, so each listing is bounded.
    readonly property int listTimeoutMs: 10000
    // Hyprland has no auth portal here, so bound a gio mount that waits on an invisible prompt.
    readonly property int mountTimeoutMs: 15000
    // The frozen helper has its own 30 s deadline; this outer bound also contains a broken test override.
    readonly property int authTimeoutSeconds: 35
    property string _mountListing: ""
    property string _pendingUri: ""
    // OEM collectors cache finished output because onExited can race their text property.
    property string _infoOutput: ""
    property string _mountListOutput: ""
    property string _listSharesOutput: ""
    property string _mountErrOutput: ""
    // Prevent onExited from reporting the timeout a second time.
    property bool _mountTimedOut: false
    property string _pendingUnmountLabel: ""
    // Queue a re-read requested mid-listing so a new mount appears without another poll interval.
    property bool _pollAgain: false
    // Keep timeout state through stream completion so empty timeout output cannot erase good rows.
    property bool _listTimedOut: false
    // The entry's own label at activation time, carried through to the sharesListed signal.
    property string _pendingLabel: ""
    property string _pendingPassword: ""
    property bool _authAwaitingStart: false

    onBookmarksTextChanged: root.rebuild()

    // The Dropbox row and Move action share this existence watch for the stock service directory.
    readonly property bool dropboxReady: dropboxFile.loaded

    FileView {
        id: dropboxFile
        path: Quickshell.env("HOME") + "/Dropbox"
        watchChanges: true
        printErrors: false
        onLoaded: root.rebuild()
        onLoadFailed: root.rebuild()
    }

    // Keep bookmark writes separate from Sidebar's read-only FileView.
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

    // A root-only remote mount covers its saved addressable paths; SMB shares remain path-specific.
    function addressMountCovers(liveUri, savedUri) {
        var live = Mounts.normalize(liveUri)
        var saved = Mounts.normalize(savedUri)
        return /^(sftp|ftp|ftps|dav|davs):\/\/[^\/]+\/$/i.test(live)
            && saved.length > live.length && saved.indexOf(live) === 0
    }

    function rebuild() {
        var home = Quickshell.env("HOME")
        var out = []
        var seen = {}
        var mounts = Mounts.parseMounts(root._mountListing)
        var marks = Mounts.nonFileBookmarks(root.bookmarksText)
        for (var i = 0; i < mounts.length; i++) {
            var covered = false
            for (var j = 0; j < marks.length; j++) {
                if (!root.addressMountCovers(mounts[i].uri, marks[j].uri)) continue
                covered = true
                var coveredKey = Mounts.normalize(marks[j].uri)
                if (seen[coveredKey]) continue
                seen[coveredKey] = true
                out.push({ path: "", label: marks[j].label, group: "network", kind: "share", uri: marks[j].uri, mountUri: mounts[i].uri, mounted: true, glyph: "server" })
            }
            if (covered) continue
            var mkey = Mounts.normalize(mounts[i].uri)
            if (seen[mkey]) continue
            seen[mkey] = true
            out.push({ path: "", label: mounts[i].label, group: "network", kind: "share", uri: mounts[i].uri, mounted: true, glyph: "server" })
        }
        for (var k = 0; k < marks.length; k++) {
            var bkey = Mounts.normalize(marks[k].uri)
            if (seen[bkey]) continue
            seen[bkey] = true
            out.push({ path: "", label: marks[k].label, group: "network", kind: "share", uri: marks[k].uri, mounted: false, glyph: "server" })
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

    function credentialed(uri) {
        return /^(smb|sftp|ftp|ftps|dav|davs):\/\/[^\/]*@/i.test(String(uri || ""))
    }

    function passwordFor(uri) {
        return root._passwords[Mounts.normalize(uri)] || ""
    }

    function remember(uri, password) {
        if (password.length === 0) return
        var next = Object.assign({}, root._passwords)
        next[Mounts.normalize(uri)] = password
        root._passwords = next
    }

    function saveLocation(uri, label, password) {
        root.remember(uri, password)
        root.openShare(uri, false, label, password.length > 0)
    }

    function openChildShare(uri, label) {
        var password = root.passwordFor(root._pendingUri)
        root.remember(uri, password)
        root.openShare(uri, false, label, password.length > 0)
    }

    function openShare(uri, alreadyMounted, label, authenticated) {
        if (mountProcess.running || authProcess.running || infoProcess.running || listSharesProcess.running) return
        if (root.result === "failed") root.message("", false)
        root._pendingUri = Mounts.normalize(uri)
        root._pendingLabel = label || ""
        if (alreadyMounted) {
            root.result = "resolving"
            root.runInfo(uri)
            return
        }
        if (authenticated === true || root.credentialed(uri)) {
            var password = root.passwordFor(uri)
            if (password.length === 0) {
                root.result = "missing-credential"
                root.retryRequested(uri, root._pendingLabel, "",
                                    "Enter the password to mount this location.", false)
                return
            }
            root._pendingPassword = password
            password = ""
            root._authAwaitingStart = true
            root.result = "mounting"
            authProcess.command = ["timeout", String(root.authTimeoutSeconds),
                                   Quickshell.env("FLEA_GIO_AUTH") || "/usr/lib/flea/flea-gio-auth", root._pendingUri]
            authProcess.running = true
            return
        }
        root.result = "mounting"
        mountProcess.command = /^smb:\/\//i.test(uri)
            ? ["gio", "mount", "--anonymous", uri] : ["gio", "mount", uri]
        root._mountErrOutput = ""
        mountProcess.running = true
        mountTimeout.restart()
    }

    function runInfo(uri) {
        infoProcess.command = ["gio", "info", uri]
        root._infoOutput = ""
        infoProcess.running = true
    }

    function failMount(reason, password) {
        root._pendingPassword = ""
        root.result = "failed"
        root.message(reason, true)
        root.retryRequested(root._pendingUri, root._pendingLabel, password || "", reason, true)
    }

    function authFailure(exitCode) {
        if (exitCode === 124) return "Connect failed: host did not respond"
        if (exitCode === 126 || exitCode === 127)
            return "Connect failed: authentication helper is unavailable"
        if (/^(ftp|ftps|dav|davs):/i.test(root._pendingUri))
            return "Connect failed: host refused the TLS handshake"
        return "Connect failed: authentication was refused"
    }

    // A server root with no share segment mounts but has no FUSE path of its own.
    function isBareRoot(uri) {
        return /^[a-z][a-z0-9+.-]*:\/\/[^\/]+\/?$/i.test(uri)
    }

    // Read gio's own result because already-mounted is not limited to bare roots.
    function isAlreadyMountedQuirk(text) {
        return /already mounted/i.test(String(text || ""))
    }

    // Client-side only, never writes bookmarks; see AGENTS.md "The share browser overlay".
    function listShares(uri) {
        listSharesProcess.command = ["gio", "list", uri]
        root._listSharesOutput = ""
        listSharesProcess.running = true
    }

    // One shared ContextMenu preserves keyboard focus; this service performs its Unmount action.
    function unmount(index) {
        var e = root.entries[index]
        if (!e || e.kind !== "share" || !e.mounted || unmountProcess.running) return
        root._pendingUnmountLabel = e.label
        unmountProcess.command = ["gio", "mount", "-u", e.mountUri || e.uri]
        unmountProcess.running = true
    }

    // Block until the rename bookmark lands so the caller's reload cannot read stale bytes.
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
            root.failMount("Connect failed: host did not respond", "")
        }
    }

    Process {
        id: authProcess
        stdinEnabled: true
        stderr: StdioCollector { waitForEnd: true }
        onStarted: {
            root._authAwaitingStart = false
            var password = root._pendingPassword
            root._pendingPassword = ""
            authProcess.write(password + "\n")
            password = ""
        }
        onRunningChanged: {
            if (root._authAwaitingStart && !authProcess.running) {
                root._authAwaitingStart = false
                root.failMount("Connect failed: authentication helper is unavailable",
                               root.passwordFor(root._pendingUri))
            }
        }
        onExited: function (exitCode) {
            root._pendingPassword = ""
            if (exitCode === 0) {
                root.runInfo(root._pendingUri)
                return
            }
            root.failMount(root.authFailure(exitCode), root.passwordFor(root._pendingUri))
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
                root.failMount("Connect failed: network location was refused", "")
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
                root.result = "mounted"
                root.opened(line.substring("local path: ".length).trim())
                return
            }
            if (root.isBareRoot(root._pendingUri)) {
                root.result = "mounted"
                root.listShares(root._pendingUri)
                return
            }
            root.failMount("Connect failed: location has no browsable folder", root.passwordFor(root._pendingUri))
        }
    }

    Process {
        id: listSharesProcess
        stdout: StdioCollector { id: listSharesOut; waitForEnd: true; onStreamFinished: root._listSharesOutput = text }
        onExited: function (exitCode) {
            var body = String(listSharesOut.text || root._listSharesOutput || "")
            var names = body.split("\n").map(function (s) { return s.trim() }).filter(function (s) { return s.length > 0 })
            if (exitCode !== 0 || names.length === 0) {
                root.failMount("Connect failed: location has no browsable folder", root.passwordFor(root._pendingUri))
                return
            }
            root.result = "mounted"
            root.sharesListed(root._pendingUri, root._pendingLabel, names)
        }
    }

    Process {
        id: unmountProcess
        onExited: function (exitCode) {
            root.pollMounts()
            root.result = exitCode === 0 ? "unmounted" : "failed"
            // Replace the arm prompt immediately after successful unmount.
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
            // Timeout output is not an empty mount list; retain the last good rail.
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
