.pragma library

// The chrome's breadcrumb: one path as the pieces a click can land on, and what fits when the bar is
// narrower than the path. Split out of ui/js/Nav.js, which walks the tree; this only draws where it is.

// Issue 45: the chrome's path as the pieces a click can land on. text is what is drawn, including
// the separator that follows it, so the pieces concatenate to exactly the one line they replace;
// path is the directory the piece names, which is what ui/ChromeBar.qml hands to pathEntered. The
// home test is the whole-component one, home itself or home and a separator, which ui/js/Format.js
// tilde and ui/js/Search.js scopeRoot now both make too: a sibling like /home/gmx is not inside home.
function crumbs(path, home) {
    if (path === "flea:stack")
        return [{ text: "The Stack", path: "flea:stack", last: true }]
    var text = String(path)
    var base = String(home)
    var inHome = base.length > 0 && (text === base || text.indexOf(base + "/") === 0)
    var display = inHome ? "~" + text.substring(base.length) : text
    var parts = display.split("/")
    var walked = inHome ? base : ""
    // The leading "~" and the leading "/" are each a crumb of their own: one names home and the
    // other names the root, and neither is a component the split hands back.
    var out = [{ text: parts.length > 1 ? parts[0] + "/" : parts[0],
                 path: walked.length > 0 ? walked : "/", last: false }]
    for (var i = 1; i < parts.length; i++) {
        if (parts[i].length === 0) {
            continue
        }
        walked = walked + "/" + parts[i]
        out.push({ text: parts[i] + "/", path: walked, last: false })
    }
    // Only a crumb with another after it carries a separator, so the last one gives its own back.
    var end = out[out.length - 1]
    if (out.length > 1) {
        end.text = end.text.substring(0, end.text.length - 1)
    }
    end.last = true
    return out
}

// Below this a leaf gives up more to the ellipsis than the ellipsis saves, so it is drawn whole.
var LEAF_FLOOR = 6

// What the crumbs before it leave the leaf, in characters.
function leafRoom(list, budget) {
    return budget - (crumbChars(list) - list[list.length - 1].text.length)
}

// Chrome rule 2 collapses whole crumbs, but the leaf is the one crumb that cannot be dropped: when
// even it does not fit, it takes the ellipsis in its own middle rather than a cut at the strip's edge.
function elideLeaf(list, budget) {
    var end = list[list.length - 1]
    var room = leafRoom(list, budget)
    if (room >= end.text.length || end.text.length <= LEAF_FLOOR) {
        return list
    }
    // corner: a strip with less room than the floor draws the floor, which is short rather than cut.
    var keep = Math.max(LEAF_FLOOR, room)
    var head = Math.ceil((keep - 1) / 2)
    var tail = keep - 1 - head
    var cut = end.text.substring(0, head) + "\u2026" + (tail > 0 ? end.text.substring(end.text.length - tail) : "")
    return list.slice(0, list.length - 1).concat([{ text: cut, path: end.path, last: true }])
}

// Chrome rule 2: a path too long for the strip reads as its root, one collapsed crumb and the segments nearest you, whole crumbs only, so the marker is a crumb of its own. budget is the strip's width in characters, monospace.
function fitCrumbs(list, budget) {
    if (list.length < 4 || crumbChars(list) <= budget) {
        return elideLeaf(list, budget)
    }
    var shown = [list[0], { text: "\u2026/", path: "", last: false, elided: true },
                 list[list.length - 2], list[list.length - 1]]
    // Whatever room the marker leaves goes back to the crumbs nearest the root, in walking order.
    for (var i = 1; i < list.length - 2; i++) {
        var next = shown.slice()
        next.splice(i, 0, list[i])
        if (crumbChars(next) > budget) {
            break
        }
        shown = next
    }
    // A parent that leaves the leaf less than it needs and less than its floor goes the way the
    // middle went: rule 2 elides whole crumbs, and the leaf is the one that may not be the one to go.
    if (shown.length === 4) {
        var left = leafRoom(shown, budget)
        if (left < shown[3].text.length && left < LEAF_FLOOR) {
            shown = [shown[0], shown[1], shown[3]]
        }
    }
    return elideLeaf(shown, budget)
}

function crumbChars(list) {
    var chars = 0
    for (var i = 0; i < list.length; i++) {
        chars += list[i].text.length
    }
    return chars
}
