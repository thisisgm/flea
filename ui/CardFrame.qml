import QtQuick
import "." as Flea

// A dialog card's inner layout: the title and the button row stay pinned, and only the body between them scrolls (ui/CardScroll.qml).
Item {
    id: root

    // The body is the default slot; the title and the button row stand outside the scroll.
    default property alias content: scroll.content
    property alias title: titleHolder.data
    property alias buttons: buttonHolder.data
    // The gap between the parts, the gap the single column kept between its children.
    property real spacing: Theme.card.gap
    // Room kept inside the clip, past each control's own frame, as CardScroll keeps it.
    property int bleed: 0
    property int bleedX: bleed
    property int bleedY: bleed
    property real revealClearY: 0

    // What the three parts want together, for the card to clamp against the window.
    readonly property real wanted: root.titleHeight + root.gapAfterTitle + scroll.wanted + root.gapBeforeButtons + root.buttonsHeight
    readonly property real holderWidth: root.width - 2 * root.bleedX
    property alias contentY: scroll.contentY
    readonly property alias contentHeight: scroll.contentHeight
    readonly property alias scrollItem: scroll
    readonly property real titleHeight: titleHolder.childrenRect.height
    readonly property real buttonsHeight: buttonHolder.childrenRect.height
    // A part with no height takes no gap, as an invisible child of the column did.
    readonly property real gapAfterTitle: root.titleHeight > 0 && (scroll.wanted > 0 || root.buttonsHeight > 0) ? root.spacing : 0
    readonly property real gapBeforeButtons: root.buttonsHeight > 0 && scroll.wanted > 0 ? root.spacing : 0
    // The body's room: what the card leaves between the pinned parts, never more than the body wants.
    readonly property real bodyHeight: Math.max(0, Math.min(scroll.wanted,
        root.height - 2 * root.bleedY - root.titleHeight - root.gapAfterTitle - root.gapBeforeButtons - root.buttonsHeight))

    function reveal(item) { scroll.reveal(item) }
    function holds(item) { return scroll.holds(item) }
    function resetSteps() { scroll.resetSteps() }

    Item {
        id: titleHolder
        objectName: "cardTitle"
        x: root.bleedX
        y: root.bleedY
        width: root.holderWidth
        height: root.titleHeight
    }

    Flea.CardScroll {
        id: scroll
        x: 0
        y: root.titleHeight + root.gapAfterTitle
        width: root.width
        height: root.bodyHeight + 2 * root.bleedY
        bleedX: root.bleedX
        bleedY: root.bleedY
        revealClearY: root.revealClearY
    }

    Item {
        id: buttonHolder
        objectName: "cardButtons"
        x: root.bleedX
        y: root.bleedY + root.titleHeight + root.gapAfterTitle + root.bodyHeight + root.gapBeforeButtons
        width: root.holderWidth
        height: root.buttonsHeight
    }
}
