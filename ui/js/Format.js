.pragma library

// SI, because a file's size on disk has no power-of-two basis; GLib's own rule and the whole GUI bracket.
var BYTES_PER_UNIT = 1000
var UNITS = ["B", "kB", "MB", "GB", "TB"]
// GNOME renders a narrow no-break space before the unit, and Flea's neighbours are GLib-formatted.
var UNIT_SPACE = " "

// Below one kilobyte a fraction is noise, so bytes print whole.
// Counts reach six figures on a real directory, so they are grouped the way the canvas draws them.
function count(n) {
    var digits = String(n)
    var out = ""
    for (var i = 0; i < digits.length; i++) {
        if (i > 0 && (digits.length - i) % 3 === 0)
            out += ","
        out += digits.charAt(i)
    }
    return out
}

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

// Directory st_size describes the entry itself, even when a populated cloud folder reports zero.
function directorySize(result) {
    if (!result) return "·"
    if (result.partial && result.bytes === 0) return "Unknown"
    return (result.partial ? ">" : "") + size(result.bytes)
}

function propertySize(facts) {
    if (facts.directory) return "Not calculated"
    return size(facts.bytes) + " (" + facts.bytes + " bytes)"
}

function storageFacts(facts) {
    if (!facts.filesystem) return []
    var rows = [["Storage", facts.filesystem === "fuse.rclone" ? "rclone mount" : facts.filesystem]]
    if (facts.filesystem === "fuse.rclone") {
        rows.push(["Upload status", "See the cloud status bar. A completed copy may still be uploading."])
    }
    return rows
}

// "2026-09-12 15:29", the one form every surface prints, in the machine's local wall clock.
function stamp(d) {
    return d.getFullYear() + "-" + pad(d.getMonth() + 1) + "-" + pad(d.getDate())
        + " " + pad(d.getHours()) + ":" + pad(d.getMinutes())
}

// One form, sortable and unambiguous, so no surface has to invent a relative word for a time.
function date(mtime) {
    return stamp(new Date(mtime * 1000))
}

// The send picker's column is SendPicker.html's 80 and not the window's 125, which holds about ten
// characters: the one place the full stamp does not fit. It drops the time and keeps the date, so it
// is still sortable and still unambiguous. Preview board, "One function, four surfaces".
function compactDate(mtime) {
    var d = new Date(mtime * 1000)
    return d.getFullYear() + "-" + pad(d.getMonth() + 1) + "-" + pad(d.getDate())
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
// Issue 95, nixfred: a bare prefix made /home/gmx into "~x", a sibling wearing home's name. The test
// is home itself or home and a separator, the one ui/js/Nav.js crumbs and Search.scopeRoot both make.
function tilde(path, home) {
    var text = String(path)
    if (home.length > 0 && (text === home || text.indexOf(home + "/") === 0)) {
        return "~" + text.substring(home.length)
    }
    return text
}

// A tab is named after the directory it is standing in, so the label is the path's last segment.
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

// Issue 67, jesedv: a yanked path is quoted unless a shell reads every character of it as itself.
function shellQuoted(path) {
    var text = String(path)
    if (/^[A-Za-z0-9_@%+=:,.\/-]+$/.test(text)) {
        return text
    }
    return "'" + text.split("'").join("'\\''") + "'"
}

// A preview has no recursive directory result: never substitute the entry size.
function rowSize(row) {
    return row.d ? "Not calculated" : size(row.s)
}
