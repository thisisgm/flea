.import "../../ui/js/Format.js" as Format

// js.sh runs this suite alone under TZ=America/New_York, whose offset changes twice
// in 2026: spring forward on 8 March (local 02:00 does not exist, EST to EDT) and
// fall back on 1 November (local 01:30 happens twice, EDT then EST). Every instant
// pinned here sits on one of those boundaries, so a day boundary computed from UTC
// numbers or from one fixed offset reddens; the Tokyo run cannot see either.
function run(check) {
    // Spring forward, 8 March 2026: 01:30 is EST (UTC-5) and 03:30 is EDT (UTC-4),
    // and both are the same local calendar morning.
    var springNow = 1772985600          // noon EDT that day
    check("the hour before spring forward is Today at 01:30",
          Format.date(1772951400, springNow * 1000), "Today, 01:30")
    check("the hour after the skip is Today at 03:30",
          Format.date(1772955000, springNow * 1000), "Today, 03:30")
    check("the evening before spring forward is Yesterday at 23:30",
          Format.date(1772944200, springNow * 1000), "Yesterday, 23:30")

    // Fall back, 1 November 2026: the wall clock reads 01:30 twice, an hour of real
    // time apart, and both passes belong to the same Today.
    var fallNow = 1793552400            // noon EST that day
    check("the first pass of the repeated hour is Today at 01:30",
          Format.date(1793511000, fallNow * 1000), "Today, 01:30")
    check("the second pass of the repeated hour is Today at 01:30 too",
          Format.date(1793514600, fallNow * 1000), "Today, 01:30")
    // The DST catch: the instant is 03:30 on 1 November in UTC but 23:30 on 31
    // October on the wall clock, so a UTC day number would call it Today.
    check("the evening before fall back is Yesterday at 23:30",
          Format.date(1793503800, fallNow * 1000), "Yesterday, 23:30")
}
