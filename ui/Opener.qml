import Quickshell
import Quickshell.Io
import QtQuick

// The one component that runs Flea's own opening modes, so the huge page corner has one owner; see AGENTS.md "Opening a file".
Item {
    id: root

    signal failed(string path)
    signal isDirectory(string path)
    signal terminalFailed(string path)
    // Raised where the two single-flight guards below drop a request, so a swallowed press says so.
    signal busy(string path)
    signal terminalBusy(string path)
    signal gitCloneDone(string dir, bool success)
    signal trashDone(bool success)

    // The status src/open.rs returns for a directory, which the caller navigates to instead.
    readonly property int isDirectoryStatus: 3

    // Track if current operation is a git clone.
    property bool isGitClone: false
    property string gitCloneDir: ""
    // Track if current operation is emptying trash.
    property bool isTrashEmpty: false

    property string current: ""
    // The terminal launch's own path: open() and openTerminal() run on separate
    // Processes guarded only against themselves, so sharing current let whichever
    // started second rewrite the path the first one's onExited still reports.
    property string terminalCurrent: ""

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
            if (root.isGitClone) {
                root.isGitClone = false
                root.gitCloneDone(root.gitCloneDir, exitCode === 0)
                return
            }
            if (root.isTrashEmpty) {
                root.isTrashEmpty = false
                root.trashDone(exitCode === 0)
                return
            }
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
            root.terminalBusy(path)
            return
        }
        root.terminalCurrent = path
        terminalChild.command = [Quickshell.env("FLEA_BIN") || "flea", "--terminal", path]
        terminalChild.running = true
    }

    // The default code agent in the current directory, through flea --agent.
    function openAgent(path) {
        if (agentChild.running) {
            root.terminalBusy(path)
            return
        }
        root.terminalCurrent = path
        agentChild.command = [Quickshell.env("FLEA_BIN") || "flea", "--agent", path]
        agentChild.running = true
    }

    // A specific agent in the given directory, through flea --agent <dir> <agent>.
    function openAgentWith(agent, dir) {
        if (agentChild.running) {
            root.terminalBusy(dir)
            return
        }
        root.terminalCurrent = dir
        agentChild.command = [Quickshell.env("FLEA_BIN") || "flea", "--agent", dir, agent]
        agentChild.running = true
    }

    // Open a file with a specific program.
    function openWith(program, filePath) {
        if (child.running) {
            root.busy(filePath)
            return
        }
        root.current = filePath
        child.command = [program, filePath]
        child.running = true
    }

    // Git clone a URL into a directory.
    function gitClone(url, dir) {
        if (child.running) {
            root.busy(url)
            return
        }
        root.current = dir
        root.isGitClone = true
        root.gitCloneDir = dir
        child.command = ["sh", "-c", "cd '" + dir + "' && git clone '" + url + "'"]
        child.running = true
    }

    // Empty the trash directory.
    function emptyTrash(dir) {
        if (child.running) {
            root.busy(dir)
            return
        }
        root.current = dir
        root.isTrashEmpty = true
        child.command = ["sh", "-c", "rm -rf '" + dir + "'/*"]
        child.running = true
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

    Process {
        id: agentChild

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
