import QtQuick
import "." as Flea
import "js/PickerEntry.js" as PickerEntry
import "js/ShareUrl.js" as ShareUrl

// What the window does with an smb, sftp or ssh URL the location field reported: the share is
// mounted at its root through ui/ShareResolve.qml and the rest of the URL is then walked on the
// FUSE path as if it had been typed as a local path, so a folder opens, a file is selected and
// Return answers its FUSE path, which the document portal exports for a sandboxed caller. Nothing
// is downloaded, and no password is asked for: a share that wants one is mounted from the rail.
Item {
    id: root

    property var picker: null
    property var navigate: null

    // The rest of the URL being mounted, joined on once the root's FUSE path is known.
    property string _rest: ""

    function enter(answer) {
        if (root.picker.saving) {
            root.picker.say(ShareUrl.SAVE_LOCAL)
            return
        }
        var url = ShareUrl.split(answer.url)
        if (url.reason === "share") {
            root.picker.say(ShareUrl.NAME_SHARE)
            return
        }
        if (url.reason.length > 0) {
            root.picker.say(root.navigate.refusal(url.reason))
            return
        }
        root._rest = url.rest
        // Held rather than timed: the legs can take their whole deadline, and the footer says so
        // until they answer.
        root.picker.say(ShareUrl.connecting(url.root), true)
        resolver.resolve(url.root)
    }

    // Wired here rather than in ui/picker.qml, which is at its line cap.
    Connections {
        target: root.navigate
        function onShareEntered(answer) { root.enter(answer) }
    }

    Flea.ShareResolve {
        id: resolver
        onResolved: function (shareRoot, localPath) {
            root.picker.say("")
            var line = ShareUrl.localLine(localPath, root._rest)
            root.navigate.act(PickerEntry.classify(line, root.picker.path, root.picker.home))
        }
        onResolveFailed: function (shareRoot, message) {
            // A second line while the legs run is refused without clearing the first one's notice.
            if (message === ShareUrl.BUSY) {
                root.picker.say(message, true)
                return
            }
            root.picker.message(message, true)
        }
    }
}
