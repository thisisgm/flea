import QtQuick
import Quickshell.Io
import "js/Mounts.js" as Mounts
import "js/Eject.js" as Eject

// The DEVICES group's own Service, the shape ui/NetworkMounts.qml has for NETWORK: the only thing
// here that runs a process, while ui/Sidebar.qml reads "entries" and renders it. Enumeration is
// lsblk, which any user may run; acting on a volume is gio, the tool the shares already drive.
Item {
    id: root

    property var entries: []

    signal opened(string path)
    signal message(string text, bool isError)

    // lsblk costs 5 ms on this box where gio mount -l costs 513 ms, so the rail's own five second
    // rhythm carries this too rather than earning a slower clock of its own.
    readonly property int pollMs: 5000
    // A listing is bounded the same way a mount is below, and for the same reason ui/NetworkMounts.qml
    // bounds its own: an lsblk that never answers left poll() refusing forever and froze the rail.
    readonly property int listTimeoutMs: 10000
    // A mount that never grows a mountpoint has to stop waiting, or the row waits on the poll forever.
    readonly property int mountTimeoutMs: 15000
    // How long an eject waits for a listing it can judge after gio exits, before the user is told
    // that nothing was confirmed: three polls, the same bound the mount above gets.
    readonly property int ejectVerdictMs: 15000

    property string _listing: ""
    // A Process's own onExited can race its StdioCollector's text property, the same guard every
    // stdout-reading process in ui/NetworkMounts.qml already carries.
    property string _listOutput: ""
    // The device a mount was asked for; rebuild() opens it the moment lsblk reports its mountpoint.
    property string _pendingOpenDevice: ""
    property string _pendingOpenLabel: ""
    // The device whose eject awaits its verdict, "" when none. The listing taken after gio exits is
    // the only witness: gio's own exit code has been 0 over a volume that was still mounted.
    property string _ejectDevice: ""
    property string _ejectLabel: ""
    // Listings are counted as they start. _verdictFromListing is 0 while gio runs, so nothing is
    // judged before it exits; at exit it becomes one past the count, so the listing in flight at
    // that moment, taken before the eject finished, is never the witness.
    property int _listingsStarted: 0
    property int _verdictFromListing: 0
    // poll() refuses to start a listing until the previous one's stream has finished, so the body a
    // verdict is read from and the count it is gated on always belong to the same run.
    property bool _streamPending: false
    // Cleared only when the next listing starts, which onExited's own release of _streamPending
    // guarantees cannot happen until the ended listing is fully done with.
    property bool _listTimedOut: false

    // The internal disk row reads "<host> · <kernel name>" per the canvas, and /etc/hostname is the
    // one source for that host name that costs no process.
    FileView {
        id: hostnameFile
        path: "/etc/hostname"
        blockLoading: true
        printErrors: false
        onLoaded: root.rebuild()
        onLoadFailed: root.rebuild()
    }

    Timer {
        interval: root.pollMs
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.poll()
    }

    function poll() {
        if (listProcess.running || root._streamPending)
            return
        root._listTimedOut = false
        root._streamPending = true
        root._listingsStarted += 1
        // lsblk can legitimately return an empty tree. Never rebuild from the previous poll.
        root._listOutput = ""
        listProcess.running = true
        listTimeout.restart()
    }

    Timer {
        id: listTimeout
        interval: root.listTimeoutMs
        repeat: false
        onTriggered: {
            if (!listProcess.running)
                return
            // Ending it is what lets the next poll run at all; a listing nobody can end froze the rail.
            root._listTimedOut = true
            listProcess.running = false
        }
    }

    function hostPrefix() {
        var host = String(hostnameFile.text() || "").trim()
        return host.length > 0 ? host + " · " : ""
    }

    function rebuild() {
        var rows = Mounts.parseDevices(root._listing)
        var out = []
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            var label = r.kind === "disk" ? root.hostPrefix() + r.label : r.label
            out.push({ path: r.path, label: label, group: "device", kind: r.kind,
                       device: r.device, mounted: r.mounted, glyph: "drive" })
        }
        // Same rule as ui/NetworkMounts.qml's: an unchanged poll assigns nothing, see Mounts.sameEntries.
        if (!Mounts.sameEntries(root.entries, out))
            root.entries = out
        root.openWhenMounted()
    }

    // gio returns before udisks has published the mountpoint, so the poll after it is what
    // resolves the path; this is the one place a just-mounted volume is opened.
    function openWhenMounted() {
        if (root._pendingOpenDevice.length === 0)
            return
        for (var i = 0; i < root.entries.length; i++) {
            var e = root.entries[i]
            if (e.device === root._pendingOpenDevice && e.mounted) {
                root._pendingOpenDevice = ""
                mountTimeout.stop()
                root.opened(e.path)
                return
            }
        }
    }

    // The disk row and a mounted volume open directly; an unmounted volume mounts first, then
    // opens, because a user who picks a disk means to look inside it.
    function activate(index) {
        var e = root.entries[index]
        if (!e)
            return
        if (e.mounted) {
            root.opened(e.path)
            return
        }
        root.mountVolume(e)
    }

    function mountVolume(e) {
        if (mountProcess.running)
            return
        root._pendingOpenDevice = e.device
        root._pendingOpenLabel = e.label
        mountProcess.command = ["gio", "mount", "-d", e.device]
        mountProcess.running = true
        mountTimeout.restart()
    }

    // Eject goes through the mount point. gio mount dispatches on --device before it ever reads
    // --eject (glib 2.88.3 gio-tool-mount.c:1264 against :1278), so "-e -d <device>" mounts instead.
    // gio's own -f is never passed: forcing an unmount over an open write is how a file manager
    // loses somebody's data.
    function eject(index) {
        var e = root.entries[index]
        if (!e || e.kind !== "volume" || !e.mounted)
            return
        if (ejectProcess.running || root._ejectDevice.length > 0) {
            root.message("Still ejecting " + root._ejectLabel + "; wait for its result.", false)
            return
        }
        root._ejectDevice = e.device
        root._ejectLabel = e.label
        root._verdictFromListing = 0
        ejectProcess.command = ["gio", "mount", "-e", e.path]
        ejectProcess.running = true
        // Replaces the arm prompt, and a stick mid-flush can take a while to come unmounted.
        root.message("Ejecting " + e.label + ", do not unplug it yet.", false)
    }

    // Nothing is judged while gio still runs, and only a listing started after it exited counts.
    // A listing that cannot be judged settles nothing, so the next poll tries again, until
    // ejectVerdictTimeout ends the wait.
    function judgeEject(body) {
        if (root._ejectDevice.length === 0 || root._verdictFromListing === 0 || root._listingsStarted < root._verdictFromListing)
            return
        var verdict = Eject.verdict(body, root._ejectDevice)
        if (verdict !== "unknown")
            root.reportEject(verdict, Eject.blockers(body, root._ejectDevice))
    }

    // "Safe to unplug" is a safety claim: Eject.sentence says it for the "safe" verdict only.
    function reportEject(verdict, others) {
        if (root._ejectDevice.length === 0)
            return
        ejectVerdictTimeout.stop()
        var s = Eject.sentence(verdict, root._ejectLabel, others)
        root._ejectDevice = ""
        root.message(s.text, s.isError)
    }

    Timer {
        id: mountTimeout
        interval: root.mountTimeoutMs
        repeat: false
        onTriggered: {
            if (root._pendingOpenDevice.length === 0)
                return
            root._pendingOpenDevice = ""
            root.message(root._pendingOpenLabel + " mounted but never reported a folder to open.", true)
        }
    }

    Timer {
        id: ejectVerdictTimeout
        interval: root.ejectVerdictMs
        repeat: false
        onTriggered: root.reportEject("unknown", [])
    }

    Process {
        id: listProcess
        command: ["lsblk", "--json", "-o", "NAME,LABEL,MOUNTPOINT,RM,SIZE,TYPE,MODEL"]
        stdout: StdioCollector {
            id: listOut
            waitForEnd: true
            // The verdict is read here, from this run's own text, so it can never come off the
            // stale fallback onExited keeps for the rail.
            onStreamFinished: {
                if (root._listTimedOut)
                    return
                root._streamPending = false
                root._listOutput = text
                root.judgeEject(text)
            }
        }
        onExited: {
            listTimeout.stop()
            // A listing this timer ended collected nothing, and reading that as "no devices" would
            // empty the rail, taking Eject with it exactly when a device is misbehaving. The stream
            // guard is released here because a stream this timer cut off may never finish on its own.
            if (root._listTimedOut) {
                root._streamPending = false
                return
            }
            root._listing = listOut.text || root._listOutput || ""
            root.rebuild()
        }
    }

    Process {
        id: mountProcess
        onExited: function (exitCode) {
            if (exitCode === 0) {
                root.poll()
                return
            }
            mountTimeout.stop()
            root._pendingOpenDevice = ""
            root.message(root._pendingOpenLabel + " could not be mounted; unplug it and plug it back in.", true)
        }
    }

    Process {
        id: ejectProcess
        // The exit code is not read on purpose: the verdict is the listing that follows, see judgeEject.
        onExited: {
            root._verdictFromListing = root._listingsStarted + 1
            ejectVerdictTimeout.restart()
            root.poll()
        }
    }
}
