import Quickshell
import Quickshell.Io
import QtQuick

// The one component that launches a foreign program, so the huge page corner has one owner; see AGENTS.md "Opening a file".
Item {
    id: root

    signal failed(string path)
    signal isDirectory(string path)

    // The status src/open.rs returns for a directory, which the caller navigates to instead.
    readonly property int isDirectoryStatus: 3

    property string current: ""

    // flea --open decides and exits in milliseconds, so one Process serves every open.
    function open(path) {
        if (child.running) {
            return
        }
        root.current = path
        child.command = [Quickshell.env("FLEA_BIN") || "flea", "--open", path]
        child.running = true
    }

    Process {
        id: child

        onExited: function (exitCode, exitStatus) {
            if (exitCode === 0) {
                return
            }
            if (exitCode === root.isDirectoryStatus) {
                root.isDirectory(root.current)
                return
            }
            root.failed(root.current)
        }
    }

    // The system clipboard, for the listing menu's Copy Path row. wl-copy reads the text on stdin,
    // so the one-liner hands it over; flea's own copy clipboard (Ops.clip) is a different thing
    // and must stay a different thing.
    function copyText(text) {
        if (copier.running) {
            return
        }
        copier.command = ["sh", "-c", "printf '%s' \"$1\" | wl-copy", "_", text]
        copier.running = true
    }

    Process {
        id: copier
    }

}
