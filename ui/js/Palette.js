// The Omarchy theme palette in colors.toml, parsed and queried. Pure, with no QML imports, so
// tests/js/palette.js can run it under qml6 with no window.

// Sample input: cyan = "#27a6a2"   # the ANSI ring
function parse(body) {
    var found = {};
    var lines = String(body || "").split("\n");
    for (var i = 0; i < lines.length; i++) {
        var kv = lines[i].match(/^\s*([A-Za-z0-9_-]+)\s*=\s*["']?(#[0-9A-Fa-f]{6})/);
        if (kv)
            found[kv[1]] = kv[2];
    }
    return found;
}

// ThemeRoles.html's surface ladder: the chrome plane, then background as the neutral fallback when
// it is absent, then selection for the alacritty-derived file that emits neither ladder key at all.
var SURFACE_KEYS = ["dark_background", "background", "selection"];

// The first key the theme actually set wins, and a role no theme models keeps Flea's own colour.
function pick(found, keys, fallback) {
    for (var i = 0; i < keys.length; i++) {
        if (found[keys[i]])
            return found[keys[i]];
    }
    return fallback;
}

// A theme need not set every role and pick() already falls back per role, so the only question a
// readiness flag can answer honestly is whether the file yielded any colour at all.
function isPalette(found) {
    return Object.keys(found).length > 0;
}
