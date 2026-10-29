pragma ComponentBehavior: Bound

import QtQuick
import "." as Flea
import "js/CodeRuns.js" as CodeRuns
import "js/Markdown.js" as Markdown
import "js/MdHtmlImage.js" as HtmlImage

// One Markdown block, built as the one kind it is, at the top of the document or inside an item or a quote.
Item {
    id: view

    property var block: ({})
    property int blockIndex: -1
    // How many blocks the list holding this one has, so the last one knows nothing follows it.
    property int blockCount: 0
    // The preview that sets the document's sizes and colours.
    property Item preview: null
    // Only a figure intersecting the viewport may send a render request.
    property bool inView: true

    // The drawn run text this view built, else null for any other kind or a maths run.
    readonly property Item runText: view.block.type === "run" && view.block.maths === undefined ? (kind.item as Item) : null
    // A picture followed by a picture block sits at the list spacing; a picture followed by text keeps the block gap, so what follows never touches it.
    readonly property bool nextIsPicture: view.nextBlock !== null && view.nextBlock !== undefined && (view.nextBlock.type === "image" || view.nextBlock.type === "images" || view.nextBlock.type === "remote" || view.nextBlock.type === "figure")
    readonly property int pictureGap: (view.block.type === "images" || view.block.type === "image") && view.blockIndex < view.blockCount - 1 && !view.nextIsPicture ? view.preview.blockGap : 0
    // The blocks beside this one in the list that holds it: the document's list at the top, the item's or quote's parts inside one.
    property var siblings: null
    // Two consecutive figures share one canvas margin each, so the upper lends both back and the list's spacing reads exact.
    readonly property var neighbourList: view.siblings !== null ? view.siblings : (view.preview !== null ? view.preview.blockList : null)
    readonly property var nextBlock: neighbourList === null || neighbourList === undefined || view.blockIndex + 1 >= neighbourList.length
        ? null : neighbourList[view.blockIndex + 1]
    // One canvas margin per mermaid side, FigureWorker.mjs CANVAS_MARGIN.
    readonly property int canvasMargin: 1
    readonly property int pairTrim: view.block.type === "figure" && nextBlock !== null && nextBlock !== undefined && nextBlock.type === "figure"
        ? (view.block.kind === "mermaid" ? view.canvasMargin : 0) + (nextBlock.kind === "mermaid" ? view.canvasMargin : 0) : 0
    // Only the drawn block lends its height, and a list or table chunk lies flush by its own negative y.
    height: kind.item ? kind.item.height + kind.item.y + view.pictureGap - view.pairTrim : 0

    // A block builds only the parts its own kind draws, on the view; an empty heading or quote builds none, so it has no height.
    Loader {
        id: kind
        sourceComponent: view.block.text === "" && view.block.parts === undefined && (view.block.type === "heading" || view.block.type === "quote") ? null : view.block.type === "run" && view.block.maths !== undefined ? mathsBlock
            : view.block.type === "run" || view.block.type === "heading" ? textBlock
            : view.block.type === "fence" ? fenceBlock
            : view.block.type === "figure" ? figureBlock
            : view.block.type === "quote" ? quoteBlock
            : view.block.type === "remote" ? remoteBlock
            : view.block.type === "list" ? listBlock
            : view.block.type === "table" ? tableBlock
            : view.block.type === "images" ? imagesBlock : imageBlock
        onLoaded: kind.item.parent = view
    }

    // Headings use the prescribed bold text size and line box.
    Component {
        id: textBlock
        Flea.MarkdownText {
            linkGate: Markdown.isExternalLink
            width: view.width
            bodyPx: view.preview.bodyPx
            markdown: view.block.text
            font.pixelSize: view.block.type === "heading" ? view.preview.headingPx(view.block.level) : view.preview.bodyPx
            font.bold: view.block.type === "heading"
            // h1 and h2 take the bright foreground; deeper levels and body stay the foreground.
            color: view.block.type === "heading" && view.block.level <= view.preview.boardHeadings.length ? Theme.color.foregroundBright : Theme.color.foreground
            // An HTML heading keeps its align attribute; Markdown headings stay left.
            horizontalAlignment: view.block.align === "center" ? Text.AlignHCenter : view.block.align === "right" ? Text.AlignRight : Text.AlignLeft
        }
    }

    // A run with inline formulas draws each in its line once the helper answers; the other runs build none of its parts.
    Component {
        id: mathsBlock
        Flea.MarkdownMathsText {
            objectName: "mathsText"
            linkGate: Markdown.isExternalLink
            width: view.width
            bodyPx: view.preview.bodyPx
            source: view.block.text
            maths: view.block.maths
            askArmed: view.preview.figuresArmed
            inView: view.inView
            bgHex: view.preview.hexOf(Theme.color.background)
            fgHex: view.preview.inkHex
            accentHex: view.preview.accentHex
            mutedHex: view.preview.mutedHex
            surfaceHex: view.preview.surfaceHex
            fencePadX: view.preview.fencePadX
            fencePadY: view.preview.fencePadY
            font.pixelSize: view.preview.bodyPx
        }
    }

    Component {
        id: tableBlock
        Flea.MarkdownTable {
            block: view.block
            preview: view.preview
            availableWidth: view.width
        }
    }

    // A named fence takes engine colours after its reply; the escaped foreground text before it preserves its height.
    Component {
        id: fenceBlock
        Rectangle {
            id: fenceBox
            objectName: "fenceBox"
            width: view.width
            height: fenceText.implicitHeight + 2 * view.preview.fencePadY
            color: view.preview.codeSurface
            readonly property string info: String(view.block.info || "").trim()
            readonly property var backend: view.preview.backend
            readonly property bool coloured: fenceBox.info !== "" && view.preview.shownView !== Markdown.SOURCE
                && fenceBox.backend !== null && typeof fenceBox.backend.askCode === "function"
            // The one slot this fence asks on, handed out by the preview so no two fences share one.
            property string slot: ""
            property int token: 0
            // The first 1-based line not yet answered, and the runs of each answered line.
            property int next: 1
            property var held: ({})
            property int stamp: 0
            // The engine refused this fence (binary, large or failed), so it stays in the foreground.
            property bool refused: false
            readonly property var fenceLines: view.block.text.split("\n")
            readonly property int lineCap: 400
            readonly property bool awaiting: fenceBox.coloured && !fenceBox.refused && fenceBox.next <= fenceBox.fenceLines.length
            readonly property bool holdsScreen: fenceBox.awaiting && view.inView
            readonly property string markup: {
                if (fenceBox.stamp < 0)
                    return ""
                var out = []
                for (var i = 0; i < fenceBox.fenceLines.length; i++) {
                    var runs = fenceBox.held[i + 1]
                    out.push(CodeRuns.styled(runs !== undefined ? runs : [0, fenceBox.fenceLines[i]], view.preview.inks))
                }
                return out.join("<br>")
            }

            function ask() {
                if (fenceBox.slot === "" || !fenceBox.awaiting || fenceBox.token !== 0)
                    return
                var count = Math.min(fenceBox.lineCap, fenceBox.fenceLines.length - fenceBox.next + 1)
                fenceBox.token = fenceBox.backend.askCode(fenceBox.slot, { text: view.block.text, name: "", lang: fenceBox.info }, fenceBox.next, count)
            }
            function rebind() {
                fenceBox.token = 0
                fenceBox.held = {}
                fenceBox.stamp += 1
                fenceBox.next = 1
                fenceBox.refused = false
                Qt.callLater(fenceBox.ask)
            }
            onBackendChanged: fenceBox.rebind()
            function answered(message) {
                if (fenceBox.token === 0 || message.token !== fenceBox.token)
                    return
                fenceBox.token = 0
                var rows = message.lines || []
                if ((message.state !== "styled" && message.state !== "plain") || rows.length === 0) {
                    fenceBox.refused = true
                    return
                }
                for (var i = 0; i < rows.length; i++)
                    fenceBox.held[fenceBox.next + i] = rows[i]
                fenceBox.next += rows.length
                fenceBox.stamp += 1
                fenceBox.ask()
            }

            // Only a viewport fence holds the first screen; tearing its block down releases that hold too.
            property bool waiting: false
            function setWaiting(on) {
                if (on === fenceBox.waiting)
                    return
                fenceBox.waiting = on
                view.preview.fenceWaiting(on ? 1 : -1)
            }
            onHoldsScreenChanged: fenceBox.setWaiting(fenceBox.holdsScreen)
            onAwaitingChanged: fenceBox.ask()
            Component.onCompleted: {
                fenceBox.slot = view.preview.fenceSlot(fenceBox)
                fenceBox.setWaiting(fenceBox.holdsScreen)
                if (fenceBox.awaiting)
                    fenceBox.ask()
            }
            Component.onDestruction: {
                if (fenceBox.waiting) view.preview.fenceWaiting(-1)
                delete view.preview.fenceReplies[fenceBox.slot]
            }

            Text {
                id: fenceText
                property var selectionEditor: null
                property var linkGate: null
                anchors.fill: parent
                anchors.leftMargin: view.preview.fencePadX
                anchors.rightMargin: view.preview.fencePadX
                anchors.topMargin: view.preview.fencePadY
                anchors.bottomMargin: view.preview.fencePadY
                text: fenceBox.coloured ? fenceBox.markup : view.block.text
                textFormat: fenceBox.coloured ? Text.StyledText : Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: view.preview.bodyPx
            }
        }
    }

    // Display maths use the figure fallback's vertical inset; the list owns the block gap.
    Component {
        id: figureBlock
        Item {
            id: figureBox
            objectName: "figureBox"
            width: view.width
            readonly property int figureInset: figureItem.ready && figureItem.kind === "math" && figureItem.fitHeight > 0 ? Theme.spacing.gap : 0
            height: figureItem.implicitHeight + 2 * figureInset

            Flea.MarkdownFigure {
                id: figureItem
                objectName: "figureItem"
                y: parent.figureInset
                width: parent.width
                kind: view.block.kind
                source: view.block.source
                display: true
                askArmed: view.preview.figuresArmed
                inView: view.inView
                bgHex: view.preview.hexOf(Theme.color.background)
                fgHex: view.preview.inkHex
                accentHex: view.preview.accentHex
                mutedHex: view.preview.mutedHex
                surfaceHex: view.preview.surfaceHex
                fallbackColor: view.preview.codeSurface
                fencePadX: view.preview.fencePadX
                fencePadY: view.preview.fencePadY
                fontFamily: Theme.font.family
                bodyPx: view.preview.bodyPx
            }
        }
    }

    // A quote draws one bar per level, and a joined block sits flush under the one above so the outer bars run on.
    Component {
        id: quoteBlock
        Flea.MarkdownQuote {
            objectName: "quoteRow"
            y: view.block.joined === true ? -view.preview.blockGap : 0
            width: view.width
            levels: view.block.depth !== undefined ? view.block.depth : 1
            linkGate: Markdown.isExternalLink
            bodyPx: view.preview.bodyPx
            text: view.block.text
            parts: view.block.parts !== undefined ? view.block.parts : []
            preview: view.preview
            inView: view.inView
        }
    }

    // A chunk after the first sits flush under its predecessor, across the gap the list puts between blocks.
    Component {
        id: listBlock
        Flea.MarkdownList {
            objectName: "listColumn"
            y: view.block.joined === true ? -view.preview.blockGap : 0
            width: view.width
            list: view.block
            gap: view.preview.blockGap
            linkGate: Markdown.isExternalLink
            bodyPx: view.preview.bodyPx
            preview: view.preview
            inView: view.inView
        }
    }

    Component {
        id: remoteBlock
        Flea.MarkdownRemote {
            width: view.width
            host: view.block.host
        }
    }

    // A badge row builds only its own pictures, wrapped and aligned by the row itself.
    Component {
        id: imagesBlock
        Flea.MarkdownImages {
            width: view.width
            images: view.block.type === "images" ? view.block.items : []
            centred: view.block.type === "images" && view.block.align === "center"
            gap: view.preview.blockGap
            linkGate: Markdown.isExternalLink
        }
    }

    Component {
        id: imageBlock
        Image {
            id: localImage
            // The size rule of the badge row (HtmlImage.pictureSize); its paragraph's align="center" centres it.
            readonly property var fit: HtmlImage.pictureSize(view.block.type === "image" ? view.block : ({}), localImage.implicitWidth, localImage.implicitHeight, view.width)
            width: localImage.fit.w
            height: localImage.fit.h
            x: view.block.type === "image" && view.block.align === "center" ? Math.round((view.width - localImage.width) / 2) : 0
            fillMode: localImage.fit.stretch ? Image.Stretch : Image.PreserveAspectFit
            visible: view.block.type === "image"
            asynchronous: true
            autoTransform: true
            source: view.block.type === "image" ? view.block.url : ""

            // A logo wrapped in a link opens it through the pane's link gate.
            TapHandler { enabled: view.block.link !== undefined; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: if (Markdown.isExternalLink(view.block.link)) Qt.openUrlExternally(view.block.link) }
            HoverHandler { enabled: view.block.link !== undefined; cursorShape: Qt.PointingHandCursor }
        }
    }
}
