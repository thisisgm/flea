.pragma library

.import "Archive.js" as Archive
.import "Format.js" as Format

// How a row is classified: which of the twelve preview states it is, and the extension families that
// decide the three the backend's icon cannot. Split out of Facts.js, which owns the fact tables.

var IMAGE = "image"
var VIDEO = "video"
var AUDIO = "audio"
var PDF = "pdf"
var TEXT = "text"
var CODE = "code"
var ARCHIVE = "archive"
var SYMLINK = "symlink"
var MULTI = "multi"
var LOADING = "loading"
var ERROR = "error"
var UNSUPPORTED = "unsupported"
var FONT = "font"

// src/backend/rows.rs's own UNKNOWN_KIND: the Kind a row gets when nothing could describe its type.
var UNKNOWN_KIND = "Data"

var CODE_EXTENSIONS = [
    ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".c", ".h", ".cpp", ".hpp", ".cc", ".go", ".rb",
    ".java", ".kt", ".swift", ".lua", ".pl", ".php", ".sh", ".bash", ".zsh", ".fish", ".vim",
    ".qml", ".css", ".scss", ".html", ".xml", ".json", ".toml", ".yaml", ".yml", ".ini", ".conf",
    ".sql", ".mjs", ".cjs"
]

function isCode(name) {
    var lower = String(name).toLowerCase()
    for (var i = 0; i < CODE_EXTENSIONS.length; i++) {
        var e = CODE_EXTENSIONS[i]
        if (lower.length > e.length && lower.indexOf(e, lower.length - e.length) !== -1) {
            return true
        }
    }
    return false
}

var MARKDOWN_EXTENSIONS = [".md", ".markdown", ".mkd"]

function isMarkdown(name) {
    var lower = String(name).toLowerCase()
    for (var i = 0; i < MARKDOWN_EXTENSIONS.length; i++) {
        var e = MARKDOWN_EXTENSIONS[i]
        if (lower.length > e.length && lower.indexOf(e, lower.length - e.length) !== -1) {
            return true
        }
    }
    return false
}

function isHtml(name) { return /\.(html|htm)$/i.test(String(name)) }

// TrueType, OpenType and collections (font/ttf, font/otf, font/collection); the backend checks the MIME type again.
var FONT_EXTENSIONS = [".ttf", ".otf", ".ttc"]

function isFont(name) { return /\.(ttf|otf|ttc|otc|woff2?|eot|pfa|pfb|t1|t42|cid|cff|pfr|fon|fnt|bdf|pcf|otb|dfont|suit|suitcase|afm|pfm)(\.(gz|bz2|Z))?$/i.test(String(name)) }

function isSampleFont(name) {
    var lower = String(name).toLowerCase()
    return FONT_EXTENSIONS.some(function (e) { return lower.length > e.length && lower.indexOf(e, lower.length - e.length) !== -1 })
}

// Quick Look's font sample: one sentence at three sizes, in board pixels before the text-size ratio and the pixel ratio.
var SAMPLE_TEXT = "The quick brown fox jumps over the lazy dog"
var SAMPLE_SIZES = [22, 15, 12]

// One count for every font pane, so an answer meant for an earlier pane never carries a later pane's id.
var lookSeq = 0
function nextLookId() { lookSeq += 1; return lookSeq }

// "Inter · Bold · 312.0 kB · 2,548 glyphs"; a table the font lacks drops its part, and the size is always known.
// pluralWord is Ops.pluralWord, handed in because Ops reaches Sort.js and RecentMode.js, which import this file.
function fontLine(family, style, bytes, glyphs, pluralWord) {
    var parts = [family, style, Format.size(bytes)].filter(function (p) { return p })
    if (glyphs > 0)
        parts.push(pluralWord(glyphs, "%1 glyph", "%1 glyphs").replace("%1", Format.count(glyphs)))
    return parts.join(" \u00b7 ")
}

// The eleven camera RAW types the backend copies a preview out of, told apart by extension like a PDF; Sample input: "IMG_0412.CR3" is true.
function isRaw(name) {
    return /\.(cr[23]|nef|nrw|arw|sr2|dng|raf|orf|rw2|pef)$/i.test(String(name))
}

function isPdf(name) {
    var lower = String(name).toLowerCase()
    return lower.length > 4 && lower.indexOf(".pdf", lower.length - 4) !== -1
}

// The MIME icon table files 3D models under image-x-generic, so they route as images, but no Qt image plugin decodes one.
function isModel(name) {
    return /\.(stl|3mf|obj|mtl|gltf|glb)$/i.test(String(name))
}

// The icon name is the classification the backend already made, so the column never re-derives one.
// Its first segment is the freedesktop family, and the family is what classifies: this box's
// generic-icons table maps text/* to five different text- names, and a list of the names seen so
// far is exactly how the Quick Look refused every image for as long as it kept one.
function kindState(icon) {
    var name = String(icon)
    switch (name) {
    case "text-x-script":
    case "text-html":
    case "application-xml": return CODE
    case "package-x-generic": return ARCHIVE
    }
    if (name.indexOf("image-") === 0) return IMAGE
    if (name.indexOf("video-") === 0) return VIDEO
    if (name.indexOf("audio-") === 0) return AUDIO
    if (name.indexOf("text-") === 0) return TEXT
    // A .doc or .odt shares the office icon with a PDF, and a PDF was already told apart by its
    // extension. What is left is binary, so it is Unsupported rather than a newline count over a blob.
    return UNSUPPORTED
}

// A directory is not a preview subject: the column shows what is inside it instead.
function isPreviewable(row) {
    return row !== null && row !== undefined && row.d !== true
}

// A name the icon cannot route, told apart by its own extension: a PDF shares x-office-document, and widened archives draw other marks.
function stateByExtension(name) {
    if (isPdf(name)) return PDF
    if (Archive.isArchive(name)) return ARCHIVE
    if (/\.(csv|tsv)$/i.test(String(name))) return TEXT
    return ""
}

// Quick Look's kind from the icon, the name and a served thumbnail, Facts.state's row-free tests; code and markdown read as text.
function quickLookKind(icon, name, thumbnailed) {
    if (isPdf(name)) return PDF
    if (isFont(name)) return FONT
    if (isMarkdown(name)) return TEXT
    if (/\.(csv|tsv)$/i.test(String(name))) return TEXT
    // The client's own extension rule, because the backend's icon cannot name the widened formats.
    if (Archive.isArchive(name)) return ARCHIVE
    var byIcon = kindState(icon)
    // A model no thumbnailer serves has nothing to draw, so it takes the generic file pane.
    if (byIcon === IMAGE && isModel(name) && !thumbnailed) return UNSUPPORTED
    return byIcon === CODE ? TEXT : byIcon
}
