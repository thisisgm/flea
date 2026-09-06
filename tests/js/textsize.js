.import "../../ui/js/TextSize.js" as TextSize

// Flea's text size. The stop list, the two modes and the two font ratios all come off the
// SettingsScale board, and the board's own layout table is reproduced here row by row, because a
// list that reads fine is exactly the kind of thing that is wrong by one stop or one pixel.

// The board's "02 Historical layout projection across documented stops" table, verbatim:
// base, bodySmall, caption, paddingY, rowHeight, iconSize, mark. paddingY, rowHeight, iconSize and
// mark are ui/Theme.qml's, so they are composed here from the same formulas that file uses.
var BOARD = [
    { base: 9, bodySmall: 8, caption: 7, paddingY: 5, rowHeight: 24, iconSize: 14, mark: 11.60 },
    { base: 10, bodySmall: 9, caption: 8, paddingY: 5, rowHeight: 26, iconSize: 16, mark: 13.05 },
    { base: 11, bodySmall: 10, caption: 9, paddingY: 6, rowHeight: 30, iconSize: 18, mark: 14.50 },
    { base: 12, bodySmall: 11, caption: 10, paddingY: 6, rowHeight: 32, iconSize: 20, mark: 15.95 },
    { base: 14, bodySmall: 13, caption: 12, paddingY: 7, rowHeight: 37, iconSize: 23, mark: 18.85 },
    { base: 16, bodySmall: 15, caption: 13, paddingY: 8, rowHeight: 43, iconSize: 27, mark: 21.75 },
    { base: 20, bodySmall: 18, caption: 17, paddingY: 10, rowHeight: 52, iconSize: 32, mark: 26.10 }
]

// ui/Theme.qml's own three, so a change to either side reddens rather than drifting quietly.
var LINE_BOX_RATIO = 1.8
var MARK_RATIO = 1.45
// The board's paddingY column, round(6 x base / 12), where the 12 is the stock anchor. ui/Theme.qml
// reaches the same number by rescaling Omarchy's own controlPaddingY, which is space(6) over the
// same anchor; both are asserted below.
var STOCK_PADDING_Y = 6
var STOCK_BASE = 12

function run(check) {
    runStops(check)
    runModes(check)
    runSteps(check)
    runStored(check)
    runBoardTable(check)
    runAnnounce(check)
}

function runStops(check) {
    check("there are seven stops, which is what the board's own table has rows for",
          TextSize.STOPS.length, 7)
    check("and they are the board's list, gaps and all", TextSize.STOPS.join(","),
          "9,10,11,12,14,16,20")
    check("the range is the OEM one, so Flea offers no size Omarchy does not",
          TextSize.STOPS[0] + "-" + TextSize.STOPS[TextSize.STOPS.length - 1], "9-20")
    // The list is not contiguous, and a stepper written against a range would silently invent these.
    check("13 is not a stop", TextSize.STOPS.indexOf(13), -1)
    check("15, 17, 18 and 19 are not stops either",
          [15, 17, 18, 19].filter(function (px) { return TextSize.STOPS.indexOf(px) >= 0 }).length, 0)

    check("a size off the list enters on its nearest stop", TextSize.nearest(19), 20)
    check("and takes the smaller stop when two are equally near, so nothing grows unasked",
          TextSize.nearest(13), 12)
    check("which is the same answer at the other tie", TextSize.nearest(15), 14)
    check("a size under the floor lands on the floor", TextSize.nearest(4), 9)
    check("and one over the ceiling on the ceiling", TextSize.nearest(64), 20)
    check("a stop is its own nearest", TextSize.nearest(14), 14)
}

function runModes(check) {
    check("the default is Follow Omarchy", TextSize.following(TextSize.follow()), true)
    check("and it stores the shape the board names",
          JSON.stringify(TextSize.follow()), '{"mode":"system"}')
    check("an override is not following",
          TextSize.following({ mode: 16 }), false)
    check("nothing stored at all reads as following", TextSize.following(undefined), true)

    check("following draws at Omarchy's own size",
          TextSize.effective(TextSize.follow(), 14), 14)
    check("and a later omarchy display text size moves it with no write of Flea's own",
          TextSize.effective(TextSize.follow(), 16), 16)
    check("an override draws at its stop and ignores Omarchy's",
          TextSize.effective({ mode: 20 }, 14), 20)
}

