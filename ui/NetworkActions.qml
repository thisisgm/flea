import QtQuick
import Quickshell
import Quickshell.Io
import "js/Remote.js" as Remote

// Host-side actions remain argv-direct. Privileged recovery is guidance only; this service never
// starts tailscaled or logs a user into a tailnet behind their back.
Item {
    id: root

    signal message(string text, bool isError)

    function copyAddress(entry) {
        if (copyProcess.running || !entry) return
        copyProcess.command = ["wl-copy", String(entry.address || entry.uri || "")]
        copyProcess.running = true
    }

    function openSsh(entry) {
        var argv = Remote.terminalArgv(entry)
        if (argv.length === 0) {
            root.message("That location has no safe default SSH target.", true)
            return
        }
        Quickshell.execDetached(argv)
    }

    function wake(entry) {
        if (wakeProcess.running || !entry || !Remote.validMac(entry.mac)) return
        wakeProcess.command = [Quickshell.env("FLEA_BIN") || "flea", "--wake", entry.mac]
        wakeProcess.running = true
    }

    Process {
        id: copyProcess
        onExited: function (exitCode) {
            root.message(exitCode === 0 ? "Network address copied to the clipboard."
                                        : "The network address could not be copied.", exitCode !== 0)
        }
    }

    Process {
        id: wakeProcess
        onExited: function (exitCode) {
            root.message(exitCode === 0 ? "Wake-on-LAN packet sent."
                                        : "Wake-on-LAN could not send a packet.", exitCode !== 0)
        }
    }
}
