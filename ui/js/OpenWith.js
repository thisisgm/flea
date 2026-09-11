.pragma library

// OpenWith.html rule 5: the dialog draws two groups under eyebrows, the type's own registry first with
// the desktop's default at its head, then every installed application alphabetically. The search
// filters both, and a group that matches nothing takes its eyebrow with it.
function rows(handlers, installed, kind, query) {
    var needle = String(query || "").trim().toLowerCase()
    var out = []
    var seat = 0
    var registered = matching(handlers, needle)
    if (registered.length) {
        out.push({ eyebrow: "Registered for " + kind, rule: false })
        for (var i = 0; i < registered.length; i++) out.push(seated(registered[i], seat++))
    }
    var every = matching(installed, needle)
    if (every.length) {
        out.push({ eyebrow: "All applications", rule: registered.length > 0 })
        for (var j = 0; j < every.length; j++) out.push(seated(every[j], seat++))
    }
    return out
}

function matching(apps, needle) {
    var out = []
    for (var i = 0; i < (apps || []).length; i++) {
        if (!needle.length || String(apps[i].label).toLowerCase().indexOf(needle) >= 0) out.push(apps[i])
    }
    return out
}

// A copy carrying the cursor seat it holds, so the drawn row never counts the list to find its own.
// A registered application is drawn twice, once per group, and the two copies take different seats.
// The seat is carried, not recounted: counting made this quadratic over a hundred-application box.
function seated(app, seat) {
    return { id: app.id, label: app.label, icon: app.icon, default: app.default === true, at: seat }
}

// The rows the cursor can land on, in the order they are drawn; an eyebrow is a caption it steps over.
function applications(rows) {
    var out = []
    for (var i = 0; i < (rows || []).length; i++) {
        if (rows[i].eyebrow === undefined) out.push(rows[i])
    }
    return out
}

// The row index a cursor position holds, so the drawn list and the keys agree on one order.
function rowOf(rows, cursor) {
    var seen = -1
    for (var i = 0; i < (rows || []).length; i++) {
        if (rows[i].eyebrow !== undefined) continue
        if (++seen === cursor) return i
    }
    return -1
}

// Rule 5's viewport: seven applications show before the list scrolls, and the eyebrows standing over
// them are in view too, so the height lands on a row boundary instead of cutting the eighth in half.
function viewportHeight(rows, applications, rowHeight, eyebrowHeight) {
    var seen = 0, total = 0
    for (var i = 0; i < (rows || []).length && seen < applications; i++) {
        if (rows[i].eyebrow !== undefined) { total += eyebrowHeight; continue }
        total += rowHeight
        seen++
    }
    // Rule 6 keeps this height when a search matches nothing, so a short catalogue still fills it.
    return seen < applications ? total + (applications - seen) * rowHeight : total
}

// The caption rule 6 centres in the list's own height when the search names nothing installed.
function noMatch(query) {
    return "No application matches “" + String(query).trim()
         + "”. Clear the search to see every installed application."
}
