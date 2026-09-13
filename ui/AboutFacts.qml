import QtQuick
import Quickshell
import Quickshell.Io

// Installed facts are read only when About is shown; each failed query keeps an explicit unknown.
QtObject {
    id: root
    property bool active: false
    property bool loaded: false
    property var facts: ({})
    readonly property string binary: Quickshell.env("FLEA_BIN") || "flea"

    function setFact(key, value) {
        var next = Object.assign({}, root.facts)
        next[key] = value
        root.facts = next
    }

    onActiveChanged: {
        if (!root.active || root.loaded) return
        root.loaded = true
        version.running = true
        owner.running = true
        handler.running = true
    }

    property var versionQuery: Process {
        id: version
        command: [root.binary, "--version"]
        property string answer: ""
        stdout: StdioCollector { onStreamFinished: version.answer = this.text.trim() }
        // Sample output: "0.2.0". src/main.rs prints CARGO_PKG_VERSION and nothing else, so the
        // name is tolerated rather than required: demanding it left the Version row reading
        // "Not reported" on every build Flea has ever shipped.
        onExited: function (code) {
            if (code !== 0) return
            var text = version.answer.indexOf("flea ") === 0 ? version.answer.substring(5) : version.answer
            if (text.length > 0) root.setFact("version", text)
        }
    }
    property var ownerQuery: Process {
        id: owner
        command: ["pacman", "-Qqo", root.binary]
        property string answer: ""
        stdout: StdioCollector { onStreamFinished: owner.answer = this.text.trim() }
        // Sample output: flea
        onExited: function (code) {
            if (code !== 0 || owner.answer.length === 0) {
                root.setFact("source", "Unpackaged candidate")
                root.setFact("package", "Not owned by a package")
                return
            }
            packageVersion.command = ["pacman", "-Qi", owner.answer]
            packageVersion.running = true
            repository.command = ["pacman", "-Si", owner.answer]
            repository.running = true
        }
    }
    // -Qi rather than -Q: the same query carries the build date, and the Built row had nothing
    // setting it at all, so every box read "Not recorded in this build" whatever it was running.
    // Sample output: "Name            : flea", "Version         : 0.1.6-1", "Build Date      : Tue Sep  8 01:52:54 2026".
    property var packageQuery: Process {
        id: packageVersion
        environment: ({ LC_ALL: "C" })
        property string answer: ""
        stdout: StdioCollector { onStreamFinished: packageVersion.answer = this.text }
        onExited: function (code) {
            if (code !== 0) return
            var lines = packageVersion.answer.split("\n"), name = "", built = ""
            for (var i = 0; i < lines.length; i++) {
                var cut = lines[i].indexOf(":")
                if (cut < 0) continue
                var key = lines[i].substring(0, cut).trim(), value = lines[i].substring(cut + 1).trim()
                if (key === "Name") name = value
                else if (key === "Version") name = name.length > 0 ? name + " " + value : value
                else if (key === "Build Date") built = value
            }
            if (name.length > 0) root.setFact("package", name)
            if (built.length > 0) root.setFact("built", built)
        }
    }
    property var repositoryQuery: Process {
        id: repository
        environment: ({ LC_ALL: "C" })
        property string answer: ""
        stdout: StdioCollector { onStreamFinished: repository.answer = this.text }
        // Sample output: Repository      : omarchy
        onExited: function (code) {
            if (code !== 0) { root.setFact("source", "Local package"); return }
            var lines = repository.answer.split("\n")
            for (var i = 0; i < lines.length; i++) {
                if (lines[i].indexOf("Repository") !== 0) continue
                var name = lines[i].substring(lines[i].indexOf(":") + 1).trim()
                root.setFact("source", name === "omarchy" ? "Omarchy Package Repository" : name)
                return
            }
        }
    }
    property var handlerQuery: Process {
        id: handler
        command: ["xdg-mime", "query", "default", "inode/directory"]
        property string answer: ""
        stdout: StdioCollector { onStreamFinished: handler.answer = this.text.trim() }
        onExited: function (code) { if (code === 0 && handler.answer.length > 0) root.setFact("handler", handler.answer) }
    }
}
