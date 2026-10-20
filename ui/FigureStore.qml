import QtQuick
import Quickshell
import Quickshell.Io

// The persistent figure cache's process: lazy, one JSON line per request. Every failure is a miss, so a figure then draws by the helper as it always did.
Item {
    id: root

    // The drawn svg for a get, or "" for a miss; the figure service owns what a miss means.
    signal answered(int id, string svg)
    // Whether every figure of a warm query already has an entry.
    signal known(int id, bool all)

    // A failed start or an unexpected exit latches the store off for the session, with no retry storm.
    property bool available: true
    // A reply later than this ends the store: its figures draw by the helper.
    property int replyMs: 2000
    // A get or known queued behind a drain waits this long, then it is a miss for the helper to draw; behind a start it waits for the start, since only its outcome answers it.
    property int drainMs: root.replyMs
    // A drain still running at this bound is a hung store and is killed, never latched: 60 s commits 15 puts of the 4 MiB entry cap (MAX_ENTRY_BYTES, src/figurestore.rs) at a 1 MiB/s slow-disk write rate.
    property int drainHangMs: 60000
    // The suite reads this to prove a hung drain was killed and a slow one was not.
    property int hangKills: 0
    // The suite reads these to prove a repeat was served from disk and nothing else started.
    property int hits: 0
    property int misses: 0
    property int puts: 0
    property int exits: 0
    readonly property int pollMs: 250
    readonly property int killSignal: 9
    readonly property bool active: process.running || root.starting
    readonly property var pid: process.processId
    property bool starting: false
    property bool stopping: false
    property var queued: []
    property var outstanding: ({})
    // The replies still owed; a notifying count, because a var property announces only a reassignment and the reply timer binds to this.
    property int owed: 0

    // Flea's own binary, found the way the helper's is; FLEA_BIN is the dev seam.
    Process {
        id: process
        command: [Quickshell.env("FLEA_BIN") || "flea", "--figure-store"]
        running: false
        stdinEnabled: true

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: function (data) { root.receive(data) }
        }

        onStarted: {
            root.starting = false;
            if (root.stopping) {
                process.signal(root.killSignal);
                return;
            }
            var lines = root.queued;
            root.queued = [];
            for (var i = 0; i < lines.length; i++)
                root.write(lines[i].line, lines[i].id);
        }

        // A spawn that fails raises runningChanged and never exited, so it latches here.
        onRunningChanged: {
            if (!process.running && root.starting && !root.stopping)
                root.fail();
        }

        onExited: {
            root.exits++;
            var wasStopping = root.stopping;
            root.starting = false;
            root.stopping = false;
            if (!wasStopping)
                root.fail();
            // Lines sent while an idle stop was draining the process start it again.
            else if (root.queued.length > 0)
                root.begin();
        }
    }

    Timer {
        interval: root.pollMs
        repeat: true
        running: root.owed > 0
        onTriggered: {
            var lapsed = [];
            for (var id in root.outstanding) {
                var asked = root.outstanding[id];
                // A line not yet written to the process has no reply clock, only the wait for a drain or a start.
                if (asked.sentAt === null) {
                    // A slow start under load still answers, so only a drain can lapse a queued line.
                    if (!root.starting && Date.now() - asked.queuedAt > root.drainMs)
                        lapsed.push(id);
                } else if (Date.now() - asked.sentAt > root.replyMs) {
                    root.fail();
                    return;
                }
            }
            for (var i = 0; i < lapsed.length; i++)
                root.lapse(lapsed[i]);
        }
    }

    // The drain's hang bound; the timer runs only while the store is stopping, and a drain that ends restarts it.
    Timer {
        interval: root.drainHangMs
        running: root.stopping
        onTriggered: {
            root.hang();
        }
    }

    // Answers one waiting id as a miss, so its figure draws by the helper.
    function miss(id, op) {
        if (op === "get")
            root.answered(Number(id), "");
        else
            root.known(Number(id), false);
    }

    // Releases everything waiting as a miss.
    function release() {
        var waiting = root.outstanding;
        root.outstanding = {};
        root.owed = 0;
        for (var id in waiting)
            root.miss(id, waiting[id].op);
    }

    // Latches off, ends the process and releases everything waiting as a miss.
    function fail() {
        root.starting = false;
        root.available = false;
        root.queued = [];
        if (process.running) {
            root.stopping = true;
            process.signal(root.killSignal);
        }
        root.release();
    }

    // Answers a line that waited past drainMs as a miss and drops it from the queue, so the store never sees it.
    function lapse(id) {
        var asked = root.outstanding[id];
        root.settle(id);
        root.queued = root.queued.filter(function (entry) { return String(entry.id) !== id; });
        root.miss(id, asked.op);
    }

    // Kills a store whose drain outlasted drainHangMs and releases anything still waiting; no latch, and the queued puts go to the next store.
    function hang() {
        root.hangKills++;
        if (process.running)
            process.signal(root.killSignal);
        root.queued = root.queued.filter(function (entry) { return entry.id === undefined; });
        root.release();
    }

    // An idle stop closes stdin so the store drains (every reply sent, every put committed) and exits at EOF; a drain is killed only at drainHangMs, and a refused stop is retried at the service's next idle.
    function stop() {
        if (!process.running || root.starting || root.stopping || root.owed > 0 || root.queued.length > 0)
            return false;
        root.stopping = true;
        process.stdinEnabled = false;
        return true;
    }

    // Starts the process with its stdin open, which an earlier stop closed.
    function begin() {
        root.starting = true;
        process.stdinEnabled = true;
        process.running = true;
    }

    function owe(id, op) {
        root.outstanding[id] = { op: op, queuedAt: Date.now(), sentAt: null };
        root.owed = Object.keys(root.outstanding).length;
    }

    function settle(id) {
        delete root.outstanding[id];
        root.owed = Object.keys(root.outstanding).length;
    }

    // Writes one line and starts its reply clock; a put has no id and no reply.
    function write(line, id) {
        process.write(line);
        if (id !== undefined && root.outstanding[id] !== undefined)
            root.outstanding[id].sentAt = Date.now();
    }

    function send(line, id) {
        if (!root.available)
            return false;
        if (process.running && !root.starting && !root.stopping) {
            root.write(line, id);
            return true;
        }
        root.queued.push({ line: line, id: id });
        if (!root.starting && !root.stopping)
            root.begin();
        return true;
    }

    // Sample input: {"op":"get","id":3,"key":"math\n#101315|...\ntrue\nx^2"}.
    function get(id, key) {
        root.owe(id, "get");
        if (!root.send(JSON.stringify({ op: "get", id: id, key: key }) + "\n", id)) {
            root.settle(id);
            Qt.callLater(function () { root.answered(id, ""); });
        }
    }

    // Sample input: {"op":"put","key":"math\n...","svg":"<svg ...>"}; there is no reply.
    function put(key, svg) {
        if (root.send(JSON.stringify({ op: "put", key: key, svg: svg }) + "\n"))
            root.puts++;
    }

    // Sample input: {"op":"known","id":4,"keys":["math\n#101315|...\ntrue\nx^2","mermaid\n#101315|...\ntrue\nflowchart TD\n    A --> B"]}.
    function ask(id, keys) {
        root.owe(id, "known");
        if (!root.send(JSON.stringify({ op: "known", id: id, keys: keys }) + "\n", id)) {
            root.settle(id);
            Qt.callLater(function () { root.known(id, false); });
        }
    }

    function receive(line) {
        // Sample input: {"id":3,"svg":"<svg ...>"}, {"id":3,"miss":true} or {"id":4,"known":true}.
        var message = null;
        try {
            message = JSON.parse(line);
        } catch (e) {
            return;
        }
        // A bare null parses and has no id to read.
        if (message === null)
            return;
        var asked = root.outstanding[message.id];
        if (asked === undefined)
            return;
        root.settle(message.id);
        if (asked.op === "known") {
            root.known(message.id, message.known === true);
        } else if (message.svg !== undefined) {
            root.hits++;
            root.answered(message.id, message.svg);
        } else {
            root.misses++;
            root.answered(message.id, "");
        }
    }
}
