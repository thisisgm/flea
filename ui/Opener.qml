import Quickshell
import Quickshell.Io
import QtQuick

// The one component that runs Flea's own opening modes, so the huge page corner has one owner; see AGENTS.md "Opening a file".
Item {
    id: root

    signal failed(string path)
    signal isDirectory(string path)
    signal terminalFailed(string path)

    // The status src/open.rs returns for a directory, which the caller navigates to instead.
    readonly property int isDirectoryStatus: 3

    property string current: ""
    // The terminal launch's own path: open() and openTerminal() run on separate
    // Processes guarded only against themselves, so sharing current let whichever
    // started second rewrite the path the first one's onExited still reports.
    property string terminalCurrent: ""

    // flea --open waits for gio open and not for the application it starts, and that wait is 11 to 15 ms
    // for an Exec= handler but 0.32 to 0.75 s for a DBusActivatable one, which is what this box's
    // twenty-five archive types default to, so this guard drops a second Enter for that long in silence.
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

    // A terminal in the current directory, through flea --terminal so the huge page,
    // process-group and stdio guards in src/terminal.rs apply. Its own Process, so a terminal
    // launch and a file open in flight cannot take each other's exit status.
    function openTerminal(path) {
        if (terminalChild.running) {
            return
        }
        root.terminalCurrent = path
        terminalChild.command = [Quickshell.env("FLEA_BIN") || "flea", "--terminal", path]
        terminalChild.running = true
    }

    Process {
        id: terminalChild

        onExited: function (exitCode, exitStatus) {
            if (exitCode === 0) {
                return
            }
            root.terminalFailed(root.terminalCurrent)
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
