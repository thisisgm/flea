.pragma library

.import "Recent.js" as Recent

// The system clipboard's file payload, x-special/gnome-copied-files: one verb line, cut or copy,
// then one file:// URI per line with no trailing newline, captured live from a real copy on this
// box (2026-09-25):
// copy
// file:///mnt/terachad/Emulators/EmuDeck/roms_mid/psp
// It is the format Nautilus, Nemo and Thunar exchange, so writing and reading it is what lets a
// second Flea window, and the desktop around it, see this window's Copy and Cut. wl-copy owns one
// MIME type per selection, and this is the one Flea offers, over text/uri-list, because it is the
// only one of the two that carries the cut.

// One line per path. encodeURI keeps a path's own slashes and escapes its spaces, and '#' and '?'
// are the two bytes it leaves literal that a URI reader then takes for a fragment and a query, the
// same hand-replace ui/Row.qml's thumbnail URL already makes.
function payload(clip) {
    var lines = [clip.moving === true ? "cut" : "copy"]
    for (var i = 0; i < clip.paths.length; i++)
        lines.push("file://" + encodeURI(String(clip.paths[i])).replace(/#/g, "%23").replace(/\?/g, "%3F"))
    return lines.join("\n")
}

// The clipboard is every application's, so a payload is untrusted text: the verb must be cut or
// copy, and every line after it must decode through ui/js/Recent.js pathOf, the reader that already
// refuses a foreign scheme, a foreign authority, a malformed escape and a control character. One
// refused line refuses the whole payload, because pasting half of what somebody copied is worse
// than pasting nothing: a batch the operator saw as N items must never arrive as fewer.
function parse(text) {
    var lines = String(text || "").replace(/\r/g, "").split("\n")
    if (lines[0] !== "cut" && lines[0] !== "copy")
        return null
    var paths = []
    for (var i = 1; i < lines.length; i++) {
        if (lines[i].length === 0)
            continue
        var path = Recent.pathOf(lines[i])
        if (path.length === 0)
            return null
        paths.push(path)
    }
    if (paths.length === 0)
        return null
    return { paths: paths, moving: lines[0] === "cut" }
}

// One string per clipboard state, "" for an empty one: what the bridge compares so publishing what
// it just read, and reading its own publish back, are both no-ops rather than a loop.
function key(clip) {
    if (!clip || !clip.paths || clip.paths.length === 0)
        return ""
    return payload(clip)
}
