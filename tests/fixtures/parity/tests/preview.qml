import QtQuick
QtObject {
    function check(label, value) {}
    function run() {
        check("first case", true)
        root.check("second case", /^file:\/\//.test("check('not a case')"))
        // check("comment", false)
        /* check("block comment", false) */
    }
    function test_named() { check("assertion inside one test", true) }
    function test_named_data() { return [] }
}
