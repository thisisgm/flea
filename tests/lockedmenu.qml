//@ pragma ShellId flea-lockedmenu-test

import QtQuick
import Quickshell
import "flea" as Flea

// tests/lockedmenu.sh's harness: the real ui/ContextMenu.qml over stub owners, proving openLocked refuses empty and closes a stale frame.
ShellRoot {
    id: root

    readonly property string want: "Open in terminal, Permissions and Copy as are hidden in Settings > Menus."
    readonly property string wantNotice: "Open in terminal and Copy as are hidden in Settings > Menus."
    // The shipped list shows Permissions, so the refusal cases hide all three rows themselves.
    readonly property var allHidden: ({ menu: ({ hidden: ["openTerminal", "permissions", "copyAs"] }) })
    property int checks: 0
    property var failures: []
    property var refused: []
    property var noticed: []

    function check(label, actual, expected) {
        root.checks += 1
        if (actual === expected) {
            console.log("LOCKEDMENU ok " + label)
            return
        }
        root.failures.push(label)
        console.log("LOCKEDMENU FAIL " + label + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function report() {
        if (root.failures.length === 0)
            console.log("LOCKEDMENU PASS " + root.checks + " checks")
        root.quit()
    }

    FloatingWindow {
        implicitWidth: 640
        implicitHeight: 480
        color: Flea.Theme.color.background

        TextInput {
            id: probe
            width: 200
            height: 30
            focus: true
        }
        Flea.ContextMenu {
            id: menu
            focusOwner: probe
        }
    }

    Connections {
        target: menu
        function onRefused(reason) { root.refused.push(reason) }
        function onNoticed(text) { root.noticed.push(text) }
    }

    Timer {
        interval: 800
        running: true
        repeat: false
        onTriggered: root.run()
    }

    // All steps run synchronously: openLocked and openForRail build entries and move focus at once.
    function run() {
        Flea.ViewState.state = root.allHidden
        probe.forceActiveFocus()
        check("probe holds keyboard at start", probe.activeFocus, true)
        menu.openLocked("/locked", 0o040000, Qt.point(60, 60))
        check("all hidden refuses once", root.refused.length, 1)
        check("refusal names Menus switch", root.refused.length > 0 ? root.refused[0] : "", root.want)
        check("fresh refusal opens nothing", menu.opened, false)
        check("fresh refusal leaves no target", menu.lockedPath, "")
        check("fresh refusal keeps rail clear", menu.railEntries.length, 0)
        check("fresh refusal shows no frame", menu.visible, false)
        check("fresh refusal takes no focus", menu.keyboardFocused, false)
        check("fresh refusal keeps probe focus", probe.activeFocus, true)
        check("fresh refusal adds no notice", root.noticed.length, 0)
        Flea.ViewState.state = ({ menu: ({ hidden: ["openTerminal", "permissions"] }) })
        menu.openLocked("/locked", 0o040000, Qt.point(60, 60))
        check("one row opens", menu.opened, true)
        check("one row targets folder", menu.lockedPath, "/locked")
        check("one row is locked menu", menu.forLocked, true)
        check("one row draws copypath alone", menu.entries.length, 1)
        check("one row action", menu.entries.length > 0 ? menu.entries[0].action : "", "copypath")
        check("success focuses catcher", menu.keyboardFocused, true)
        check("open adds no refusal", root.refused.length, 1)
        check("Copy path shown adds no notice", root.noticed.length, 0)
        menu.close()
        // The shipped list shows Permissions alone: the menu opens, and the bar names the two rows still hidden.
        Flea.ViewState.state = {}
        menu.openLocked("/locked", 0o040000, Qt.point(60, 60))
        check("the shipped list opens the tile", menu.opened, true)
        check("with Permissions alone", menu.entries.map(function (row) { return row.action }).join(","), "permissions")
        check("and refuses nothing", root.refused.length, 1)
        check("and names the two hidden rows once", JSON.stringify(root.noticed), JSON.stringify([root.wantNotice]))
        menu.close()
        // Q13: the Locked tile's Permissions row says why it is red in the hint slot the row already has.
        Flea.ViewState.state = ({ menu: ({ hidden: ["openTerminal"] }) })
        menu.openLocked("/locked", 0o040755, Qt.point(60, 60))
        var stranger = menu.entries.filter(function (row) { return row.action === "permissions" })[0] || {}
        check("a locked folder owned by somebody else is red", stranger.errored, true)
        check("and its entry says not yours", stranger.hint, "not yours")
        var strangerRow = menu.itemFor(menu.entries.indexOf(stranger))
        check("and the row draws that word in its hint slot", strangerRow ? strangerRow.hint : "", "not yours")
        check("and the word wants no more than a default menu is wide", strangerRow ? strangerRow.wantedWidth <= Flea.Theme.menuWidth : false, true)
        menu.close()
        menu.openLocked("/locked", 0o040000, Qt.point(60, 60))
        var owner = menu.entries.filter(function (row) { return row.action === "permissions" })[0] || {}
        check("the way back in stays plain, with no word", String(owner.errored) + "|" + String(owner.hint), "undefined|undefined")
        menu.close()
        Flea.ViewState.state = root.allHidden
        probe.forceActiveFocus()
        menu.openForRail("k1", [{ action: "open", label: "Open" }], Qt.point(10, 10))
        check("rail menu opens", menu.opened, true)
        menu.openLocked("/locked", 0o040000, Qt.point(60, 60))
        check("stale refusal refuses again", root.refused.length, 2)
        check("stale refusal names switch", root.refused.length > 1 ? root.refused[1] : "", root.want)
        check("stale refusal closes frame", menu.opened, false)
        check("stale refusal clears rail", menu.railEntries.length, 0)
        check("stale refusal clears target", menu.lockedPath, "")
        check("stale refusal shows no frame", menu.visible, false)
        check("stale refusal returns focus", probe.activeFocus, true)
        check("stale refusal drops catcher", menu.keyboardFocused, false)
        check("no other opening adds a notice", root.noticed.length, 1)
        root.report()
    }
}
