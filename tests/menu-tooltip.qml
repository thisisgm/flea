//@ pragma ShellId flea-menu-tooltip-test

import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

ShellRoot {
    id: shell
    readonly property string longName: "A long menu label with <literal markup> & a distinguishing ending"
    property int step: 0
    property int checks: 0
    property int failures: 0
    property var tip: null
    property var tipText: null
    property var tipScroll: null
    property string chosen: ""

    function check(label, pass) {
        shell.checks++
        if (!pass) shell.failures++
        console.log("MENU_TOOLTIP " + (pass ? "ok " : "FAIL ") + label)
    }
    function finish() {
        clock.stop()
        console.log("MENU_TOOLTIP DONE " + shell.checks + " checks, " + shell.failures + " failed")
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    function hover(row) { events.mouseMove(row, row.width / 2, row.height / 2, -1, Qt.NoButton, Qt.NoModifier) }

    TestResult { id: result }

    FloatingWindow {
        implicitWidth: 640
        implicitHeight: 480
        color: "#303030"
        Flea.ContextMenu {
            id: menu
            onChosen: action => shell.chosen = action
        }
        TestEvent { id: events }
    }

    Timer {
        id: clock
        interval: 600
        running: true
        repeat: true
        onTriggered: {
            switch (shell.step++) {
            case 0:
                menu.openAt(Qt.point(620, 450))
                menu.entries = [
                    {label: shell.longName, action: "open"},
                    {label: "Run script", action: "runScript", submenu: [
                        {id: "keep", label: shell.longName}, {id: "short", label: "Short"}
                    ]}
                ]
                break
            case 1:
                shell.hover(menu.itemFor(0))
                break
            case 2:
                shell.check("real hover reaches the long menu row", menu.itemFor(0).hovered)
                shell.tip = menu.children.find(item => item.objectName === "menuTooltip") || null
                shell.check("hover reveals the clipped label", shell.tip !== null && shell.tip.visible)
                if (!shell.tip) { shell.finish(); return }
                shell.tipText = result.findChild(shell.tip, "tooltipText")
                shell.check("full label is plain text", shell.tipText.text === shell.longName
                            && shell.tipText.textFormat === Text.PlainText)
                shell.check("tooltip stays inside the work area", shell.tip.x >= 0 && shell.tip.y >= 0
                            && shell.tip.x + shell.tip.width <= menu.width
                            && shell.tip.y + shell.tip.height <= menu.height)
                shell.check("hover leaves keyboard focus in the menu", menu.keyboardFocused)
                shell.hover(menu.itemFor(1))
                break
            case 3:
                shell.check("short parent hides the tooltip", !shell.tip.visible)
                shell.check("hover opens the script submenu", menu.submenuOpen)
                shell.hover(menu.submenuItemFor(0))
                break
            case 4:
                shell.check("script hover reveals its full name", shell.tip.visible && shell.tipText.text === shell.longName)
                events.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
                break
            case 5:
                shell.check("closing the submenu hides its tooltip", !menu.submenuOpen && !shell.tip.visible)
                shell.hover(menu.itemFor(0))
                break
            case 6:
                shell.check("hovering again reveals the name", shell.tip.visible)
                var row = menu.itemFor(0)
                events.mouseClick(row, row.width / 2, row.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                break
            case 7:
                shell.check("tooltip leaves the row clickable", shell.chosen === "open" && !menu.opened)
                shell.check("closing the menu hides the tooltip", !shell.tip.visible)
                menu.workArea = Qt.rect(30, 40, 180, 120)
                menu.openAt(Qt.point(30, 40))
                menu.entries = [{label: "Long menu entry ".repeat(15) + "END", action: "open"}]
                break
            case 8:
                shell.hover(menu.itemFor(0))
                break
            case 9:
                shell.check("oversized tooltip stays inside the work area", shell.tip.visible
                            && shell.tip.x >= menu.workArea.x && shell.tip.y >= menu.workArea.y
                            && shell.tip.x + shell.tip.width <= menu.workArea.x + menu.workArea.width
                            && shell.tip.y + shell.tip.height <= menu.workArea.y + menu.workArea.height)
                shell.tipScroll = shell.tip.children.find(item => item.objectName === "tooltipScroll") || null
                shell.check("oversized content has a clipped scroll viewport", shell.tipScroll !== null
                            && shell.tipScroll.clip && shell.tipScroll.contentHeight > shell.tipScroll.height)
                if (!shell.tipScroll) { shell.finish(); return }
                shell.hover(shell.tip)
                break
            case 10:
                shell.check("moving onto overflowing text keeps it visible", shell.tip.visible)
                events.mouseWheel(shell.tipScroll, shell.tipScroll.width / 2, shell.tipScroll.height / 2,
                                  Qt.NoButton, Qt.NoModifier, 0, -12000, -1)
                break
            case 11:
                shell.check("wheel reveals the end of the label", shell.tip.visible && shell.tipScroll.contentY > 0
                            && shell.tipScroll.atYEnd)
                shell.check("scrolling leaves keyboard focus in the menu", menu.keyboardFocused)
                events.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
                break
            case 12:
                shell.check("Escape closes a tooltip being hovered", !menu.opened && !shell.tip.visible)
                shell.finish()
            }
        }
    }
}
