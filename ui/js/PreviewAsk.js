.pragma library

.import "PreviewSettle.js" as PreviewSettle
.import "ExtThumbs.js" as ExtThumbs

// The cursor row's single thumbnail ask, shared by the early ask and load()'s askThumb, so the second sends nothing twice.

// Sample input: canRead, storageKnown, one selection, index 3, rowT, unasked, local storage answers {ask: [3], drop: [], cacheOnly: false}.
function workFor(o) {
    if (!o.canRead || !o.storageKnown || o.selectionCount > 1) return null
    if (o.index < 0 || o.rowT !== true || o.asked === true) return null
    return { ask: [o.index], drop: [], cacheOnly: ExtThumbs.cacheOnly(o.storageClass, o.preview) }
}

// The idle half of the early ask is the settle's own rule: a walk step trails and asks nothing early.
function earlyWorkFor(o) {
    if (PreviewSettle.plan(o.now, o.lastMoveAt, o.interval, o.key, o.lastMoveKey, o.armed, o.pending) !== "now")
        return null
    if (ExtThumbs.manualHold(o.storageClass, o.preview)) return null
    return workFor(o)
}
