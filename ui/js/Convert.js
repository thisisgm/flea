.pragma library

// The convert popup's own two decisions: which format, and whether to strip. The destination name is
// built here because the popup is what decides it; the wire carries no format field at all, since
// ImageMagick reads the codec off the destination's extension.

// What the popup offers. Every one of these is in this box's ImageMagick delegate list.
var FORMATS = ["jpg", "png", "webp", "avif", "heic", "tiff", "bmp"]

// The canvas's own suffixes: a real conversion is "(converted)", a same-format strip is "(stripped)".
var CONVERTED = "(converted)"
var STRIPPED = "(stripped)"

function extensionOf(name) {
    var cut = String(name).lastIndexOf(".")
    return cut > 0 ? String(name).substring(cut + 1).toLowerCase() : ""
}

function stemOf(name) {
    var cut = String(name).lastIndexOf(".")
    return cut > 0 ? String(name).substring(0, cut) : String(name)
}

// Never the file it came from: an interrupted in-place strip destroys the file it was given, so no
// in-place option is offered at all.
function destName(name, format) {
    var want = String(format).replace(/^\./, "").toLowerCase()
    var word = want === extensionOf(name) ? STRIPPED : CONVERTED
    return stemOf(name) + " " + word + "." + want
}

// The format row that starts selected: the canvas shows JPEG picked on a PNG, so it is the first
// format that is not the one the file already is.
function defaultFormat(name) {
    var current = extensionOf(name)
    for (var i = 0; i < FORMATS.length; i++) {
        if (FORMATS[i] !== current) {
            return FORMATS[i]
        }
    }
    return FORMATS[0]
}

function destination(source, format) {
    var parent = source.path.substring(0, source.path.lastIndexOf("/") + 1)
    return parent + destName(source.name, format)
}

function matchesReply(source, requestId, reply) {
    return source !== null && reply.requestId === requestId
            && reply.source === source.path
}
