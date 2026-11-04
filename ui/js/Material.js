.pragma library
.import "Contrast.js" as Contrast

// Omarchy Glassy dims its blurred backdrop to 0.75; channels and alpha use whole 8-bit steps.
var STEPS = 255
var MIN_ALPHA = Math.ceil(0.75 * STEPS)
var INK_STEP = 1.25
// Twenty bisections resolve lightness more finely than one 8-bit colour channel.
var LIGHTNESS_SEARCH_STEPS = 20
// Half an 8-bit channel step repairs rounding without skipping the first passing colour.
var ROUNDING_REPAIR_STEPS = 512

function ground(plane, a, backdrop) {
    return Contrast.hexOf(Contrast.over(String(plane), a / STEPS, backdrop))
}

function worst(ink, c, a, planes) {
    var least = Infinity
    var grounds = planes || [c.background, c.surface]
    var brightness = c.blurBrightness === undefined ? 0.75 : c.blurBrightness
    var white = Contrast.hexOf([brightness, brightness, brightness])
    var backdrops = [white, "#000000"]
    for (var p = 0; p < grounds.length; p++)
        for (var b = 0; b < backdrops.length; b++)
            least = Math.min(least, Contrast.ratio(String(ink), ground(grounds[p], a, backdrops[b])))
    return least
}

function direction(c) {
    var fg = Contrast.parse(String(c.foreground)), bg = Contrast.parse(String(c.background))
    return Contrast.luminance(fg[0], fg[1], fg[2]) > Contrast.luminance(bg[0], bg[1], bg[2]) ? [1, 1, 1] : [0, 0, 0]
}

// The R3 lightness walk, including its rounding repair, applied to the live palette rather than a theme-name table.
function inkHolds(ink, c, a, need, quiet) {
    if (worst(ink, c, a) < need) return false
    if (quiet === undefined) return true
    var x = Contrast.parse(String(ink)), q = Contrast.parse(String(quiet))
    var lx = Contrast.luminance(x[0], x[1], x[2]), lq = Contrast.luminance(q[0], q[1], q[2])
    return direction(c)[0] === 1 ? lx >= lq : lx <= lq
}

function liftInk(ink, c, a, need, quiet) {
    if (inkHolds(ink, c, a, need, quiet)) return String(ink)
    var from = Contrast.parse(String(ink)), to = direction(c), lo = 0, hi = 1
    for (var i = 0; i < LIGHTNESS_SEARCH_STEPS; i++) {
        var mid = (lo + hi) / 2
        if (inkHolds(Contrast.hexOf(Contrast.mix(from, to, mid)), c, a, need, quiet)) hi = mid
        else lo = mid
    }
    for (var t = hi; t <= 1; t += 1 / ROUNDING_REPAIR_STEPS) {
        var hex = Contrast.hexOf(Contrast.mix(from, to, t))
        if (inkHolds(hex, c, a, need, quiet)) return hex
    }
    return Contrast.hexOf(to)
}

function quieter(ink, primary, c) {
    var x = Contrast.parse(String(ink)), fg = Contrast.parse(String(primary))
    var lx = Contrast.luminance(x[0], x[1], x[2]), lf = Contrast.luminance(fg[0], fg[1], fg[2])
    return (direction(c)[0] === 1 ? lx < lf : lx > lf) && Contrast.ratio(String(ink), String(primary)) >= INK_STEP
}

function primaryHolds(primary, c, a, need, muted, metadata) {
    return worst(primary, c, a) >= need && quieter(muted, primary, c) && quieter(metadata, primary, c)
}

// A mode may lift primary ink too, so legible secondary ink never becomes the primary rung.
function primaryInk(c, a, need, muted, metadata) {
    if (primaryHolds(c.foreground, c, a, need, muted, metadata)) return String(c.foreground)
    var from = Contrast.parse(String(c.foreground)), to = direction(c), lo = 0, hi = 1
    for (var i = 0; i < LIGHTNESS_SEARCH_STEPS; i++) {
        var mid = (lo + hi) / 2
        if (primaryHolds(Contrast.hexOf(Contrast.mix(from, to, mid)), c, a, need, muted, metadata)) hi = mid
        else lo = mid
    }
    for (var t = hi; t <= 1; t += 1 / ROUNDING_REPAIR_STEPS) {
        var hex = Contrast.hexOf(Contrast.mix(from, to, t))
        if (primaryHolds(hex, c, a, need, muted, metadata)) return hex
    }
    return Contrast.hexOf(to)
}

function inks(c, a, need) {
    var muted = liftInk(c.muted, c, a, need), metadata = liftInk(c.metadata, c, a, need)
    var meta = Contrast.parse(metadata), fg = Contrast.parse(String(c.foreground))
    var lm = Contrast.luminance(meta[0], meta[1], meta[2]), lf = Contrast.luminance(fg[0], fg[1], fg[2])
    // Darkening a light palette's foreground put metadata beyond primary, or on it in White; reuse its muted rung.
    if (direction(c)[0] === 1 ? lm >= lf : lm <= lf) metadata = muted
    var primary = primaryInk(c, a, need, muted, metadata)
    return {foreground: primary, muted: muted, metadata: metadata}
}

// One alpha for every main plane in A, raised only if its ink cannot hold the two text rungs.
function floor(c, need) {
    for (var a = MIN_ALPHA; a < STEPS; a++) {
        var ink = inks(c, a, need)
        if (primaryHolds(ink.foreground, c, a, need, ink.muted, ink.metadata)
                && worst(ink.muted, c, a) >= need && worst(ink.metadata, c, a) >= need) return a
    }
    return STEPS
}

// B keeps the unlifted listing text legible on its own background plane, never thinner than A.
function listFloor(c, need) {
    var a = STEPS
    while (a > 0 && Math.min(worst(c.muted, c, a - 1, [c.background]), worst(c.metadata, c, a - 1, [c.background])) >= need) a--
    return a
}
