.pragma library

// ui/ViewState.qml's one-writer bookkeeping, and nothing else: `saved` is what the state file is
// known to hold, `inflight` is what the running `flea --ui-state` carries, and `pending` is the
// newest patch waiting behind it. Imports no QML, so tests/js/uistate.js can redden on a mutation.

// The window's own read of ui.json. main() leaves a document it cannot read as a JSON object
// exactly as the operator wrote it, so `unreadable` is what makes the pane say the file was not used;
// no file at all is a first launch and says nothing.
function fromFile(text) {
    try {
        var found = JSON.parse(text)
        if (found && typeof found === "object" && !Array.isArray(found)) {
            return { state: found, unreadable: false }
        }
    } catch (e) {
        // A hand edit this cannot parse, which is the ordinary way in and is not an error here.
    }
    return { state: {}, unreadable: text.length > 0 }
}

// The book a window starts with: what the file it has just read already holds, and no writer running.
function book(saved) {
    return { saved: saved || "", inflight: "", pending: "" }
}

// A change asks for a write. The answer is the next book plus `start`, the patch to launch now.
function asked(b, patch) {
    // The bytes the file will hold once everything already on its way has landed, so a toggle back
    // to what a running writer is carrying sends nothing and a refused one is never short-circuited.
    if (patch === (b.pending || b.inflight || b.saved)) {
        return { saved: b.saved, inflight: b.inflight, pending: b.pending, start: "" }
    }
    // One writer at a time, and the newest patch waits rather than being dropped on the floor.
    if (b.inflight.length > 0) {
        return { saved: b.saved, inflight: b.inflight, pending: patch, start: "" }
    }
    return { saved: b.saved, inflight: patch, pending: "", start: patch }
}

// The writer exited. The answer is the next book plus `start`, and `failed` for the pane to report.
function exited(b, code) {
    // Only a zero status proves the patch reached the file: src/main.rs exits 2 on a refused patch
    // and on a state directory it could not write, and the change is on screen either way.
    return {
        saved: code === 0 ? b.inflight : b.saved,
        inflight: b.pending,
        pending: "",
        start: b.pending,
        failed: code !== 0
    }
}
