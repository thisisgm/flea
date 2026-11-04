//@ pragma ShellId flea-theme-contrast-test

import QtQuick
import Quickshell
import qs.Commons
import "flea" as Flea
import "flea/js/Contrast.js" as Contrast
import "button-probe.js" as Probe
import "qml-probe.js" as QmlProbe
import "palettes.js" as Palettes

// tests/theme-contrast.sh's harness: each palette is applied to the real Theme, and the text and frame roles it derives are held to their floors, and a cursor row draws the plain hover wash.
ShellRoot {
    id: shell

    // Quiet ticks, a count of frames and never time, before the window is up and the rows are built.
    readonly property int settleTicks: 4
    readonly property int tickMs: 16
    readonly property int outerTimeoutS: Number(Quickshell.env("BUTTONSYS_TIMEOUT_S")) || 60
    readonly property int capShare: 2
    readonly property int msPerSecond: 1000
    readonly property double startedAt: Date.now()
    readonly property int capMs: shell.outerTimeoutS * shell.msPerSecond / shell.capShare
    readonly property string rootDir: Quickshell.env("BUTTONSYS_ROOT") || ""
    // The shell pins 24 palettes on stable and 25 on dev, where Starship joins the installed stock themes.
    readonly property int paletteCount: Number(Quickshell.env("THEMECONTRAST_PALETTES")) || 24
    // Alpha tolerance for a drawn wash against its role.
    readonly property real alphaEpsilon: 0.0001
    // glass.lua's blur brightness: Hyprland dims the blurred backdrop to this share before the translucent ground composes over it.
    readonly property real blurBrightness: 0.75
    readonly property var backdrops: [{ name: "white", level: 1 }, { name: "black", level: 0 }]
    readonly property int glassSteps: 255
    // The lowest 8-bit glass alpha every palette passes at and above, taken over the whole sweep.
    property int glassFloor: 0
    readonly property var sampleRow: ({ n: "contrast.txt", i: "text-x-generic", p: 420, d: false, s: 13, m: 1758835200, t: false, k: 0, v: 0 })

    property int checks: 0
    property int ticks: 0
    property var failures: []

    function log(line) { QmlProbe.log("THEMECONTRAST", line) }
    function check(name, actual, expected) { QmlProbe.checkLoud(shell, name, actual, expected) }
    function finish() {
        QmlProbe.logDone(shell)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    // One line per palette and role: the ground, the ratio and the floor, so a red line names all four.
    function atLeast(theme, role, ground, got, floor) {
        shell.checks += 1
        var line = theme + " " + role + " on " + ground + " " + (Math.round(got * 1000) / 1000) + " >= " + floor
        if (got >= floor) {
            shell.log("ok " + line)
            return
        }
        shell.failures.push(line)
        shell.log("FAIL " + line)
    }

    FloatingWindow {
        id: window
        implicitWidth: 800
        implicitHeight: 400
        color: "#303030"

        Column {
            Flea.Row { id: restRow; width: 800; row: shell.sampleRow; kindNames: []; hiddenCols: []; clipMark: "" }
            Flea.MenuRow { id: litRow; width: 400; current: true; entry: ({ label: "Open", glyph: "folder-open", action: "open", hint: "enter" }) }
            Flea.DialogButton { id: secondaryButton; label: "Cancel" }
            // The same entries as a cursor row, a picked row and a resting row: the pairs the recipe check compares.
            Flea.MenuRow { id: restKeyRow; width: 400; entry: ({ label: "Open", glyph: "folder-open", action: "open", hint: "enter" }) }
            Flea.MenuRow { id: litDangerRow; width: 400; current: true; entry: ({ label: "Move to Trash", glyph: "trash", action: "trash", danger: true, hint: "d" }) }
            Flea.MenuRow { id: restDangerRow; width: 400; entry: ({ label: "Move to Trash", glyph: "trash", action: "trash", danger: true, hint: "d" }) }
            Flea.MenuRow { id: litSubRow; width: 400; current: true; entry: ({ label: "Open With", glyph: "folder-open", action: "openWith", submenu: [{ label: "x" }] }) }
            Flea.MenuRow { id: restSubRow; width: 400; entry: ({ label: "Open With", glyph: "folder-open", action: "openWith", submenu: [{ label: "x" }] }) }
            Flea.MenuRow { id: pickedRow; width: 400; picked: true; current: true; entry: ({ label: "Name", glyph: "file", action: "name" }) }
        }
    }

    // The first item under a node whose property key reads value (an objectName, a glyph name or a drawn string).
    function findNamed(item, key, value) {
        var stack = [item]
        while (stack.length > 0) {
            var node = stack.pop()
            if (node[key] === value && node.color !== undefined) return node
            for (var i = 0; i < node.children.length; i++) stack.push(node.children[i])
        }
        return null
    }
    // A colour as opaque hex, so a wash's colour reads apart from its alpha.
    function solid(color) { return Contrast.hexOf(Contrast.parse(String(color))) }
    // A row's wash as v0.3.9 drew it, in every palette: a cursor row is the foreground at the hover lift, a picked row the selected accent fill whole, a resting row nothing, no frame object on either, and a cursor row keeps every ink a resting row has.
    function cursorRecipe(name, fg) {
        var cursor = shell.findNamed(litRow, "objectName", "menuWash"), picked = shell.findNamed(pickedRow, "objectName", "menuWash"), rest = shell.findNamed(restKeyRow, "objectName", "menuWash")
        var selected = Style.selectedAccentFill
        shell.check(name + " cursor wash is the foreground", cursor !== null && shell.solid(cursor.color) === shell.solid(fg), true)
        shell.check(name + " cursor wash is the hover lift", cursor !== null && Math.abs(cursor.color.a * cursor.opacity - Flea.Theme.washHover) < shell.alphaEpsilon, true)
        shell.check(name + " resting row draws no wash", rest !== null && rest.opacity === 0, true)
        shell.check(name + " picked wash is the selected accent fill", picked !== null && String(picked.color) === String(selected), true)
        shell.check(name + " picked wash is drawn at opacity 1", picked !== null && picked.opacity === 1, true)
        shell.check(name + " cursor and picked rows draw no cursor frame", shell.findNamed(litRow, "objectName", "menuCursorFrame") === null && shell.findNamed(pickedRow, "objectName", "menuCursorFrame") === null, true)
        var pairs = [[litRow, restKeyRow, "text", "Open"], [litRow, restKeyRow, "text", "enter"], [litRow, restKeyRow, "name", "folder-open"],
                     [litDangerRow, restDangerRow, "text", "Move to Trash"], [litDangerRow, restDangerRow, "name", "trash"], [litSubRow, restSubRow, "name", "chevron-right"]]
        var same = true, drawn = 0
        for (var i = 0; i < pairs.length; i++) {
            var asLit = shell.findNamed(pairs[i][0], pairs[i][2], pairs[i][3]), atRest = shell.findNamed(pairs[i][1], pairs[i][2], pairs[i][3])
            if (asLit === null || atRest === null) continue
            drawn += 1
            same = same && String(asLit.color) === String(atRest.color)
        }
        shell.check(name + " cursor row label, hint, mark, danger and chevron take the resting ink", same && drawn === pairs.length, true)
    }
    // The ground a wash draws over a surface: the colour's own alpha times the item's opacity (Contrast.parse drops the alpha of #AARRGGBB).
    function groundOf(wash, surface) {
        return Contrast.hexOf(Contrast.over(String(wash.color), wash.color.a * wash.opacity, surface))
    }

    // The ground a glass plane draws at 8-bit alpha a over the dimmed backdrop, as a hex.
    function glassGround(plane, a, backdrop) {
        var g = Math.round(shell.glassSteps * backdrop.level * shell.blurBrightness) / shell.glassSteps
        return Contrast.hexOf(Contrast.over(String(plane), a / shell.glassSteps, Contrast.hexOf([g, g, g])))
    }
    // The least ratio muted and metadata text reach on either plane over either backdrop, and where it is lowest.
    function glassWorst(c, a) {
        var worst = { ratio: Infinity, at: "" }
        var roles = [["muted text", c.muted], ["metadata text", c.metadata]], planes = [["background", c.background], ["surface", c.surface]]
        for (var r = 0; r < roles.length; r++)
            for (var p = 0; p < planes.length; p++)
                for (var b = 0; b < shell.backdrops.length; b++) {
                    var got = Contrast.ratio(String(roles[r][1]), shell.glassGround(planes[p][1], a, shell.backdrops[b]))
                    if (got < worst.ratio) worst = { ratio: got, at: roles[r][0] + " on the " + planes[p][0] + " over " + shell.backdrops[b].name }
                }
        return worst
    }
    // Muted text sits near 4.5 on the opaque ground, so no translucent ground holds it: the floor over white is 255/255 for 16 palettes (RESULT, 2026-10-12). A palette's recorded floor only moves down, when a look lifts the ink on glass.
    readonly property var glassFloors: ({ "catppuccin": 253, "catppuccin-latte": 255, "ethereal": 240, "everforest": 255, "flexoki-light": 255,
        "gruvbox": 255, "hackerman": 255, "kanagawa": 255, "last-horizon": 253, "lumon": 255, "lupine": 255, "matte-black": 253, "miasma": 255,
        "nord": 255, "jade-horse": 255, "retro-82": 255, "ristretto": 255, "rose-pine": 255, "solitude": 255, "tokyo-night": 255,
        "vantablack": 249, "white": 254, "starship": 236, "cool-dawn": 232, "sunset": 253 })
    // Option B: the lowest 8-bit alpha at which the unlifted muted and metadata ink hold 4.5 on the background plane alone, the plane under the list and preview.
    function listFloor(c) {
        var floor = shell.glassSteps
        while (floor > 0 && Math.min(shell.inkWorstOn(c.muted, c.background, floor - 1), shell.inkWorstOn(c.metadata, c.background, floor - 1)) >= Flea.Theme.textContrast) floor--
        return floor
    }
    function inkWorstOn(ink, plane, a) {
        var worst = Infinity
        for (var b = 0; b < shell.backdrops.length; b++)
            worst = Math.min(worst, Contrast.ratio(String(ink), shell.glassGround(plane, a, shell.backdrops[b])))
        return worst
    }
    function glassSweep(name, c) {
        // Walk down from opaque while every step still passes; the step before the first miss is this palette's floor.
        var floor = shell.glassSteps
        while (floor > 0 && shell.glassWorst(c, floor - 1).ratio >= Flea.Theme.textContrast) floor--
        var worst = shell.glassWorst(c, floor)
        shell.log("floor " + name + " " + floor + "/255 (" + (Math.round(floor / shell.glassSteps * 1000) / 1000) + "), lowest ratio there " + (Math.round(worst.ratio * 100) / 100) + " " + worst.at)
        shell.glassFloor = Math.max(shell.glassFloor, floor)
        shell.atLeast(name, "glass ink at opaque", "both planes over both backdrops", shell.glassWorst(c, shell.glassSteps).ratio, Flea.Theme.textContrast)
        var recorded = shell.glassFloors[name]
        shell.check(name + " glass floor " + floor + " is at or under the recorded " + recorded, recorded !== undefined && floor <= recorded, true)
    }

    // Option A: the glass alpha the lift table is drawn for, 0.75 in whole 8-bit steps.
    readonly property int liftAlpha: 191
    // Muted and metadata ink per palette, lifted toward the foreground to 4.5 on both planes over both backdrops at liftAlpha (RESULT, GLASSR3).
    readonly property var glassLifts: ({
        "catppuccin": { muted: "#b2b4be", metadata: "#afb5c4" },
        "catppuccin-latte": { muted: "#3f4045", metadata: "#36384b" },
        "ethereal": { muted: "#94a0c9", metadata: "#bb9b85" },
        "everforest": { muted: "#c9cdce", metadata: "#d0ccc3" },
        "flexoki-light": { muted: "#464642", metadata: "#0b0b0b" },
        "gruvbox": { muted: "#c2beba", metadata: "#c6beaf" },
        "hackerman": { muted: "#9fa2af", metadata: "#9eb0b6" },
        "kanagawa": { muted: "#b5b5bf", metadata: "#b8b6a6" },
        "last-horizon": { muted: "#a4a0a1", metadata: "#b3b4b3" },
        "lumon": { muted: "#b0b9c2", metadata: "#b3b9bf" },
        "lupine": { muted: "#444444", metadata: "#181818" },
        "matte-black": { muted: "#a7a7a7", metadata: "#a7a7a7" },
        "miasma": { muted: "#b8b8b8", metadata: "#b8b8af" },
        "nord": { muted: "#c8ccd1", metadata: "#c9cccf" },
        "jade-horse": { muted: "#a8bcb5", metadata: "#b1bab5" },
        "retro-82": { muted: "#8fb2b8", metadata: "#baaa8c" },
        "ristretto": { muted: "#c1bdbd", metadata: "#c2bcbc" },
        "rose-pine": { muted: "#434242", metadata: "#3e3b56" },
        "solitude": { muted: "#a6a8ab", metadata: "#a7a8a8" },
        "starship": { muted: "#b2b2b5", metadata: "#b2b2b2" },
        "tokyo-night": { muted: "#aeb1be", metadata: "#adb1c1" },
        "vantablack": { muted: "#a0a0a0", metadata: "#b6b6b6" },
        "white": { muted: "#494949", metadata: "#000000" },
        "cool-dawn": { muted: "#b8c9bf", metadata: "#c2c6c2" },
        "sunset": { muted: "#b1a7a2", metadata: "#b1a898" } })
    // The closest shipped muted sits 1.31:1 from the primary ink (rose-pine), so a lifted muted nearer than this, or past the primary ink, has collapsed into it.
    readonly property real stepFloor: 1.25
    // Palettes whose lifted muted collapses into the primary ink: the primary ink itself sits near 4.5 on glass there, so no quieter ink holds the floor.
    readonly property var glassCollapsed: ["catppuccin-latte", "everforest", "gruvbox", "miasma", "nord", "rose-pine", "tokyo-night", "sunset"]
    // True where the lifted ink is nearer the primary ink than the step floor, or on its far side.
    function collapsedInto(ink, c) {
        var fg = Contrast.parse(c.foreground), bg = Contrast.parse(c.background), x = Contrast.parse(ink)
        var lf = Contrast.luminance(fg[0], fg[1], fg[2]), lb = Contrast.luminance(bg[0], bg[1], bg[2]), lx = Contrast.luminance(x[0], x[1], x[2])
        return (lf > lb ? lf - lx : lx - lf) <= 0 || Contrast.ratio(String(c.foreground), ink) < shell.stepFloor
    }
    // The least ratio one ink reaches on either plane over either backdrop at alpha a.
    function inkWorst(ink, c, a) {
        var worst = Infinity, planes = [c.background, c.surface]
        for (var p = 0; p < planes.length; p++)
            for (var b = 0; b < shell.backdrops.length; b++)
                worst = Math.min(worst, Contrast.ratio(String(ink), shell.glassGround(planes[p], a, shell.backdrops[b])))
        return worst
    }
    // The smallest lightness step from ink, keeping its hue, toward the side the foreground sits on that holds the floor on every glass ground (Theme's ensureRatio walk, over four grounds).
    function liftToward(ink, c, a) {
        if (shell.inkWorst(ink, c, a) >= Flea.Theme.textContrast) return String(ink)
        var fg = Contrast.parse(c.foreground), bg = Contrast.parse(c.background)
        var from = Contrast.parse(ink), to = Contrast.luminance(fg[0], fg[1], fg[2]) > Contrast.luminance(bg[0], bg[1], bg[2]) ? [1, 1, 1] : [0, 0, 0], lo = 0, hi = 1
        for (var i = 0; i < 20; i++) {
            var mid = (lo + hi) / 2
            if (shell.inkWorst(Contrast.hexOf(Contrast.mix(from, to, mid)), c, a) >= Flea.Theme.textContrast) hi = mid
            else lo = mid
        }
        for (var t = hi; t <= 1; t += 1 / 512) {
            var hex = Contrast.hexOf(Contrast.mix(from, to, t))
            if (shell.inkWorst(hex, c, a) >= Flea.Theme.textContrast) return hex
        }
        return Contrast.hexOf(to)
    }
    // The ink the glass look draws: the table's lift under alpha 255, none at it, so an opaque window draws what it drew before.
    function glassInk(name, role, ink, a) {
        var row = shell.glassLifts[name]
        return a >= shell.glassSteps || row === undefined ? String(ink) : row[role]
    }
    function liftSweep(name, c) {
        var roles = [["muted", c.muted], ["metadata", c.metadata]], lifted = {}
        for (var r = 0; r < roles.length; r++) {
            var role = roles[r][0], ink = roles[r][1]
            lifted[role] = shell.liftToward(ink, c, shell.liftAlpha)
            var got = shell.glassInk(name, role, ink, shell.liftAlpha)
            shell.log("floor lift " + name + " " + role + " " + String(ink) + " to " + lifted[role] + ", ratio " + (Math.round(shell.inkWorst(lifted[role], c, shell.liftAlpha) * 100) / 100)
                      + ", step to ink " + (Math.round(Contrast.ratio(String(c.foreground), lifted[role]) * 100) / 100) + " (opaque " + (Math.round(Contrast.ratio(String(c.foreground), String(ink)) * 100) / 100) + ")")
            shell.atLeast(name, "lifted " + role + " ink at 0.75", "both planes over both backdrops", shell.inkWorst(got, c, shell.liftAlpha), Flea.Theme.textContrast)
            shell.check(name + " " + role + " lift is the recorded " + got, got === lifted[role], true)
        }
        var collapsed = shell.collapsedInto(lifted.muted, c)
        shell.log("floor collapse " + name + " " + collapsed + ", primary ink ratio at 0.75 " + (Math.round(shell.inkWorst(c.foreground, c, shell.liftAlpha) * 100) / 100))
        shell.check(name + " lifted muted " + (shell.glassCollapsed.indexOf(name) >= 0 ? "is recorded as collapsed into the primary ink" : "keeps a step of " + shell.stepFloor + " from the primary ink"),
                    collapsed, shell.glassCollapsed.indexOf(name) >= 0)
        shell.check(name + " alpha 1 draws today's muted and metadata ink", [shell.glassInk(name, "muted", c.muted, shell.glassSteps), shell.glassInk(name, "metadata", c.metadata, shell.glassSteps)],
                    [String(c.muted), String(c.metadata)])
        var floor = shell.glassSteps
        for (var a = shell.glassSteps; a >= shell.liftAlpha; a--) {
            var muted = shell.liftToward(c.muted, c, a), metadata = shell.liftToward(c.metadata, c, a)
            if (shell.collapsedInto(muted, c) || shell.collapsedInto(metadata, c) || shell.inkWorst(c.foreground, c, a) < Flea.Theme.textContrast) break
            floor = a
        }
        shell.log("floor distinct " + name + " " + floor + "/255")
    }

    // The drawn mode roles, independently checked against both grounds and both extreme backdrops.
    function modeWorst(ink, c, a, brightness) {
        var planes = [c.background, c.surface], worst = Infinity
        var white = Contrast.hexOf([brightness, brightness, brightness])
        for (var p = 0; p < planes.length; p++)
            for (var b = 0; b < 2; b++)
                worst = Math.min(worst, Contrast.ratio(String(ink), Contrast.hexOf(Contrast.over(String(planes[p]), a / 255, b === 0 ? white : "#000000"))))
        return worst
    }
    function pickedSweep(name, c) {
        Flea.Glass.mode = "glassy"
        Flea.Glass.focused = true
        Flea.Glass.clear = true
        var drawn = Flea.Theme.color, a = Math.round(Flea.Glass.alpha * 255), list = Math.round(Flea.Glass.listAlpha * 255)
        shell.check(name + " A uses at least Nautilus's 0.75", a >= Math.ceil(0.75 * 255) && a < 255, true)
        shell.atLeast(name, "picked primary", "both planes over both backdrops", shell.modeWorst(drawn.foreground, c, a, shell.blurBrightness), Flea.Theme.textContrast)
        shell.atLeast(name, "picked muted", "both planes over both backdrops", shell.modeWorst(drawn.muted, c, a, shell.blurBrightness), Flea.Theme.textContrast)
        shell.atLeast(name, "picked metadata", "both planes over both backdrops", shell.modeWorst(drawn.metadata, c, a, shell.blurBrightness), Flea.Theme.textContrast)
        var primary = {foreground: drawn.foreground, background: c.background}
        shell.check(name + " picked muted keeps its primary step", shell.collapsedInto(String(drawn.muted), primary), false)
        shell.check(name + " picked metadata keeps its primary step", shell.collapsedInto(String(drawn.metadata), primary), false)
        shell.check(name + " picked listing has its own floor", list, Flea.Glass.split ? Math.max(a, shell.listFloor(c)) : a)
        var frames = [["muted frame", drawn.mutedFrame], ["metadata glyph", drawn.metadataFrame], ["accent frame", drawn.accentFrame]]
        var texts = [["error", drawn.error], ["status error", drawn.errorOnSurface], ["symlink", drawn.symlink], ["executable", drawn.executable]]
        for (var g = 0; g < frames.length; g++)
            shell.atLeast(name, "picked " + frames[g][0], "both planes over both backdrops", shell.modeWorst(frames[g][1], c, a, shell.blurBrightness), Flea.Theme.frameContrast)
        for (var t = 0; t < texts.length; t++)
            shell.atLeast(name, "picked " + texts[t][0], "both planes over both backdrops", shell.modeWorst(texts[t][1], c, a, shell.blurBrightness), Flea.Theme.textContrast)
        shell.log("floor PICK " + name + " alpha=" + a + " list=" + list + " primary=" + String(drawn.foreground) + " muted=" + String(drawn.muted) + " metadata=" + String(drawn.metadata))
        Flea.Glass.mode = "transparent"
        var focuses = [true, false]
        for (var f = 0; f < focuses.length; f++) {
            Flea.Glass.focused = focuses[f]
            shell.check(name + " transparent focus " + focuses[f] + " alpha", Math.round(Flea.Glass.alpha * 255), 255)
            shell.check(name + " transparent draws opaque inks", [String(drawn.foreground), String(drawn.muted), String(drawn.metadata)], [String(c.foreground), String(c.muted), String(c.metadata)])
        }
        Flea.Glass.mode = "opaque"
        shell.check(name + " opaque restores all three inks", [String(drawn.foreground), String(drawn.muted), String(drawn.metadata)], [String(c.foreground), String(c.muted), String(c.metadata)])
        Flea.Glass.clear = false
    }

    function sweep(entry) {
        var c = Flea.Theme.color, rgba = Probe.rgba, name = entry.name
        Flea.Theme.applyColors(entry.body)
        var bg = String(c.background), surface = String(c.surface), fg = String(c.foreground)
        // The ground a cursor row draws on: the plain hover wash the row itself draws, over the card's surface.
        var wash = Probe.find(litRow, "menuWash")
        shell.check(name + " cursor row draws its wash", wash !== null && wash.visible, true)
        shell.cursorRecipe(name, fg)
        shell.atLeast(name, "muted text", "background", Contrast.ratio(String(c.muted), bg), Flea.Theme.textContrast)
        shell.atLeast(name, "muted text", "surface", Contrast.ratio(String(c.muted), surface), Flea.Theme.textContrast)
        shell.atLeast(name, "metadata text", "background", Contrast.ratio(String(c.metadata), bg), Flea.Theme.textContrast)
        shell.atLeast(name, "metadata text", "surface", Contrast.ratio(String(c.metadata), surface), Flea.Theme.textContrast)
        shell.atLeast(name, "muted frame", "background", Contrast.ratio(String(c.mutedFrame), bg), Flea.Theme.frameContrast)
        // The error ink is never the quieter of the two beside a caption, on each ground it draws on.
        shell.atLeast(name, "error against the caption", "background", Contrast.ratio(String(c.error), bg), Contrast.ratio(String(c.muted), bg))
        shell.atLeast(name, "status error against the caption", "surface", Contrast.ratio(String(c.errorOnSurface), surface), Contrast.ratio(String(c.muted), surface))
        // A secondary dialog button's rest frame is the frame role; the sunset fixture sets a muted that the frame and text lifts take apart, so a frame left on muted is red there.
        shell.check(name + " secondary dialog button frame draws mutedFrame", Probe.read(secondaryButton).frameColor, rgba(c.mutedFrame))
        if (name === "sunset") shell.check("sunset: the frame role and the text role differ", rgba(c.mutedFrame) !== rgba(c.muted), true)
        // A resting list row draws its metadata cells in the lifted ink and its file glyph in the unlifted step, so no glyph moves.
        shell.check(name + " resting row metadata ink", rgba(restRow.cellInk), rgba(c.metadata))
        shell.check(name + " resting row file glyph", rgba(restRow.markColor()), rgba(c.metadataFrame))
        shell.glassSweep(name, c)
        shell.liftSweep(name, c)
        shell.log("floor list ground " + name + " " + shell.listFloor(c) + "/255")
        shell.pickedSweep(name, {background: String(c.background), surface: String(c.surface), foreground: String(c.foreground), muted: String(c.muted), metadata: String(c.metadata)})
    }

    function run() {
        var inside = shell.rootDir.length > 0 && (Quickshell.env("HOME") || "").indexOf(shell.rootDir + "/") === 0
        shell.check("sandbox: HOME lies under the harness root", inside, true)
        if (!inside) { shell.finish(); return }
        shell.check("the sweep reads the installed stock palettes and two fixtures", Palettes.PALETTES.length, shell.paletteCount)
        shell.check("a wash ground keeps the colour's own alpha", shell.groundOf({ color: Qt.rgba(1, 1, 1, 0.5), opacity: 0.5 }, "#000000"), "#404040")
        for (var i = 0; i < Palettes.PALETTES.length; i++)
            shell.sweep(Palettes.PALETTES[i])
        var want = Math.round(Flea.Glass.glassAlpha * shell.glassSteps)
        shell.log("floor all palettes " + shell.glassFloor + "/255 (" + (Math.round(shell.glassFloor / shell.glassSteps * 1000) / 1000) + "), glass alpha " + want + "/255")
        shell.finish()
    }

    Timer {
        interval: shell.tickMs
        repeat: true
        running: true
        onTriggered: {
            if (Date.now() - shell.startedAt > shell.capMs) {
                shell.failures.push("the harness did not finish")
                shell.log("FAIL the harness did not finish")
                shell.finish()
                running = false
                return
            }
            shell.ticks += 1
            if (shell.ticks === shell.settleTicks) { running = false; shell.run() }
        }
    }
}
