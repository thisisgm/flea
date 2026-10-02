.pragma library

// The chrome's breadcrumb: one path as the pieces a click can land on, and what fits when the bar is
// narrower than the path. Split out of ui/js/Nav.js, which walks the tree; this only draws where it is.

// Issue 45: the chrome's path as the pieces a click can land on. text is what is drawn, including
// the separator that follows it, so the pieces concatenate to exactly the one line they replace;
// path is the directory the piece names, which is what ui/ChromeBar.qml hands to pathEntered. The
// home test is the whole-component one, home itself or home and a separator, which ui/js/Format.js
// tilde and ui/js/Search.js scopeRoot now both make too: a sibling like /home/gmx is not inside home.
function crumbs(path, home, rootAlias) {
    var text = String(path)
    var aliasRoot = rootAlias ? cleanRoot(rootAlias.path) : ""
    var aliasLabel = rootAlias ? String(rootAlias.label || "") : ""
    var aliased = aliasRoot.length > 0 && aliasLabel.length > 0 && inside(text, aliasRoot)
    if (aliased) {
        // The label is one crumb even when it contains "/"; only the real path's suffix supplies
        // separators and navigation targets.
        var tail = text.substring(aliasRoot.length)
        var aliasedParts = tail.length > 0 ? tail.substring(1).split("/") : []
        var aliasedOut = [{ text: aliasLabel + (aliasedParts.length > 0 ? "/" : ""),
                            path: aliasRoot, last: false }]
        var aliasedWalked = aliasRoot
        for (var a = 0; a < aliasedParts.length; a++) {
            if (aliasedParts[a].length === 0)
                continue
            aliasedWalked += "/" + aliasedParts[a]
            aliasedOut.push({ text: aliasedParts[a] + "/", path: aliasedWalked, last: false })
        }
        var aliasedEnd = aliasedOut[aliasedOut.length - 1]
        if (aliasedOut.length > 1)
            aliasedEnd.text = aliasedEnd.text.substring(0, aliasedEnd.text.length - 1)
        aliasedEnd.last = true
        return aliasedOut
    }

    var base = String(home)
    var inHome = base.length > 0 && inside(text, base)
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

function cleanRoot(path) {
    var text = String(path || "")
    while (text.length > 1 && text.charAt(text.length - 1) === "/")
        text = text.substring(0, text.length - 1)
    return text
}

function inside(path, root) {
    return path === root || path.indexOf(root + "/") === 0
}

// Issue 237: the network service remembers every mounted display root it has resolved. The deepest
// one wins, so two WebDAV spaces on one host and a saved location inside a broader mount stay distinct.
function aliasAt(path, aliases) {
    var text = String(path || "")
    var best = null
    for (var i = 0; aliases && i < aliases.length; i++) {
        var root = cleanRoot(aliases[i].path)
        if (root.length > 0 && inside(text, root) && (!best || root.length > best.path.length))
            best = { path: root, label: String(aliases[i].label || ""), key: aliases[i].key }
    }
    return best && best.label.length > 0 ? best : null
}

function rememberAlias(aliases, path, label, key) {
    var root = cleanRoot(path)
    var name = String(label || "")
    if (root.length === 0 || name.length === 0)
        return aliases || []
    var id = String(key || root)
    var out = []
    for (var i = 0; aliases && i < aliases.length; i++) {
        if (aliases[i].key !== id && cleanRoot(aliases[i].path) !== root)
            out.push(aliases[i])
    }
    out.push({ path: root, label: name, key: id })
    return out
}

// Only a persisted source may rename an alias; a live mount's generated label is not a saved name.
function labelsByKey(entries, keyFor) {
    var out = {}
    for (var i = 0; entries && i < entries.length; i++) {
        var address = entries[i].uri || entries[i].path
        if (entries[i].saved === true && address && entries[i].label)
            out[keyFor(address)] = entries[i].label
    }
    return out
}

// A saved-place rename changes the visible root without changing a real path or navigation target.
function relabelAliases(aliases, labels) {
    var changed = false
    var out = []
    for (var i = 0; aliases && i < aliases.length; i++) {
        var name = labels && labels[aliases[i].key] ? String(labels[aliases[i].key]) : aliases[i].label
        changed = changed || name !== aliases[i].label
        out.push(name === aliases[i].label ? aliases[i]
                                          : { path: aliases[i].path, label: name, key: aliases[i].key })
    }
    return changed ? out : (aliases || [])
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
