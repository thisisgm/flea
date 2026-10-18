function requireValue(ok) { if (!ok) throw new Error("JS assertion failed") }
function verifyValue(ok) { requireValue(ok) }
function run() {
    verifyValue(false)
    finish(false, "JS completion failure")
    console.warn("FAIL JS receipt")
    Qt.exit(3)
    fail("JS explicit failure")
    expect(false, "JS known assertion")
    failures.push("JS collected failure")
}
run()
