//@ pragma ShellId flea-uiwriter-navigation-test
import QtQuick
import Quickshell
import Quickshell.Io
import "flea" as Flea
import "flea/js/Tabs.js" as Tabs

// Real path and tab signals reach WindowBody; check patch spawns and saved navigation state.
ShellRoot {
    id: shell
    property int phase: 0
    property int before: 0
    property int failures: 0
    property int checks: 0
    property string destination: Quickshell.env("PROBE_NAV_TO")
    function check(name, actual, expected) {
        checks += 1
        if (actual !== expected) {
            failures += 1
            console.log("NAVWRITE FAIL " + name + " got=" + actual + " expected=" + expected)
        } else console.log("NAVWRITE ok " + name)
    }
    FileView {
        id: countFile
        path: Quickshell.env("PROBE_WRAP_DIR") + "/count"
        printErrors: false
        blockLoading: true
    }
    function count() { countFile.reload(); countFile.waitForJob(); return Number(countFile.text()) || 0 }
    FileView {
        id: savedFile
        path: Quickshell.env("XDG_STATE_HOME") + "/flea/ui.json"
        printErrors: false
        blockLoading: true
    }
    function checkSaved(name) {
        savedFile.reload()
        savedFile.waitForJob()
        // Sample saved ui.json: {"lastPath":"/to","lastTabs":{"paths":["/to"],"index":0}}.
        var saved = JSON.parse(savedFile.text())
        check(name + " saved lastPath", saved.lastPath, Flea.ViewState.state.lastPath)
        check(name + " saved lastTabs", JSON.stringify(saved.lastTabs), JSON.stringify(Flea.ViewState.state.lastTabs))
    }
    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 700
        Flea.WindowBody { id: body; host: window }
    }
    function ready() {
        return body.initialized && !body.currentPane.listInFlight && body.currentPane.path.length > 0
            && !body.tabStripQueued && Flea.ViewState.writeBook.inflight.length === 0
            && Flea.ViewState.saveStatus.indexOf("Saving") !== 0
    }
    function advance() {
        if (!ready()) return
        if (phase === 0) {
            before = count()
            body.currentPane.open(destination)
            phase = 1
        } else if (phase === 1 && body.currentPane.path === destination) {
            check("folder change patch spawns", count() - before, 1)
            check("lastPath preserved", Flea.ViewState.state.lastPath, destination)
            check("lastTabs preserved", JSON.stringify(Flea.ViewState.state.lastTabs), JSON.stringify({paths:[destination], index:0}))
            checkSaved("folder change")
            before = count()
            body.rememberPaths()
            body.queueTabStrip()
            body.queueTabStrip()
            phase = 2
        } else if (phase === 2) {
            check("unchanged path and tabs spawn nothing", count() - before, 0)
            body.currentPane.tabs = Tabs.pack(Tabs.restoreItems(body.currentPane, [destination, Quickshell.env("FLEA_PATH")]), 0)
            body.queueTabStrip()
            phase = 3
        } else if (phase === 3) {
            check("tab-only change patch spawns", count() - before, 1)
            check("tab order preserved", JSON.stringify(Flea.ViewState.state.lastTabs.paths), JSON.stringify([destination, Quickshell.env("FLEA_PATH")]))
            check("tab-only change keeps lastPath", Flea.ViewState.state.lastPath, destination)
            checkSaved("tab-only change")
            before = count()
            Tabs.selectAt(body.currentPane, 1)
            phase = 4
        } else if (phase === 4 && body.currentPane.path === Quickshell.env("FLEA_PATH")) {
            check("tab switch patch spawns", count() - before, 1)
            check("tab switch lastPath", Flea.ViewState.state.lastPath, Quickshell.env("FLEA_PATH"))
            check("tab switch index", Flea.ViewState.state.lastTabs.index, 1)
            checkSaved("tab switch")
            before = count()
            Tabs.openNew(body.currentPane, destination)
            phase = 5
        } else if (phase === 5 && body.currentPane.path === destination) {
            check("new tab folder patch spawns", count() - before, 1)
            check("new tab lastPath", Flea.ViewState.state.lastPath, destination)
            check("new tab strip", JSON.stringify(Flea.ViewState.state.lastTabs), JSON.stringify({paths:[destination, Quickshell.env("FLEA_PATH"), destination], index:2}))
            checkSaved("new tab")
            console.log("NAVWRITE DONE " + checks + " checks, " + failures + " failed")
            Qt.quit()
        }
    }
    Timer { interval: 100; running: true; repeat: true; onTriggered: shell.advance() }
    // Five writers run one after another here, so the backstop sits inside the suite's 60 s kill and names the step a loaded box held up.
    Timer { interval: 50000; running: true; onTriggered: { console.log("NAVWRITE FAIL stalled phase=" + shell.phase); Qt.quit() } }
}
