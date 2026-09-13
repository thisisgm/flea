.import "../../ui/js/TrashDates.js" as Trash
function run(check) {
    var now = new Date(2026, 8, 8, 12).getTime()
    check("today", Trash.deleted("2026-09-08T01:00:00", now), "today")
    check("yesterday", Trash.deleted("2026-09-07T23:00:00", now), "yesterday")
    check("older day", Trash.deleted("2026-08-30T12:00:00", now), "Aug 30")
    check("older year", Trash.deleted("2025-08-30T12:00:00", now), "Aug 30 2025")
    check("missing date is honest", Trash.deleted("", now), "Unknown")
    check("home child location", Trash.location("/home/gm/Pictures/photo.png", "/home/gm"), "~/Pictures")
    check("home file location", Trash.location("/home/gm/photo.png", "/home/gm"), "~")
    check("similar home prefix stays absolute", Trash.location("/home/gm-other/photo.png", "/home/gm"), "/home/gm-other")
    check("root location", Trash.location("/photo.png", "/home/gm"), "/")
    check("missing location is honest", Trash.location("photo.png", "/home/gm"), "Unknown")

    // The 30 day sweep's own two questions, GM's ruling of 2026-09-11.
    // Its own clock: the suite's "now" above belongs to the date labels and is eight days earlier.
    var sweepNow = new Date(2026, 8, 11, 12, 0, 0).getTime()
    var day = 24 * 60 * 60 * 1000
    check("an item deleted 30 days ago is swept",
          Trash.expired(new Date(sweepNow - 30 * day).toISOString(), sweepNow, 30), true)
    check("and one deleted 31 days ago is too",
          Trash.expired(new Date(sweepNow - 31 * day).toISOString(), sweepNow, 30), true)
    check("one deleted 29 days ago is left alone",
          Trash.expired(new Date(sweepNow - 29 * day).toISOString(), sweepNow, 30), false)
    check("one deleted this morning is left alone",
          Trash.expired(new Date(sweepNow - 3600000).toISOString(), sweepNow, 30), false)
    // The one direction a wrong answer can be taken in: an item whose date cannot be read stays.
    check("an unreadable date is never swept", Trash.expired("not a date", sweepNow, 30), false)
    check("an empty date is never swept", Trash.expired("", sweepNow, 30), false)
    check("an absent date is never swept", Trash.expired(undefined, sweepNow, 30), false)
    check("a null date is never swept", Trash.expired(null, sweepNow, 30), false)
    // A date in the future is not 30 days old, whatever wrote it.
    check("a date in the future is never swept",
          Trash.expired(new Date(sweepNow + 5 * day).toISOString(), sweepNow, 30), false)

    // The once-a-day guard, which is what keeps the sweep off every launch.
    check("the day number is the same all day",
          Trash.dayNumber(new Date(2026, 8, 11, 0, 1, 0).getTime())
          === Trash.dayNumber(new Date(2026, 8, 11, 23, 59, 0).getTime()), true)
    check("and the next day is one more",
          Trash.dayNumber(new Date(2026, 8, 12, 9, 0, 0).getTime())
          - Trash.dayNumber(new Date(2026, 8, 11, 9, 0, 0).getTime()), 1)
    check("a fresh ui.json has never swept, and today is never day zero",
          Trash.dayNumber(sweepNow) > 0, true)

    // The same defect in the label above the sweep: new Date(null) is the epoch, so an item with no
    // date read as a day in 1970 rather than as Unknown.
    check("a null date has no label either", Trash.deleted(null, sweepNow), "Unknown")
    check("an absent date has no label", Trash.deleted(undefined, sweepNow), "Unknown")
    check("an empty date has no label", Trash.deleted("", sweepNow), "Unknown")
}
