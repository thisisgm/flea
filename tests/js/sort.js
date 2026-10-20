.import "../../ui/js/Sort.js" as Sort
.import "sourcefixture.js" as Source

// Header clicks and the s/S keys share this path; tests/protocol.sh checks the resulting backend order.

// Only the members the sort path touches. The backend stub records the wire in order, because sort
// before window is the protocol's own rule and a window sent first would read the old order's rows.
function pane(sortBy, sortDesc) {
    var p = {
        windowSize: 200,
        total: 40,
        recentMode: "",
        thumbState: "stale",
        dirSizeState: "stale",
        cursor: -1,
        cleared: 0,
        said: [],
        sent: [],
        renamingIndex: -1,
        renamePending: false,
        pendingSort: null,
        listInFlight: false,
        committed: 0
    }
    p.message = function (text, isError) { p.said.push(text) }
    p.clearSelection = function () { p.cleared += 1 }
    p.setCursor = function (index) { p.cursor = index }
    p.commitOpenRename = function () { p.committed += 1 }
    p.backend = {
        sortBy: sortBy,
        sortDesc: sortDesc,
        running: true,
        quitting: false,
        sort: function (by, desc) { p.sent.push("sort " + by + " " + (desc ? "desc" : "asc")) },
        window: function (start, count) { p.sent.push("window " + start + " " + count) }
    }
    return p
}

