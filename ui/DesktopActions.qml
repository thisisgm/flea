import QtQuick
import "."
import "js/DesktopKeys.js" as DesktopKeys

QtObject {
    id: root
    required property var pane

    function act(action) { return DesktopKeys.act(action, root.pane, root) }
    function typeAhead(event) { return DesktopKeys.typeAhead(event, root.pane) }
    function closeWindow() { if (pane.Window.window) pane.Window.window.close() }
    function bookmark() {
        var path = pane.path
        if (Favourites.records.some(function (record) { return record.path === path })) {
            pane.message("This folder is already in Favorites.", false)
            return
        }
        Favourites.add(path, path.substring(path.lastIndexOf("/") + 1) || path)
    }
}
