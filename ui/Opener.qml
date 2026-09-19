import Quickshell
import Quickshell.Io
import QtQuick

// The one component that runs Flea's own opening modes, so the huge page corner has one owner; see AGENTS.md "Opening a file".
Item {
    id: root

    signal failed(string path)
    signal isDirectory(string path)
    signal terminalFailed(string path)
    // notExecutable is the one refusal the operator can act on, so the bar names the bit for it.
    signal runFailed(string path, bool notExecutable)
    // Raised where the three single-flight guards below drop a request, so a swallowed press says so.
    signal busy(string path)
    signal terminalBusy(string path)
    signal runBusy(string path)

    // The status src/open.rs returns for a directory, which the caller navigates to instead.
    readonly property int isDirectoryStatus: 3
    // The status src/program.rs returns for a file carrying no execute bit, which the window says
    // in its own words rather than passing the mode's sentence on.
    readonly property int notExecutableStatus: 4
    // Long enough that nothing healthy meets it: everything flea --run does after canonicalize is a
    // spawn that returns, and canonicalize is the one call that can hang, inside a dead network
    // mount. The same bound ui/NetworkMounts.qml gives a leg of an open, and for the same reason.
    readonly property int runDeadlineMs: 15000

    property string current: ""
    // The terminal launch's own path: open() and openTerminal() run on separate
    // Processes guarded only against themselves, so sharing current let whichever
    // started second rewrite the path the first one's onExited still reports.
    property string terminalCurrent: ""
    // The run launch's own path, for the same reason terminalCurrent is the terminal's.
    property string runCurrent: ""
    // Set by the deadline so the leg it ended cannot report a second, contradictory failure, the
    // rule ui/NetworkMounts.qml's _infoTimedOut follows; see AGENTS.md "A single-flight guard".
    property bool runTimedOut: false

    // flea --open waits for gio open and not for the application it starts, and that wait is 11 to 15 ms
    // for an Exec= handler but 0.32 to 0.75 s for a DBusActivatable one, which is what this box's
    // twenty-five archive types default to, so this guard drops a second Enter for that long and says so.
    function open(path) {
        if (child.running) {
            root.busy(path)
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

    // Starting a program is not opening a file: flea --open asks the desktop database for a handler
    // and nothing on a stock box claims an AppImage, so Enter on one answered with the opener's own
    // refusal. Its own Process, so a run and an open in flight cannot take each other's exit status.
    function run(path) {
        if (runChild.running) {
            root.runBusy(path)
            return
        }
        root.runCurrent = path
        root.runTimedOut = false
        runChild.command = [Quickshell.env("FLEA_BIN") || "flea", "--run", path]
        runChild.running = true
        runDeadline.restart()
    }

    Process {
        id: runChild

        onExited: function (exitCode, exitStatus) {
            runDeadline.stop()
            if (root.runTimedOut || exitCode === 0) {
                return
            }
            root.runFailed(root.runCurrent, exitCode === root.notExecutableStatus)
        }
    }

    Timer {
        id: runDeadline
        interval: root.runDeadlineMs
        // Ending the guard, not the program: by now it has either started or was never going to.
        onTriggered: {
            root.runTimedOut = true
            runChild.running = false
            root.runFailed(root.runCurrent, false)
        }
    }

    // A terminal in the current directory, through flea --terminal so the huge page,
    // process-group and stdio guards in src/terminal.rs apply. Its own Process, so a terminal
    // launch and a file open in flight cannot take each other's exit status.
    function openTerminal(path) {
        if (terminalChild.running) {
            root.terminalBusy(path)
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
