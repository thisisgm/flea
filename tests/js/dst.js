.import "../../ui/js/Format.js" as Format

function run(check, suite) {
    if (suite === "edmonton") {
        runEdmonton(check)
        return
    }
    // America/New_York skips from 01:59 EST to 03:00 EDT on 8 March 2026.
    var springNow = 1772985600
    check("the hour before spring forward stays Today",
          Format.date(1772951400, springNow * 1000), "Today, 01:30")
    check("the first hour after the spring gap stays Today",
          Format.date(1772955000, springNow * 1000), "Today, 03:30")
    check("the evening before spring forward is Yesterday",
          Format.date(1772944200, springNow * 1000), "Yesterday, 23:30")

    // America/New_York repeats 01:00 through 01:59 on 1 November 2026.
    var fallNow = 1793552400
    check("the hour before fall back stays Today",
          Format.date(1793507400, fallNow * 1000), "Today, 00:30")
    check("the first pass through the repeated hour stays Today",
          Format.date(1793511000, fallNow * 1000), "Today, 01:30")
    check("the second pass through the repeated hour stays Today",
          Format.date(1793514600, fallNow * 1000), "Today, 01:30")
    check("the evening before fall back is Yesterday",
          Format.date(1793503800, fallNow * 1000), "Yesterday, 23:30")
}

function runEdmonton(check) {
    // Issue #40 was observed at 23:21 MDT on 3 September 2026.
    var now = 1788499260000
    check("the reporter's first late-evening file stays Today",
          Format.date(1788499140, now), "Today, 23:19")
    check("the reporter's second late-evening file stays Today",
          Format.date(1788498720, now), "Today, 23:12")
    check("the reporter's August date does not advance into September",
          Format.date(1788237660, now), "31 Aug, 22:41")
    check("the reporter's older date keeps its local day",
          Format.date(1787631840, now), "24 Aug, 22:24")
}
