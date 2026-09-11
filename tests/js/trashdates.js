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
}
