.import "../../ui/js/UiState.js" as UiState

// ui/ViewState.qml's writer bookkeeping and the patch it builds. The window shows the column change
// the instant it is made and the state file learns about it through a process, so the only thing
// that can tell the two apart is the writer's own exit status. A book that records the patch before
// the process runs reports a save that never happened, and then refuses the retry that would have
// fixed it.

var OLD = "{\"columns\":[\"name\",\"size\",\"date\"]}"
var NEW = "{\"columns\":[\"name\",\"date\"]}"
var THIRD = "{\"columns\":[\"name\"]}"

function run(check) {
    check("a fresh book has landed nothing of its own", UiState.book().saved, "")
    check("and nothing is in flight behind it", UiState.book().inflight, "")
    check("so the first change a window makes starts a write", UiState.asked(UiState.book(), NEW).start, NEW)

    // A book whose last writer landed OLD, which is what the short-circuit compares against. A fresh
    // book cannot stand in for it: a window's own read of ui.json is not a patch that window sent.
    var held = { saved: OLD, inflight: "", pending: "" }
    var same = UiState.asked(held, OLD)
    check("a patch the last landed write already stored starts nothing", same.start, "")

    var asked = UiState.asked(held, NEW)
    check("a change starts a write", asked.start, NEW)
    check("and the change is in flight, not saved", asked.inflight, NEW)
    check("the file is still known to hold what it held", asked.saved, OLD)

    // Scenario A: ~/.local/state is unwritable, so src/main.rs prints its sentence and exits 2.
    var refused = UiState.exited(asked, 2, NEW)
    check("a refused write is not believed", refused.saved, OLD)
    check("a refused write is reported", refused.failed, true)
    check("and it leaves no writer in flight", refused.inflight, "")

    // The whole cost of believing it: the identical toggle can never even be attempted again.
    check("an identical retry is attempted after a refusal", UiState.asked(refused, NEW).start, NEW)

    var landed = UiState.exited(asked, 0, "")
    check("a write that exited zero is believed", landed.saved, NEW)
    check("and it is not reported", landed.failed, false)
    check("a patch the landed write already stored starts nothing", UiState.asked(landed, NEW).start, "")

    // One writer at a time, and the newest patch waits rather than being dropped on the floor.
    var queued = UiState.asked(asked, THIRD)
    check("a second change queues behind the running writer", queued.pending, THIRD)
    check("and starts nothing of its own", queued.start, "")
    check("the running writer still carries the first", queued.inflight, NEW)
    // Toggling back to what the queue already asks for must not send the same bytes twice.
    check("the queued patch is not sent twice", UiState.asked(queued, THIRD).start, "")

    // The window's own read of the file. main() leaves a ui.json it cannot read as a JSON object
    // exactly as the operator wrote it, so the window draws the defaults and has to be what says so.
    check("a document the window can read is read", UiState.fromFile(OLD).state.columns[2], "date")
    check("and nothing is said about it", UiState.fromFile(OLD).unreadable, false)
    check("a trailing comma reads as the full default shape", JSON.stringify(UiState.fromFile("{\"columns\":[\"name\"],}").state), "{}")
    check("and the pane is told to say so", UiState.fromFile("{\"columns\":[\"name\"],}").unreadable, true)
    check("a document that is not an object is the same", UiState.fromFile("[1,2]").unreadable, true)
    check("no file at all is a first launch and says nothing", UiState.fromFile("").unreadable, false)
    var favouritesDraft = { view: "columns", places: { sidebarWidth: 256, favourites: [{ label: "Before", path: "/before" }] } }
    var refresh = UiState.refreshedFavourites(favouritesDraft, '{"view":"grid","places":{"sidebarWidth":160,"favourites":[17,{"label":"New","path":"/new"},{"label":"New","path":"/new"}]}}')
    check("external favourites preserve invalid and duplicate originals", JSON.stringify(refresh.state.places.favourites), '[17,{"label":"New","path":"/new"},{"label":"New","path":"/new"}]')
    check("external favourites leave the active view alone", refresh.state.view, "columns")
    check("external favourites leave other Places drafts alone", refresh.state.places.sidebarWidth, 256)
    check("refresh does not mutate its input", favouritesDraft.places.favourites[0].label, "Before")
    check("malformed external bytes retain visible records", UiState.refreshedFavourites(favouritesDraft, '{').state, favouritesDraft)
    check("malformed external bytes report a read error", UiState.refreshedFavourites(favouritesDraft, '{').error.length > 0, true)
    check("empty live bytes retain visible records", UiState.refreshedFavourites(favouritesDraft, '').state, favouritesDraft)
    check("empty live bytes report a read error", UiState.refreshedFavourites(favouritesDraft, '').error.length > 0, true)
    check("nonarray external favourites retain visible records", UiState.refreshedFavourites(favouritesDraft, '{"places":{"favourites":17}}').state, favouritesDraft)
    check("malformed Places group retains visible records", UiState.refreshedFavourites(favouritesDraft, '{"places":[]}').state, favouritesDraft)
    check("missing favourites group is the empty new store", UiState.refreshedFavourites(favouritesDraft, '{}').state.places.favourites.length, 0)
    check("unchanged favourites keep the existing state object", UiState.refreshedFavourites(favouritesDraft, JSON.stringify(favouritesDraft)).state, favouritesDraft)
    var a = {label:"A", path:"/a"}, b = {label:"B", path:"/b"}, c = {label:"C", path:"/c"}
    var before = [a, b], ownAdd = UiState.favouritesAfter(before, {op:"add", record:c})
    check("own add expects every original sibling plus its new record", JSON.stringify(ownAdd), JSON.stringify([a, b, c]))
    check("a concurrent removal in the successful response is still external", JSON.stringify(ownAdd) === JSON.stringify([b, c]), false)
    check("own reorder preserves records and predicts their new order", JSON.stringify(UiState.favouritesAfter(before, {op:"move", index:1, to:0})), JSON.stringify([b, a]))
    check("own removal predicts only its captured index removal", JSON.stringify(UiState.favouritesAfter(before, {op:"remove", index:0})), JSON.stringify([b]))
    check("own relabel preserves its stored path", JSON.stringify(UiState.favouritesAfter(before, {op:"rename", index:1, label:"Renamed"})), JSON.stringify([a, {label:"Renamed", path:"/b"}]))
    check("expected operation never mutates the held records", JSON.stringify(before), JSON.stringify([a, b]))

    // The document the window holds, rebuilt rather than mutated: a var property notifies on
    // assignment and not on a reach-in, and a whole-group assignment would take one writer's half
    // of display or places as the whole of it.
    var held = { columns: ["name"], display: { textSize: { mode: "system" } } }
    check("one key is replaced and the rest carried",
          JSON.stringify(UiState.withKey(held, "keys", "windows")),
          '{"columns":["name"],"display":{"textSize":{"mode":"system"}},"keys":"windows"}')
    check("a group merges into what is beside it rather than replacing the group",
          JSON.stringify(UiState.withGroup({ places: { sidebarWidth: 240, showHome: true } },
                                           "places", { showHome: false })),
          '{"places":{"sidebarWidth":240,"showHome":false}}')
    check("a group that is not there yet is created",
          JSON.stringify(UiState.withGroup({}, "display", { textSize: { mode: 16 } })),
          '{"display":{"textSize":{"mode":16}}}')
    check("and neither writer mutates the document it was handed",
          JSON.stringify(held), '{"columns":["name"],"display":{"textSize":{"mode":"system"}}}')

    // The lost update this whole file exists to keep out. Two windows read one document; window A
    // changes the keys preset and window B, still holding the read from before that change, saves a
    // text size. What B owes the state file is built over an EMPTY document rather than over its own
    // read, so the patch names what B changed and nothing else: src/uistate.rs merges key by key and
    // keeps every key a patch leaves out, and no lock can protect a key the caller overwrites by name.
    var owed = UiState.withGroup({}, "display", { textSize: { mode: 16 } })
    check("a text-size change owes the text size alone",
          JSON.stringify(owed), '{"display":{"textSize":{"mode":16}}}')
    check("and the patch cannot name keys at all", JSON.stringify(owed).indexOf("keys"), -1)
    check("nor columns", JSON.stringify(owed).indexOf("columns"), -1)
    check("nor the hidden menu set", JSON.stringify(owed).indexOf("hidden"), -1)
    check("a window that has changed nothing owes an empty patch", JSON.stringify({}), "{}")

    // The coalesce the old whole-document snapshot used to provide: a second change behind a running
    // writer joins the patch already owed rather than replacing it, so neither is dropped.
    check("a second change joins the patch already owed",
          JSON.stringify(UiState.withKey(owed, "keys", "windows")),
          '{"display":{"textSize":{"mode":16}},"keys":"windows"}')
    check("and a second change to the same group joins it too",
          JSON.stringify(UiState.withGroup(owed, "display", { hidden: ["paste"] })),
          '{"display":{"textSize":{"mode":16},"hidden":["paste"]}}')

    // The two documents differ, and that is the point of running the rebuild over both. A sub-key a
    // newer Flea left in `display` stays in what this window DRAWS, and never enters the patch: this
    // Flea has no rule for it, and src/uistate.rs refuses a whole patch that names one.
    var read = { display: { textSize: { mode: "system" }, aKeyThisBuildHasNeverHeardOf: true } }
    check("the document keeps a newer Flea's own sub-key",
          JSON.stringify(UiState.withGroup(read, "display", { textSize: { mode: 16 } })),
          '{"display":{"textSize":{"mode":16},"aKeyThisBuildHasNeverHeardOf":true}}')
    check("and the patch beside it never carries one", JSON.stringify(owed).indexOf("NeverHeardOf"), -1)

    var drained = UiState.exited(queued, 0, THIRD)
    check("the queued patch starts when the writer exits", drained.start, THIRD)
    check("and the exited writer's own patch is what the file now holds", drained.saved, NEW)
    check("a refusal underneath a queue still runs the queue", UiState.exited(queued, 2, THIRD).start, THIRD)
    check("and still reports the refusal", UiState.exited(queued, 2, THIRD).failed, true)
    check("a writer that exits with nothing behind it starts nothing", UiState.exited(asked, 0, "{}").start, "")
    // A size stepped up and back down under one writer: what was queued differs from what is in
    // flight, so it queued, and the landed patch then takes the whole of it back out again.
    check("a queued writer with nothing left owed under it is not launched",
          UiState.exited(queued, 0, "{}").start, "")
    check("and nothing is left in flight for the pane to wait on",
          UiState.exited(queued, 0, "{}").inflight, "")

    // What a landed writer stored comes out of what the window owes, and only that. Waiting for the
    // whole queue to drain instead left a stored setting owed, so it rode along inside every later
    // patch and overwrote whatever another window or the CLI had put there in the meantime.
    check("a landed setting is no longer owed",
          JSON.stringify(UiState.acknowledged({ keys: "windows" }, '{"keys":"windows"}')), "{}")
    check("a setting the writer never carried stays owed",
          JSON.stringify(UiState.acknowledged({ keys: "windows", columns: ["name"] }, '{"keys":"windows"}')),
          '{"columns":["name"]}')
    check("an array setting is compared whole",
          JSON.stringify(UiState.acknowledged({ columns: ["name", "size"] }, '{"columns":["name","size"]}')), "{}")
    check("and a different array is not cleared by it",
          JSON.stringify(UiState.acknowledged({ columns: ["name"] }, '{"columns":["name","size"]}')),
          '{"columns":["name"]}')

    // Leaf-accurate, because changeLeaf owes the leaf alone: clearing the group would drop a leaf
    // beside it that no writer has taken yet, and keeping the group would re-send the one that landed.
    var twoLeaves = { display: { textSize: { mode: 16 }, hidden: ["paste"] } }
    check("a landed leaf is cleared out of the group it sits in",
          JSON.stringify(UiState.acknowledged(twoLeaves, '{"display":{"textSize":{"mode":16}}}')),
          '{"display":{"hidden":["paste"]}}')
    check("and the group goes when its last owed leaf does",
          JSON.stringify(UiState.acknowledged({ display: { textSize: { mode: 16 } } },
                                              '{"display":{"textSize":{"mode":16}}}')), "{}")
    check("and nothing is mutated in place",
          JSON.stringify(twoLeaves), '{"display":{"textSize":{"mode":16},"hidden":["paste"]}}')
    check("bytes that are not a patch clear nothing",
          JSON.stringify(UiState.acknowledged({ keys: "windows" }, "not json")), '{"keys":"windows"}')

    // A change made WHILE the writer ran is a newer value for a setting that writer carried, and the
    // file does not have that one: the older value landing must not clear it.
    check("a newer value for a landed setting is still owed",
          JSON.stringify(UiState.acknowledged({ keys: "mac" }, '{"keys":"windows"}')), '{"keys":"mac"}')
    check("and a newer value for a landed leaf is too",
          JSON.stringify(UiState.acknowledged({ display: { textSize: { mode: 20 } } },
                                              '{"display":{"textSize":{"mode":16}}}')),
          '{"display":{"textSize":{"mode":20}}}')

    // The interleave all of the above exists for, driven through the book. A window saves the keys
    // preset; while that writer runs it changes the text size, so the queued patch carries both. The
    // preset lands, and what the queued writer STARTS with has to be rebuilt from what is still owed:
    // the bytes waiting in `pending` were built before the preset landed and still name it.
    var ownKeys = UiState.withKey({}, "keys", "windows")
    var alsoSize = UiState.withGroup(ownKeys, "display", { textSize: { mode: 16 } })
    var first = UiState.asked(UiState.book(), JSON.stringify(ownKeys))
    check("the preset starts a writer", first.start, '{"keys":"windows"}')
    var behind = UiState.asked(first, JSON.stringify(alsoSize))
    check("the text size queues behind it carrying both",
          behind.pending, '{"keys":"windows","display":{"textSize":{"mode":16}}}')
    var stillOwes = UiState.acknowledged(alsoSize, behind.inflight)
    check("the landed preset drops out of what is owed",
          JSON.stringify(stillOwes), '{"display":{"textSize":{"mode":16}}}')
    var drainedAfter = UiState.exited(behind, 0, JSON.stringify(stillOwes))
    check("so the queued writer starts with the text size alone",
          drainedAfter.start, '{"display":{"textSize":{"mode":16}}}')
    check("and cannot name the preset another window may have changed since",
          drainedAfter.start.indexOf("keys"), -1)

    // The failure arm of the same interleave: nothing landed, so nothing is acknowledged and the
    // queued writer still carries the refused setting as well as the newer one.
    var refusedUnder = UiState.exited(behind, 2, JSON.stringify(alsoSize))
    check("a refused writer leaves its own setting in the patch behind it",
          refusedUnder.start, '{"keys":"windows","display":{"textSize":{"mode":16}}}')
    check("and the refusal is still reported", refusedUnder.failed, true)
}
