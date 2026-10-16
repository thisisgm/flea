//@ pragma ShellId flea-font-defaults-test
import QtQuick
import Quickshell
import "flea" as Flea

ShellRoot {
    id: root
    property int stage: 0
    property int ticks: 0
    property int checks: 0
    property int failed: 0
    function check(ok, message) { checks++; if (!ok) { failed++; console.log("FONT FAIL " + message) } }
    function find(item, name) {
        if (item.objectName === name) return item
        for (var i = 0; i < item.children.length; i++) { var result = find(item.children[i], name); if (result) return result }
        return null
    }
    function finish() {
        tick.stop()
        console.log("FONT DONE checks=" + checks + " failed=" + failed)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    FloatingWindow {
        implicitWidth: 800
        implicitHeight: 900
        visible: true
        Flea.FontDefaults {
            id: defaults
            path: Quickshell.env("FONT_DEFAULT_FIXTURE")
            face: ({index: 0, monospace: true})
            installed: true
        }
    }
    Timer {
        id: tick
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (++root.ticks > 350) { root.check(false, "defaults timed out stage " + root.stage + " " + defaults.message); root.finish(); return }
            if (root.stage === 0) {
                root.check(defaults.targets.length === 0, "defaults are lazy")
                root.find(defaults, "font-default-toggle").activated()
                root.stage++
            } else if (root.stage === 1 && !defaults.busy && defaults.targets.length) {
                root.check(defaults.targets.length === 7, "seven destinations loaded")
                root.check(!root.find(defaults, "font-restore-monospace").available, "restore disabled before apply")
                defaults.face = ({index: 0, monospace: false})
                root.check(!root.find(defaults, "font-apply-monospace").available, "proportional font disabled for monospace")
                root.check(root.find(defaults, "font-apply-desktop").available, "proportional font allowed on desktop")
                defaults.installed = false
                root.check(!root.find(defaults, "font-apply-desktop").available, "uninstalled font disabled")
                defaults.installed = true
                defaults.face = ({index: 0, monospace: true})
                root.find(defaults, "font-apply-monospace").activated()
                root.stage++
            } else if (root.stage === 2 && !defaults.busy) {
                root.check(root.find(defaults, "font-restore-monospace").available, "apply button enables restore")
                root.check(defaults.targets[2].family.length > 0, "applied family returned")
                root.find(defaults, "font-restore-monospace").activated()
                root.stage++
            } else if (root.stage === 3 && !defaults.busy) {
                root.check(!root.find(defaults, "font-restore-monospace").available, "restore button restores preference")
                root.check(defaults.message.indexOf("Preference saved") >= 0, "completion explains application reload")
                root.finish()
            }
        }
    }
}
