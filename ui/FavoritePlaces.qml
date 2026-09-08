import QtQuick
import Quickshell
import Quickshell.Io
import "js/Places.js" as Places

// Local favorites share GTK's file with network places. Read before every edit, and wait for
// the write before reloading the rail, as NetworkPlaces does for a saved share's name.
Item {
    id: root
    property var entries: []
    signal wrote()
    signal message(string text, bool isError)

    function action(path) { return Places.favoriteAction(root.entries, path) }

    function edit(path, add) {
        if (root.action(path) !== (add ? "addFavorite" : "removeFavorite")) return
        marks.readError = FileViewError.Success
        marks.reload()
        marks.waitForJob()
        if (marks.readError !== FileViewError.Success && marks.readError !== FileViewError.FileNotFound) {
            root.message("Saved places could not be read, so no changes were saved.", true)
            return
        }
        var body = Places.editFavorite(marks.text(), path, add)
        if (body === marks.text()) { root.wrote(); return }
        marks.writeFailed = false
        marks.setText(body)
        marks.waitForJob()
        if (marks.writeFailed) {
            root.message("Changes to saved places could not be written.", true)
            return
        }
        root.wrote()
    }

    FileView {
        id: marks
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/bookmarks"
        printErrors: false
        property int readError: FileViewError.Success
        property bool writeFailed: false
        onLoadFailed: function (error) { marks.readError = error }
        onSaveFailed: marks.writeFailed = true
    }
}
