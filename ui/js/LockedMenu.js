.pragma library

.import "Menu.js" as Menu
.import "Errors.js" as Errors
.import "StateWords.js" as StateWords

// The Locked tile's own menu, issue 193 follow-up (2026-09-24). A right click on the
// tile names the locked folder itself, never the parent behind it, so only rows that act
// on that folder without listing it are offered: terminal, permissions and path. Kept in
// ui/js/Menu.js INVENTORY order, one group, so the menu never reorders under its caller.
// For 0.3.8: the tile keeps its single Copy path row, acting as before,
// while its visibility follows the Copy as switch that replaced Copy path in Settings > Menus.
// A Places row's menu keeps the flat row the same way (ui/js/Menu.js availableEntry).
var LOCKED_IDS = ["openTerminal", "permissions", "copyAs"]

// The bar's sentence when every locked id is hidden, naming the switch that brings them back.
var LOCKED_REFUSAL = "Open in terminal, Permissions and Copy as are hidden in Settings > Menus."
// The shipped default shows Permissions alone, so the bar still names the two hidden rows, as the refusal names all three.
var LOCKED_NOTICE = "Open in terminal and Copy as are hidden in Settings > Menus."

// Sample input: { lockedMode: 0o040700, hiddenActions: [] }
function lockedEntries(p) {
    var out = [], group = ""
    var scoped = { rowMode: p.lockedMode, selectionCount: 1, hiddenActions: p.hiddenActions }
    for (var i = 0; i < Menu.INVENTORY.length; i++) {
        var spec = Menu.INVENTORY[i]
        if (LOCKED_IDS.indexOf(spec[0]) < 0 || Menu.isHidden(p.hiddenActions, spec[0])) continue
        // The Copy as spec lends its inventory slot and its hidden switch; the row it draws
        // is still the flat Copy path row, which ui/Pane.qml performLocked answers with copyText.
        var id = spec[0] === "copyAs" ? "copypath" : spec[0]
        var entry = { id: id, action: id, label: spec[0] === "copyAs" ? "Copy path" : spec[1], glyph: spec[2] }
        if (!Menu.availableEntry(entry, scoped, "F")) continue
        if (entry.action === "permissions") {
            var permission = lockedPermissions(p.lockedMode)
            entry.disabled = permission.disabled
            if (permission.errored) { entry.errored = true; if (permission.hint) entry.hint = permission.hint }
            else { delete entry.errored; delete entry.hint }
        }
        if (out.length && group !== spec[4]) out.push({ separator: true })
        group = spec[4]
        out.push(entry)
    }
    return out
}

// Sample input: { lockedMode: 0o040700, hiddenActions: ["openTerminal", "permissions", "copyAs"] }
function lockedRefusal(p) {
    return lockedEntries(p).length === 0 ? LOCKED_REFUSAL : ""
}

// Sample input: { hiddenActions: ["openTerminal", "copyAs"] }
function lockedNotice(p) {
    var shown = LOCKED_IDS.filter(function (id) { return !Menu.isHidden(p.hiddenActions, id) })
    return shown.length === 1 && shown[0] === "permissions" ? LOCKED_NOTICE : ""
}

// The mode is the locked directory's own, so ownership decides: a denial on owner-readable
// owner-executable proves somebody else owns it, and nobody but the owner can chmod it back,
// which reads red with its state word, the way a provider that cannot answer does. An owner
// locked out too may still be the owner, so that row stays plain: it is the way back in.
function lockedPermissions(mode) {
    var kind = (Number(mode) || 0) & 0o170000
    if (kind !== 0o100000 && kind !== 0o040000) {
        var bad = { disabled: true, errored: true }
        var word = StateWords.permissions(mode)
        if (word) bad.hint = word
        return bad
    }
    if (Errors.notYours(mode)) return { disabled: true, errored: true, hint: StateWords.WORDS.permissionsNotYours }
    return { disabled: false }
}
