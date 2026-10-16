pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "." as Flea

// Collections install as one file; this selector changes only the face shown in the specimen.
Column {
    id: root
    property var faces: []
    property int faceIndex: 0
    property var axes: []
    property var values: ({})
    signal faceSelected(int index)
    signal axisChanged(string tag, real value)
    signal resetRequested()
    spacing: Theme.spacing.gap
    Flow {
        width: parent.width
        spacing: Theme.spacing.gap
        visible: root.faces.length > 1
        Flea.DialogButton { label: "‹"; tabStop: true; available: root.faceIndex > 0; Accessible.name: "Previous font face"; onActivated: root.faceSelected(root.faceIndex - 1) }
        Text {
            text: (root.faceIndex + 1) + " / " + root.faces.length + " faces · installs the whole collection"
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            color: Theme.color.muted
            height: Theme.rowHeight
            verticalAlignment: Text.AlignVCenter
        }
        Flea.DialogButton { label: "›"; tabStop: true; available: root.faceIndex + 1 < root.faces.length; Accessible.name: "Next font face"; onActivated: root.faceSelected(root.faceIndex + 1) }
    }
    Flow {
        width: parent.width
        spacing: Theme.spacing.gap
        Repeater {
            model: root.axes
            delegate: Row {
                id: axisRow
                required property var modelData
                readonly property real value: root.values[axisRow.modelData.tag] === undefined ? axisRow.modelData.default : root.values[axisRow.modelData.tag]
                spacing: Theme.spacing.hairline * 4
                Flea.DialogButton {
                    label: "−"; tabStop: true
                    available: parent.value > axisRow.modelData.minimum
                    Accessible.name: "Decrease " + axisRow.modelData.tag
                    onActivated: root.axisChanged(axisRow.modelData.tag, Math.max(axisRow.modelData.minimum, parent.value - (axisRow.modelData.maximum - axisRow.modelData.minimum) / 20))
                }
                Text {
                    text: axisRow.modelData.tag + " " + Math.round(parent.value * 100) / 100
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    height: Theme.rowHeight
                    verticalAlignment: Text.AlignVCenter
                }
                Flea.DialogButton {
                    label: "+"; tabStop: true
                    available: parent.value < axisRow.modelData.maximum
                    Accessible.name: "Increase " + axisRow.modelData.tag
                    onActivated: root.axisChanged(axisRow.modelData.tag, Math.min(axisRow.modelData.maximum, parent.value + (axisRow.modelData.maximum - axisRow.modelData.minimum) / 20))
                }
            }
        }
        Flea.DialogButton { label: "Reset axes"; visible: root.axes.length > 0; tabStop: visible; onActivated: root.resetRequested() }
    }
}
