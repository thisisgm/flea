.import "../../ui/js/PathMenu.js" as PathMenu

function run(check) {
    var calls = [], folder = "/fixture/Places/Downloads"
    var pane = {
        path: "/fixture/unrelated", cursorIndex: 5,
        clipboard: {paths: ["/fixture/copied.txt"], moving: false},
        open: function(path) { calls.push(["open", path]) },
        openTerminal: function(path) { calls.push(["terminal", path]) },
        permissionsRequested: function(path) { calls.push(["permissions", path]) },
        join: function(base, name) { return base + "/" + name },
        sticky: function() {},
        backend: {
            duplicate: function(path, id) { calls.push(["duplicate", path, id]) },
            send: function(message) { calls.push(message) },
            compress: function(paths, dest, format, id) { calls.push(["compress", paths, dest, format, id]) }
        }
    }
    for (var action of ["open", "openTerminal", "permissions", "duplicate", "trash", "paste", "compress:zip"])
        PathMenu.perform(pane, action, 12, folder)
    check("Places actions target the clicked folder while another listing is open", JSON.stringify(calls), JSON.stringify([
        ["open", folder], ["terminal", folder], ["permissions", folder], ["duplicate", folder, 12],
        {c: "trash", paths: [folder], menuId: 12},
        {c: "transfer", op: "copy", paths: ["/fixture/copied.txt"], dest: folder},
        ["compress", [folder], "/fixture/Places/Downloads.zip", "zip", 12]
    ]))
    check("Places menu dispatch does not replace the listing cursor", pane.path + "|" + pane.cursorIndex, "/fixture/unrelated|5")
    pane.clipboard = {paths: ["/fixture/cut.txt"], moving: true}
    PathMenu.perform(pane, "paste", 12, folder)
    check("cut paste uses the Places destination and consumes the clipboard", calls[calls.length - 1].op + "|" + pane.clipboard.paths.length, "move|0")
    check("dialogs continue through the shared menu controller", PathMenu.perform(pane, "rename", 12, folder), false)
    check("top-level folder's parent is the filesystem root", PathMenu.parent("/Downloads"), "/")
}
