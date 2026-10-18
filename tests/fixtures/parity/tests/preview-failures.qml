import QtQuick
QtObject {
    property var failures: []
    function fail(why) { failures.push(why); console.log("FAIL " + why) }
    function run() {
        if (false) fail("conditional failure")
        failures.push("collected failure")
        console.log(false ? "PASS" : "FAIL conditional result")
    }
}
