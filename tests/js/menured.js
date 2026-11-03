.import "../../ui/js/Menu.js" as Menu

function rows(changes) {
    var p = { hasRow: true, selectionCount: 1, rowMode: 0o100644, rowIsFile: true,
        taildropInstalled: true, taildropPeers: [{ id: "box", label: "Box" }],
        localSendInstalled: true, localSendPeers: [{ id: "box", label: "Box" }],
        dropboxInstalled: true, dropboxPath: "/tmp/Dropbox", rowInDropbox: false }
    for (var key in changes) p[key] = changes[key]
    return Menu.listingEntries(p)
}
function entry(list, action) { return list.filter(function (e) { return e.action === action })[0] || {} }
function run(check) {
    var live = rows({})
    var cases = [
        ["taildrop", { taildropPeers: [], taildropCode: "signedOut" }],
        ["taildrop", { taildropPeers: [], taildropCode: "noPeers" }],
        ["taildrop", { taildropPeers: [], taildropCode: "statusFailed", taildropReason: "A provider sentence" }],
        ["taildrop", { taildropPeers: [], rowIsFile: false, rowMode: 0o040755 }],
        ["localsend", { localSendPeers: [] }],
        ["localsend", { localSendPeers: [], rowIsFile: false, rowMode: 0o040755 }],
        ["dropbox", { dropboxPath: "", dropboxCode: "signedOut" }],
        ["dropbox", { dropboxPath: "", dropboxCode: "notRunning" }],
        ["sharelink", { rowInDropbox: true, dropboxPath: "", dropboxCode: "signedOut" }]
    ]
    cases.forEach(function (c, i) {
        var row = entry(rows(c[1]), c[0]), tag = c[0] + "/" + i
        check(tag + " remains unavailable", row.disabled, true)
        check(tag + " uses the row's error ink", row.errored, true)
        check(tag + " has no state word", row.hint, undefined)
        check(tag + " keeps its label", row.label, entry(c[0] === "sharelink" ? rows({ rowInDropbox: true }) : live, c[0]).label)
    })
    ;["taildrop", "localsend", "dropbox"].forEach(function (action) {
        var row = entry(live, action)
        check(action + " answering stays enabled", row.disabled, false)
        check(action + " answering has no error ink", row.errored, undefined)
        check(action + " answering has no state word", row.hint, undefined)
    })
    var reading = rows({ taildropRefreshing: true, localSendChecking: true, dropboxRefreshing: true })
    ;["taildrop", "localsend", "dropbox"].forEach(function (action) {
        var row = entry(reading, action)
        check(action + " refreshing stays unavailable", row.disabled, true)
        check(action + " refreshing keeps existing muted recipe", row.errored, undefined)
        check(action + " refreshing has no state word", row.hint, undefined)
    })
    var down = rows({ taildropPeers: [], localSendPeers: [], dropboxPath: "" })
    var index = down.findIndex(function (row) { return row.action === "taildrop" })
    var next = Menu.stepRow(down, index - 1, 1)
    check("keyboard skips all unavailable app rows", down[next].action, "trash")
}
