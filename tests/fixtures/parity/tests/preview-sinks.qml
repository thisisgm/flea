import QtQuick
Item {
    function thrown() { throw new Error("independent thrown assertion") }
    function relay() { thrown() }
    function postfixProbe() {
        var postfixDivision = "i++ / 2"
        if (Work.stripLiterals(postfixDivision, coverageName) !== postfixDivision)
            throw new Error("postfix division was stripped")
    }
    function primitive(ok) { if (!ok) throw "primitive failure" }
    Component.onCompleted: {
        try { postfixProbe(); relay(); primitive(false) } catch (error) { throw error }
        fail("direct failure"); finish(false, "completion failure")
        console.log("FAIL logged receipt"); console.error("error receipt")
        Qt.exit(2); Qt.quit(7); Qt.exit(failed ? 1 : 0)
        compare(1, 2); assert(false, "assert receipt")
        errors.push("collected failure")
        Qt.exit(0); Qt.exit((0)); Qt.exit(-0); Qt.quit()
        var payload = "throw new Error('payload'); fail('payload')"
        var pattern = /throw\s+fail\(payload\)/
        // throw new Error("comment"); finish(false, "comment")
    }
}
