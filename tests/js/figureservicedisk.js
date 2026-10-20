.import "figurestore.js" as StoreSuite

// Drive the persistent figure cache's routing through the production handlers: the disk answers first, and a miss reaches the helper whole.
function run(check, service) {
    var fake = service()
    fake.disk.available = true
    var remembered = fake.ask("remembered", true)
    check("an ask with the cache available asks the disk first", fake.disk.gets.length === 1 && fake.root.pending.length === 0 && !fake.root.starting, true)
    fake.root.storeAnswered(remembered, "disk svg")
    check("a disk hit answers the figure with no helper", fake.answers[fake.answers.length - 1].svg === "disk svg" && fake.writes.length === 0 && !fake.root.starting, true)
    var unseen = fake.ask("unseen", true)
    fake.root.storeAnswered(unseen, "")
    check("a disk miss starts the helper", fake.root.starting, true)
    fake.start()
    fake.reply(unseen, "drawn svg")
    check("the helper's answer is written to the disk cache", fake.disk.puts.length === 1 && fake.disk.puts[0].svg === "drawn svg", true)

    // A figure drawn under one theme or display mode does not warm a document asked under another, so the warm query names each figure's full cache key.
    fake = service()
    fake.disk.available = true
    var themeA = { bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 14 }
    var themeB = { bg: "#101315", fg: "#c0caf5", accent: "#f7768e", font: "monospace", bodyPx: 14 }
    var keys = [fake.root.cacheKeyOf("math", "x^2", themeA, true), fake.root.cacheKeyOf("mermaid", "flowchart TD\n    A --> B", themeB, true)];
    var warmed = fake.root.warm(keys, ["math", "mermaid"])
    var asked = fake.disk.asks[0]
    check("a warm query asks the store before a helper starts", warmed === true && fake.disk.asks.length === 1 && !fake.root.starting, true)
    check("it names each figure by the key an ask would use, theme and display mode included", asked !== undefined && asked.keys.length === 2
        && asked.keys[0] === fake.root.cacheKeyOf("math", "x^2", themeA, true) && asked.keys[1] === fake.root.cacheKeyOf("mermaid", "flowchart TD\n    A --> B", themeB, true), true)
    check("another accent is another key", asked !== undefined && asked.keys[0] !== fake.root.cacheKeyOf("math", "x^2", themeB, true), true)
    fake.root.warmAnswered(fake.root.warmQuery, false)
    check("a query the store does not know starts the helper warm", fake.root.starting, true)

    // The engine's refusal leaves a ticket still with the disk to the disk's answer, which then draws it once or fails it once.
    fake = service()
    fake.disk.available = true
    var onDisk = fake.ask("still with the disk", true)
    fake.root.refuse("figure engine exited 127 (status 0)")
    check("a refusal does not fail a ticket still with the persistent cache", fake.answers.length, 0)
    fake.root.storeAnswered(onDisk, "disk svg")
    check("the disk's hit then draws it, once", fake.answers.length === 1 && fake.answers[0].svg === "disk svg" && fake.answers[0].error === "", true)
    fake = service()
    fake.disk.available = true
    var missed = fake.ask("the disk misses it", true)
    var other = fake.ask("not on disk either", true)
    fake.root.refuse("figure engine exited 127 (status 0)")
    fake.root.storeAnswered(missed, "")
    check("a disk miss after a refusal fails the ticket once, as the engine did not start", fake.answers.length === 1 && fake.answers[0].id === missed && fake.answers[0].error === "figure engine did not start", true)
    fake.root.storeAnswered(other, "")
    check("each ticket still with the disk is failed once and none twice", fake.answers.length === 2 && fake.answers[1].id === other && Object.keys(fake.root.waiting).length === 0, true)

    // A store still active after its idle stop (refused for a reply it owes, or draining and perhaps restarted for a queued put) is asked again at the next idle, so it never runs on for the session.
    fake = service()
    fake.disk.available = true
    fake.disk.active = true
    fake.disk.refuse = true
    fake.idle()
    check("a refused idle stop re-arms the idle timer", fake.disk.stops === 1 && fake.idleTimer.running, true)
    fake.disk.refuse = false
    fake.idleTimer.stop()
    fake.idle()
    check("the next idle, with the reply landed, stops the store", fake.disk.stops === 2 && fake.disk.stopping, true)
    check("an accepted stop still re-arms it, for a store the drain may restart to commit a queued put", fake.idleTimer.running, true)
    fake = service()
    fake.disk.stopping = true
    fake.disk.active = true
    fake.disk.refuse = true
    fake.idle()
    check("a store still draining is asked again at the next idle", fake.idleTimer.running, true)
    fake = service()
    fake.idle()
    check("a store that is not running is not asked again", fake.idleTimer.running, false)

    StoreSuite.run(check)
}
