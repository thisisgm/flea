import QtQuick
import Quickshell
import Quickshell.Io
import "js/Errors.js" as Errors
import "js/Mounts.js" as Mounts

// OEM-shaped Network service: nothing but this file and its two children touches gio or the saved
// places file, and Sidebar only renders its entries. The five second listing is ui/MountListing.qml's
// and the places file is ui/NetworkPlaces.qml's.
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
    // Fired once ui/NetworkPlaces.qml's write has actually landed, so a caller's reload reads it.
    signal renamed()
    signal retryRequested(string uri, string label, string password, string reason, bool failedConnect)

    // Hyprland has no auth portal here, so "gio mount" on a share that wants a credential prompt
    // hangs forever with no stdin to answer it, and "gio info" on a location gvfs cannot reach does
    // the same. Every gio leg of an open gets this bound; the helper leg has its own below.
    readonly property int mountTimeoutMs: 15000
    // The frozen helper has its own 30 s deadline; this outer bound also contains a broken test override.
    readonly property int authTimeoutSeconds: 35
    // Issue #36: every gio call this Service starts is pinned to C, so the wording of the ones it
    // reads output from (info, list, and the listing in ui/MountListing.qml) cannot be translated.
    // gvfsd composes a mount's own label and its refusals, and no client locale reaches those, which
    // is why nothing below decides anything on one.
    readonly property var gioEnvironment: ({ "LC_ALL": "C" })
    property string _mountListing: ""
    property string _pendingUri: ""
    // OEM collectors cache finished output because onExited can race their text property.
    property string _infoOutput: ""
    property string _listSharesOutput: ""
    // Set right before mountTimeout terminates that leg's process, so its own onExited does not
    // also report a second, redundant failure for the exact same open attempt.
    property bool _mountTimedOut: false
    property bool _infoTimedOut: false
    property bool _listSharesTimedOut: false
    // Set from "gio mount"'s own exit code and consumed by the "gio info" that follows it, which is
    // what actually decides whether the location is mounted; this only colours the failure message.
    property bool _mountFailed: false
    property string _pendingUnmountLabel: ""
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

    // The five second "gio mount -l" poll is ui/MountListing.qml's: this Service reads its listing
    // and asks for a re-read through pollMounts() below.
    MountListing {
        id: listing
        environment: root.gioEnvironment
        // Assign before the rebuild reads it, the order the poll always had.
        onListed: { root._mountListing = listing.text; root.rebuild() }
    }

    // The saved places file is ui/NetworkPlaces.qml's, the only writer of it in this Service.
    NetworkPlaces {
        id: places
        entries: root.entries
        bookmarksText: root.bookmarksText
        onMessage: function (text, isError) { root.message(text, isError) }
        onWrote: root.renamed()
    }

    // A root-only remote mount covers its saved addressable paths; SMB shares remain path-specific.
    function addressMountCovers(liveUri, savedUri) {
        var live = Mounts.normalize(liveUri)
        var saved = Mounts.normalize(savedUri)
        return /^(sftp|ftp|ftps|dav|davs):\/\/[^\/]+\/$/i.test(live)
            && saved.length > live.length && saved.indexOf(live) === 0
    }

    // Three sources, deduped on the normalized uri (see ui/js/Mounts.js "normalize"): a live gio mount wins over a bookmark for the same share even when the trailing slash differs.
    // The bookmark's own label wins on that merged row, or a rename of a mounted share would be written to the file and never drawn again; see ui/js/Mounts.js "railLabel".
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
            out.push({ path: "", label: Mounts.railLabel(mounts[i], marks), group: "network", kind: "share", uri: mounts[i].uri, mounted: true, glyph: "server" })
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
        // An open is single flight over four children, the share listing included, so a new one must
        // not start over the running leg and hand that leg's deadline to itself; see "listShares".
        if (mountProcess.running || authProcess.running || infoProcess.running || listSharesProcess.running) {
            // A guard that returns in silence names nothing at all, and a leg can hold it 15 s.
            root.message("Another network location is still opening; give it a moment.", false)
            return
        }
        if (root.result === "failed") root.message("", false)
        // One canonical spelling from here: tests/network-open-share.sh pins the info leg to it.
        root._pendingUri = Mounts.normalize(uri)
        root._pendingLabel = label || ""
        root._mountFailed = false
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
        mountProcess.running = true
        mountTimeout.restart()
    }

    function runInfo(uri) {
        infoProcess.command = ["gio", "info", uri]
        root._infoOutput = ""
        infoProcess.running = true
        mountTimeout.restart()
    }

    function failMount(reason, password) {
        root._pendingPassword = ""
        root.result = "failed"
        root.message(reason, true)
        root.retryRequested(root._pendingUri, root._pendingLabel, password || "", reason, true)
    }

    // A server root with no share segment mounts but has no FUSE path of its own.
    function isBareRoot(uri) {
        return /^[a-z][a-z0-9+.-]*:\/\/[^\/]+\/?$/i.test(uri)
    }

    // Client-side only, never writes bookmarks; see AGENTS.md "The share browser overlay". This is
    // the third leg of an open and takes the same bound: "gio list" on a server gvfs cannot reach
    // hangs exactly the way "gio info" does, with nothing else in the chain left to end it.
    function listShares(uri) {
        listSharesProcess.command = ["gio", "list", uri]
        root._listSharesOutput = ""
        listSharesProcess.running = true
        mountTimeout.restart()
    }

    // One shared ContextMenu preserves keyboard focus; this service performs its Unmount action.
    function unmount(index) {
        var e = root.entries[index]
        if (!e || e.kind !== "share" || !e.mounted || unmountProcess.running) return
        root._pendingUnmountLabel = e.label
        unmountProcess.command = ["gio", "mount", "-u", e.mountUri || e.uri]
        unmountProcess.running = true
    }

    // What the rail and the Processes below still call by name; the work is in the two children above.
    function rename(uri, name) { places.rename(uri, name) }
    function forget(uri) { places.forget(uri) }
    function pollMounts() { listing.poll() }

    Timer {
        id: mountTimeout
        interval: root.mountTimeoutMs
        repeat: false
        onTriggered: {
            // Whichever leg of the open is still running is the one that missed the deadline.
            if (mountProcess.running) {
                root._mountTimedOut = true
                mountProcess.running = false
            } else if (infoProcess.running) {
                root._infoTimedOut = true
                infoProcess.running = false
            } else if (listSharesProcess.running) {
                root._listSharesTimedOut = true
                listSharesProcess.running = false
            } else {
                return
            }
            // Not failMount: that offers a Retry over the rail, and the deadline's own arm in
            // tests/ui.sh presses l on the rail the instant this fires. An address that never
            // answered has no credential to correct anyway.
            root.result = "failed"
            root.message("Connect failed: host did not respond", true)
        }
    }

    // The credentialed leg: "timeout" bounds it rather than mountTimeout, so a hung helper answers
    // 124 and Errors.connectFailure names it, and the C locale above reaches the gio the helper runs.
    Process {
        id: authProcess
        environment: root.gioEnvironment
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
            root.failMount(Errors.connectFailure(exitCode, root._pendingUri), root.passwordFor(root._pendingUri))
        }
    }

    Process {
        id: mountProcess
        environment: root.gioEnvironment
        onExited: function (exitCode) {
            mountTimeout.stop()
            var timedOut = root._mountTimedOut
            root._mountTimedOut = false
            if (timedOut) return
            // A refusal for a location that is already mounted and a refusal for one that does not
            // exist differ only in a translated sentence, so neither is read: the info call below
            // answers with a FUSE path when the location really is mounted, whatever this code was.
            root._mountFailed = exitCode !== 0
            root.runInfo(root._pendingUri)
        }
    }

    // The FUSE path is ui/js/Mounts.js "localPath"'s to find, one resolver for the product and for
    // tests/js/network.js, and the C locale above is what keeps gio's own wording stable for it.
    Process {
        id: infoProcess
        environment: root.gioEnvironment
        stdout: StdioCollector { id: infoOut; waitForEnd: true; onStreamFinished: root._infoOutput = text }
        onExited: function (exitCode) {
            mountTimeout.stop()
            root.pollMounts()
            var timedOut = root._infoTimedOut
            root._infoTimedOut = false
            var failed = root._mountFailed
            root._mountFailed = false
            if (timedOut) return
            var path = Mounts.localPath(String(infoOut.text || root._infoOutput || ""))
            if (exitCode === 0 && path.length > 0) {
                root.result = "mounted"
                root.opened(path)
                return
            }
            // A server root has no FUSE path of its own, so its shares are listed instead, and the
            // exit code is not read for that: gio describes a reachable root on some servers and
            // refuses on others, and the listing that follows is what answers either way.
            if (root.isBareRoot(root._pendingUri)) {
                root.result = "mounted"
                root.listShares(root._pendingUri)
                return
            }
            if (failed) {
                root.failMount("Connect failed: network location was refused", "")
                return
            }
            root.failMount("Connect failed: location has no browsable folder", root.passwordFor(root._pendingUri))
        }
    }

    Process {
        id: listSharesProcess
        environment: root.gioEnvironment
        stdout: StdioCollector { id: listSharesOut; waitForEnd: true; onStreamFinished: root._listSharesOutput = text }
        onExited: function (exitCode) {
            mountTimeout.stop()
            var timedOut = root._listSharesTimedOut
            root._listSharesTimedOut = false
            if (timedOut) return
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
        environment: root.gioEnvironment
        onExited: function (exitCode) {
            root.pollMounts()
            root.result = exitCode === 0 ? "unmounted" : "failed"
            // Replace the arm prompt immediately after successful unmount.
            root.message(exitCode === 0
                ? "Unmounted " + root._pendingUnmountLabel + "."
                : "That share could not be unmounted; it may still be in use.", exitCode !== 0)
        }
    }
}
