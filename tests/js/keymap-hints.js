.import "../../ui/js/Keymap.js" as Keymap

function run(check) {
    // A preset shift row must not replace the plain hint for an action.
    var keepRows = Keymap.bindingRows
    Keymap.bindingRows = function (name) {
        return [{ mods: "none", key: "Delete", keys: "delete", action: "trash", preset: "all" },
                { mods: "shift", key: "X", keys: "shift-x", action: "trash", preset: name }]
    }
    Keymap.setPreset("vim")
    check("a preset shift row never replaces the plain hint", Keymap.hintFor("trash"), "delete")
    Keymap.bindingRows = keepRows
    Keymap.setPreset("vim")
    check("deletePermanently still gets its shift hint where no plain row exists",
          Keymap.hintFor("deletePermanently"), "shift-delete")

    // The status strip asks for this one key while the first window builds.
    Keymap.setPreset("default")
    var keepLookup = Keymap.lookupFor, lookups = 0
    Keymap.lookupFor = function () { lookups++; return keepLookup.apply(null, arguments) }
    var firstAsk = Keymap.hintFor("deletePermanently")
    Keymap.lookupFor = keepLookup
    var ownRows = Keymap.PRESET_KEYS.concat(Keymap.SHARED_KEYS).filter(function (row) {
        return (row.preset === "all" || row.preset === "default") && Keymap.applies(row, "listing", "gui")
            && Keymap.actionGroup(row.action) === "deletePermanently"
    }).length
    check("the action asked for has rows of its own, so the count below can go red", ownRows > 0, true)
    check("a first hint ask looks up only the rows of the action it asks for", lookups, ownRows)
    check("and still answers the shift chord", firstAsk, "shift-delete")
}
