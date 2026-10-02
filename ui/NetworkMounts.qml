import QtQuick
import Quickshell
import Quickshell.Io
import "js/Errors.js" as Errors
import "js/Cloud.js" as Cloud
import "js/Mounts.js" as Mounts
import "js/Protocols.js" as Protocols
import "js/Dropbox.js" as Dropbox

// OEM-shaped Network service: nothing but this file, its two children and ui/PhoneMounts.qml, which
// only unmounts off this listing, touches gio or the saved places file; Sidebar renders their rows.
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
    // A FUSE path that is a file, not a folder: the opener takes it, so a typed network URL
    // naming a file is never listed as a folder.
    signal openFileRequested(string path, var origin)
    signal message(string text, bool isError)
    // The bridge wait's own sticky line, cleared with "" when the folder lands or fails.
    signal sticky(string text, var origin)
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
    // What the rail waits for before it draws NETWORK: the shared gio listing's own answer, timed out or not.
    readonly property bool listingAnswered: listing.answered
    // ui/PhoneMounts.qml builds its rows off the same five second poll rather than walking the gvfs volume monitors a second time.
    readonly property alias mountListing: root._mountListing
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
    // Issue 194's repair: gio info answered with a folder that is not the requested share.
    // Its own path is peeked first, then the FUSE root it names, all through backend.peek,
    // so no new poll is added and a second open waits on the same single-flight guard.
    // Only a root that peeks empty or unreadable starts the bridge for the gvfs root, then
    // peeks once under the same deadline; the #194 folder itself does not exist, so waiting
    // on it first costs 15 s before failing. _repairWaiting covers that bridge leg,
    // _repairActive the peek legs, _repairBridged the one bridge start a repair may spend.
    property bool _repairWaiting: false
    property string _repairUri: ""
    property string _repairPath: ""
    property string _repairRoot: ""
    property bool _repairFailed: false
    property bool _repairActive: false
    property bool _repairBridged: false

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
    // Any finished status check answers, whatever it said; the providers' determination is only the startup answer.
    onDropboxRefreshed: root.dropboxAnswered = true
    // What the rail waits for from Dropbox: the providers' determination, or a finished status check.
    property bool dropboxAnswered: false
    readonly property bool dropboxReady: dropboxPath.length > 0 && dropboxReason.length === 0 && !dropboxChecking
    property int _dropboxMetadataRequest: 0
    property bool _dropboxMetadataAgain: false

    function readDropboxAccount(facts) {
        if (!facts || facts.dropboxInfo === undefined) return
        root.dropboxAnswered = true
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
        // Issue 194's repair answers here. The chrome answers only the Tab request its own key
        // names and the columns view only ever reads the ancestors it asked for, so sharing the
        // peek wire costs those readers nothing they would otherwise notice.
        function onPeeked(path, hidden, total, rows, readFailed, mode) { root.repairPeeked(path, rows, readFailed) }
    }

    function refreshDropbox(facts) {
        if (dropboxChecking) return false
        var provider = facts.dropbox || {}
        root.readDropboxAccount(facts)
        dropboxReason = provider.reason || (dropboxPath ? "Checking Dropbox" : dropboxReason)
        if (!provider.command || !dropboxPath) { root.dropboxAnswered = true; return true }
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

    // The five second "gio mount -li" poll is ui/MountListing.qml's: this Service reads its listing and asks for a re-read through pollMounts() below.
    // How many rails are loaded now; an open or a bridge wait also holds the poll, so a
    // hidden rail costs no gio while a mount in flight still refreshes behind it.
    property int _rails: 0
    // The listing's timer polls on start when this arrival activates it, so only an already active one is asked again.
    function railArrived() { var wasActive = listing.active; root._rails++; if (wasActive) root.pollMounts() }
    function railLeft() { root._rails = Math.max(0, root._rails - 1) }
    MountListing {
        id: listing
        environment: root.gioEnvironment
        active: root._rails > 0 || mountProcess.running || authProcess.running || infoProcess.running
            || listSharesProcess.running || root._repairActive || root._repairWaiting || root.bridgeWaiting
        // Cloud rows ride the same five second rhythm: the mount table is a file read, so
        // nothing new runs. The read is asynchronous: /proc has no write of ours to wait for,
        // and a blocking read every tick stalled the GUI thread mid-scroll.
        onListed: { root._polledListing = listing.text; root._listedOnce = true; cloudFile.reload() }
    }

    // The last texts a rebuild parsed; see ui/js/Mounts.js pollDecision for the change gate.
    property string _polledListing: ""
    property bool _listedOnce: false
    property string _lastMountListing: ""
    property string _lastMountinfo: ""
    function pollAnswered() {
        var infoText = cloudFile.text()
        if (Mounts.pollDecision(root._listedOnce, root._polledListing, root._lastMountListing, infoText, root._lastMountinfo) !== "rebuild")
            return
        root._lastMountListing = root._polledListing
        root._lastMountinfo = infoText
        root._mountListing = root._polledListing
        root.rebuild()
    }

    // /proc/self/mountinfo, read on the listing's own poll for ui/js/Cloud.js's FUSE rows.
    // A FileView watch is not set up: proc files report no change events, so the poll drives it.
    FileView {
        id: cloudFile
        path: "/proc/self/mountinfo"
        printErrors: false
        onLoaded: root.pollAnswered()
        onLoadFailed: root.pollAnswered()
    }

    // The saved places file is ui/NetworkPlaces.qml's, the only writer of it in this Service.
    NetworkPlaces {
        id: places
        entries: root.entries
        onMessage: function (text, isError) { root.message(text, isError) }
        onWrote: root.renamed()
    }

    // Phones and shares open through the GVFS FUSE bridge, hosted window-long rather than
    // in the rail: hiding the rail mid-wait kills no wait, and the ready, the failure and the
    // Starting line it showed all still land. Files land on openFileRequested, so a typed
    // network URL naming a file is never listed as a folder.
    Loader {
        id: bridgeLoader
        active: false // built on first ensure, so launch pays no Timers, Process or compile
        source: "GvfsBridge.qml"
    }
    // Null until the first ensure; every reader below guards it.
    readonly property var bridge: bridgeLoader.item
    readonly property bool bridgeBuilt: bridgeLoader.active
    // Whether a bridge wait is in flight, so the listing poll holds while one stands.
    readonly property bool bridgeWaiting: root.bridge !== null && root.bridge.flow.waiter !== null
    // Synchronous: a local source: URL answers item on the same call that sets active.
    function ensureBridge() {
        if (!bridgeLoader.active)
            bridgeLoader.active = true
        return bridgeLoader.item
    }
    // A null bridge is a missing component, not a refused mount, so it names the file it failed to build.
    function bridgeMissingReason() {
        return "Could not open " + (root._pendingLabel || root._pendingUri) + ": GvfsBridge.qml did not load"
    }
    // The Loader status is the cause, so it is warned with the file named and the wait ends with no retry.
    function failBridgeMissing() {
        console.warn("GvfsBridge.qml did not load: Loader status " + bridgeLoader.status)
        if (root._requestPassword.length > 0) root.remember(root._pendingUri, root._requestPassword)
        mountTimeout.stop()
        root._repairActive = false
        root._repairWaiting = false
        root._repairBridged = false
        root._repairRoot = ""
        root._repairPath = ""
        root.result = "failed"
        var reason = root.bridgeMissingReason()
        root.message(reason, true)
        root.finishRequest(false, reason)
    }
    Connections {
        target: root.bridge
        function onReady(path, origin, isDir) {
            if (root._repairWaiting) {
                root._repairWaiting = false
                root.sticky("", origin)
                root._repairActive = true
                mountTimeout.restart()
                root.backend.peek(root._repairRoot, 512, false)
                return
            }
            root.sticky("", origin)
            if (!isDir) {
                root.openFileRequested(path, origin)
                return
            }
            root.opened(path, origin)
        }
        function onStarting(text, origin) { root.sticky(text, origin) }
        function onFailed(text, origin) {
            if (root._repairWaiting) {
                root._repairWaiting = false
                root.sticky("", origin)
                root.repairFailed()
                return
            }
            root.result = "failed"
            root.sticky("", origin)
            root.message(text, true)
        }
        // The bridge's busy refusal reaches a repair the same way: a second bridge is never
        // started, so the repair ends instead of waiting on a wait it did not start.
        function onNotice(text, origin) {
            if (root._repairWaiting) {
                root._repairWaiting = false
                root.sticky("", origin)
                root.repairFailed()
                return
            }
            root.message(text, false)
        }
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
                if (!Mounts.addressMountCovers(mounts[i].uri, marks[j].uri)) continue
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
        // CloudMounts: a FUSE mount inside the home folder shows under its folder's name,
        // the way gio lists its own mounts. No menu at defaults, like Dropbox's row: the
        // tool that made the mount owns it, so Flea never mounts, unmounts or configures it.
        var clouds = Cloud.parseCloudMounts(cloudFile.text(), Quickshell.env("HOME"))
        for (var c = 0; c < clouds.length; c++) {
            out.push({ path: clouds[c].path, label: Mounts.leaf(clouds[c].path), group: "network",
                       kind: "cloud", uri: "", mounted: true, glyph: "server" })
        }
        // Every five seconds forever, so an unchanged poll must not assign: see Mounts.sameEntries.
        if (!Mounts.sameEntries(root.entries, out))
            root.entries = out
    }

    // A favourite's path is already real; a share needs mounting (if not live) then resolving.
    // A cloud row's path is already real too: its mount is the tool that made it, not Flea's.
    // origin rides along explicitly, because this service outlives the rail that renders it and
    // the rail's own origin is gone by the time a hidden-rail answer lands.
    function activate(index, origin) {
        var e = root.entries[index]
        if (!e) return
        if (e.kind === "cloud") {
            root.opened(e.path, origin === undefined ? root.origin : origin)
            return
        }
        if (e.kind === "share") {
            root.openShare(e.uri, e.mounted, e.label, false, { origin: origin === undefined ? root.origin : origin })
            return
        }
        root.opened(e.path, origin === undefined ? root.origin : origin)
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
        // An open is single flight over four children, the bridge wait and the repair peek,
        // the share listing included, so a new one must not start over the running leg and hand
        // that leg's deadline to itself; see "listShares". A refusal names the moment and
        // touches no flight state.
        if (mountProcess.running || authProcess.running || infoProcess.running || listSharesProcess.running || root._repairActive || root._repairWaiting) {
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
        var password = request.password || root.passwordFor(uri)
        // A remembered password is a fallback for sftp, never a first resort: Mounts.keyless.
        if (authenticated === true || (!Mounts.keyless(uri)
                && (password.length > 0 || Mounts.credentialed(uri)))) {
            if (password.length === 0) {
                root.result = "missing-credential"
                if (!root.finishRequest(false, "Enter the password to mount this location."))
                    root.retryRequested(uri, root._pendingLabel, "", "Enter the password to mount this location.", false, root._pendingOrigin)
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
    function failMount(reason, password, refused, missing) {
        root._pendingPassword = ""
        root.result = missing === true ? "missing-credential" : "failed"
        root.message(reason, true)
        var attempted = password || root._requestPassword
        // Keeping a refused secret meant every later click on that location replayed it with no
        // prompt at all, which is how a domain account locks itself out.
        if (refused === true) root.forgetPassword(root._pendingUri)
        else if (attempted.length > 0) root.remember(root._pendingUri, attempted)
        if (!root.finishRequest(false, reason))
            root.retryRequested(root._pendingUri, root._pendingLabel, attempted, reason,
                                missing !== true, root._pendingOrigin)
    }

    // Only when nothing in the FUSE root answers for the request: the mount fails with the
    // reason gio gave, never as an unreadable directory.
    function repairFailed() {
        var uri = root._repairUri
        var refused = root._repairFailed
        root._repairActive = false
        root._repairWaiting = false
        root._repairBridged = false
        root._repairRoot = ""
        root._repairPath = ""
        if (refused) root.failMount("Connect failed: network location was refused", root.passwordFor(uri))
        else root.failMount("Connect failed: location has no browsable folder", root.passwordFor(uri))
    }

    // Issue 194's repair, answered through backend.peek: gio's own folder first, then every
    // entry of the FUSE root it names, matched by ui/js/Protocols.js smbResolve. A real
    // directory always opens; several users prefer the desktop login and otherwise name
    // themselves; anything else is the mount failing with the reason gio gave. Only a root
    // that peeks empty or unreadable starts the bridge for the gvfs root, then peeks that
    // root once under the same deadline.
    function repairPeeked(path, rows, readFailed) {
        if (!root._repairActive) return
        if (path === root._repairPath) {
            if (!readFailed) {
                var openPath = root._repairPath
                mountTimeout.stop()
                root._repairActive = false
                root._repairBridged = false
                root._repairPath = ""
                root.result = "mounted"
                root.finishRequest(true, "")
                root.opened(openPath, root._pendingOrigin)
                return
            }
            var cut = root._repairPath.indexOf("/gvfs/")
            if (cut < 0 || !root.backend) {
                mountTimeout.stop()
                root.repairFailed()
                return
            }
            root._repairRoot = root._repairPath.substring(0, cut + 5)
            mountTimeout.restart()
            root.backend.peek(root._repairRoot, 512, false)
            return
        }
        if (root._repairRoot.length === 0 || path !== root._repairRoot) return
        // The bridge down reads as an empty root, so only then does the repair spend its one
        // bridge start; a root that answered with entries but no match fails without one.
        if ((readFailed || rows.length === 0) && !root._repairBridged && root.backend) {
            root._repairBridged = true
            root._repairWaiting = true
            mountTimeout.restart()
            var repairBridge = root.ensureBridge()
            if (repairBridge)
                repairBridge.ensure(root._repairRoot, root._pendingLabel, root._pendingOrigin)
            else
                root.failBridgeMissing()
            return
        }
        mountTimeout.stop()
        var candidates = []
        for (var i = 0; i < rows.length; i++) candidates.push(root._repairRoot + "/" + rows[i].n)
        var login = Quickshell.env("USER") || Quickshell.env("LOGNAME") || ""
        var picked = Protocols.smbResolve(root._repairUri, candidates, login)
        root._repairActive = false
        root._repairBridged = false
        root._repairRoot = ""
        root._repairPath = ""
        if (picked.path.length > 0) {
            root.result = "mounted"
            root.finishRequest(true, "")
            root.opened(picked.path, root._pendingOrigin)
            return
        }
        if (picked.users.length > 0) {
            root.failMount("Connect failed: that share is mounted as " + picked.users.join(", "),
                           root.passwordFor(root._repairUri))
            return
        }
        root.repairFailed()
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
        var origin = root._pendingOrigin
        root._requestId = ""
        root._requestPassword = ""
        root._pendingPassword = ""
        root._authAwaitingStart = false
        root._repairActive = false
        root._repairWaiting = false
        root._repairBridged = false
        // Only this flight's own bridge waiter ends; another flight's wait is never cancelled,
        // and with no bridge built no wait can stand.
        if (root.bridge)
            root.bridge.cancelFor(origin)
        root.sticky("", origin)
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
    function replacePlace(oldUri) { places.replace(oldUri, root._pendingUri, root._pendingLabel) }
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
            } else if (root._repairActive) {
                root._repairActive = false
                root._repairWaiting = false
                root._repairBridged = false
                root._repairRoot = ""
                root._repairPath = ""
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
            root.failMount(Errors.connectFailure(exitCode),
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
            if (exitCode === 0 && path.length > 0 && root.backend && Protocols.schemeOf(root._pendingUri) === "smb"
                    && !Protocols.smbFuseMatches(root._pendingUri, path)) {
                // Issue 194: gio answered with a folder that is not this share (a user its line
                // drops above all). Whether it exists is the backend's own answer to say: the
                // folder peeks first, then the FUSE root, and only an empty or unreadable root
                // starts the bridge for the gvfs root under the same deadline.
                root._repairUri = root._pendingUri
                root._repairPath = path
                root._repairFailed = failed
                root._repairRoot = ""
                root._repairWaiting = false
                root._repairBridged = false
                root._repairActive = true
                mountTimeout.restart()
                root.backend.peek(path, 1, false)
                return
            }
            if (exitCode === 0 && path.length > 0) {
                var openBridge = root.ensureBridge()
                if (!openBridge) {
                    root.failBridgeMissing()
                    return
                }
                root.result = "mounted"
                root.finishRequest(true, "")
                openBridge.ensure(path, root._pendingLabel, root._pendingOrigin)
                return
            }
            // A refused keyless sftp attempt is a missing credential and not a refused location, sftp only.
            if (failed && Mounts.keyless(root._pendingUri) && Mounts.credentialed(root._pendingUri)) {
                root.failMount("Enter the password to mount this location.",
                               root.passwordFor(root._pendingUri), false, true)
                return
            }
            // A server root lists its shares whatever the exit code said, so this stays ahead of the refusal.
            if (root.isBareRoot(root._pendingUri)) {
                root.result = "resolving"
                root.listShares(root._pendingUri)
                return
            }
            if (failed) {
                root.failMount("Connect failed: network location was refused",
                               root.passwordFor(root._pendingUri))
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