function runSteps(check) {
    // Switching to Override changes nothing on screen: only a step does, which is what makes the
    // switch safe to press to see what is there.
    check("pinning starts on the stop already on screen",
          TextSize.pin(TextSize.follow(), 14).mode, 14)
    check("and on the nearest one when Omarchy is between two",
          TextSize.pin(TextSize.follow(), 13).mode, 12)
    check("pinning an override leaves it where it is",
          TextSize.pin({ mode: 10 }, 14).mode, 10)

    check("a step up from following becomes an override one stop above Omarchy's size",
          JSON.stringify(TextSize.stepped(TextSize.follow(), 14, 1)), '{"mode":16}')
    check("a step down from following goes the other way",
          TextSize.stepped(TextSize.follow(), 14, -1).mode, 12)
    check("a step is one stop and not one pixel",
          TextSize.stepped({ mode: 12 }, 14, 1).mode, 14)
    check("it clamps at the ceiling rather than growing forever",
          TextSize.stepped({ mode: 20 }, 14, 1).mode, 20)
    check("and at the floor", TextSize.stepped({ mode: 9 }, 14, -1).mode, 9)

    // Up and back down has to land on the same stop, or the chord walks the list off centre. The
    // walk stays inside the list: a clamp is not reversible, which the two checks above pin.
    var walked = TextSize.follow()
    for (var i = 0; i < 2; i++) walked = TextSize.stepped(walked, 14, 1)
    check("two steps up from this box's own size reaches the ceiling", walked.mode, 20)
    for (var j = 0; j < 2; j++) walked = TextSize.stepped(walked, 14, -1)
    check("and two back returns to the size it started on", walked.mode, 14)
}

function runStored(check) {
    check("a stored override on a real stop is kept",
          JSON.stringify(TextSize.parse({ mode: 16 })), '{"mode":16}')
    // A hand-edited ui.json is not a trust boundary but it is an input, so it is snapped on read.
    check("a stored size off the list snaps to a stop rather than being honoured",
          TextSize.parse({ mode: 13 }).mode, 12)
    check("a stored zero reads as following rather than collapsing every token",
          TextSize.following(TextSize.parse({ mode: 0 })), true)
    check("so does a stored size that is not a number",
          TextSize.following(TextSize.parse({ mode: "big" })), true)
    // src/uischema.rs takes "system" or a stop and nothing else, so this is the one stored shape.
    check("0.1.3's own override shape reads as following rather than as a second vocabulary",
          TextSize.following(TextSize.parse({ mode: "override", px: 16 })), true)
    // The old build stored an interface-scale multiplier under uiScale and no textSize at all.
    check("and so does a file written before this control existed",
          TextSize.following(TextSize.parse(undefined)), true)
    // The window writes what src/uischema.rs validates, so the two vocabularies are one.
    check("the stored override is the shape the schema's stop rule takes",
          JSON.stringify(TextSize.stepped(TextSize.follow(), 14, 1)), '{"mode":16}')
    check("and following is the shape its system rule takes",
          JSON.stringify(TextSize.parse(TextSize.follow())), '{"mode":"system"}')
}

// The board's table is the contract, so it is asserted stop by stop rather than summarised.
function runBoardTable(check) {
    for (var i = 0; i < BOARD.length; i++) {
        var row = BOARD[i]
        check("the board's " + row.base + "px row draws body text at " + row.bodySmall,
              TextSize.bodySmall(row.base), row.bodySmall)
        check("and its caption at " + row.caption, TextSize.caption(row.base), row.caption)

        var paddingY = Math.round(STOCK_PADDING_Y * row.base / STOCK_BASE)
        check("its row padding is " + row.paddingY, paddingY, row.paddingY)
        var rowHeight = Math.round(row.bodySmall * LINE_BOX_RATIO) + 2 * paddingY
        check("its row height is " + row.rowHeight, rowHeight, row.rowHeight)
        check("its icon slot is " + row.iconSize, rowHeight - 2 * paddingY, row.iconSize)
        // The board prints the mark to two decimals and ui/Theme.qml draws whole pixels, so the
        // product is compared at the board's own precision rather than at the float's.
        check("its mark is " + row.mark,
              Math.round(row.bodySmall * MARK_RATIO * 100) / 100, row.mark)
    }
    check("every stop the board tabulates is a stop this build has",
          BOARD.filter(function (row) { return TextSize.STOPS.indexOf(row.base) < 0 }).length, 0)
    check("and the table has a row for every stop", BOARD.length, TextSize.STOPS.length)

    // ui/Theme.qml rescales Omarchy's own controlPaddingY instead of running the board's formula,
    // so the two are pinned together at this box's base of 14, where space(6) resolves to 7.
    for (var s = 0; s < BOARD.length; s++) {
        check("rescaling this box's own padding reaches the board's " + BOARD[s].base + "px row",
              Math.round(7 * BOARD[s].base / 14), BOARD[s].paddingY)
    }
}

function runAnnounce(check) {
    check("the sentence names the size and the way back",
          TextSize.announce({ mode: 16 }, 14),
          "Text size 16px. Ctrl+Shift+0 follows Omarchy again.")
    check("and following reports whose size it is",
          TextSize.announce(TextSize.follow(), 14), "Text size follows Omarchy, 14px.")
}
