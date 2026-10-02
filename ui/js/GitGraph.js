.pragma library

// Lane colours and small formatting for the git graph panel. Pure so tests can drive it without QML.
// Palette mirrors Switchboard's laneColor; Theme ink is applied by the panel around these hues.

var LANE_COLORS = ["#d4b06a", "#7aa2d4", "#6fbf7a", "#c49bdb", "#7ec8c8", "#d27a7a", "#e0a36b", "#8d93a0"]

function laneColor(i) {
    var n = Math.abs(Number(i) || 0)
    return LANE_COLORS[n % LANE_COLORS.length]
}

// Sample input: "2026-01-03T12:34:56+00:00" -> "2026-01-03"
function shortDate(iso) {
    if (!iso || typeof iso !== "string") return ""
    var t = iso.indexOf("T")
    return t > 0 ? iso.substring(0, t) : iso
}

function maxWidth(commits) {
    var w = 1
    if (!commits) return w
    for (var i = 0; i < commits.length; i++) {
        var n = Number(commits[i].width) || 1
        if (n > w) w = n
    }
    return w
}

function title(branch, root) {
    if (branch && branch.length) return "Git · " + branch
    if (root && root.length) return "Git · " + root
    return "Git"
}
