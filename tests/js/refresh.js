.import "../../ui/js/Anchor.js" as Anchor
.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Menu.js" as Menu
.import "watchfixture.js" as Fixture

// F5 and the empty-space Refresh: the watched re-read in tests/js/watch.js, asked for rather
// than waited for, with the guards that hold it while an answer is still to come.

function run(check) {
    // F5: the same anchored re-read, asked for rather than waited for.
    var asked = Fixture.watched(0, [{ n: "a" }, { n: "b" }, { n: "c" }], 2)
    var askedAnchor = Anchor.manual(asked)
    check("F5 lists the same directory again", asked.sent.join(","), "list /home/gm,fsinfo")
    check("and anchors on the cursor's name without selecting it",
          askedAnchor.name + "|" + askedAnchor.select, "c|false")
    check("and keeps the filter", asked.filterQuery, "scr")
    check("and clears a selection rather than re-pointing it", asked.cleared, 1)
    var early = Fixture.watched(0, [{ n: "a" }], 0)
    early.listInFlight = true
    check("F5 while a list is in flight sends nothing and says why",
          (Anchor.manual(early) === null) + "|" + early.sent.length + "|" + early.said.join(""),
          "true|0|A directory is already loading.")
    // busy() holds for these too, and they are not the operator's to override: each answer still to
    // come names a row by index, and a re-read would hand it a row that is now another file.
    var renaming = Fixture.watched(0, [{ n: "a" }], 0)
    renaming.renamePending = true
    check("F5 while a rename is finishing sends nothing and says why",
          (Anchor.manual(renaming) === null) + "|" + renaming.sent.length + "|" + renaming.said.join(""),
          "true|0|Rename is still finishing.")
    // Menu actions bypass the keyboard's live-editor guard, including over search results.
    var refresh = Menu.backgroundEntries({}).filter(function (row) { return row.action === "refresh" })[0]
    for (var mode of ["", "results"]) {
        var editing = Fixture.watched(0, [{ n: "a" }], 0)
        var editor = { text: "unfinished name" }
        editing.searchMode = mode
        editing.searchQuery = "a"
        editing.searchFrom = editing.path
        editing.home = editing.path
        editing.renamePending = false
        editing.renamingIndex = 0
        editing.renameEditor = function () { return editing.renamingIndex >= 0 ? editor : null }
        editing.refreshListing = function () { Anchor.manual(editing) }
        editing.backend.search = function () { editing.sent.push("search") }
        Focus.act(refresh.action, editing)
        check("background Refresh preserves a live rename in " + mode,
              editing.renameEditor() === editor && editor.text === "unfinished name", true)
        check("background Refresh sends nothing and explains the live rename in " + mode,
              editing.sent.length + "|" + editing.cleared + "|" + editing.said.join(""),
              "0|0|Finish or cancel the rename first.")
    }
    var clash = Fixture.watched(0, [{ n: "a" }], 0)
    clash.collide = { pending: { c: "transfer" } }
    check("F5 while the card asks about existing files sends nothing and says why",
          (Anchor.manual(clash) === null) + "|" + clash.sent.length + "|" + clash.said.join(""),
          "true|0|Choose what to do about the existing files first.")
    // Search results are a walk, not a directory, so F5 runs the query again over the same scope.
    var results = Fixture.watched(0, [{ n: "src/a.rs" }], 0)
    results.searchMode = "results"
    results.searchQuery = "a.rs"
    results.searchFrom = "/home/gm/Code"
    results.home = "/home/gm"
    results.backend.search = function (scope, query, hidden) { results.sent.push("search " + scope + " " + query) }
    check("F5 over search results runs the search again and lists nothing",
          (Anchor.manual(results) === null) + "|" + results.sent.join(","), "true|search /home/gm a.rs")
    check("and keeps where the search began", results.searchFrom, "/home/gm/Code")
    results.sent = []
    results.searchRunning = false
    results.collide = { pending: { c: "transfer" } }
    var resultRows = results.rows
    check("F5 preserves search rows while a collision answer is pending",
          (Anchor.manual(results) === null) + "|" + results.sent.length + "|"
          + (results.rows === resultRows) + "|" + results.said.join(""),
          "true|0|true|Choose what to do about the existing files first.")
    results.collide.pending = null
    results.said = []
    // A committed rename keeps its request after its row scrolls out and the editor goes with it.
    results.renamePending = true
    check("F5 preserves search rows while a rename is finishing",
          (Anchor.manual(results) === null) + "|" + results.sent.length + "|"
          + (results.rows === resultRows) + "|" + results.said.join(""),
          "true|0|true|Rename is still finishing.")
    results.renamePending = false
    results.said = []
    results.sent = []
    results.searchRunning = true
    check("F5 over a search still walking sends nothing and says why",
          (Anchor.manual(results) === null) + "|" + results.sent.length + "|" + results.said.join(""),
          "true|0|The search is still running.")
}
