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

// Use the supplied surface unless it is far from a light canvas.
var SURFACE_KEYS = ["dark_background", "background", "selection"];
function surface(background, foreground, themed) {
    function linear(v) { v /= 255; return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }
    var b0 = parseInt(background.slice(1), 16), s0 = parseInt(themed.slice(1), 16);
    var br = (b0 >> 16) & 255, bg = (b0 >> 8) & 255, bb = b0 & 255;
    var delta = Math.max(Math.abs(br - ((s0 >> 16) & 255)), Math.abs(bg - ((s0 >> 8) & 255)), Math.abs(bb - (s0 & 255))) / 255;
    if (0.2126 * linear(br) + 0.7152 * linear(bg) + 0.0722 * linear(bb) <= 0.5 || delta <= 0.20) return themed;
    var bg = parseInt(background.slice(1), 16), fg = parseInt(foreground.slice(1), 16);
    var value = 0;
    for (var shift = 16; shift >= 0; shift -= 8) {
        var b = (bg >> shift) & 255, f = (fg >> shift) & 255;
        value |= Math.round(b + (f - b) * 0.08) << shift;
    }
    return "#" + ("000000" + value.toString(16)).slice(-6);
}

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
