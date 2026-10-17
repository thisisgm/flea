import Quickshell
import Quickshell.Io
import QtQuick

// The one component that runs Flea's own opening modes, so the huge page corner has one owner; see AGENTS.md "Opening a file".
Item {
    id: root

    signal failed(string path)
    signal isDirectory(string path)
    signal terminalFailed(string path)
    signal clipboardFailed()
    // Raised where the two single-flight guards below drop a request, so a swallowed press says so.
    signal busy(string path)
    signal terminalBusy(string path)
    // flea --open answers 4 with the run card's own ask line on stdout, and 5 where no application claims the file.
    signal askRunRaw(string path, string text)
    signal askOpenWith(string path)

    // The status src/open.rs returns for a directory, which the caller navigates to instead.
    readonly property int isDirectoryStatus: 3
    // The two statuses src/runfile.rs added beside it: ask the card, or ask Open with.
    readonly property int askRunStatus: 4
    readonly property int askOpenWithStatus: 5

    property string current: ""
    // The terminal launch's own path: open() and openTerminal() run on separate
    // Processes guarded only against themselves, so sharing current let whichever
    // started second rewrite the path the first one's onExited still reports.
    property string terminalCurrent: ""
    // One open at a time: pending is the launch with no exit yet, and the stream arrives before the exit.
    property bool pending: false
    property string askText: ""

    // flea --open waits for gio open and not for the application it starts, and that wait is 11 to 15 ms
    // for an Exec= handler but 0.32 to 0.75 s for a DBusActivatable one, which is what this box's
    // twenty-five archive types default to, so this guard drops a second Enter for that long and says so.
    function open(path) {
        if (child.running || root.pending) {
            root.busy(path)
            return
        }
        root.current = path
        root.pending = true
        root.askText = ""
        child.command = [Quickshell.env("FLEA_BIN") || "flea", "--open", path]
        child.running = true
    }

    Process {
        id: child
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: root.askText = this.text
        }

        // An ask status without its line is a launch that died mid-answer, so it says the plain failure and opens no card.
        onExited: function (exitCode, exitStatus) {
            root.pending = false
            if (exitCode === 0)
                return
            if (exitCode === root.isDirectoryStatus)
                root.isDirectory(root.current)
            else if (exitCode === root.askRunStatus && root.askText !== "")
                root.askRunRaw(root.current, root.askText)
            else if (exitCode === root.askOpenWithStatus)
                root.askOpenWith(root.current)
            else
                root.failed(root.current)
        }
        // A program that never started reports no exit, only running turning false.
        onRunningChanged: {
            if (child.running || !root.pending)
                return
            root.pending = false
            root.failed(root.current)
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
        var x11 = Quickshell.env("XDG_SESSION_TYPE") === "x11"
                  || (!Quickshell.env("WAYLAND_DISPLAY") && !!Quickshell.env("DISPLAY"))
        copier.command = ["sh", "-c", x11
                          ? "printf '%s' \"$1\" | xclip -selection clipboard -in"
                          : "printf '%s' \"$1\" | wl-copy", "_", text]
        copier.running = true
    }

    Process {
        id: copier
        onExited: function (exitCode) {
            if (exitCode !== 0) root.clipboardFailed()
        }
    }

    // A library cannot name a singleton, so the menu's Update Flea row comes through here, and the pane's footer says what it did.
    function updateFlea(pane) {
        UpdateCheck.launch(pane)
    }

}
