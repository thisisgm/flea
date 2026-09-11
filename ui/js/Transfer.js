.pragma library

.import "Format.js" as Format

// The model behind ui/TransferCard.qml, kept out of the QML so every state the card can draw is
// checked without a window. The object each function takes is ui/js/Ops.js's transfer.

// The byte sample for the item in flight. done is the count already finished, so it stays where it
// was: a sample fills the item in, it does not complete it.
function sampled(t, index, name, bytes, total) {
    return Object.assign({}, t, {index: index, name: name, done: index, bytes: bytes, total: total})
}

// That item's own terminal line: it counts whole from here, and its byte sample is spent.
function itemDone(t, index, name) {
    return Object.assign({}, t, {index: index, name: name, done: index + 1, bytes: 0, total: 0})
}

// The card's headline, the count with no name in it: the card gives the name a row of its own, and
// ui/js/Ops.js builds the status bar's one-line form from this same string.
function head(t) {
    return (t.redo ? "Redoing " + t.redo + " " : t.moving ? "Moving " : "Copying ") + (t.index + 1) + " of " + t.n
}

// The card's second row: the item in flight and how big it is. total is 0 for a directory, whose
// size is not known in advance without a sweep, so that one names itself and claims nothing more.
function fileLine(t) {
    if (t.name.length === 0) {
        return ""
    }
    return t.total > 0 ? t.name + " · " + Format.size(t.total) : t.name
}

// The bar is the whole transfer, never the one file: done carries the items already finished and
// the byte sample only fills in the one in flight. One large file is then its own byte bar, and
// thirty thousand small ones step it once each instead of restarting it thirty thousand times.
function fraction(t) {
    if (t.n <= 0) {
        return 0
    }
    var part = t.total > 0 ? t.bytes / t.total : 0
    var at = (t.done + part) / t.n
    return at < 0 ? 0 : (at > 1 ? 1 : at)
}
