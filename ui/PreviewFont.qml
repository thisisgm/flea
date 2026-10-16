pragma ComponentBehavior: Bound

import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Focus.js" as Focus

// The font is registered only while this selected-file preview exists; FontLoader never runs over a listing.
FocusScope {
    id: root
    property string path: ""
    property var pane: null
    property bool compact: false
    property int faceIndex: 0
    property var variationAxes: ({})
    readonly property var faces: actions.info ? actions.info.faces : []
    readonly property var face: root.faces.length > root.faceIndex ? root.faces[root.faceIndex] : null
    readonly property var axes: root.face && actions.info.axes ? actions.info.axes[String(root.face.index)] || [] : []
    function chooseFace(index) { root.faceIndex = Math.max(0, Math.min(root.faces.length - 1, index)); root.variationAxes = ({}) }
    function setAxis(tag, value) { var next = Object.assign({}, root.variationAxes); next[tag] = value; root.variationAxes = next }
    property real sampleSize: root.compact ? 26 : 36
    readonly property string status: actions.error ? "This font could not be read."
        : loadedFont.status === FontLoader.Error ? "Preview unavailable in Qt; Linux installation can still be checked."
        : actions.info && loadedFont.status === FontLoader.Ready ? "font" : "loading"
    readonly property var fontInfo: actions.info
    readonly property var actionItem: install
    readonly property var sampleInput: custom
    readonly property bool busy: actions.busy
    signal closeRequested()
    signal expandRequested()
    Keys.onPressed: function(event) {
        if (!root.pane || root.compact || event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) { event.accepted = false; return }
        event.accepted = Focus.handleKey(event, root.pane, root.pane.sidebar)
    }
    Keys.onEscapePressed: root.closeRequested()
    function focusFirst() { (install.available ? install : closeButton).forceActiveFocus() }
    function changeSize(delta) { root.sampleSize = Math.max(8, Math.min(120, root.sampleSize + delta)) }
    function manageFont() { actions.run(actions.info && actions.info.installed ? "uninstall" : "install") }
    onPathChanged: { custom.text = lorem; flick.contentY = 0; root.faceIndex = 0; root.variationAxes = ({}) }
    readonly property string lorem: "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua."

    Flea.FontActions {
        id: actions
        path: root.path
        onInfoChanged: if (info) loadedFont.source = Format.fileUri(root.path)
        onFinished: function(verb) { if (verb === "info" && !root.compact) root.focusFirst() }
    }
    FontLoader { id: loadedFont }

    Column {
        id: heading
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Theme.spacing.gap
        spacing: Theme.spacing.gap
        Text {
            width: parent.width
            text: actions.info ? (root.face ? root.face.family + " · " + root.face.style : actions.info.family) : root.path.split("/").pop()
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            font.bold: true
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }
        Flow {
            width: parent.width
            spacing: Theme.spacing.gap
            Flea.DialogButton {
                id: install
                label: actions.info && actions.info.sourceRemoved ? "Uninstalled" : actions.busy ? "Working…" : actions.info && actions.info.installed ? "Uninstall" : "Install for me"
                primary: actions.info !== null && !actions.info.installed
                destructive: actions.info !== null && actions.info.installed
                available: actions.info !== null && !actions.info.sourceRemoved && !actions.busy
                tabStop: true
                onActivated: root.manageFont()
            }
            Flea.DialogButton { label: "−"; tabStop: true; available: root.sampleSize > 8; onActivated: root.changeSize(-4); Accessible.name: "Smaller font sample" }
            Text {
                text: root.sampleSize + " px"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                height: install.height
                verticalAlignment: Text.AlignVCenter
            }
            Flea.DialogButton { label: "+"; tabStop: true; available: root.sampleSize < 120; onActivated: root.changeSize(4); Accessible.name: "Larger font sample" }
            Flea.DialogButton { id: closeButton; label: root.compact ? "Expand" : "Close"; tabStop: true; onActivated: root.compact ? root.expandRequested() : root.closeRequested() }
        }
        Text {
            width: parent.width
            text: actions.error || actions.notice || (loadedFont.status === FontLoader.Error ? "Preview unavailable in Qt; Linux installation can still be checked."
                : actions.info ? (actions.info.installed ? (actions.info.desktopAvailable ? "Installed for your user" : "In your font directory; unavailable to desktop applications") : "Preview only · " + actions.info.format) : "Reading font…")
            wrapMode: Text.WordWrap
            color: actions.error || loadedFont.status === FontLoader.Error ? Theme.color.error : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }

    Flea.FontOptions {
        id: options
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: heading.bottom
        anchors.margins: Theme.spacing.gap
        faces: root.faces
        faceIndex: root.faceIndex
        axes: root.axes
        values: root.variationAxes
        onFaceSelected: function(index) { root.chooseFace(index) }
        onAxisChanged: function(tag, value) { root.setAxis(tag, value) }
        onResetRequested: root.variationAxes = ({})
    }
    Flickable {
        id: flick
        anchors.top: options.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: Theme.spacing.gap
        clip: true
        contentWidth: width
        contentHeight: samples.height
        boundsBehavior: Flickable.StopAtBounds
        visible: actions.info !== null && loadedFont.status === FontLoader.Ready
        Flea.FastScrollHandler { flickable: flick }
        Flea.ViewportScrollBars { parent: flick; flickable: flick }
        Column {
            id: samples
            width: flick.width - Theme.spacing.gap
            spacing: Theme.spacing.gap * 2
            Flea.FontDefaults {
                path: root.path
                face: root.face
                installed: actions.info !== null && actions.info.desktopAvailable && !actions.info.sourceRemoved
            }
            Text {
                width: parent.width
                text: "FONT GLYPHS · missing characters in other samples may use system fallback"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: root.face ? root.face.sample : ""
                color: Theme.color.foreground
                font.family: root.face ? root.face.family : loadedFont.name
                font.styleName: root.face ? root.face.style : ""
                font.variableAxes: root.variationAxes
                font.pixelSize: root.sampleSize
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
            }
            Repeater {
                model: ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz", "0123456789", "! @ # $ % ^ & * ( ) _ + − = { } [ ] : ; ‘ ’ “ ” < > ? / \\ | ~ € £ ¥ © ® ™", "ÁÉÍÓÚÜÑ áéíóúüñ ¿¡"]
                delegate: Text {
                    required property string modelData
                    width: samples.width
                    text: modelData
                    wrapMode: Text.WrapAnywhere
                    color: Theme.color.foreground
                    font.family: root.face ? root.face.family : loadedFont.name
                    font.styleName: root.face ? root.face.style : ""
                    font.variableAxes: root.variationAxes
                    font.pixelSize: root.sampleSize
                    textFormat: Text.PlainText
                }
            }
            Flow {
                width: parent.width
                spacing: Theme.spacing.gap
                Flea.DialogButton { label: "Lorem ipsum"; tabStop: true; onActivated: custom.text = root.lorem }
                Flea.DialogButton { label: "Pangram"; tabStop: true; onActivated: custom.text = "The quick brown fox jumps over the lazy dog. El veloz murciélago hindú comía feliz cardillo y kiwi." }
            }
            Text {
                text: "TYPE YOUR OWN TEXT"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
            }
            Rectangle {
                width: parent.width
                height: Math.max(root.sampleSize * 3, custom.implicitHeight + 2 * Theme.spacing.gap)
                color: Theme.color.background
                border.color: custom.activeFocus ? Theme.color.accent : Theme.color.mutedFrame
                border.width: Theme.spacing.hairline
                TextEdit {
                    id: custom
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.gap
                    text: root.lorem
                    wrapMode: TextEdit.Wrap
                    color: Theme.color.foreground
                    selectionColor: Theme.color.accent
                    selectedTextColor: Theme.color.background
                    font.family: root.face ? root.face.family : loadedFont.name
                    font.styleName: root.face ? root.face.style : ""
                    font.variableAxes: root.variationAxes
                    font.pixelSize: root.sampleSize
                    textFormat: TextEdit.PlainText
                    activeFocusOnTab: true
                    selectByMouse: true
                    Accessible.name: "Custom font sample"
                    Keys.onEscapePressed: { install.forceActiveFocus(); root.closeRequested() }
                }
            }
        }
    }
}
