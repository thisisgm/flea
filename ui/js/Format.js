.pragma library

// SI, because a file's size on disk has no power-of-two basis; GLib's own rule and the whole GUI bracket.
var BYTES_PER_UNIT = 1000
var UNITS = ["B", "kB", "MB", "GB", "TB"]
// GNOME renders a narrow no-break space before the unit, and Flea's neighbours are GLib-formatted.
var UNIT_SPACE = " "

// Below one kilobyte a fraction is noise, so bytes print whole.
function size(bytes) {
    if (bytes < BYTES_PER_UNIT) {
        return bytes + UNIT_SPACE + UNITS[0]
    }
    var value = bytes
    var unit = 0
    while (value >= BYTES_PER_UNIT && unit < UNITS.length - 1) {
        value = value / BYTES_PER_UNIT
        unit += 1
    }
    return value.toFixed(1) + UNIT_SPACE + UNITS[unit]
}

function pad(n) {
    return n < 10 ? "0" + n : "" + n
}

var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
var SECONDS_PER_DAY = 86400
var MILLISECONDS_PER_MINUTE = 60000

// Never all-numeric; the stamp and Today/Yesterday boundary follow the machine's local wall clock.
function date(mtime, nowMs) {
    var d = new Date(mtime * 1000)
    var now = new Date(nowMs)
    var clock = pad(d.getHours()) + ":" + pad(d.getMinutes())
    // Each instant supplies its own offset so DST transitions keep the local day boundary.
    var day = Math.floor((d.getTime() - d.getTimezoneOffset() * MILLISECONDS_PER_MINUTE) / (SECONDS_PER_DAY * 1000))
    var today = Math.floor((now.getTime() - now.getTimezoneOffset() * MILLISECONDS_PER_MINUTE) / (SECONDS_PER_DAY * 1000))
    if (day === today) {
        return "Today, " + clock
    }
    if (day === today - 1) {
        return "Yesterday, " + clock
    }
    var stamp = d.getDate() + " " + MONTHS[d.getMonth()]
    // The distant past omits the time, per Material's second table.
    return d.getFullYear() === now.getFullYear()
        ? stamp + ", " + clock
        : stamp + " " + d.getFullYear()
}

// The low nine bits of st_mode, read three at a time.
var PERMISSION_BITS = 9
var TRIAD = "rwx"

function permissions(mode) {
    var out = ""
    for (var i = 0; i < PERMISSION_BITS; i++) {
        var bit = 1 << (PERMISSION_BITS - 1 - i)
        out += (mode & bit) ? TRIAD[i % TRIAD.length] : "-"
    }
    return out
}

// The file type lives in the top four bits of st_mode, as S_IFMT masks it.
var S_IFMT = 0o170000
var S_IFLNK = 0o120000
var ANY_EXECUTE_BIT = 0o111

function isSymlink(mode) {
    return (mode & S_IFMT) === S_IFLNK
}

function isExecutable(mode) {
    return (mode & ANY_EXECUTE_BIT) !== 0
}

// Shared by Row.qml's iconSource and PreviewMedia.qml's player source: encodeURI leaves # and ?
// literal, which Qt then reads as a URL fragment or query rather than path bytes.
function fileUri(path) {
    return "file://" + encodeURI(path).replace(/#/g, "%23").replace(/\?/g, "%3F")
}

// "3:05", or "1:03:05" once an hour is on the clock; mm/ss are always two digits, matching a media player's own clock rather than Format.date's prose.
function duration(ms) {
    var totalSeconds = Math.max(0, Math.floor(ms / 1000))
    var hours = Math.floor(totalSeconds / 3600)
    var minutes = Math.floor((totalSeconds % 3600) / 60)
    var seconds = totalSeconds % 60
    if (hours > 0) {
        return hours + ":" + pad(minutes) + ":" + pad(seconds)
    }
    return minutes + ":" + pad(seconds)
}

// The scope reads as the user writes it, so the home prefix comes back as a tilde. Both the search
// strip and the window chrome draw a path through this, so the rule has one definition.
function tilde(path, home) {
    if (home.length > 0 && String(path).indexOf(home) === 0) {
        return "~" + String(path).substring(home.length)
    }
    return String(path)
}

// The chrome draws the directory's own name at full contrast and everything above it muted, so the
// path splits after its last separator; a root or a bare name has no parent half at all.
function parentPart(display) {
    var cut = String(display).lastIndexOf("/")
    return cut <= 0 ? "" : String(display).substring(0, cut + 1)
}

function leafPart(display) {
    var text = String(display)
    var cut = text.lastIndexOf("/")
    if (cut < 0) {
        return text
    }
    // "/" itself has no leaf, and its own separator is the whole label.
    return cut === text.length - 1 ? text : text.substring(cut + 1)
}

// "44.1 kHz", the way the canvas writes an audio row's Rate; a zero is not a rate and reads empty.
function sampleRate(hz) {
    var n = Number(hz)
    if (!n || n <= 0) {
        return ""
    }
    var khz = n / 1000
    // A whole number of kilohertz reads without a decimal, so 48000 is "48 kHz" and not "48.0 kHz".
    return (khz === Math.round(khz) ? khz : khz.toFixed(1)) + " kHz"
}