function run(check) {
    // The mark moves on the click, not on the reply: the OEM optimistic rule this project follows.
    var same = pane("name", false)
    Sort.column(same, "name")
    check("clicking the sorted column asks for the reverse, then the reordered window",
          same.sent.join(","), "sort name desc,window 0 200")
    check("and the recorded direction flips at once, with no round trip", same.backend.sortDesc, true)
    check("and the recorded column stays name", same.backend.sortBy, "name")

    var back = pane("name", true)
    Sort.column(back, "name")
    check("clicking it again returns it to ascending", back.sent.join(","), "sort name asc,window 0 200")
    check("and records ascending", back.backend.sortDesc, false)

    // A re-sort moves every row, so everything keyed by a row index is as stale as a new listing's.
    var reset = pane("name", false)
    Sort.column(reset, "name")
    check("a re-sort drops the thumbnail cache", reset.thumbState === "stale", false)
    check("a re-sort drops the directory-size cache", reset.dirSizeState === "stale", false)
    check("a re-sort clears the selection, whose indices now name other files", reset.cleared, 1)
    check("a re-sort puts the cursor back on the first row", reset.cursor, 0)

    // Size and Modified are orders the backend produces, so a click records them the way it
    // records name: the mark, the caches, the selection and the cursor all move on the click.
    var size = pane("name", true)
    Sort.column(size, "size")
    check("a click on Size asks for size ascending, then the reordered window",
          size.sent.join(","), "sort size asc,window 0 200")
    check("and the mark moves onto Size at once, ascending whatever direction name was in",
          size.backend.sortBy + ":" + size.backend.sortDesc, "size:false")
    check("and the thumbnail cache goes with it", size.thumbState === "stale", false)
    check("and the directory-size cache goes with it", size.dirSizeState === "stale", false)
    check("and the selection and the cursor go with it", size.cleared + "|" + size.cursor, "1|0")

    var sizeAgain = pane("size", false)
    Sort.column(sizeAgain, "size")
    check("clicking Size while in size order reverses it", sizeAgain.sent.join(","), "sort size desc,window 0 200")
    check("and records the reverse", sizeAgain.backend.sortDesc, true)

    var date = pane("size", true)
    Sort.column(date, "mtime")
    check("a click on Modified asks for mtime ascending, not the reverse it would inherit",
          date.sent.join(","), "sort mtime asc,window 0 200")
    check("and the mark moves onto Modified", date.backend.sortBy + ":" + date.backend.sortDesc, "mtime:false")

    // Mode is a label, not an order, so it cannot clear selection or send a request.
    var mode = pane("name", true)
    Sort.column(mode, "mode")
    check("a click on Mode sends nothing", mode.sent.join(","), "")
    check("a click on Mode leaves the descending name sort alone", mode.backend.sortDesc, true)
    check("and the selection survives a sort that did not happen", mode.cleared, 0)

    var kind = pane("size", false)
    Sort.column(kind, "kind")
    check("a click on Kind asks for kind ascending, then the reordered window",
          kind.sent.join(","), "sort kind asc,window 0 200")
    check("and the mark moves onto Kind", kind.backend.sortBy + ":" + kind.backend.sortDesc, "kind:false")
    check("and Kind invalidates the thumbnail cache", kind.thumbState === "stale", false)
    check("and Kind invalidates the directory-size cache", kind.dirSizeState === "stale", false)
    check("and Kind resets selection and cursor", kind.cleared + "|" + kind.cursor, "1|0")

    var kindAgain = pane("kind", false)
    Sort.column(kindAgain, "kind")
    check("clicking Kind while in kind order reverses it", kindAgain.sent.join(","), "sort kind desc,window 0 200")

    // The cycle is name, size, mtime, kind; every new order starts ascending.
    var next = pane("name", true)
    Sort.next(next)
    check("s from name steps to size, ascending", next.sent.join(","), "sort size asc,window 0 200")
    check("s records the column it landed on", next.backend.sortBy + ":" + next.backend.sortDesc, "size:false")

    var walked = pane("name", false)
    Sort.next(walked)
    Sort.next(walked)
    Sort.next(walked)
    Sort.next(walked)
    check("four presses walk size, mtime, kind and back to name",
          walked.sent.join(","),
          "sort size asc,window 0 200,sort mtime asc,window 0 200,sort kind asc,window 0 200,sort name asc,window 0 200")
    check("and s says nothing of its own now that every step is a real order", walked.said.length, 0)

    // S reverses whichever order the listing is in, the capital-is-the-variant pair g/G and j/J use.
    var reverse = pane("mtime", false)
    Sort.reverse(reverse)
    check("S reverses the current order", reverse.sent.join(","), "sort mtime desc,window 0 200")
    check("S records the reverse", reverse.backend.sortDesc, true)

    var reverseKind = pane("kind", true)
    Sort.reverse(reverseKind)
    check("S also reverses Kind", reverseKind.sent.join(","), "sort kind asc,window 0 200")

    // Asking for the order the listing is already in would drop every cache and the cursor to redraw
    // the same rows, so it is not asked for at all.
    var noop = pane("size", true)
    Sort.resort(noop, "size", true)
    check("asking for the order already shown sends nothing", noop.sent.length, 0)
    check("and leaves the cursor, the caches and the selection alone",
          noop.cursor + "|" + noop.thumbState + "|" + noop.dirSizeState + "|" + noop.cleared, "-1|stale|stale|0")

    // The same guard must not swallow a real reversal, which is the click every column answers.
    var again = pane("name", false)
    Sort.column(again, "name")
    Sort.column(again, "name")
    check("two clicks on the sorted column are two real re-sorts",
          again.sent.join(","),
          "sort name desc,window 0 200,sort name asc,window 0 200")

    // corner: a recorded order this list does not hold cannot wedge s; it wraps to the first one.
    var stray = pane("unknown", true)
    Sort.next(stray)
    check("s from an order that is not in the list still lands on name",
          stray.sent.join(","), "sort name asc,window 0 200")

    // A walk's rows are matches, so sorting them writes no folder sort.
    var walk = pane("name", false)
    walk.searchMode = "results"
    walk.remembered = []
    walk.backend.rememberFolderSort = function (folder, key, desc) { walk.remembered.push(folder + "|" + key) }
    Sort.resort(walk, "size", false)
    check("sorting a search walk still sorts and re-reads",
          walk.sent.join(","), "sort size asc,window 0 200")
    check("but writes no folder sort for its scope", walk.remembered.length, 0)

    // A sort while an edit is open never retargets the editor: it is held and
    // the open edit is committed the way a click-away commits, applying once
    // the rename settles and never dropped.
    var editing = pane("name", false)
    editing.renamingIndex = 7
    Sort.resort(editing, "size", false)
    check("a sort with an edit open sends nothing", editing.sent.join(","), "")
    check("and holds the requested order instead", JSON.stringify(editing.pendingSort), JSON.stringify({ key: "size", desc: false }))
    check("and commits the open edit like a click-away", editing.committed, 1)
    check("and moves neither the cursor nor the caches under the editor",
          editing.cursor + "|" + editing.thumbState + "|" + editing.dirSizeState + "|" + editing.cleared, "-1|stale|stale|0")
    var pending = pane("name", false)
    pending.renamePending = true
    Sort.resort(pending, "size", false)
    check("a sort with a rename pending sends nothing either", pending.sent.join(","), "")
    check("and holds it without recommitting a write already in flight",
          JSON.stringify(pending.pendingSort) + "|" + pending.committed, JSON.stringify({ key: "size", desc: false }) + "|0")
    var settled = pane("name", false)
    settled.pendingSort = { key: "size", desc: false }
    settled.backend.running = true
    Sort.applyPending(settled)
    check("the held sort applies once the edit ends",
          settled.sent.join(","), "sort size asc,window 0 200")
    check("and is spent, never applied twice",
          JSON.stringify(settled.pendingSort) + "|" + settled.backend.sortBy, "null|size")
    var unsettled = pane("name", false)
    unsettled.pendingSort = { key: "size", desc: false }
    unsettled.renamePending = true
    Sort.applyPending(unsettled)
    check("a rename still pending keeps the hold", unsettled.sent.join(",") + "|" + JSON.stringify(unsettled.pendingSort),
          "|" + JSON.stringify({ key: "size", desc: false }))

    // A navigation during a slow rename strands the hold with no edit open:
    // Nav.forget sets renamingIndex -1 and leaves the request pending, so no
    // index change fires when it settles. The pending-false arm applies it.
    var stranded = pane("name", false)
    stranded.renamingIndex = -1
    stranded.renamePending = true
    Sort.resort(stranded, "size", false)
    check("a sort with only a pending rename holds", JSON.stringify(stranded.pendingSort),
          JSON.stringify({ key: "size", desc: false }))
    stranded.renamePending = false
    Sort.applyPending(stranded)
    check("a hold taken with no edit open applies once the pending rename settles",
          stranded.sent.join(","), "sort size asc,window 0 200")
    var wired = Source.source("ui/Pane.qml")
    // Sample input: "    onRenamePendingChanged: if (!root.renamePending) Sort.applyPending(root)".
    check("the pending-false arm applies the held sort",
          /^\s*onRenamePendingChanged: if \(!root\.renamePending\) Sort\.applyPending\(root\)/m.test(wired), true)
    var inflight = pane("name", false)
    inflight.pendingSort = { key: "size", desc: false }
    inflight.listInFlight = true
    Sort.applyPending(inflight)
    check("a hold outlives its listing and sends nothing while it is out", inflight.sent.join(","), "")
    check("and keeps the hold while the listing is out", JSON.stringify(inflight.pendingSort),
          JSON.stringify({ key: "size", desc: false }))
    inflight.listInFlight = false
    Sort.applyPending(inflight)
    check("the hold applies to the rows that landed", inflight.sent.join(","), "sort size asc,window 0 200")
    var flightSrc = Source.source("ui/Pane.qml")
    // Sample input: "    onListInFlightChanged: if (!root.listInFlight) { preferences.restart(); Sort.applyPending(root) }".
    check("a hold that outlived its listing applies once the rows land",
          /^\s*onListInFlightChanged: if \(!root\.listInFlight\) \{[^}]*Sort\.applyPending\(root\)/m.test(flightSrc), true)
    // A settled ui.json spells out the schema defaults the first save left unwritten; the pane must not re-list for them.
    var bare = Sort.preferences({ view: "list" })
    check("a document that only spells out the defaults is no change", Sort.preferences({ view: "grid", hidden: false,
          foldersFirst: true, groupByKind: false, hiddenLast: false, rememberSort: true }), bare)
    var changes = [{ hidden: true }, { sort: { key: "size", reverse: false } }, { sort: { key: "name", reverse: true } },
                   { foldersFirst: false }, { groupByKind: true }, { hiddenLast: true }, { rememberSort: false }]
    check("every real preference change is a change", changes.filter(function (c) { return Sort.preferences(c) === bare }).length, 0)
    check("the pane compares effective preferences", /^\s*readonly property string listingPreferences: Sort\.preferences\(ViewState\.state\)$/m.test(flightSrc), true)
}
