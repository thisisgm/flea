.pragma library
.import "Search.js" as Search

// Tangerine's drag wheel, ported to Flea with dynamic format conversion sub-entries.
// The geometry is the landing page's (assets/home-*.js): one annulus of equal petals,
// sector 0 at the top, clockwise, an angular gap between neighbours, and rounded corners
// cut into the SVG path rather than into the hit test.
var GEOMETRY = {
    inner: 48.64,
    outer: 119.52,
    backdrop: 125.92,
    gap: 4,
    corner: 20.16,
    labelInner: 40
}

// Supported format categories and conversion targets.
// Magick handles images and PDF; FFmpeg handles audio and video.
var IMAGES = ["jpg", "jpeg", "png", "webp", "heic", "tiff", "svg", "avif", "bmp"]
var IMAGE_TARGETS = ["jpg", "png", "webp", "heic", "tiff", "avif", "bmp", "pdf"]
var AUDIO = ["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "wma"]
var VIDEO = ["mp4", "mov", "mkv", "webm", "avi", "wmv"]
var INPUT_ONLY = ["rar", "ppt", "xls", "rtf", "rtfd", "textClipping"]

// The seven root actions: six file actions plus Convert, which opens format sub-entries.
var ACTIONS = [
    { id: "copy", label: "Copy", glyph: "copy" },
    { id: "move", label: "Move to", glyph: "move-horizontal" },
    { id: "path", label: "Copy path", glyph: "file-text" },
    { id: "shelf", label: "Shelf", glyph: "shelf" },
    { id: "compress", label: "Compress", glyph: "archive" },
    { id: "convert", label: "Convert", glyph: "sliders", sub: "convert" },
    { id: "terminal", label: "Terminal", glyph: "terminal" }
]

function normalizeExt(ext) {
    var e = String(ext || "").toLowerCase().replace(/^\./, "")
    return e === "jpeg" ? "jpg" : e
}

function extensionOf(name) {
    var cut = String(name || "").lastIndexOf(".")
    return cut > 0 ? String(name).substring(cut + 1).toLowerCase() : ""
}

// Tangerine's z(e): returns available conversion formats for the given extension.
function conversionFormats(ext) {
    var e = normalizeExt(ext)
    var t = []
    if (IMAGES.indexOf(e) >= 0) t = IMAGE_TARGETS
    else if (AUDIO.indexOf(e) >= 0) t = AUDIO
    else if (VIDEO.indexOf(e) >= 0) t = VIDEO.concat(["gif", "mp3"])
    else if (e === "gif") t = VIDEO
    else if (e === "pdf") t = ["jpg", "png"]
    else return []

    var res = []
    for (var i = 0; i < t.length; i++) {
        var fmt = t[i]
        if (fmt !== e && INPUT_ONLY.indexOf(fmt) < 0 && res.indexOf(fmt) < 0)
            res.push(fmt)
    }
    return res
}

var LABELS = {
    jpg: "JPEG", png: "PNG", webp: "WebP", heic: "HEIC", tiff: "TIFF",
    avif: "AVIF", bmp: "BMP", pdf: "PDF", mp3: "MP3", m4a: "M4A",
    wav: "WAV", flac: "FLAC", ogg: "OGG", opus: "Opus", aiff: "AIFF",
    wma: "WMA", mp4: "MP4", mov: "MOV", mkv: "MKV", webm: "WebM",
    avi: "AVI", wmv: "WMV", gif: "GIF"
}

function formatLabel(fmt) {
    return LABELS[fmt] || String(fmt).toUpperCase()
}

function formatGlyph(fmt) {
    if (IMAGES.indexOf(fmt) >= 0) return "image"
    if (AUDIO.indexOf(fmt) >= 0) return "music"
    if (VIDEO.indexOf(fmt) >= 0 || fmt === "gif") return "film"
    return "file-text"
}

function conversionTargetIndex(pane) {
    if (!pane) return -1
    var picked = pane.selectedIndices ? pane.selectedIndices() : []
    if (picked.length > 1) return -1
    if (picked.length === 1) return picked[0]
    return pane.cursorIndex !== undefined ? pane.cursorIndex : -1
}

function activeExtension(pane) {
    if (!pane || !pane.rows) return ""
    var idx = conversionTargetIndex(pane)
    if (idx < 0) return ""
    var row = pane.rowFor ? pane.rowFor(idx) : null
    if (!row || row.d) return ""
    return extensionOf(row.n)
}

function itemsFor(mode, pane) {
    if (mode === "convert") {
        if (!pane || conversionTargetIndex(pane) < 0) return []
        var ext = activeExtension(pane)
        var fmts = conversionFormats(ext)
        var items = []
        for (var i = 0; i < fmts.length; i++) {
            var f = fmts[i]
            items.push({
                id: "convert:" + f,
                format: f,
                label: formatLabel(f),
                icon: formatGlyph(f),
                glyph: formatGlyph(f),
                category: "convert"
            })
        }
        return items
    }
    if (!pane) return ACTIONS
    var actions = []
    var canConvert = conversionTargetIndex(pane) >= 0 && itemsFor("convert", pane).length > 0
    for (var j = 0; j < ACTIONS.length; j++) {
        if (ACTIONS[j].id !== "convert" || canConvert)
            actions.push(ACTIONS[j])
    }
    return actions
}

// The middle of a sector, in math coordinates: positive y is up, and sector 0 is at the top.
function angleFor(index, count) {
    return (count === 3 ? Math.PI : Math.PI / 2) - (Math.PI * 2 * index) / count
}

function pointAt(radius, angle) {
    return { x: Math.cos(angle) * radius, y: -Math.sin(angle) * radius }
}

function gapAngle(radius) {
    return Math.asin(Math.min(0.95, GEOMETRY.gap / 2 / Math.max(radius, GEOMETRY.gap / 2 + 0.01)))
}

function sectorPath(index, count, cx, cy) {
    var geometry = GEOMETRY
    var middle = angleFor(index, count)
    var step = (Math.PI * 2) / count
    var left = middle - step / 2
    var right = middle + step / 2
    function corner(radius) {
        return Math.asin(Math.min(0.95, geometry.gap / 2 / Math.max(radius, geometry.gap / 2 + 0.01)))
    }
    var outerLeft = left + corner(geometry.outer)
    var outerRight = right - corner(geometry.outer)
    var innerLeft = left + corner(geometry.inner)
    var innerRight = right - corner(geometry.inner)
    var outerFillet = Math.min(0.15, (outerRight - outerLeft) * 0.24)
    var innerFillet = Math.min(0.18, (innerRight - innerLeft) * 0.18)
    var innerWide = geometry.inner / Math.max(0.01, Math.cos(innerFillet))
    var innerEdge = innerWide + geometry.inner * Math.tan(innerFillet)
    var outerInset = geometry.outer - geometry.corner
    function at(radius, angle) {
        var point = pointAt(radius, angle)
        return (point.x + cx).toFixed(4) + " " + (point.y + cy).toFixed(4)
    }
    var outerArc = outerRight - outerFillet - (outerLeft + outerFillet) > Math.PI ? 1 : 0
    var innerArc = innerRight - innerFillet - (innerLeft + innerFillet) > Math.PI ? 1 : 0
    return "M " + at(innerEdge, left + corner(innerEdge))
        + " L " + at(outerInset, left + corner(outerInset))
        + " Q " + at(geometry.outer, outerLeft) + " " + at(geometry.outer, outerLeft + outerFillet)
        + " A " + geometry.outer + " " + geometry.outer + " 0 " + outerArc + " 0 "
        + at(geometry.outer, outerRight - outerFillet)
        + " Q " + at(geometry.outer, outerRight) + " " + at(outerInset, right - corner(outerInset))
        + " L " + at(innerEdge, right - corner(innerEdge))
        + " Q " + at(innerWide, right - corner(innerWide)) + " " + at(geometry.inner, innerRight - innerFillet)
        + " A " + geometry.inner + " " + geometry.inner + " 0 " + innerArc + " 1 "
        + at(geometry.inner, innerLeft + innerFillet)
        + " Q " + at(innerWide, left + corner(innerWide)) + " " + at(innerEdge, left + corner(innerEdge))
        + " Z"
}

function indexAt(x, y, count) {
    if (!count || count < 1) return -1
    var radius = Math.hypot(x, y)
    if (radius < GEOMETRY.inner || radius > GEOMETRY.outer)
        return -1
    var angle = Math.atan2(-y, x)
    var step = (Math.PI * 2) / count
    var turned = (((angleFor(0, count) - angle) % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2)
    var index = Math.round(turned / step) % count
    var middle = angleFor(index, count)
    var off = Math.abs(Math.atan2(Math.sin(angle - middle), Math.cos(angle - middle)))
    return off > step / 2 - Math.asin(GEOMETRY.gap / 2 / radius) ? -1 : index
}

function labelPoint(index, count) {
    var radius = GEOMETRY.labelInner + (GEOMETRY.outer - GEOMETRY.labelInner) * 0.56
    return pointAt(radius, angleFor(index, count))
}

function centerText(index, mode, items) {
    var list = items || ACTIONS
    if (index >= 0 && index < list.length)
        return list[index].label
    return mode === "convert" ? "Convert" : "Actions"
}

function centerSubText(index, mode, items) {
    var list = items || ACTIONS
    if (index >= 0 && index < list.length) {
        if (mode === "convert") return "Enter to convert"
        return list[index].sub ? "Enter for options" : "Enter to run"
    }
    if (mode === "convert") return "Esc to go back"
    return list.length + " file actions"
}

function run(actionOrIndex, pane) {
    var action = typeof actionOrIndex === "number" ? ACTIONS[actionOrIndex] : actionOrIndex
    if (!action || !pane) return ""
    if (action.id === "shelf") {
        pane.sheetMenuAction("addToShelf")
        return "addToShelf"
    }
    if (action.id === "convert" || action.sub === "convert") {
        return "convert"
    }
    if (action.id && action.id.indexOf("convert:") === 0) {
        var format = action.format || action.id.substring(8)
        var index = conversionTargetIndex(pane)
        var row = index >= 0 && pane.rowFor ? pane.rowFor(index) : null
        var base = Search.baseOf ? Search.baseOf(pane) : (pane.searchBase || pane.path)
        var fullPath = row && row.n ? (pane.join ? pane.join(base, row.n) : row.n) : ""
        var slash = fullPath.lastIndexOf("/")
        var dot = fullPath.lastIndexOf(".")
        var stem = dot > slash + 1 ? fullPath.substring(0, dot) : fullPath
        var dest = fullPath ? stem + " (converted)." + format : ""
        if (fullPath && pane.backend && pane.backend.convertImage) {
            pane.backend.convertImage(fullPath, dest, false, 0, 0, false)
        }
        return "convert:" + format
    }
    var route = action.id === "move" ? "moveTo"
              : action.id === "path" ? "copyPath"
              : action.id === "compress" ? "compress:zip"
              : action.id === "terminal" ? "openTerminal"
              : action.id
    pane.act(route)
    return route
}
