.import "sourcefixture.js" as Source

// The persistent cache's own handlers on a fake process and a fake clock, so no Quickshell and no wait.
function store() {
    var source = Source.source("ui/FigureStore.qml")
    var fake = { now: 0, signals: [], writes: [], answers: [], knowns: [], starts: 0 }
    var root = { available: true, replyMs: 2000, hits: 0, misses: 0, puts: 0, exits: 0, starting: false, stopping: false,
        queued: [], outstanding: {}, owed: 0 }
    root.drainMs = root.replyMs
    root.drainHangMs = root.replyMs * 30
    root.hangKills = 0
    root.answered = function (id, svg) { fake.answers.push({ id: id, svg: svg }) }
    root.known = function (id, all) { fake.knowns.push({ id: id, all: all }) }
    var process = { running: false, stdinEnabled: true,
        signal: function (signal) { fake.signals.push(signal) },
        write: function (line) { fake.writes.push(JSON.parse(line)) } }
    var Date = { now: function () { return fake.now } }
    var Qt = { callLater: function () {} }
    // Sample input: readonly property int killSignal: 9.
    var constants = /readonly property int (\w+): (\d+)/g
    var constant
    while ((constant = constants.exec(source)) !== null)
        root[constant[1]] = Number(constant[2])
    function compile(args, body) {
        return new Function("root", "process", "Date", "Qt", "return function (" + args + ") {" + body + "}")(root, process, Date, Qt)
    }
    // Sample input: function ask(id, figures) {
    var functions = /\bfunction (\w+)\(([^)]*)\)\s*\{/g
    var match
    while ((match = functions.exec(source)) !== null)
        root[match[1]] = compile(match[2], Source.block(source, match[0].slice(0, -1), "ui/FigureStore.qml"))
    var started = compile("", Source.block(source, "onStarted:", "ui/FigureStore.qml"))
    var runningChanged = compile("", Source.block(source, "onRunningChanged:", "ui/FigureStore.qml"))
    var exited = compile("", Source.block(source, "onExited:", "ui/FigureStore.qml"))
    fake.tick = compile("", Source.block(source.substring(source.indexOf("Timer {")), "onTriggered:", "ui/FigureStore.qml"))
    fake.hang = compile("", Source.block(source.substring(source.indexOf("interval: root.drainHangMs")), "onTriggered:", "ui/FigureStore.qml"))
    fake.start = function () {
        process.running = true
        fake.starts++
        started()
    }
    fake.exit = function () {
        process.running = false
        runningChanged()
        exited()
    }
    // A spawn that fails raises runningChanged alone: no started and no exited.
    fake.spawnFail = function () {
        process.running = false
        runningChanged()
    }
    // Sample input: readonly property bool active: process.running || root.starting.
    fake.active = compile("", "return " + /readonly property bool active: (.+)/.exec(source)[1])
    fake.reply = function (message) { root.receive(JSON.stringify(message)) }
    fake.root = root
    fake.process = process
    return fake
}

