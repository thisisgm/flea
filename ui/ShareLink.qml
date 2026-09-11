import Quickshell.Io
import QtQuick

Item {
    id: root

    signal copied(string url)
    signal failed(string reason)

    property string pending: ""
    property string pendingUrl: ""
    property bool _shareAwaitingStart: false
    property bool _clipboardAwaitingStart: false
    property string _shareOutput: ""

    function copy(path) {
        if (pending.length > 0) {
            root.failed("A share link is still being copied; try again when it finishes.")
            return
        }
        if (typeof path !== "string" || path.charAt(0) !== "/" || path.indexOf("\u0000") >= 0) {
            root.failed("Share link needs an absolute file path.")
            return
        }
        root.pending = path
        root._shareOutput = ""
        root._shareAwaitingStart = true
        // DropboxCommand bounds each socket operation itself; a total deadline would shorten that contract.
        sharelink.command = ["dropbox-cli", "sharelink", path]
        sharelink.running = true
    }

    function finish(reason) {
        if (pending.length === 0) return
        var url = pendingUrl
        pending = ""
        pendingUrl = ""
        _shareAwaitingStart = false
        _clipboardAwaitingStart = false
        if (reason) root.failed(reason)
        else root.copied(url)
    }

    Process {
        id: sharelink
        stdout: StdioCollector { id: out; waitForEnd: true; onStreamFinished: root._shareOutput = text }
        onStarted: root._shareAwaitingStart = false
        onRunningChanged: {
            if (root._shareAwaitingStart && !running)
                root.finish("The Dropbox share link helper could not start.")
        }

        onExited: function (exitCode) {
            root._shareAwaitingStart = false
            if (root.pending.length === 0) return
            var url = (out.text || root._shareOutput).trim()
            // Sample output: https://www.dropbox.com/s/example/file.txt?dl=0; exit-zero error text is not a URL.
            if (exitCode !== 0 || !/^https?:\/\/[^/\s?#]+(?:[/?#][^\s]*)?$/.test(url)) {
                root.finish("Dropbox could not make a share link for that file.")
                return
            }
            root.pendingUrl = url
            root._clipboardAwaitingStart = true
            copyToClipboard.command = ["wl-copy", url]
            copyToClipboard.running = true
        }
    }

    Process {
        id: copyToClipboard
        onStarted: root._clipboardAwaitingStart = false
        onRunningChanged: {
            if (root._clipboardAwaitingStart && !running)
                root.finish("The clipboard helper could not start; the share link was not copied.")
        }
        onExited: function(exitCode) {
            root._clipboardAwaitingStart = false
            root.finish(exitCode === 0 ? "" : "The share link could not be copied to the clipboard.")
        }
    }
}
