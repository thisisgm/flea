pragma Singleton
import Quickshell
import QtQuick
import qs.Commons

// shell.toml's [flea] background-alpha: the planes the listing sits on let the desktop through, their
// text does not. See AGENTS.md "Theme roles and sources" for which planes and why the rest stay opaque.
Singleton {
    id: root

    // The window's format is fixed at creation. Match boot/shell.qml's rounded color alpha
    // when deciding whether these planes can turn transparent later.
    property bool clearWindow: false
    readonly property real planeAlpha: clearWindow ? Color.pickAlpha("flea.background-alpha", 1) : 1
    readonly property color backgroundPlane: Util.alpha(Theme.color.background, planeAlpha)
    readonly property color surfacePlane: Util.alpha(Theme.color.surface, planeAlpha)

    // Finish Color's existing startup reads before choosing the window surface.
    Component.onCompleted: {
        Color.shellFile.waitForJob()
        Color.userShellFile.waitForJob()
        root.clearWindow = Util.alpha(Theme.color.background, Color.pickAlpha("flea.background-alpha", 1)).a < 1
    }
}
