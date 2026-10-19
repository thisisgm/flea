import QtQuick
QtObject {
    id: root
    property var failures: []
    property var steps: [verify]
    function check(ok, why) { if (!ok) failures.push(why) }
    function expect(ok, why) { if (!ok) console.log("FAIL " + why) }
    function verify() {
        check(true, "verify keeps its first assertion")
        check(false, "verify keeps its failing assertion")
    }
    function compare(actual, expected) {
        check(actual === expected, "compare keeps its assertion")
        expect(actual > 0, "compare keeps its expectation")
    }
    Component.onCompleted: compare(1, 1)
}
