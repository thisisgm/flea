.pragma library

// WCAG 2.1 relative luminance, so a test can assert a ratio rather than an eye judging a screenshot.
function channel(c) {
    return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
}

function luminance(r, g, b) {
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

// Sample input: "#c8ccd0"
function parse(hex) {
    var s = String(hex).replace("#", "")
    if (s.length === 8)
        s = s.substring(2)
    return [parseInt(s.substr(0, 2), 16) / 255,
            parseInt(s.substr(2, 2), 16) / 255,
            parseInt(s.substr(4, 2), 16) / 255]
}

function over(fg, alpha, bg) {
    var f = parse(fg)
    var b = parse(bg)
    return [f[0] * alpha + b[0] * (1 - alpha),
            f[1] * alpha + b[1] * (1 - alpha),
            f[2] * alpha + b[2] * (1 - alpha)]
}

function ratioOf(a, b) {
    var la = luminance(a[0], a[1], a[2])
    var lb = luminance(b[0], b[1], b[2])
    var hi = Math.max(la, lb)
    var lo = Math.min(la, lb)
    return (hi + 0.05) / (lo + 0.05)
}

function ratio(fgHex, bgHex) {
    return ratioOf(parse(fgHex), parse(bgHex))
}

function pad2(n) {
    return n < 16 ? "0" + n.toString(16) : n.toString(16)
}

function hexOf(rgb) {
    return "#" + pad2(Math.round(Math.max(0, Math.min(1, rgb[0])) * 255))
            + pad2(Math.round(Math.max(0, Math.min(1, rgb[1])) * 255))
            + pad2(Math.round(Math.max(0, Math.min(1, rgb[2])) * 255))
}

function mix(a, b, t) {
    return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

// Keep hue; walk lightness toward black or white until WCAG AA (or minRatio) holds.
function ensureRatio(fgHex, bgHex, minRatio) {
    var need = minRatio > 0 ? minRatio : 4.5
    if (ratio(fgHex, bgHex) >= need)
        return String(fgHex)
    var fg = parse(fgHex)
    var bg = parse(bgHex)
    var toward = luminance(bg[0], bg[1], bg[2]) > 0.179 ? [0, 0, 0] : [1, 1, 1]
    var lo = 0
    var hi = 1
    var best = toward
    for (var i = 0; i < 18; i++) {
        var mid = (lo + hi) / 2
        var cand = mix(fg, toward, mid)
        if (ratioOf(cand, bg) >= need) {
            best = cand
            hi = mid
        } else {
            lo = mid
        }
    }
    if (ratioOf(best, bg) < need)
        best = toward
    return hexOf(best)
}
