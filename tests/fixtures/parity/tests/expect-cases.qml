import QtQuick
QtObject {
    function expect(ok, why) { if (!ok) console.log("FAIL " + why) }
    function run() {
        expect(true, "first expectation")
        root.expect(false, "unconditional failure remains inventoried")
        // expect(false, "commented expectation")
    }
}
