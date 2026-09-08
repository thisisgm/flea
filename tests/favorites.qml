import QtQuick
import Quickshell
import Quickshell.Io
import "js/Places.js" as Places

ShellRoot {
    id: root
    property int failures: 0
    property string home: Quickshell.env("HOME")
    function check(name, actual, expected) {
        if (actual !== expected) { console.log("FAIL " + name + ": " + actual); root.failures++ }
    }
    function refresh() {
        reader.reload()
        reader.waitForJob()
        favorites.entries = Places.favorites(root.home, 'XDG_DOWNLOAD_DIR="$HOME/Downloads"\n', reader.text(), function () { return "folder" })
    }
    FileView { id: reader; path: root.home + "/.config/gtk-3.0/bookmarks"; printErrors: false }
    FavoritePlaces {
        id: favorites
        onWrote: root.refresh()
        onMessage: function (text, isError) { console.log("FAIL " + text); root.failures++ }
    }
    Component.onCompleted: {
        root.refresh()
        var folder = root.home + "/My files/#100%?\nnext"
        favorites.edit(folder, true)
        check("first save creates the bookmarks file and parent directories", Places.bookmarks(reader.text())[0].path, folder)
        var saved = reader.text()
        // An external writer changes the shared file after the component has read it.
        reader.setText(saved + "smb://nas/share NAS\n")
        reader.waitForJob()
        favorites.edit(folder, false)
        check("remove rereads and preserves another writer's changes", reader.text(), "smb://nas/share NAS\n")
        favorites.edit(folder, true)
        check("add after remove uses fresh contents", Places.bookmarks(reader.text()).length, 1)
        favorites.edit(folder, false)
        reader.setText("")
        reader.waitForJob()
        root.refresh()
        favorites.edit(folder, true)
        favorites.edit(folder, false)
        check("last favorite removal writes an empty file", reader.text(), "")
        console.log("favorites: " + root.failures + " failed")
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
}
