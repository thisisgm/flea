pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// A lazy sandboxed quickjs-ng helper answers by ticket; an overdue head fails alone and the queued tickets retry on a fresh helper.
Item {
    id: root

    signal done(int ticket, string svg, string error)
    signal sent(int ticket, string source)

    // Writable so the suite shrinks them; production never writes either.
    property int renderMs: 2000
    // With nothing waiting this long the helper exits, so its memory returns.
    property int idleExitMs: 30000
    // Revisits and theme flips back answer from here, never re-rendered.
    readonly property int cacheMax: 64
    // Helper answers prove the work happened off the GUI thread.
    property int workerAnswers: 0
    // A 127 exit or failed spawn latches the fenced-source fallback with one log line and no retry storm.
    property bool available: true

    property int seq: 0
    property int sends: 0
    property var waiting: ({})
    property var answerCache: ({})
    property var answerOrder: []
    property var pending: []
    property var written: []
    property bool starting: false
    property bool stopping: false
    property int generation: 0
    readonly property int killSignal: 9
    // The launcher's refusal status, REFUSED in src/figurehelper.rs.
    readonly property int refusalExit: 127
    // An unexpected exit strikes the head ticket, and its second strike fails it.
    readonly property int exitStrikeLimit: 2
    readonly property int deadlinePollMs: 250
    // The suite reads this to prove the idle exit stopped the helper.
    readonly property bool helperRunning: helper.running
    // The helper's pid, so the suite reads its peak RSS while it runs.
    readonly property var helperPid: helper.processId
    // The figure kinds the next helper start loads before its first request; warm() sets it and the start reads it once.
    property var warmKinds: []
    // The suite reads the persistent cache's counters and process through this.
    readonly property var persistent: disk
    // The cache keys of the last warm query, which the suite compares with the keys placed figures put.
    property var warmKeys: []
    // The warm query waiting on the persistent cache, if any.
    property int warmQuery: 0
    // The deadline timer stops between tickets so an idle session wakes for nothing.
    readonly property bool deadlineRunning: deadlineTimer.running
    // The suite observes actual exit and deadline events without timing their answers.
    property int helperExits: 0
    property int deadlineExpirations: 0

    // FNV-1a over the table's comma-joined text, "0" for none.
    function tableDigestOf(table) {
        if (!table || table.length === 0)
            return "0";
        var fnvOffset = 0x811c9dc5;
        var fnvPrime = 0x01000193;
        var hexRadix = 16;
        var text = table.join(",");
        var hash = fnvOffset;
        for (var i = 0; i < text.length; i++)
            hash = Math.imul(hash ^ text.charCodeAt(i), fnvPrime);
        return (hash >>> 0).toString(hexRadix);
    }

    // The figure cache key, including the display mode that changes formula layout; a dated figure's day joins it, so the next day's ask draws that day's chart.
    function cacheKeyOf(kind, source, t, display) {
        var key = [t.bg, t.fg, t.accent || "", t.muted || "", t.line || "", t.surface || "",
            t.border || "", t.font || "", t.bodyPx || 0, t.exPx || 0, root.tableDigestOf(t.advances), root.tableDigestOf(t.boldAdvances)].join("|");
        return kind + "\n" + key + (t.today ? "|" + t.today : "") + "\n" + !!display + "\n" + source;
    }

    function cached(kind, source, theme, display) {
        var key = root.cacheKeyOf(kind, source, theme, display);
        var hit = root.answerCache[key];
        if (hit !== undefined) {
            var at = root.answerOrder.indexOf(key);
            if (at >= 0)
                root.answerOrder.splice(at, 1);
            root.answerOrder.push(key);
        }
        return hit;
    }

    function store(kind, source, theme, display, svg) {
        var key = root.cacheKeyOf(kind, source, theme, display);
        var at = root.answerOrder.indexOf(key);
        if (at >= 0)
            root.answerOrder.splice(at, 1);
        root.answerCache[key] = svg;
        root.answerOrder.push(key);
        while (root.answerOrder.length > root.cacheMax)
            delete root.answerCache[root.answerOrder.shift()];
    }

    // Figures drawn in any earlier session answer from here before the helper is ever started.
    FigureStore {
        id: disk
        onAnswered: function (id, svg) { root.storeAnswered(id, svg) }
        onKnown: function (id, all) { root.warmAnswered(id, all) }
    }

    Process {
        id: helper
        // FLEA_BIN is the dev seam, see AGENTS.md "Where the backend binary comes from".
        command: [Quickshell.env("FLEA_BIN") || "flea", "--figure-helper"].concat(root.warmKinds.length > 0 ? ["--warm=" + root.warmKinds.join(",")] : [])
        running: false
        stdinEnabled: true

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: function (data) { root.receive(data) }
        }

        onStarted: {
            root.starting = false;
            if (root.stopping) {
                helper.signal(root.killSignal);
                return;
            }
            root.warmKinds = [];
            var pending = root.pending;
            root.pending = [];
            for (var i = 0; i < pending.length; i++)
                root.writeLine(pending[i]);
            // A warm start with nothing asked yet still ends at the idle exit.
            root.armIdle();
        }

        // A spawn that fails raises runningChanged and never exited, measured, so it reports here.
        onRunningChanged: {
            if (!helper.running && root.starting && !root.stopping) {
                root.starting = false;
                root.refuse("figure engine did not start");
            }
        }

        onExited: function (exitCode, exitStatus) {
            root.helperExits++;
            root.starting = false;
            root.stopping = true;
            var error = "figure engine exited " + exitCode + " (status " + exitStatus + ")";
            if (exitCode === root.refusalExit)
                root.refuse(error);
            else {
                // A kill or an idle stop leaves nothing written, so only a crash finds a head to strike.
                var failed = root.strikeHead();
                root.requeueWritten();
                if (failed !== undefined) {
                    console.log("FigureService: the figure engine stopped twice on one figure, which shows its fenced source");
                    root.done(failed, "", error);
                }
            }
            root.stopping = false;
            if (root.pending.length > 0)
                root.ensureHelper();
        }
    }

    Timer {
        id: deadlineTimer
        interval: root.deadlinePollMs
        repeat: true
        running: false
        onTriggered: {
            var head = root.written[0];
            if (head !== undefined && Date.now() > root.waiting[head].deadline) {
                root.deadlineExpirations++;
                root.killHelper(head);
            }
            // The tick that finds waiting empty arms the idle exit, then stops.
            if (Object.keys(root.waiting).length === 0) {
                root.armIdle();
                deadlineTimer.stop();
            }
        }
    }

    Timer {
        id: idleTimer
        interval: root.idleExitMs
        onTriggered: {
            if (Object.keys(root.waiting).length !== 0)
                return;
            disk.stop();
            // A store still active (refused, or draining and maybe restarted for a queued put) is asked again at the next idle, so it never runs on for the session.
            if (disk.active)
                idleTimer.restart();
            if (helper.running) {
                root.stopping = true;
                helper.running = false;
            }
        }
    }

    function armIdle() {
        if (Object.keys(root.waiting).length === 0 && (helper.running || disk.active) && !root.stopping)
            idleTimer.restart();
    }

    // The head is the ticket the helper was working on, so only it can have caused the exit; undefined until its second strike.
    function strikeHead() {
        var head = root.written[0];
        if (head === undefined || ++root.waiting[head].strikes < root.exitStrikeLimit)
            return undefined;
        root.written.shift();
        delete root.waiting[head];
        return head;
    }

    function requeueWritten() {
        var retry = root.written;
        root.written = [];
        for (var i = 0; i < retry.length; i++) {
            root.waiting[retry[i]].generation = 0;
            root.waiting[retry[i]].deadline = 0;
        }
        root.pending = retry.concat(root.pending);
    }

    function killHelper(id) {
        root.stopping = true;
        delete root.waiting[id];
        root.written = root.written.filter(function (ticket) { return ticket !== id; });
        root.requeueWritten();
        root.done(id, "", "render timed out");
        if (helper.running)
            helper.signal(root.killSignal);
    }

    // Refusal fails every ticket into its fenced source without retries.
    function refuse(error) {
        if (!root.available)
            return;
        root.available = false;
        console.log("FigureService: " + error + ", figures show their fenced source");
        for (var id in root.waiting) {
            // A ticket still with the persistent cache may yet be answered from disk.
            if (root.waiting[id].onDisk)
                continue;
            root.done(Number(id), "", error);
            delete root.waiting[id];
        }
        root.pending = [];
        root.written = [];
    }

    function receive(line) {
        // Sample input: {"id":3,"svg":"<svg ...>...</svg>"} or {"id":3,"error":"formula over 4 KiB"}.
        if (!line || line.length === 0)
            return;
        var message = null;
        try {
            message = JSON.parse(line);
        } catch (e) {
            return;
        }
        var id = message.id;
        if (root.waiting[id] === undefined || root.stopping
                || root.waiting[id].generation !== root.generation)
            return;
        root.workerAnswers++;
        var asked = root.waiting[id];
        delete root.waiting[id];
        var at = root.written.indexOf(id);
        if (at >= 0)
            root.written.splice(at, 1);
        if (at === 0 && root.written.length > 0)
            root.waiting[root.written[0]].deadline = Date.now() + root.renderMs;
        if (message.svg !== undefined) {
            root.store(asked.kind, asked.source, asked.theme, asked.display, message.svg);
            disk.put(root.cacheKeyOf(asked.kind, asked.source, asked.theme, asked.display), message.svg);
            root.done(id, message.svg, "");
        } else {
            root.done(id, "", message.error || "render failed");
        }
        root.armIdle();
    }

    function ensureHelper() {
        if (!root.available)
            return false;
        if (root.stopping || helper.running || root.starting)
            return true;
        root.generation++;
        root.starting = true;
        idleTimer.stop();
        helper.running = true;
        return true;
    }

    function writeLine(id) {
        var w = root.waiting[id];
        if (w === undefined || w.generation !== 0 || root.stopping || !helper.running)
            return;
        w.generation = root.generation;
        root.written.push(id);
        if (root.written.length === 1)
            w.deadline = Date.now() + root.renderMs;
        // Sample input: {"id":3,"kind":"math","source":"\\frac{a}{b}","display":true,"theme":{"bg":"#101315"}}.
        var line = JSON.stringify({ id: id, kind: w.kind, source: w.source, display: w.display, theme: w.theme }) + "\n";
        root.sends++;
        helper.write(line);
        root.sent(id, w.source);
    }

    // A document with figures is on screen and none is asked for yet: start the helper now, so it is warm when they are asked; the caller names the keys an ask would use.
    function warm(keys, kinds) {
        if (!root.available || root.stopping || root.starting || helper.running)
            return false;
        if (kinds.length === 0)
            return false;
        root.warmKinds = kinds;
        root.warmKeys = keys;
        // A document whose figures are all on disk likely needs no helper, so the cache is asked before one is started.
        if (disk.available) {
            root.warmQuery = ++root.seq;
            disk.ask(root.warmQuery, keys);
            return true;
        }
        return root.ensureHelper();
    }

    function warmAnswered(id, all) {
        if (id !== root.warmQuery)
            return;
        root.warmQuery = 0;
        if (!all && !root.stopping && !root.starting && !helper.running && Object.keys(root.waiting).length === 0)
            root.ensureHelper();
        else if (all)
            root.warmKinds = [];
        root.armIdle();
    }

    // The caller dates a Gantt chart's theme, so the key, the stored entry and the drawing name one day.
    function ask(kind, source, display, theme) {
        root.seq++;
        var id = root.seq;
        // A revisit or a theme flip back never reaches the helper at all.
        var hit = root.cached(kind, source, theme, display);
        if (hit !== undefined) {
            // Deferred past this return, so the caller's ticket is set before its answer lands.
            Qt.callLater(function () { root.done(id, hit, ""); });
            return id;
        }
        if (!root.available) {
            Qt.callLater(function () { root.done(id, "", "figure engine did not start"); });
            return id;
        }
        root.waiting[id] = { kind: kind, source: source, display: display,
            theme: theme, generation: 0, deadline: 0, strikes: 0, onDisk: disk.available };
        idleTimer.stop();
        deadlineTimer.start();
        if (disk.available)
            disk.get(id, root.cacheKeyOf(kind, source, theme, display));
        else
            root.dispatch(id);
        return id;
    }

    // The persistent cache's answer: a hit is the figure, a miss goes on to the helper.
    function storeAnswered(id, svg) {
        var asked = root.waiting[id];
        if (asked === undefined || !asked.onDisk)
            return;
        asked.onDisk = false;
        if (svg !== "") {
            delete root.waiting[id];
            root.store(asked.kind, asked.source, asked.theme, asked.display, svg);
            root.done(id, svg, "");
            root.armIdle();
        } else if (!root.available) {
            delete root.waiting[id];
            root.done(id, "", "figure engine did not start");
        } else {
            root.dispatch(id);
        }
    }

    function dispatch(id) {
        if (helper.running && !root.starting && !root.stopping) {
            root.writeLine(id);
        } else {
            root.pending.push(id);
            root.ensureHelper();
        }
    }
}
