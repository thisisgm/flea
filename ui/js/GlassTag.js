.pragma library

// The Hyprland tag Omarchy's transparency hotkey sets; Omarchy's own rule then draws the window at its Transparent opacity.
var TAG = "transparent"
// Hyprland addresses are lowercase hex with a 0x prefix; nothing else reaches a dispatch.
var ADDRESS = /^0x[0-9a-f]+$/
// Prints "address opacity inactive" for each address argument, read from Hyprland's own window properties.
var PROBE = 'for a in "$@"; do printf "%s %s %s\\n" "$a" "$(hyprctl getprop "address:$a" opacity)" "$(hyprctl getprop "address:$a" opacity_inactive)"; done'
// Applies "address tag" argument pairs with the Lua dispatcher, else the classic one, as omarchy-hyprland-window-pop does.
var DISPATCH = 'while [ "$#" -ge 2 ]; do hyprctl dispatch "hl.dsp.window.tag({ window = \\"address:$1\\", tag = \\"$2\\" })" >/dev/null 2>&1 || hyprctl dispatch tagwindow "$2" "address:$1" >/dev/null; shift 2; done'

// Sample input: hyprctl clients -j, '[{"address":"0x55bacb0a1f80","pid":4211,"class":"com.thisisgm.flea"}]'.
function ownAddresses(json, pid) {
    var list
    try {
        list = JSON.parse(json)
    } catch (e) {
        return []
    }
    if (!Array.isArray(list))
        return []
    var out = []
    for (var i = 0; i < list.length; i++) {
        var c = list[i]
        if (c && c.pid === pid && ADDRESS.test(String(c.address)))
            out.push(String(c.address))
    }
    return out
}

// Sample input, one PROBE line: "0x55bacb0a1f80 0.985 0.96".
function parseProps(line) {
    var f = String(line).trim().split(/\s+/)
    if (f.length !== 3 || !ADDRESS.test(f[0]))
        return null
    return {address: f[0], opacity: Number(f[1]), inactive: Number(f[2])}
}

// "add", "remove" or "keep" for one Flea window: the tag goes only where Omarchy draws Transparent and Hyprland leaves the window at 1.
function decide(mode, usesDefault, opacity, inactive, taggedByFlea) {
    var wanted = mode === "transparent" && usesDefault
    // The hotkey's own test, which also re-tags a window that reuses a closed one's address: a window Hyprland already draws
    // below 1, as stable Omarchy draws every window, keeps its rule.
    if (wanted && opacity === 1 && inactive === 1)
        return "add"
    return taggedByFlea && !wanted ? "remove" : "keep"
}

// The dispatch pairs for PROBE's output, and the set Flea has tagged once they run; a closed window's record drops out.
function plan(text, mode, usesDefault, tagged) {
    var next = {}
    var pairs = []
    var lines = String(text).split("\n")
    for (var i = 0; i < lines.length; i++) {
        var w = parseProps(lines[i])
        if (!w)
            continue
        var mine = tagged[w.address] === true
        if (mine)
            next[w.address] = true
        var act = decide(mode, usesDefault, w.opacity, w.inactive, mine)
        if (act === "add") {
            pairs.push(w.address, "+" + TAG)
            next[w.address] = true
        }
        if (act === "remove") {
            pairs.push(w.address, "-" + TAG)
            delete next[w.address]
        }
    }
    return {pairs: pairs, tagged: next}
}
