.pragma library

// A dated figure is asked for its day at this hour UTC, so the helper, which formats in UTC, draws the asked date in every GUI zone.
var MIDDAY_HOUR = 12;

// Sample input: ("mermaid", "gantt\n    title Plan", theme) answers a copy of theme whose today is this day's noon UTC in ms, since a Gantt chart draws today's date; any other figure answers theme itself.
function datedTheme(kind, source, theme) {
    if (kind !== "mermaid" || !theme || !/(?:^|%%)\s*gantt\b/im.test(source))
        return theme;
    var now = new Date();
    var dated = Object.assign({}, theme);
    dated.today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate(), MIDDAY_HOUR);
    return dated;
}
