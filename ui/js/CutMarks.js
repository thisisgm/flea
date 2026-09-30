.pragma library

.import "Ops.js" as Ops

// Whether a row is on the clipboard as a cut, so the three views can draw it the way Nautilus does:
// the scissors mark in place of its icon or thumbnail until the paste moves it. A copy marks nothing,
// because a copied file stays where it is. The clipboard holds absolute paths (ui/js/Ops.js clip), so
// the row is joined against the directory the pane is listing, which a search result's relative name
// also resolves against. The path set is built once per clipboard object and not once per row: a
// cut of a whole 100,000 file directory would otherwise be a linear scan in every visible delegate.
var memoClip = null
var memoSet = null

function isCut(clip, path) {
    if (!clip || !clip.moving || !clip.paths || clip.paths.length === 0)
        return false
    if (clip !== memoClip) {
        memoSet = {}
        for (var i = 0; i < clip.paths.length; i++)
            memoSet[clip.paths[i]] = true
        memoClip = clip
    }
    return memoSet[path] === true
}

function rowIsCut(pane, row) {
    return !!pane && !!row && typeof row.n === "string" && isCut(pane.clipboard, pane.join(pane.path, row.n))
}

// Escape takes a pending cut back: the clipboard empties, which restores every marked row at once
// because each one's mark is a binding on pane.clipboard. A copy is left alone, since nothing on
// screen says it is there to cancel. Answers whether it cancelled, so Escape stops there.
function cancel(pane) {
    if (!pane.clipboard || !pane.clipboard.moving || pane.clipboard.paths.length === 0)
        return false
    pane.clipboard = Ops.emptyClipboard()
    pane.message("Cut cancelled.", false)
    return true
}
