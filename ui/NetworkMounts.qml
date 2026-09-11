import QtQuick
import Quickshell
import Quickshell.Io
import "js/Errors.js" as Errors
import "js/Mounts.js" as Mounts
import "js/Dropbox.js" as Dropbox

// OEM-shaped Network service: nothing but this file and its two children touches gio or the saved
// places file, and Sidebar only renders its entries. The five second listing is ui/MountListing.qml's
// and the places file is ui/NetworkPlaces.qml's.
Item {
    id: root

    property string bookmarksText: ""
    property var backend: null
    property var entries: []
    // Secrets live only here for this QML process lifetime; the map is never serialized or exposed.
    property var _passwords: ({})
    property string result: "idle"
    property Item origin: null
    property Item _pendingOrigin: null

    signal opened(string path, var origin)
    signal message(string text, bool isError)
    // Client-side only, see "listShares" below: ui/ShareBrowser.qml renders these as pane rows.
    signal sharesListed(string baseUri, string baseLabel, var names, var origin)
    // Fired once ui/NetworkPlaces.qml's write has actually landed, so a caller's reload reads it.
    signal renamed()
    signal retryRequested(string uri, string label, string password, string reason, bool failedConnect, var origin)
    signal completed(string requestId, string uri, bool success, string reason)

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
    property bool _authCancelled: false
    property string _requestId: ""
    property string _requestPassword: ""

    onBookmarksTextChanged: root.rebuild()

    property string dropboxPath: ""
    onDropboxPathChanged: root.rebuild()
    property string dropboxReason: "Checking Dropbox"
    property bool dropboxChecking: false
    property bool _dropboxAwaitingStart: false
    property string _dropboxOutput: ""
    property string _dropboxError: ""
    // OEM dropbox/status.py uses a four-second daemon status deadline.
    readonly property int dropboxStatusTimeoutSeconds: 4
    signal dropboxRefreshed()
    readonly property bool dropboxReady: dropboxPath.length > 0 && dropboxReason.length === 0 && !dropboxChecking
    property int _dropboxMetadataRequest: 0
    property bool _dropboxMetadataAgain: false

    function readDropboxAccount(facts) {
        if (!facts || facts.dropboxInfo === undefined) return
        var account = Dropbox.account(facts.dropboxInfo, facts.dropboxError)
        if (dropboxPath !== account.path || account.reason)
            dropboxReason = account.reason || "Checking Dropbox"
        dropboxPath = account.path
    }

    function refreshDropboxAccount() {
        if (!backend) return
        if (_dropboxMetadataRequest) { _dropboxMetadataAgain = true; return }
        _dropboxMetadataRequest = backend.askFormats()
    }

    function rearmDropboxAccount() {
        dropboxAccountFile.path = Quickshell.env("HOME") + "/.dropbox/info.json"
        root.refreshDropboxAccount()
    }

    onBackendChanged: if (backend) root.readDropboxAccount(backend.providers)
    Connections {
        target: root.backend
        function onProvidersChanged() { root.readDropboxAccount(root.backend.providers) }
        function onFormatsResult(message) {
            if (message.id !== root._dropboxMetadataRequest) return
            root._dropboxMetadataRequest = 0
            if (root._dropboxMetadataAgain) {
                root._dropboxMetadataAgain = false
                root.refreshDropboxAccount()
            }
        }
    }

    function refreshDropbox(facts) {
        if (dropboxChecking) return false
        var provider = facts.dropbox || {}
        root.readDropboxAccount(facts)
        dropboxReason = provider.reason || (dropboxPath ? "Checking Dropbox" : dropboxReason)
        if (!provider.command || !dropboxPath) return true
        dropboxChecking = true
        _dropboxAwaitingStart = true
        _dropboxOutput = ""
        _dropboxError = ""
        dropboxStatus.command = ["timeout", "--signal=KILL", String(dropboxStatusTimeoutSeconds), provider.command, "status"]
        dropboxStatus.running = true
        return true
    }

    Process {
        id: dropboxStatus
        environment: root.gioEnvironment
        stdout: StdioCollector { id: dropboxOut; waitForEnd: true; onStreamFinished: root._dropboxOutput = text }
        stderr: StdioCollector { id: dropboxErr; waitForEnd: true; onStreamFinished: root._dropboxError = text }
        onStarted: root._dropboxAwaitingStart = false
        onRunningChanged: {
            if (root._dropboxAwaitingStart && !running) {
                root._dropboxAwaitingStart = false
                root.dropboxChecking = false
                root.dropboxReason = "Dropbox status helper could not start"
                root.dropboxRefreshed()
            }
        }
        onExited: function(exitCode) {
            root._dropboxAwaitingStart = false
            root.dropboxReason = exitCode === 137 || exitCode === 9 ? "Dropbox status was interrupted or timed out"
                : Dropbox.status(dropboxOut.text || root._dropboxOutput, exitCode, dropboxErr.text || root._dropboxError)
            root.dropboxChecking = false
            root.dropboxRefreshed()
        }
    }

    FileView {
        id: dropboxAccountFile
        path: Quickshell.env("HOME") + "/.dropbox/info.json"
        // Watch only; providers.rs owns the regular-file check and bounded metadata read.
        preload: false
        watchChanges: true
        printErrors: false
        onFileChanged: Qt.callLater(root.refreshDropboxAccount)
    }

    FileView {
        // A FileView directory watch does not report its own removal; its parent does.
        path: Quickshell.env("HOME")
        preload: false
        watchChanges: true
        printErrors: false
        onFileChanged: {
            // FileView must observe the empty path for an event turn before it can re-arm a new parent.
            dropboxAccountFile.path = ""
            Qt.callLater(root.rearmDropboxAccount)
        }
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
        if (root.dropboxPath.length > 0) {
            out.push({ path: root.dropboxPath, label: "Dropbox", group: "network", kind: "dropbox", uri: "", mounted: true, glyph: "" })
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
        root.opened(e.path, root.origin)
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

    // forgetPassword, not forget: forget(uri) below is the bookmark writer, and one name for both
    // would have made a refused connect delete the saved place.
    function forgetPassword(uri) {
        var key = Mounts.normalize(uri)
        if (root._passwords[key] === undefined) return
        var next = Object.assign({}, root._passwords)
        delete next[key]
        root._passwords = next
    }

    function saveLocation(uri, label, password, requestId, origin) {
        return root.openShare(uri, false, label, password.length > 0, { id: requestId, password: password, origin: origin })
    }

    function openChildShare(uri, label, origin) {
        var password = root.passwordFor(root._pendingUri)
        root.remember(uri, password)
        root.openShare(uri, false, label, password.length > 0, { origin: origin })
    }

    function openShare(uri, alreadyMounted, label, authenticated, request) {
        request = request || ({})
        // An open is single flight over four children, the share listing included, so a new one must
        // not start over the running leg and hand that leg's deadline to itself; see "listShares".
        if (mountProcess.running || authProcess.running || infoProcess.running || listSharesProcess.running) {
            // A guard that returns in silence names nothing at all, and a leg can hold it 15 s.
            var reason = "Another network location is still opening; give it a moment."
            if (request.id) root.completed(request.id, Mounts.normalize(uri), false, reason)
            else root.message(reason, false)
            return
        }
        if (root.result === "failed") root.message("", false)
        // One canonical spelling from here: tests/network-open-share.sh pins the info leg to it.
        root._pendingUri = Mounts.normalize(uri)
        root._pendingLabel = label || ""
        root._requestId = request.id || ""
        root._requestPassword = request.password || ""
        root._pendingOrigin = request.origin === undefined ? root.origin : request.origin
        root._mountFailed = false
        if (alreadyMounted) {
            root.result = "resolving"
            root.runInfo(uri)
            return
        }
        if (authenticated === true || root.credentialed(uri)) {
            var password = request.password || root.passwordFor(uri)
            if (password.length === 0) {
                root.result = "missing-credential"
                var reason = "Enter the password to mount this location."
                if (!root.finishRequest(false, reason))
                    root.retryRequested(uri, root._pendingLabel, "", reason, false, root._pendingOrigin)
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

    // refused says the server itself turned the credential down. Only that invalidates it: a helper
    // that could not start and a host that never answered say nothing about the password, and the
    // reopened dialog has to be populated from it exactly as 0.1.6 populated it.
    function failMount(reason, password, refused) {
        root._pendingPassword = ""
        root.result = "failed"
        root.message(reason, true)
        var attempted = password || root._requestPassword
        // Keeping a refused secret meant every later click on that location replayed it with no
        // prompt at all, which is how a domain account locks itself out.
        if (refused === true) root.forgetPassword(root._pendingUri)
        else if (attempted.length > 0) root.remember(root._pendingUri, attempted)
        if (!root.finishRequest(false, reason))
            root.retryRequested(root._pendingUri, root._pendingLabel, attempted, reason, true, root._pendingOrigin)
    }

    function finishRequest(success, reason) {
        var requestId = root._requestId
        if (!requestId) return false
        root._requestId = ""
        if (success) root.remember(root._pendingUri, root._requestPassword)
        root._requestPassword = ""
        root.completed(requestId, root._pendingUri, success, reason || "")
        return true
    }

    function cancelLocation(requestId) {
        if (!requestId || requestId !== root._requestId) return
        root._requestId = ""
        root._requestPassword = ""
        root._pendingPassword = ""
        root._authAwaitingStart = false
        mountTimeout.stop()
        if (mountProcess.running) { root._mountTimedOut = true; mountProcess.running = false }
        if (infoProcess.running) { root._infoTimedOut = true; infoProcess.running = false }
        if (listSharesProcess.running) { root._listSharesTimedOut = true; listSharesProcess.running = false }
        if (authProcess.running) { root._authCancelled = true; authProcess.running = false }
        root.result = "cancelled"
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
            root.finishRequest(false, "Connect failed: host did not respond")
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
            if (root._authCancelled) { root._authCancelled = false; return }
            root._pendingPassword = ""
            if (exitCode === 0) {
                root.runInfo(root._pendingUri)
                return
            }
            // 124 is a host that never answered and 126/127 a helper that could not start; neither
            // is the server turning the credential down, so neither invalidates it.
            var refused = exitCode !== 124 && exitCode !== 126 && exitCode !== 127
            root.failMount(Errors.connectFailure(exitCode, root._pendingUri),
                           root.passwordFor(root._pendingUri), refused)
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
                root.finishRequest(true, "")
                root.opened(path, root._pendingOrigin)
                return
            }
            // A server root has no FUSE path of its own, so its shares are listed instead, and the
            // exit code is not read for that: gio describes a reachable root on some servers and
            // refuses on others, and the listing that follows is what answers either way.
            if (root.isBareRoot(root._pendingUri)) {
                root.result = "resolving"
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
            root.finishRequest(true, "")
            root.sharesListed(root._pendingUri, root._pendingLabel, names, root._pendingOrigin)
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
