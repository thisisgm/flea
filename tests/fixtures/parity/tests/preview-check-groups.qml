import QtQuick
Item {
    id: root
    property var failures: []
    property var files: []
    property bool ready: false
    function fail(why) { failures.push(why) }
    property QtObject assertion: QtObject {
        function canonical(value) { return JSON.stringify(value) }
        function check(condition, why) { if (!condition && !root.ready) root.fail(why) }
        function expect(name, actual, expected) {
            if (canonical(actual) === canonical(expected)) return
            root.fail(name)
        }
        function verify(name, actual, expected) {
            var ok = JSON.stringify(actual) === JSON.stringify(expected)
            console.log((ok ? "PASS " : "FAIL ") + name)
        }
    }
    property QtObject behavior: QtObject {
        function check() {
            try {
                if (root.files.length === 0) root.fail("no files to inspect")
            } catch (error) {
                root.fail("inspection threw " + error)
            }
        }
    }
    function compare(row) {
        if (row.count !== 2) root.fail("row count differs")
        if (!root.ready) root.fail("the scene is not ready")
    }
    function verify(condition) {
        if (!condition) root.fail("the supplied assertion failed")
        root.fail("independent unconditional failure outside the assertion path")
    }
    Component.onCompleted: {
        assertion.check(true, "primitive assertion call")
        assertion.expect("primitive comparison with early return", true, true)
        assertion.verify("primitive assertion with a local condition", true, true)
        behavior.check()
        compare({count: 2})
        verify(true)
    }
}