function run(check) {
    var named = ""
    try {
        Source.block("var a = 1", "onExited:", "ui/FigureStore.qml")
    } catch (e) {
        named = String(e)
    }
    check("a block missing from a file names that file", named.indexOf("ui/FigureStore.qml: missing onExited:") >= 0, true)
    named = ""
    try {
        Source.block("onExited: { var a = 1", "onExited:", "ui/FigureStore.qml")
    } catch (e) {
        named = String(e)
    }
    check("an unterminated block names its file", named.indexOf("ui/FigureStore.qml: unterminated onExited:") >= 0, true)
    var key = "math\n#101315|#c0caf5|||||||monospace|14|7|0|0\ntrue\nx^2"
    var fake = store()
    fake.root.ask(7, [key])
    fake.start()
    check("a known query is owed a reply, counted by a property the reply timer can bind", fake.root.owed, 1)
    check("an idle stop is refused while a reply is owed", fake.root.stop(), false)
    check("the refused stop neither kills the store nor closes its stdin", fake.signals.length === 0 && fake.process.running && fake.process.stdinEnabled && !fake.root.stopping, true)
    fake.reply({ id: 7, known: true })
    check("the owed reply still lands after the refused stop", fake.knowns.length === 1 && fake.knowns[0].all === true && fake.root.owed === 0 && fake.root.available, true)

    check("with nothing owed the stop is accepted", fake.root.stop(), true)
    check("it closes stdin and leaves the store running to drain, never a kill", fake.process.stdinEnabled === false && fake.process.running && fake.signals.length === 0, true)
    fake.exit()
    check("the exit at EOF after a stop is no failure", fake.root.available && fake.root.exits === 1 && !fake.root.stopping, true)

    fake = store()
    fake.root.put(key, "<svg/>")
    fake.start()
    check("a put is written to the store", fake.writes.length === 1 && fake.writes[0].op === "put" && fake.root.puts === 1, true)
    check("a put has no reply, so a stop right after it is accepted", fake.root.stop(), true)
    check("the stop drains the put by closing stdin instead of killing the store", fake.process.running && fake.process.stdinEnabled === false && fake.signals.length === 0, true)
    fake.root.get(8, key)
    check("a line sent while the store drains waits for the next store", fake.writes.length === 1 && fake.root.queued.length === 1 && fake.root.owed === 1, true)
    fake.exit()
    check("the exit starts the store again for the queued line, with stdin open", fake.root.starting && fake.process.running && fake.process.stdinEnabled, true)
    fake.start()
    check("the queued get reaches the new store", fake.writes.length === 2 && fake.writes[1].op === "get" && fake.root.available, true)

    fake = store()
    fake.root.get(9, key)
    fake.start()
    fake.now = fake.root.replyMs + 1
    fake.tick()
    check("a reply later than replyMs ends the store and falls to the helper as a miss", fake.root.available === false && fake.answers.length === 1 && fake.answers[0].svg === "" && fake.root.owed === 0, true)
    check("the late store is killed", fake.signals.length === 1 && fake.signals[0] === fake.root.killSignal, true)
    fake = store()
    fake.root.get(10, key)
    fake.start()
    fake.now = fake.root.replyMs - 1
    fake.tick()
    check("a reply inside replyMs is waited for", fake.root.available === true && fake.answers.length === 0, true)

    // A line queued behind a drain that ends at once is owed only once it is written, so the drain never counts against its reply.
    fake = store()
    fake.root.put(key, "<svg/>")
    fake.start()
    check("a stop with only a put in flight drains", fake.root.stop(), true)
    fake.root.get(12, key)
    fake.now = 1
    fake.tick()
    check("a line queued behind a drain is not late inside drainMs", fake.root.available === true && fake.answers.length === 0 && fake.signals.length === 0, true)
    fake.exit()
    fake.start()
    fake.tick()
    check("its reply clock starts when it is written to the new store", fake.root.available === true && fake.answers.length === 0 && fake.writes.length === 2, true)
    fake.now += fake.root.replyMs + 1
    fake.tick()
    check("and a reply that then outlasts replyMs still ends the store", fake.root.available === false && fake.answers.length === 1, true)

    // A slow honest drain keeps its store: past drainMs what waits behind it is a miss for the helper, the store is not killed and its put reaches disk.
    fake = store()
    fake.root.put(key, "<svg/>")
    fake.start()
    check("a stop with only a put in flight drains", fake.root.stop(), true)
    fake.root.get(14, key)
    fake.root.ask(15, [key])
    fake.root.put(key, "<svg>2</svg>")
    fake.now = fake.root.drainMs - 1
    fake.tick()
    check("a queued line inside drainMs is still waiting", fake.answers.length === 0 && fake.knowns.length === 0 && fake.root.owed === 2, true)
    fake.now = fake.root.drainMs + 1
    fake.tick()
    check("past drainMs a queued get and known are misses, for the helper to draw", fake.answers.length === 1 && fake.answers[0].id === 14 && fake.answers[0].svg === "" && fake.knowns.length === 1 && fake.knowns[0].all === false && fake.root.owed === 0, true)
    check("the draining store is not killed, latched or stopped early", fake.signals.length === 0 && fake.root.hangKills === 0 && fake.root.available === true && fake.root.stopping === true && fake.process.running, true)
    check("only the queued put waits for the next store", fake.root.queued.length === 1 && fake.root.queued[0].id === undefined, true)
    fake.exit()
    fake.start()
    check("the drain's exit starts a clean store that gets the put", fake.root.available === true && fake.writes.length === 2 && fake.writes[1].op === "put" && fake.root.owed === 0 && fake.signals.length === 0, true)

    // A line queued behind a slow start waits past drainMs, then the start writes it and its reply lands.
    fake = store()
    fake.root.ask(16, [key])
    check("a query while the store is down queues behind its start", fake.root.queued.length === 1 && fake.root.starting && fake.root.owed === 1, true)
    fake.now = fake.root.drainMs + 1
    fake.tick()
    check("a queued line behind a start is not lapsed past drainMs", fake.knowns.length === 0 && fake.root.owed === 1, true)
    fake.start()
    check("the start writes the queued line", fake.writes.length === 1 && fake.writes[0].op === "known", true)
    fake.reply({ id: 16, known: true })
    check("its reply lands when the start does", fake.knowns.length === 1 && fake.knowns[0].all === true && fake.root.owed === 0, true)

    // A store that ignores EOF is hung at drainHangMs and killed, never latched; its lines were answered at drainMs and its queued puts go to the next store.
    fake = store()
    fake.root.put(key, "<svg/>")
    fake.start()
    check("a stop with only a put in flight drains", fake.root.stop(), true)
    fake.root.get(14, key)
    fake.root.put(key, "<svg>2</svg>")
    fake.now = fake.root.drainMs + 1
    fake.tick()
    check("the line behind the hung drain is a miss at drainMs, before any kill", fake.answers.length === 1 && fake.signals.length === 0, true)
    fake.hang()
    check("the drain hang bound kills the store", fake.signals.length === 1 && fake.signals[0] === fake.root.killSignal && fake.root.hangKills === 1, true)
    check("it latches nothing and keeps only the queued put", fake.root.available === true && fake.root.queued.length === 1 && fake.root.queued[0].id === undefined, true)
    fake.exit()
    check("the exit is no failure and starts a clean store with stdin open", fake.root.available === true && fake.root.starting && fake.process.stdinEnabled && !fake.root.stopping && fake.root.exits === 1, true)
    fake.start()
    check("the next store gets the put and nothing stale", fake.writes.length === 2 && fake.writes[1].op === "put" && fake.root.owed === 0 && fake.signals.length === 1, true)
    fake.root.get(16, key)
    fake.reply({ id: 16, miss: true })
    check("and answers the next get itself", fake.answers.length === 2 && fake.answers[1].id === 16 && fake.root.misses === 1, true)

    // A store whose spawn fails latches off and stops reading as active, so a later figure never waits on a process that never came.
    fake = store()
    fake.root.get(17, key)
    check("a get starts the store", fake.root.starting && fake.process.running && fake.active(), true)
    fake.spawnFail()
    check("the failed spawn latches the store off and answers the get as a miss", fake.root.available === false && fake.answers.length === 1 && fake.answers[0].svg === "" && fake.root.owed === 0, true)
    check("the failed store no longer reads as active", fake.active(), false)

    fake = store()
    fake.root.ask(11, [key])
    fake.start()
    var threw = false
    try {
        fake.root.receive("null")
        fake.root.receive("not json")
    } catch (e) {
        threw = true
    }
    check("a null or garbled line from the store is ignored", threw === false && fake.root.owed === 1 && fake.root.available, true)
}
