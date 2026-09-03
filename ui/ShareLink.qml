import Quickshell.Io
import QtQuick

// dropbox-cli prints one line, the URL, and it goes to the clipboard the same way the Tailscale
// panel's own copyToClipboard does. No Rust: this is a QML Process call and nothing else.
Item {
    id: root

    signal copied(string url)
    signal failed()

    property string pending: ""

    function copy(path) {
        if (path.length === 0 || sharelink.running)
            return
        root.pending = path
        sharelink.command = ["dropbox-cli", "sharelink", path]
        sharelink.running = true
    }

    Process {
        id: sharelink
        stdout: StdioCollector { id: out }

        onExited: function (exitCode) {
            var url = out.text.trim()
            // dropbox-cli prints its own diagnosis and does not always exit non-zero, so the answer
            // is judged by what it printed rather than by the status alone.
            if (exitCode !== 0 || url.indexOf("http") !== 0) {
                root.failed()
                return
            }
            // argv-direct, no shell: wl-copy takes the text as an argument, so a URL holding
            // $(...) or backticks can never reach a shell to be expanded.
            copyToClipboard.command = ["wl-copy", url]
            copyToClipboard.running = true
            root.copied(url)
        }
    }

    Process {
        id: copyToClipboard
    }
}
