import QtQuick

// Only selection ink is drawn: the host continues drawing the Markdown itself.
TextEdit {
    id: root

    required property var host
    property var controller: null
    anchors.fill: parent
    readOnly: true
    selectByMouse: false
    selectByKeyboard: true
    persistentSelection: true
    color: "transparent"
    selectionColor: Theme.color.accent
    selectedTextColor: Theme.color.background
    font: root.host.font
    wrapMode: root.host.wrapMode === Text.NoWrap ? TextEdit.NoWrap : TextEdit.Wrap
    horizontalAlignment: root.host.horizontalAlignment
    leftPadding: root.host.leftPadding
    rightPadding: root.host.rightPadding
    topPadding: root.host.topPadding + (root.host.textFormat !== Text.PlainText && root.host.restBaseline !== undefined ? root.host.restBaseline - root.host.naturalBaseline : 0)
    bottomPadding: root.host.bottomPadding
    textFormat: TextEdit.RichText

    function sync() {
        // Qt converts Markdown to its own sanitized document; plain hosts stay plain.
        root.textFormat = root.host.textFormat === Text.PlainText ? TextEdit.PlainText : root.host.textFormat === Text.StyledText ? TextEdit.RichText : TextEdit.MarkdownText
        root.text = root.host.text
        if (root.textFormat !== TextEdit.PlainText) {
            root.textFormat = TextEdit.RichText
            // Fragment markers exclude the first paragraph style when Qt imports it again.
            var html = root.getFormattedText(0, root.length).replace(/<!--(?:Start|End)Fragment-->/g, "")
            // Remove ink, retaining faces, sizes, links, images and word boundaries.
            html = html.replace(/style="([^"]*)"/g, function(match, style) {
                return 'style="' + style.replace(/(?:background-)?color\s*:[^;]+;?/gi, "").replace(/margin-top:[^;]+;/g, "margin-top:0px;").replace(/margin-bottom:[^;]+;/g, "margin-bottom:0px;") + '"'
            })
            var height = root.host.lineHeightMode === Text.FixedHeight ? root.host.lineHeight + "px" : root.host.lineHeight * 100 + "%"
            html = html.replace(/<p style="/g, '<p style="color:transparent;line-height:' + height + ';-qt-line-height-type:' + (root.host.lineHeightMode === Text.FixedHeight ? 'fixed' : 'proportional') + ';')
            root.text = html
        }
    }
    Component.onCompleted: root.sync()
    Connections {
        target: root.host
        function onTextChanged() { Qt.callLater(root.sync) }
        function onFontChanged() { Qt.callLater(root.sync) }
        function onTextFormatChanged() { Qt.callLater(root.sync) }
        function onLineHeightChanged() { Qt.callLater(root.sync) }
        function onLineHeightModeChanged() { Qt.callLater(root.sync) }
    }
    Keys.onShortcutOverride: function(event) {
        if (event.key === Qt.Key_C && (event.modifiers & Qt.ControlModifier)) event.accepted = true
    }
    Keys.priority: Keys.BeforeItem
    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_C && (event.modifiers & Qt.ControlModifier) && root.controller !== null) {
            root.controller.copy()
            event.accepted = true
        } else {
            if (root.controller !== null && ((event.modifiers & Qt.ShiftModifier) || (event.key === Qt.Key_A && (event.modifiers & Qt.ControlModifier))))
                root.controller.keyboardSelection(root.host)
            event.accepted = false
        }
    }
}
