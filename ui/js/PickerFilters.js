.pragma library

// The chooser's file filters as data: ~/.config/flea/filters.toml parsed, the label a pill shows for
// a filter that named none, and the predicate that says whether a listing row stands under one.
// Pure, with no FileView and no window, so tests/js/pickerfilters.js runs it under qml6 alone.
//
// Sample input, one filter per [[filter]] table, globs required and the other two keys optional:
//   [[filter]]
//   globs = ["*.jpg", "*.jpeg"]
//   mimes = ["image/*"]
//   [[filter]]
//   name = "Documents"
//   globs = ["*.doc", "*.docx", "*.odt"]
// A line scanner in the spirit of Palette.js, not a TOML parser: one key per line, arrays on one
// line, double or single quoted strings. A table missing globs is skipped and an empty body says [].
function parse(body) {
    var out = []
    var table = null
    var lines = String(body || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i]
        if (/^\s*\[\[\s*filter\s*\]\]/.test(line)) {
            keep(out, table)
            table = { name: "", globs: [], mimes: [] }
            continue
        }
        if (/^\s*\[/.test(line)) {
            // Some other table: whatever it holds is not a filter.
            keep(out, table)
            table = null
            continue
        }
        if (!table) {
            continue
        }
        var list = line.match(/^\s*(globs|mimes)\s*=\s*\[(.*)\]/)
        if (list) {
            table[list[1]] = strings(list[2])
            continue
        }
        var name = line.match(/^\s*name\s*=\s*(?:"([^"]*)"|'([^']*)')/)
        if (name) {
            table.name = name[1] !== undefined ? name[1] : name[2]
        }
    }
    keep(out, table)
    return out
}

// Every quoted string in an array body, in order, with nothing else in between honoured.
function strings(body) {
    var out = []
    var re = /"([^"]*)"|'([^']*)'/g
    var m
    while ((m = re.exec(body)) !== null) {
        var s = m[1] !== undefined ? m[1] : m[2]
        if (s !== "") {
            out.push(s)
        }
    }
    return out
}

// A table stands only when it has a glob to match by: a filter that matches by nothing hides nothing
// and would still take a pill.
function keep(out, table) {
    if (table && table.globs.length > 0) {
        out.push(table)
    }
}

// The extension a glob asks for: the first dot after the last * or /, lowercased, so *.JPG and
// IMG_*.jpg both say .jpg and docs/*.tar.gz says .tar.gz. A glob asking for none, like "Makefile" or
// "photo.*", is its own text.
function extensionOf(glob) {
    // Chromium spells each letter as a case pair, "*.[jJ][pP][gG]"; the label reads it as ".jpg".
    var text = String(glob).replace(/\[([a-z])([a-z])\]/gi, function (whole, a, b) {
        return a.toLowerCase() === b.toLowerCase() ? a : whole
    })
    var mark = Math.max(text.lastIndexOf("*"), text.lastIndexOf("/"))
    var dot = text.indexOf(".", mark + 1)
    return dot < 0 ? text : text.slice(dot).toLowerCase()
}

// The pill's label: the name when the file gave one, else the extensions the Windows way, first one
// out front and the whole set in brackets: ".jpg (.jpg, .jpeg)". One extension is only itself.
function labelFor(filter) {
    if (filter.name) {
        return String(filter.name)
    }
    var exts = []
    for (var i = 0; i < filter.globs.length; i++) {
        var ext = extensionOf(filter.globs[i])
        if (exts.indexOf(ext) < 0) {
            exts.push(ext)
        }
    }
    if (exts.length <= 1) {
        return exts.join("")
    }
    return exts[0] + " (" + exts.join(", ") + ")"
}

// Whether a filter lets every file through: no rule at all, a glob "*" or "*.*", or a mime "*/*".
// Chromium sends "All Files" as "*" beside "Image Files", so such a filter is the caller's own All
// files pill and the chooser draws no second one; ui/js/Picker.js chips and shownRows read this.
function matchesAll(filter) {
    var globs = Array.isArray(filter.globs) ? filter.globs.map(String) : []
    var mimes = Array.isArray(filter.mimes) ? filter.mimes.map(String) : []
    if (globs.length + mimes.length === 0) {
        return true
    }
    return globs.indexOf("*") >= 0 || globs.indexOf("*.*") >= 0 || mimes.indexOf("*/*") >= 0
}

