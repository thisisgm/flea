.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Focus.js" as Focus
.import "filterfixture.js" as Fixture

// Issue 27's own suite, split out of tests/js/focus.js at its 300-line cap the way focus-lines.js
// was: what ui/js/Focus.js step does at an end, with wrapAtEnds off and on.

// The fixture lists seven rows and no filter, so a view position is a listing index and the last
// row is 6; every expectation below is written as that index and not as an offset.
function run(check) {
    // The state file's wrapAtEnds is off by default, so the shipped answer is still the clamp the
    // operator who reported the jump wanted; with the key on the cursor comes round, which is what
    // the operator who asked for it wanted.
    var stopping = Fixture.pane()
    Focus.step(stopping, -1)
    check("with the key off, up at the top leaves the cursor where it was", stopping.cursorIndex, 0)
    // The other end of the same clamp, which had no case at all: a build that wrapped only downward
    // passed the check above and shipped the jump the operator reported.
    stopping.cursorIndex = 6
    Focus.step(stopping, 1)
    check("and with the key off, down at the bottom leaves it there too", stopping.cursorIndex, 6)

    var wrapping = Fixture.pane()
    wrapping.wrapAtEnds = true
    Focus.step(wrapping, -1)
    check("with the key on, up at the top lands on the last row", wrapping.cursorIndex, 6)
    Focus.step(wrapping, 1)
    check("and down from the last row comes back to the first", wrapping.cursorIndex, 0)

    // A page key overshoots on purpose, so only a step taken FROM an end may wrap.
    wrapping.cursorIndex = 3
    Focus.step(wrapping, 20)
    check("a page key from the middle still stops at the end it was heading for", wrapping.cursorIndex, 6)
    // The one case that rule leaves live, and the one nothing was driving: the same page-sized step
    // taken from an end, where the clamp has nowhere left to go and the wrap is the whole answer.
    Focus.step(wrapping, 20)
    check("but the next one, taken from that end, comes round to the first row", wrapping.cursorIndex, 0)
    Focus.step(wrapping, -20)
    check("and a page step taken from the first row lands on the last", wrapping.cursorIndex, 6)

    // The selection keys keep ui/js/Filter.js moveCursor's plain clamp: an extend that wrapped would
    // run the anchor to the other end of the listing and select every row between the two.
    var extending = Fixture.pane()
    extending.wrapAtEnds = true
    Filter.moveCursor(extending, -1)
    check("moveCursor, which shift+j and shift+k move through, never wraps", extending.cursorIndex, 0)
    extending.cursorIndex = 6
    Filter.moveCursor(extending, 1)
    check("and it clamps at the bottom too, with the key on", extending.cursorIndex, 6)
}
