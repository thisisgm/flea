.import "../../ui/js/Scale.js" as Scale

// Issue 9's interface scale. The clamp and the rounding are the whole of the policy, and both are
// the kind of thing that reads fine and is wrong by 0.0000001, so they are asserted rather than eyeballed.

function run(check) {
    check("a step up is a tenth", Scale.stepped(1, 1), 1.1)
    check("a step down is a tenth", Scale.stepped(1, -1), 0.9)
    check("zero direction is the identity, which is how a stored value is re-clamped",
          Scale.stepped(1.4, 0), 1.4)

    // Up and back down has to land on exactly 1, or the reset sentence claims 100 percent while
    // every token is multiplied by 0.9999999999999999 and the metrics gate reads one pixel out.
    check("up then down returns to exactly one", Scale.stepped(Scale.stepped(1, 1), -1), 1)
    var walked = 1
    for (var i = 0; i < 7; i++) walked = Scale.stepped(walked, 1)
    for (var j = 0; j < 7; j++) walked = Scale.stepped(walked, -1)
    check("and so does a seven step walk each way", walked, 1)

    check("it clamps at the ceiling rather than growing forever", Scale.stepped(Scale.MAX, 1), Scale.MAX)
    check("and at the floor", Scale.stepped(Scale.MIN, -1), Scale.MIN)
    // A hand-edited view.json is not a trust boundary but it is an input, so it is clamped on read.
    check("a stored value above the ceiling is clamped, not honoured", Scale.stepped(9, 0), Scale.MAX)
    check("a stored zero reads as the default rather than collapsing every token",
          Scale.stepped(0, 0), 1)

    check("the sentence names the percentage and the way back",
          Scale.announce(1.3), "Interface scale 130 percent. Ctrl+Shift+0 resets.")
    check("and reads as a whole number at the default", Scale.announce(1),
          "Interface scale 100 percent. Ctrl+Shift+0 resets.")
}