// Whether one glob matches the name, case aside: "*" is any run, "?" is one character, "[...]" is
// one character out of a set, and every other character is itself. Two pointers that return to the
// last star, never a RegExp: the glob is the portal caller's, and one like "*a*a*a*a*b" backtracks
// a RegExp exponentially on a name it misses, once per row on the UI thread.
function matchesGlob(name, glob) {
    var n = String(name).toLowerCase()
    var g = String(glob).toLowerCase()
    var i = 0
    var j = 0
    var star = -1
    var mark = 0
    while (i < n.length) {
        var next = j < g.length && g.charAt(j) !== "*" ? takeOne(g, j, n.charAt(i)) : -1
        if (next >= 0) {
            i++
            j = next
        } else if (j < g.length && g.charAt(j) === "*") {
            star = j++
            mark = i
        } else if (star >= 0) {
            j = star + 1
            i = ++mark
        } else {
            return false
        }
    }
    while (j < g.length && g.charAt(j) === "*") {
        j++
    }
    return j === g.length
}

// The index past the glob element at j once it takes the character c, or -1 when it does not: "?"
// takes any, "[...]" takes one of its set, anything else takes itself. Chromium sends a jpg as
// "*.[jJ][pP][gG]", the portal spec's own way to match any case, so a bracket is a class; one no
// "]" closes is its own text, as in fnmatch.
function takeOne(g, j, c) {
    var head = g.charAt(j)
    if (head === "?") {
        return j + 1
    }
    if (head !== "[") {
        return head === c ? j + 1 : -1
    }
    var end = classEnd(g, j)
    if (end < 0) {
        return c === "[" ? j + 1 : -1
    }
    return inClass(g, j + 1, end, c) ? end + 1 : -1
}

// The index of the "]" closing the class opened at j, or -1 when none does. A "]" right after the
// "[" or after a leading "!" or "^" is a member, not the close.
function classEnd(g, j) {
    var k = j + 1
    if (g.charAt(k) === "!" || g.charAt(k) === "^") {
        k++
    }
    if (g.charAt(k) === "]") {
        k++
    }
    return g.indexOf("]", k)
}

// Whether c is in the class body g[from, end): "a-z" is a range by character code, a "-" first or
// last is itself, and a leading "!" or "^" takes the complement.
function inClass(g, from, end, c) {
    var k = from
    var negate = g.charAt(k) === "!" || g.charAt(k) === "^"
    if (negate) {
        k++
    }
    var hit = false
    while (k < end) {
        if (k + 2 < end && g.charAt(k + 1) === "-") {
            hit = hit || (g.charAt(k) <= c && c <= g.charAt(k + 2))
            k += 3
        } else {
            hit = hit || g.charAt(k) === c
            k++
        }
    }
    return hit !== negate
}

// Any glob matching the name; no globs at all is no glob leg.
function matchesGlobs(name, globs) {
    if (!Array.isArray(globs) || globs.length === 0) {
        return true
    }
    for (var i = 0; i < globs.length; i++) {
        if (matchesGlob(name, globs[i])) {
            return true
        }
    }
    return false
}

// The media classes src/backend/icons.rs names a "<class>-x-generic" icon for. A row carries that
// icon name and never its MIME type (docs/protocol.md, "i"), so the class is all a row can confirm.
var CLASSES = ["image", "video", "audio", "text", "font", "application"]

// The media class a row's icon name speaks for: image-x-generic and image-jpeg both say image;
// x-office-document says nothing a rule can hold on to.
function iconClass(icon) {
    var head = String(icon || "").split("-")[0]
    return CLASSES.indexOf(head) < 0 ? "" : head
}

// Any mime rule the icon can confirm; no rules at all is no mime leg. A class rule like image/*
// matches by class and */* matches everything. An exact subtype like image/jpeg does not narrow:
// no row can confirm it, so a filter of exact subtypes alone hides nothing rather than every file.
// An optional per-row "y" mime field computed in src/backend/rows.rs would lift that, at a wire
// cost per row the hot path refuses.
function matchesMimes(icon, mimes) {
    if (!Array.isArray(mimes) || mimes.length === 0) {
        return true
    }
    var cls = iconClass(icon)
    var confirmable = false
    for (var i = 0; i < mimes.length; i++) {
        var parts = String(mimes[i]).split("/")
        if (parts.length !== 2 || parts[1] !== "*") {
            continue
        }
        confirmable = true
        if (parts[0] === "*" || (cls !== "" && parts[0] === cls)) {
            return true
        }
    }
    return !confirmable
}

// Whether a listing row stands under a filter: globs on the name AND mime rules on the icon class,
// so a filter carrying both narrows to the intersection. A directory always stands: a filter that
// hides the way out of a directory is a trap. No filter at all hides nothing.
function matchesRow(row, filter) {
    if (!filter || row.d === true) {
        return true
    }
    return matchesGlobs(String(row.n || ""), filter.globs) && matchesMimes(row.i, filter.mimes)
}
