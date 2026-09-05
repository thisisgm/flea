.import "../../ui/js/UiState.js" as UiState

// ui/ViewState.qml's writer bookkeeping. The window shows the column change the instant it is made
// and the state file learns about it through a process, so the only thing that can tell the two
// apart is the writer's own exit status. A book that records the patch before the process runs
// reports a save that never happened, and then refuses the retry that would have fixed it.

var OLD = "{\"columns\":[\"name\",\"size\",\"date\"]}"
var NEW = "{\"columns\":[\"name\",\"date\"]}"
var THIRD = "{\"columns\":[\"name\"]}"

function run(check) {
    check("a fresh book holds what the file it read already has", UiState.book(OLD).saved, OLD)
    check("and nothing is in flight behind it", UiState.book(OLD).inflight, "")

    // The state file already holds this, so ui/shell.qml's scale step writes nothing.
    var same = UiState.asked(UiState.book(OLD), OLD)
    check("a patch the file already holds starts nothing", same.start, "")

    var asked = UiState.asked(UiState.book(OLD), NEW)
    check("a change starts a write", asked.start, NEW)
    check("and the change is in flight, not saved", asked.inflight, NEW)
    check("the file is still known to hold what it held", asked.saved, OLD)

    // Scenario A: ~/.local/state is unwritable, so src/main.rs prints its sentence and exits 2.
    var refused = UiState.exited(asked, 2)
    check("a refused write is not believed", refused.saved, OLD)
    check("a refused write is reported", refused.failed, true)
    check("and it leaves no writer in flight", refused.inflight, "")

    // The whole cost of believing it: the identical toggle can never even be attempted again.
    check("an identical retry is attempted after a refusal", UiState.asked(refused, NEW).start, NEW)

    var landed = UiState.exited(asked, 0)
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

    var drained = UiState.exited(queued, 0)
    check("the queued patch starts when the writer exits", drained.start, THIRD)
    check("and the exited writer's own patch is what the file now holds", drained.saved, NEW)
    check("a refusal underneath a queue still runs the queue", UiState.exited(queued, 2).start, THIRD)
    check("and still reports the refusal", UiState.exited(queued, 2).failed, true)
}
