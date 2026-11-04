import QtQuick
Item {
    id: root
    property var steps: []
    property var forward: function(label) { root.seeView(label, true) }
    property var arrow: label => root.forward(label)
    property var block: (label) => { root.arrow(label) }
    function check(label, actual, expected) { if (actual !== expected) console.log("FAIL " + label) }
    function finish(ok, reason) { console.log("STARTUP " + (ok ? "PASS " : "FAIL ") + reason) }
    function step(run) { steps.push(run) }
    function see(label, read, expected) { root.step(function () { root.check(label, read(), expected) }) }
    function seeView(label, expected) { root.see(label, function () { return false }, expected) }
    function relay(label) { root.seeView(label, true) }
    function cycleA(label, again) { root.cycleB(label, again) }
    function cycleB(label, again) { if (again) root.cycleA(label, false); else root.relay(label) }
    function pureA() { root.pureB() }
    function pureB() { root.pureA() }
    function declarationOnly() { function unused() { root.check("unused declaration", true, true) } }
    function demand() {
        root.finish(false, "power reader is absent")
        root.finish(false, "power reader has the wrong owner")
    }
    function plan() {
        root.see("deferred view", function () { return false }, true)
        root.seeView("stored view", true)
        root.relay("transitive view")
        root.cycleA("recursive wrapper", true)
        root.forward("assigned wrapper")
        root.arrow("expression arrow wrapper")
        root.block("block arrow wrapper")
        root.pureA()
    }
    Component.onCompleted: { root.demand(); root.plan(); root.finish(true, "completed") }
}
