.pragma library

.import "Contrast.js" as Contrast
.import "Palette.js" as Palette

// The ink names after the foreground, in the order the code reply numbers its roles: 1 to 6 are bat's SGR 31 to 36.
var INKS = ["red", "green", "yellow", "blue", "magenta", "cyan"]
// The inks a theme without that colour draws in.
var FALLBACK = { red: "#f38ba8", green: "#a6e3a1", yellow: "#f9e2af", blue: "#89b4fa", magenta: "#cba6f7", cyan: "#94e2d5" }
// A tab fills to the next multiple of this many columns.
var TAB = 8

// The foreground, then bat's six colours, each the palette's own (its ANSI slot when unnamed) lifted to 4.5:1 on the background or surface it contrasts less with.
function inks(found, fallback, foreground) {
    var bg = Palette.pick(found, ["background"], fallback.background)
    var surface = Palette.surface(bg, Palette.pick(found, ["foreground", "color7"], foreground),
                                  Palette.pick(found, Palette.SURFACE_KEYS, fallback.surface))
    var out = [foreground]
    for (var i = 0; i < INKS.length; i++) {
        var ink = Palette.pick(found, [INKS[i], "color" + (i + 1)], FALLBACK[INKS[i]])
        out.push(Contrast.ensureRatio(ink, Contrast.ratio(ink, bg) < Contrast.ratio(ink, surface) ? bg : surface, 4.5))
    }
    return out
}

function roleName(role) {
    return role >= 1 && role <= INKS.length ? INKS[role - 1] : "foreground"
}

function escape(text) {
    return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;")
}

// The reply's flat [role, text, role, text, ...] as [role, text] pairs with every tab filled to its stop, since a rich text folds a tab into one space.
// Sample input: [5, "ab", 0, "\tx"] answers [[5, "ab"], [0, "      x"]].
function pairs(runs) {
    var out = []
    var column = 0
    for (var i = 0; i + 1 < runs.length; i += 2) {
        var text = ""
        var raw = String(runs[i + 1])
        for (var j = 0; j < raw.length; j++) {
            var fill = raw.charAt(j) === "\t" ? TAB - column % TAB : 1
            text += raw.charAt(j) === "\t" ? " ".repeat(fill) : raw.charAt(j)
            column += fill
        }
        out.push([runs[i], text])
    }
    return out
}

function plain(runs) {
    return pairs(runs).map(function (pair) { return pair[1] }).join("")
}

// One line as StyledText: every run escaped, a coloured run in one font tag, and each space kept as a non-breaking space unless it is a single space between two characters, which a rich text keeps and may wrap at.
// Sample input: [5, "fn", 0, "  x <b>"] answers <font color="#cba6f7">fn</font>&nbsp;&nbsp;x &lt;b&gt; with the magenta ink.
function styled(runs, colors) {
    var parts = pairs(runs)
    var line = parts.map(function (pair) { return pair[1] }).join("")
    var out = ""
    var at = 0
    for (var i = 0; i < parts.length; i++) {
        var body = ""
        for (var j = 0; j < parts[i][1].length; j++, at++) {
            var c = parts[i][1].charAt(j)
            var kept = c === " " && at > 0 && at < line.length - 1 && line.charAt(at - 1) !== " " && line.charAt(at + 1) !== " "
            body += c === " " ? (kept ? " " : "&nbsp;") : escape(c)
        }
        out += parts[i][0] > 0 ? "<font color=\"" + colors[parts[i][0]] + "\">" + body + "</font>" : body
    }
    return out
}
