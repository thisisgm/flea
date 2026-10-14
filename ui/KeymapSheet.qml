import QtQuick
import qs.Commons
import "." as Flea
import "js/Keymap.js" as Keymap
import "js/Menu.js" as Menu
import "js/Places.js" as Places
import "js/RailKeys.js" as RailKeys
import "js/SheetKeys.js" as SheetKeys
import "js/SheetQuery.js" as SheetQuery

// The keymap sheet ? opens, drawn as the Keys panel on Operations.dc.html draws it. Every row comes
// from keys.toml through Keymap.sheetFor, so a key that loses its binding cannot go on being advertised.
Item {
    id: root

    property bool opened: false
    property Item focusHolder: null
    readonly property var pane: root.focusHolder ? (root.focusHolder.sheetPane || root.focusHolder) : null
    // The query field appears on the first typed key only; an exact place name ranks first.
    property string query: ""
    // The history read once when the query line opens, never per keystroke, bounded by
    // Recent.LIMIT with no per-entry stat, listed whether or not the Recent rail row is on.
    property var recentPaths: []
    property bool recentAsked: false
    // The cursor sits on the first row, the menu lift, and arrows move it.
    property int resultCursor: 0
    onQueryChanged: root.resultCursor = 0
    // The whole table in every state, so the cap column keeps one width whatever the query narrows to.
    readonly property var sheet: root.actionRows()
    // The query results across actions, the cursor row's menu, places and recent files, bounded.
    readonly property var queryResults: {
        if (root.query.length === 0 || !root.focusHolder)
            return []
        var rows = root.sheet
        var actions = SheetQuery.actionCandidates(rows)
        var menus = SheetQuery.menuCandidates(root.menuModel(), function (a) { return Keymap.hintFor(a) })
        var places = SheetQuery.placeCandidates(root.railModel())
        var recents = SheetQuery.recentCandidates(root.recentPaths, root.pane.home)
        return SheetQuery.rank(actions.concat(menus, places, recents), root.query)
    }
    function actionRows() {
        var rows = Keymap.sheetFor(ViewState.keysPreset, "gui")
        return root.focusHolder && root.focusHolder.sheetActionRows ? root.focusHolder.sheetActionRows(rows) : rows
    }
    // The cursor row's menu rows, hidden ones included so the sheet finds a row Settings Menus hides; disabled ones never run.
    function menuModel() {
        var holder = root.focusHolder
        if (holder && holder.sheetMenuModel) return holder.sheetMenuModel()
        if (!holder || !holder.cursorRow) return []
        var context = holder.contextMenu().listingContext("", true)
        context.hasRow = true
        return Menu.listingEntries(context)
    }
    function railModel() {
        var bar = root.pane ? root.pane.sidebar : null
        return Flea.RailPlaces.entries().concat(bar ? bar.networkEntries.concat(bar.deviceEntries) : [])
    }
    function ensureRecent() {
        if (root.recentAsked)
            return
        root.recentAsked = true
        recentLoader.active = true
    }
    // Enter runs the highlighted row the way its own surface would.
    function activateResult() {
        var pick = root.queryResults[root.resultCursor]
        if (!pick)
            return
        var decided = SheetQuery.dispatch(pick)
        var holder = root.focusHolder
        if (!holder || decided.kind === "disabled" || decided.kind === "none")
            return
        if (decided.kind === "action") {
            SheetQuery.runAction(holder, decided.action, function () { root.close() })
            return
        }
        if (decided.kind === "menu" || decided.kind === "confirm") {
            SheetQuery.runMenu(holder, decided.menuAction, function () { root.close() })
            return
        }
        if (decided.kind === "place") {
            var entries = root.railModel()
            var at = SheetQuery.placeIndex(entries, decided)
            root.close()
            if (at < 0) return
            var pane = root.pane, entry = entries[at]
            if (pane.sidebar && entry.group !== "home" && entry.kind !== "favourite" && entry.kind !== "trash") {
                var railAt = SheetQuery.placeIndex(pane.sidebar.entries, decided)
                if (railAt >= 0) pane.sidebar.activate(railAt)
            } else Places.openEntry(entry, { pane: pane,
                opened: function (path) { RailKeys.openFrom(pane, path, pane.sidebar) },
                networkHost: function () { return pane.ensureNetworkService() },
                trash: function () { pane.trash.open() }, message: pane.message })
            return
        }
        if (decided.kind === "recent") {
            root.close()
            root.pane.openFile(decided.path)
            return
        }
    }
    // Directive 18's footprint: the four blocks flow into two columns of equal length rather than a
    // 2x2 grid, which paid twice for the taller block of each pair and grew the card to the screen.
    readonly property var columnSlots: {
        var flat = []
        for (var g = 0; g < root.groups.length; g++) {
            flat.push({ heading: root.groups[g].title, row: null })
            for (var i = 0; i < root.groups[g].rows.length; i++)
                flat.push({ heading: "", row: root.groups[g].rows[i] })
        }
        if (root.columns === 1)
            return [flat]
        var half = Math.ceil(flat.length / 2)
        // The fold prefers a group's own boundary, which costs at most one row of imbalance here.
        for (var b = half - 1; b <= half + 1; b++)
            if (b > 0 && b < flat.length && flat[b].heading.length > 0)
                return [flat.slice(0, b), flat.slice(b)]
        // It cannot always: a group longer than a column has to break inside itself, and then the
        // continuation names it again, so no row on this sheet ever sits under no heading at all.
        var owner = ""
        for (var k = half - 1; k >= 0; k--)
            if (flat[k].heading.length > 0) {
                owner = flat[k].heading
                break
            }
        var tail = flat.slice(half)
        if (owner.length > 0)
            tail = [{ heading: owner, row: null }].concat(tail)
        return [flat.slice(0, half), tail]
    }

    // KeymapSheet rule 1: a binding gets a group in keys.toml and the sheet reads it, in the order
    // that table lists them, with the heading its own key carries under a raised first letter.
    readonly property var groups: {
        var out = []
        for (var name in Keymap.SHEET_GROUPS) {
            var order = Keymap.SHEET_GROUPS[name], claimed = []
            // The group's own list is the order the board draws, cursor keys before the chords;
            // the generated sheet is in keys.toml's file order, which puts every preset row first.
            for (var i = 0; i < order.length; i++) {
                var row = root.sheet.filter(function (candidate) { return candidate.action === order[i] })
                if (row.length > 0)
                    claimed.push(row[0])
            }
            if (claimed.length > 0)
                out.push({ title: name.charAt(0).toUpperCase() + name.slice(1), rows: claimed })
        }
        return out
    }

    // The canvas drew this panel at 300, the convert popup's width, beside four illustrative rows.
    // The real sheet is sixty rows whose chords run to eighteen characters: at 300 a cap took the
    // whole half-cell, the wording beside it elided to a single ellipsis, and the two columns
    // overprinted each other. 480 is the Open with card's own anchor and leaves both room.
    readonly property int clampMargin: Theme.card.inset
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card
    // A cap is sized from the type scale, never from the text inside it, so every cap is one height.
    readonly property int capSize: Theme.markSize
    // The 0.2.1 sheet's own row, which directive 18 makes the ceiling: the cap with one row padding
    // under it. The board draws 32 around a 19 cap, but its pane holds 36 rows and this one holds 56.
    readonly property int rowPitch: root.capSize + Theme.spacing.rowPaddingY
    // A heading carries no cap box, so it takes the cap's own height rather than a whole row's.
    readonly property int headingPitch: root.capSize
    readonly property int capGap: Theme.spacing.gap

    // One cap column for the whole sheet, measured off the widest chord this preset spells. Sizing
    // each cap to its own text left every wording starting on a different x, and a wide chord took
    // the cell whole and drew across the column beside it.
    readonly property string widestCap: {
        var out = ""
        for (var i = 0; i < root.sheet.length; i++)
            if (root.sheet[i].keys.length > out.length) out = root.sheet[i].keys
        return out
    }
    readonly property int capWidth: Math.max(root.capSize, Math.ceil(capMetrics.width) + root.capGap)
    // Board rule 4: no label is ever cut, so the floor is the widest wording this preset really
    // draws. A window that cannot give two cells that much takes one column and scrolls instead.
    readonly property string widestLabel: {
        var out = ""
        for (var i = 0; i < root.sheet.length; i++)
            if (root.sheet[i].label.length > out.length) out = root.sheet[i].label
        return out
    }
    readonly property int cellFloor: root.capWidth + root.capGap + Math.ceil(labelFloor.width)
    // Two columns is what the canvas draws, and what keeps the whole map on one panel where it fits.
    readonly property int columns: root.contentWidth >= 2 * root.cellFloor + Theme.spacing.rowPaddingX ? 2 : 1

    // The body spans the card's inner width so a result's lift can run edge to edge; the text keeps its padded columns.
    readonly property real textInset: Theme.spacing.rowPaddingX - Theme.spacing.hairline
    readonly property real contentWidth: body.width - 2 * root.textInset

    TextMetrics {
        id: capMetrics
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: root.widestCap
    }

    TextMetrics {
        id: labelFloor
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: root.widestLabel
    }
    readonly property real groundOpacity: 0.5
    // The most of a result's label column its muted suffix may take, so a deep folder elides too and the name keeps the rest.
    readonly property real whereShare: 0.5

    anchors.fill: parent
    visible: root.opened || root.opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: root.opened
    opacity: 0
    z: 2
    Flea.Reveal { target: root; shown: root.opened; lift: card }

    function open(holder) {
        root.focusHolder = holder
        root.query = ""
        root.recentPaths = []
        root.recentAsked = false
        root.resultCursor = 0
        recentLoader.active = false
        // MenuAdditions040 callout 10: one two-byte read for the cursor row, so Make executable reads as the menu shows.
        if (!holder.sheetPane) holder.checkShebang()
        root.opened = true
        keys.forceActiveFocus()
    }
    Loader {
        id: recentLoader
        active: false
        source: "PickerRecent.qml"
        onLoaded: item.refresh()
    }
    Connections {
        target: recentLoader.item
        function onRefreshed() { root.recentPaths = recentLoader.item.paths }
    }

    function close() {
        if (!root.opened)
            return
        root.opened = false
        if (root.focusHolder) {
            if (root.focusHolder.sheetFocus) root.focusHolder.sheetFocus()
            else root.focusHolder.forceActiveFocus()
        }
    }

    // What a test reads instead of running OCR over the panel, the same idiom ui/Pane.qml's
    // menuEntries() uses: one row per line, the cap and the wording it is drawn beside.
    function rows() {
        var out = []
        // Under a query the sheet draws its results, so a test reads those instead of the resting grid.
        if (root.query.length > 0) {
            for (var r = 0; r < root.queryResults.length; r++)
                out.push(root.queryResults[r].keys + " " + root.queryResults[r].label
                    + (root.queryResults[r].disabled === true ? " (disabled)" : ""))
            return out.join("\n")
        }
        for (var g = 0; g < root.groups.length; g++) {
            out.push(root.groups[g].title)
            for (var i = 0; i < root.groups[g].rows.length; i++)
                out.push(root.groups[g].rows[i].keys + " " + root.groups[g].rows[i].label)
        }
        return out.join("\n")
    }

    // The query results as JSON, each row's cap, wording and the muted where its delegate draws, for a test that reads the screen.
    function resultState() {
        var out = []
        for (var r = 0; r < resultRepeater.count; r++) {
            var item = resultRepeater.itemAt(r)
            if (item)
                out.push({ keys: item.modelData.keys, label: item.modelData.label, where: item.whereShown, disabled: item.modelData.disabled === true })
        }
        return JSON.stringify(out)
    }

    // A dimmed ground, and a click on it closes, the same shape ui/ConvertDialog.qml uses.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.groundOpacity

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onClicked: root.close()
            onWheel: function (wheel) { wheel.accepted = true }
        }
    }

    // The resting card's body: the title, then the taller of the two columns of the key grid.
    readonly property real restBodyHeight: {
        var tall = 0
        for (var c = 0; c < root.columnSlots.length; c++) {
            var column = 0
            for (var r = 0; r < root.columnSlots[c].length; r++)
                column += root.columnSlots[c][r].heading.length > 0 ? root.headingPitch : root.rowPitch
            tall = Math.max(tall, column)
        }
        return title.implicitHeight + Theme.spacing.gap + tall
    }
    readonly property real restCardHeight: Math.min(root.restBodyHeight + 2 * Theme.spacing.rowPaddingX, root.height - 2 * root.clampMargin)

    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        // The rest card centres, and a query grows or shrinks it from its bottom, so the title never moves.
        y: Math.round((root.height - root.restCardHeight) / 2)
        width: Theme.cardSpan(Theme.card.wide, root.width - 2 * root.clampMargin)
        // Clamped to the window below its top; the body scrolls whatever the clamp cut, see ui/CardScroll.qml.
        height: Theme.cardSpan(body.wanted + 2 * Theme.spacing.rowPaddingX, root.height - card.y - root.clampMargin)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        // Mirrors hyprland decoration:rounding, same as ui/ConvertDialog.qml; 0 on a stock box stays square.
        radius: Style.cornerRadius

        Flea.CardFrame {
            id: body
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingX
            anchors.bottomMargin: Theme.spacing.rowPaddingX
            anchors.leftMargin: card.border.width
            anchors.rightMargin: card.border.width

            // Rule 2: the one clause a reader needs, in the corner every other surface puts it in.
            title: Item {
                x: root.textInset
                width: root.contentWidth
                height: title.implicitHeight

                Text {
                    id: title
                    anchors.left: parent.left
                    text: "Keys"
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    textFormat: Text.PlainText
                }

                Text {
                    anchors.right: parent.right
                    anchors.baseline: title.baseline
                    text: "esc closes"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
            }

            Column {
                x: root.textInset
                width: root.contentWidth
                spacing: Theme.spacing.gap

                // The query line, drawn only while one stands: a ? in the cap column, the query in a hairline accent field (decided 2026-10-03: no ring).
                Item {
                    id: queryRow
                    width: parent.width
                    height: queryRow.visible ? queryBox.height : 0
                    visible: root.query.length > 0

                    Text {
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: root.capWidth
                        horizontalAlignment: Text.AlignHCenter
                        text: "?"
                        color: Theme.color.muted
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.bodySmall
                        textFormat: Text.PlainText
                    }

                    Rectangle {
                        id: queryBox
                        anchors.left: parent.left
                        anchors.leftMargin: root.capWidth + root.capGap
                        anchors.right: parent.right
                        height: Theme.rowHeight - Theme.spacing.rowPaddingY
                        color: Theme.color.background
                        border.width: Theme.spacing.hairline
                        border.color: Theme.color.accent

                        Text {
                            id: queryLine
                            anchors.left: parent.left
                            anchors.leftMargin: Theme.spacing.gap
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, parent.width - 2 * Theme.spacing.gap - queryCaret.width)
                            text: root.query
                            color: Theme.color.foreground
                            font.family: Theme.font.family
                            font.pixelSize: Theme.font.body
                            textFormat: Text.PlainText
                            elide: Text.ElideLeft
                        }

                        // The field always holds the caret while it shows, and a TextInput's caret takes its text ink.
                        Rectangle {
                            id: queryCaret
                            anchors.left: queryLine.right
                            anchors.verticalCenter: parent.verticalCenter
                            width: Theme.spacing.hairline
                            height: Theme.font.body + Theme.spacing.hairline
                            color: Theme.color.foreground
                        }
                    }
                }

                // The query results replace the key grid, one row per match with the run washed.
                Column {
                    width: parent.width
                    visible: root.query.length > 0
                    Repeater {
                        id: resultRepeater
                        model: root.queryResults
                        delegate: Flea.KeymapSheetResult {
                            width: parent.width
                            resultCursor: root.resultCursor
                            query: root.query
                            capWidth: root.capWidth
                            capSize: root.capSize
                            capGap: root.capGap
                            rowPitch: root.rowPitch
                            textInset: root.textInset
                            whereShare: root.whereShare
                        }
                    }
                }
                Row {
                    visible: root.query.length === 0
                    spacing: Theme.spacing.rowPaddingX

                    Repeater {
                        model: root.columnSlots

                        // Equal halves, so the second column starts on one x the whole way down.
                        delegate: Column {
                            id: block
                            required property var modelData
                            width: (root.contentWidth - (root.columns - 1) * Theme.spacing.rowPaddingX) / root.columns

                            Repeater {
                                model: block.modelData

                                delegate: Item {
                                    id: slot
                                    required property var modelData
                                    width: block.width
                                    height: slot.modelData.heading.length > 0 ? root.headingPitch : root.rowPitch
                                    // Nothing this cell draws may reach the cell beside it, whatever it holds.
                                    clip: true

                                    // A heading takes a row's own slot, so both columns keep one rhythm.
                                    Text {
                                        anchors.left: parent.left
                                        anchors.bottom: parent.bottom
                                        anchors.bottomMargin: Theme.spacing.hairline * 2
                                        visible: slot.modelData.heading.length > 0
                                        text: slot.modelData.heading
                                        color: Theme.color.muted
                                        font.family: Theme.font.family
                                        font.pixelSize: Theme.font.caption
                                        font.capitalization: Font.AllUppercase
                                        font.letterSpacing: Theme.spacing.hairline
                                        textFormat: Text.PlainText
                                    }

                                    Rectangle {
                                        id: capBox
                                        anchors.left: parent.left
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: slot.modelData.row !== null
                                        // The sheet's own cap column, never this row's text: see root.capWidth.
                                        width: root.capWidth
                                        height: root.capSize
                                        color: "transparent"
                                        border.width: Theme.spacing.hairline
                                        border.color: Theme.color.mutedFrame

                                        Text {
                                            anchors.fill: parent
                                            anchors.margins: Theme.spacing.hairline
                                            horizontalAlignment: Text.AlignHCenter
                                            verticalAlignment: Text.AlignVCenter
                                            text: slot.modelData.row ? slot.modelData.row.keys : ""
                                            color: Theme.color.foreground
                                            font.family: Theme.font.family
                                            font.pixelSize: Theme.font.caption
                                            textFormat: Text.PlainText
                                            elide: Text.ElideRight
                                        }
                                    }

                                    Text {
                                        anchors.left: capBox.right
                                        anchors.leftMargin: root.capGap
                                        anchors.right: parent.right
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: slot.modelData.row !== null
                                        text: slot.modelData.row ? slot.modelData.row.label : ""
                                        // The wording is the sheet's own running text, so it takes the
                                        // foreground the board draws it in; muted is for the headings.
                                        color: Theme.color.foreground
                                        font.family: Theme.font.family
                                        font.pixelSize: Theme.font.caption
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true

        // Esc closes the sheet from any state; every key answers through SheetKeys.sheetKey.
        Keys.onPressed: function (event) {
            var decision = SheetKeys.sheetKey(root.query, event.key, event.text)
            if (decision === "up" || decision === "down") {
                root.resultCursor = SheetKeys.stepCursor(root.resultCursor, decision === "up" ? -1 : 1, root.queryResults.length)
            }
            else if (decision === "activate") { root.activateResult() }
            else if (decision === "backspace") { root.query = root.query.substring(0, root.query.length - 1) }
            else if (decision === "type") {
                if (root.query.length === 0)
                    root.ensureRecent()
                root.query += event.text
            }
            else if (decision === "ignore") { event.accepted = true; return }
            else { root.close() }
            event.accepted = true
        }
    }
}
