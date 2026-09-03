import QtQuick
import "." as Flea
import "js/Tabs.js" as Tabs

// The window's tab strip. Hidden with no height until a second tab exists, so the default window
// keeps the chrome-to-list layout every existing click test and the first-paint path already have.
Item {
    id: root

    property var pane: null

    // pane.tabs is a replaced JS object, so these bindings have to read it directly; a helper
    // call alone would not re-run when t opens a second tab.
    readonly property var tabs: pane ? pane.tabs : null
    readonly property string path: pane ? pane.path : ""
    readonly property int tabCount: root.tabs && root.tabs.items && root.tabs.items.length > 0 ? root.tabs.items.length : 1
    readonly property int currentIndex: root.tabs ? root.tabs.index : 0
    readonly property bool open: root.tabCount > 1
    readonly property int tabWidth: {
        var n = Math.max(1, root.tabCount)
        var avail = Math.max(0, root.width - Theme.hitMin)
        var maxW = Math.round(Theme.font.caption * 12) + Theme.hitMin + 2 * Theme.spacing.rowPaddingX
        var minW = Theme.hitMin * 3
        return Math.round(Math.max(minW, Math.min(maxW, avail / n)))
    }

    visible: root.open
    implicitHeight: Theme.chromeHeight
    height: visible ? implicitHeight : 0

    function itemAt(index) {
        return repeater.itemAt(index)
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }

    Row {
        id: strip
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        spacing: 0

        Repeater {
            id: repeater
            model: root.open ? root.tabCount : 0
            delegate: Item {
                id: tab
                required property int index
                width: root.tabWidth
                height: strip.height

                readonly property bool current: root.currentIndex === tab.index
                readonly property string title: (root.tabs, root.path, pane) ? Tabs.labelAt(pane, tab.index) : ""

                Accessible.role: Accessible.PageTab
                Accessible.name: tab.title
                Accessible.onPressAction: if (pane) Tabs.selectAt(pane, tab.index)

                HoverHandler { cursorShape: Qt.PointingHandCursor }

                Rectangle {
                    anchors.fill: parent
                    color: tab.current ? Theme.color.background : "transparent"
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left
                    anchors.right: parent.right
                    height: Theme.spacing.hairline * 2
                    color: Theme.color.accent
                    visible: tab.current
                }

                Rectangle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.spacing.hairline
                    height: parent.height * 0.45
                    color: Theme.color.foreground
                    opacity: 0.12
                    visible: tab.index < root.tabCount - 1 && !tab.current
                }

                Text {
                    id: titleText
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.right: closeHit.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: tab.title
                    color: tab.current ? Theme.color.foreground : Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }

                Item {
                    id: closeHit
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.hitMin
                    height: parent.height

                    Flea.Glyph {
                        anchors.centerIn: parent
                        width: Theme.chromeMarkSize
                        height: Theme.chromeMarkSize
                        name: "x"
                        color: Theme.color.muted
                    }
                }

                TapHandler {
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                    onTapped: function (eventPoint, button) {
                        if (!pane)
                            return
                        if (button === Qt.MiddleButton) {
                            Tabs.closeAt(pane, tab.index)
                            return
                        }
                        var local = closeHit.mapFromItem(tab, eventPoint.position.x, eventPoint.position.y)
                        if (local.x >= 0 && local.x <= closeHit.width && local.y >= 0 && local.y <= closeHit.height)
                            Tabs.closeAt(pane, tab.index)
                        else
                            Tabs.selectAt(pane, tab.index)
                    }
                }
            }
        }

        Item {
            id: addButton
            width: Theme.hitMin
            height: strip.height
            Accessible.role: Accessible.Button
            Accessible.name: "New tab"
            Accessible.onPressAction: if (pane) Tabs.openNew(pane)
            HoverHandler { cursorShape: Qt.PointingHandCursor }

            Flea.Glyph {
                anchors.centerIn: parent
                width: Theme.chromeMarkSize
                height: Theme.chromeMarkSize
                name: "plus"
                color: Theme.color.muted
            }

            TapHandler {
                acceptedButtons: Qt.LeftButton
                onTapped: if (pane) Tabs.openNew(pane)
            }
        }
    }
}
