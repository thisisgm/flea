.pragma library

// Grid tile size, Settings > Preview > Thumbnail size. Named stops rather than a free pixel
// number, the same closed-list rule src/uischema.rs applies to preview.thumbSize. The backend
// already writes Freedesktop "large" thumbs at 256 px, so the top stop is that size and not
// an upscale. Extra large stays 128, because bumping it would silently double existing installs.

var NAMES = ["small", "medium", "large", "xlarge", "xxlarge", "huge"]
var PIXELS = [48, 64, 96, 128, 192, 256]
var LABELS = ["Small", "Medium", "Large", "Extra large", "XX-large", "Huge"]
var DEFAULT = "medium"

function indexOf(name) {
    var at = NAMES.indexOf(name)
    return at >= 0 ? at : NAMES.indexOf(DEFAULT)
}

function pixels(name) {
    return PIXELS[indexOf(name)]
}

function label(name) {
    return LABELS[indexOf(name)]
}

function caption(name) {
    return pixels(name) + " px"
}

function parse(name) {
    return NAMES.indexOf(name) >= 0 ? name : DEFAULT
}

function step(name, direction) {
    var to = Math.max(0, Math.min(NAMES.length - 1, indexOf(name) + direction))
    return NAMES[to]
}

function announce(name) {
    return "Thumbnail size " + caption(parse(name)) + "."
}
