.import "../../ui/js/Palette.js" as Palette
.import "../../ui/js/Contrast.js" as Contrast
.import "sourcefixture.js" as Source

// Frozen stock palettes keep the pure contrast checks independent of the builder's Omarchy version.
var THEMES = ["catppuccin", "catppuccin-latte", "ethereal", "everforest", "flexoki-light", "gruvbox",
              "hackerman", "jade-horse", "kanagawa", "last-horizon", "lumon", "lupine", "matte-black", "miasma",
              "nord", "retro-82", "ristretto", "rose-pine", "solitude", "starship", "tokyo-night",
              "vantablack", "white"]
var FIXTURE = "tests/fixtures/theme-palettes.json"

// Every threshold carries the rule it answers to; none of them is fitted to what the palettes do.
var TEXT_MIN = 4.5          // HANDOFF rule 2: body text on its own ground, WCAG AA.
var DISABLED_MIN = 1.5      // Containers: a disabled row still reads as a row, never as an empty one.
var WASH_MIN = 1.06         // A 14 percent wash has to be visible against the ground it washes.
var MARK_MIN = 3.0          // A drawn mark or frame is a graphical object, AA at 3:1.
var EDGE_MIN = 1.5          // The cursor's accent edge borders the unselected ground, so a low-chroma theme keeps it.
var WASH = 0.14             // Theme.washActive, the one wash strength on every surface.
var SELECTED_FILL = 0.18    // Style.selectedFillAlpha, the OEM default a theme's shell.toml may raise.
var DISABLED = 0.55         // Theme.disabledOpacity.

// gvfs-free file read: qml6 allows it only with QML_XHR_ALLOW_FILE_READ, which tests/js.sh sets.
// Measured on this box: a file that reads answers status 200, and one that is not there answers 0.
function read(url) {
    var request = new XMLHttpRequest()
    request.open("GET", url, false)
    request.send()
    return { body: String(request.responseText || ""), status: request.status }
}

// Commons/Color.qml's own mapping from colors.toml, key for key, plus the roles ui/Theme.qml derives
// on top of it; tests/themes.sh reads the running window, which is what catches this mirror drifting.
function roles(body) {
    var found = Palette.parse(body)
    var foreground = Palette.pick(found, ["foreground", "color7"], "#cacccc")
    var background = Palette.pick(found, ["background", "color0"], "#101315")
    var accent = Palette.pick(found, ["accent", "color4"], "#cacccc")
    var urgent = Palette.pick(found, ["red", "color1"], "#a55555")
    var surface = Palette.pick(found, Palette.SURFACE_KEYS, "#181825")
    var mutedFrame = Contrast.ensureRatio(Palette.pick(found, ["muted"], darker(foreground)), background, MARK_MIN)
    var muted = Contrast.ensureRatio(Contrast.ensureRatio(mutedFrame, background, TEXT_MIN), surface, TEXT_MIN)
    var hued = saturation(urgent) > 0.2
    var error = hued ? Contrast.ensureRatio(urgent, background, Math.max(TEXT_MIN, Contrast.ratio(muted, background))) : foreground
    var errorOnSurface = hued ? Contrast.ensureRatio(urgent, surface, Math.max(TEXT_MIN, Contrast.ratio(muted, surface))) : foreground
    return { foreground: foreground, background: background, accent: accent, urgent: urgent,
             // ui/Theme.qml's rule: the palette's muted or a darkened foreground, lifted to the frame floor for frames, then to the text floor on both grounds for text.
             mutedFrame: mutedFrame,
             muted: muted,
             // Metadata ink: the darkened foreground, unlifted for a resting glyph and lifted like muted for text.
             metadataFrame: darker(foreground),
             metadata: Contrast.ensureRatio(Contrast.ensureRatio(darker(foreground), background, TEXT_MIN), surface, TEXT_MIN),
             // ui/Theme.qml: the heading ink is the palette's bright_foreground (color15 without one) when it has more contrast on the ground than the foreground.
             foregroundBright: brighter(foreground, Palette.pick(found, ["bright_foreground", "color15"], foreground), background),
             surface: surface,
             accentFrame: Contrast.ensureRatio(accent, surface, MARK_MIN),
             // ui/Theme.qml: a red with no chroma of its own reads as switched off, so it is dropped
             // for the foreground; every other red is lifted to AA the way symlink and executable are.
             error: error,
             // ui/Theme.qml's status bar role: the same red lifted on the surface it is drawn on.
             errorOnSurface: errorOnSurface }
}

// Qt.darker(c, 1.4) in the value channel, which is ui/Theme.qml's fallback when a palette sets no
// muted of its own. Every stock palette does set one, so this is the path no installed theme takes.
function darker(hex) {
    var c = Contrast.parse(hex)
    var hi = Math.max(c[0], Math.max(c[1], c[2]))
    if (hi <= 0)
        return hex
    var scale = (hi / 1.4) / hi
    return Contrast.hexOf([c[0] * scale, c[1] * scale, c[2] * scale])
}

function brighter(foreground, candidate, background) {
    return Contrast.ratio(candidate, background) > Contrast.ratio(foreground, background) ? candidate : foreground
}

function saturation(hex) {
    var c = Contrast.parse(hex)
    var hi = Math.max(c[0], Math.max(c[1], c[2]))
    var lo = Math.min(c[0], Math.min(c[1], c[2]))
    return hi <= 0 ? 0 : (hi - lo) / hi
}

function washed(colour, alpha, ground) {
    return Contrast.hexOf(Contrast.over(colour, alpha, ground))
}

// No slack: ui/js/Contrast.js delivers the ratio it was asked for after rounding, so a lift that lands
// at 2.99 for a requested 3 is the defect this suite exists to catch rather than a tolerance to grant.
function atLeast(check, theme, rule, got, floor) {
    // Four decimals, because a lift that lands at 2.9996 prints as 3.00 and reads as a false failure.
    check(theme + ": " + rule + " is " + got.toFixed(4) + ", at least " + floor.toFixed(2),
          got >= floor ? "ok" : "under " + floor.toFixed(2) + " at " + got.toFixed(4), "ok")
}

// The two palette shapes no installed theme has: one setting no muted at all, and one setting color8
// instead. ui/Theme.qml takes neither as the caption, so the mirror must not either.
function mirrorFallbacks(check) {
    var ground = "background = \"#1e1e2e\"\nforeground = \"#cdd6f4\"\naccent = \"#89b4fa\"\nred = \"#f38ba8\"\n"
    var none = roles(ground)
    // The value tests/themes.sh measured off the running window for this exact palette, which is what
    // binds this mirror's darker() to ui/Theme.qml's own Qt.darker(c, 1.4).
    check("a palette with no muted takes the darkened foreground the window renders", none.muted, "#9299ae")
    atLeast(check, "a palette with no muted", "its derived caption still clears the floor",
            Contrast.ratio(none.muted, none.background), TEXT_MIN)
    var eight = roles(ground + "color8 = \"#585b70\"\n")
    check("and color8 is not the caption either, because ui/Theme.qml does not read it",
          eight.muted, none.muted)
}

// The heading ink: no bright key keeps the foreground, a dimmer one keeps it too, a brighter one wins; color15 stands in for an ANSI-ring palette.
function brightSamples(check) {
    var applied = Source.source("ui/Theme.qml") + Source.source("ui/ThemeColors.qml")
    check("Theme.qml derives the heading ink as color.foregroundBright from bright_foreground",
          applied.indexOf("foregroundBright") >= 0 && applied.indexOf('["bright_foreground", "color15"]') >= 0, true)
    var dark = "background = \"#1a1b26\"\nforeground = \"#a9b1d6\"\n"
    check("a palette with no bright key keeps the foreground", roles(dark).foregroundBright, "#a9b1d6")
    check("a bright_foreground with less contrast keeps the foreground", roles(dark + "bright_foreground = \"#445066\"\n").foregroundBright, "#a9b1d6")
    check("Tokyo Night's bright_foreground beats its foreground, the board's #c0caf5", roles(dark + "bright_foreground = \"#c0caf5\"\n").foregroundBright, "#c0caf5")
    check("an ANSI-ring palette's color15 stands in when it sets no bright_foreground", roles(dark + "color15 = \"#c0caf5\"\n").foregroundBright, "#c0caf5")
    check("bright_foreground wins over color15 when both are set", roles(dark + "color15 = \"#ffffff\"\nbright_foreground = \"#c0caf5\"\n").foregroundBright, "#c0caf5")
    var light = "background = \"#eff1f5\"\nforeground = \"#4c4f69\"\n"
    check("on a light ground a lighter bright_foreground loses to the foreground", roles(light + "bright_foreground = \"#bcc0cc\"\n").foregroundBright, "#4c4f69")
}

// The status bar draws error ink on the surface, where the background lift lands under 4.5.
var ERROR_ON_SURFACE_SAMPLES = [
    "background = \"#eff1f5\"\ndark_background = \"#e6e9ef\"\nforeground = \"#4c4f69\"\nred = \"#d20f39\"\n",
    "background = \"#faf4d6\"\ndark_background = \"#f0e9d0\"\nforeground = \"#4c4f69\"\nred = \"#b4637a\"\n",
    "background = \"#eef0f5\"\ndark_background = \"#dfe3ec\"\nforeground = \"#4c4f69\"\nred = \"#c2435c\"\n",
    "background = \"#f5f0e8\"\ndark_background = \"#e8dfd0\"\nforeground = \"#4c4f69\"\nred = \"#a8435f\"\n"
];

// Sample input: one palette body above; the bar's own ground is the surface it sits on.
function errorOnSurfaceSamples(check) {
    var applied = Source.source("ui/Theme.qml")
    check("Theme.qml lifts the status bar red on its surface", applied.indexOf("Contrast.ensureRatio(Commons.Color.urgent, surface,") >= 0, true)
    var weak = 0
    for (var i = 0; i < ERROR_ON_SURFACE_SAMPLES.length; i++) {
        var r = roles(ERROR_ON_SURFACE_SAMPLES[i])
        atLeast(check, "surface sample " + i, "error on surface", Contrast.ratio(r.errorOnSurface, r.surface), TEXT_MIN)
        if (Contrast.ratio(Contrast.ensureRatio(r.urgent, r.background, TEXT_MIN), r.surface) < TEXT_MIN)
            weak += 1
    }
    check("the background lift alone lands under 4.5 on at least one surface", weak >= 1, true)
}

function run(check) {
    mirrorFallbacks(check)
    brightSamples(check)
    errorOnSurfaceSamples(check)
    var palettes = JSON.parse(Source.source(FIXTURE)).palettes
    check("the frozen table retains all 23 palette cases", THEMES.length, 23)
    check("the frozen inventory matches every palette case",
          Object.keys(palettes).sort().join(","), THEMES.slice().sort().join(","))
    for (var i = 0; i < THEMES.length; i++) {
        var name = THEMES[i]
        var body = palettes[name] || ""
        check(name + ": frozen colors.toml is present", body.length > 0, true)
        check(name + ": colors.toml parses to a palette", Palette.isPalette(Palette.parse(body)), true)
        // A theme that did not read is one red check, not a throw that leaves the rest unmeasured.
        if (body.length === 0)
            continue
        var r = roles(body)

        // Text, on both grounds a listing row can sit on.
        atLeast(check, name, "foreground on background", Contrast.ratio(r.foreground, r.background), TEXT_MIN)
        atLeast(check, name, "foreground on surface", Contrast.ratio(r.foreground, r.surface), TEXT_MIN)
        atLeast(check, name, "muted text on background", Contrast.ratio(r.muted, r.background), TEXT_MIN)
        atLeast(check, name, "muted text on surface", Contrast.ratio(r.muted, r.surface), TEXT_MIN)
        atLeast(check, name, "metadata text on background", Contrast.ratio(r.metadata, r.background), TEXT_MIN)
        atLeast(check, name, "metadata text on surface", Contrast.ratio(r.metadata, r.surface), TEXT_MIN)
        atLeast(check, name, "muted frame on background", Contrast.ratio(r.mutedFrame, r.background), MARK_MIN)

        // The heading ink never reads weaker than the body ink it sits over.
        atLeast(check, name, "heading ink against the foreground",
                Contrast.ratio(r.foregroundBright, r.background), Contrast.ratio(r.foreground, r.background))

        if (name === "tokyo-night")
            check(name + ": the heading ink is the board's #c0caf5 over the body's " + r.foreground, r.foregroundBright, "#c0caf5")

        // Disabled is muted at 0.55 on the ground: still legible, and visibly weaker than muted.
        var disabled = washed(r.muted, DISABLED, r.background)
        atLeast(check, name, "disabled on background", Contrast.ratio(disabled, r.background), DISABLED_MIN)
        check(name + ": disabled reads below muted",
              Contrast.ratio(disabled, r.background) < Contrast.ratio(r.muted, r.background), true)

        // The one wash, on both of its grounds, and the search match run drawn with the same recipe.
        atLeast(check, name, "accent wash clears background", Contrast.ratio(washed(r.accent, WASH, r.background), r.background), WASH_MIN)
        atLeast(check, name, "accent wash clears surface", Contrast.ratio(washed(r.accent, WASH, r.surface), r.surface), WASH_MIN)
        atLeast(check, name, "a matched run still reads on its wash",
                Contrast.ratio(r.foreground, washed(r.accent, WASH, r.background)), TEXT_MIN)

        // The checkbox draws in the two roles above and nothing of its own: a foreground fill with the
        // ground cut out for its tick, and the muted frame when empty, so it needs no rule here.

        // The error role on its ground, and beside the drawn caption it is never the quieter of: ethereal's caption lands at 4.90.
        atLeast(check, name, "error on background", Contrast.ratio(r.error, r.background), TEXT_MIN)
        atLeast(check, name, "error is never dimmer than a caption",
                Contrast.ratio(r.error, r.background), Contrast.ratio(r.muted, r.background))
        atLeast(check, name, "error on surface is never dimmer than a caption",
                Contrast.ratio(r.errorOnSurface, r.surface), Contrast.ratio(r.muted, r.surface))

        // The cursor's edge borders the unselected ground, which is what a low-chroma theme breaks.
        atLeast(check, name, "cursor edge on the unselected ground", Contrast.ratio(r.accent, r.background), EDGE_MIN)
        atLeast(check, name, "cursor edge against the selected fill",
                Contrast.ratio(r.accent, washed(r.accent, SELECTED_FILL, r.background)), EDGE_MIN)

        // A pane without focus keeps its marked rows in the selected rung: above the ground and the hover rung, below the focused selection.
        var inactive = washed(r.foreground, SELECTED_FILL, r.background)
        atLeast(check, name, "unfocused selection clears the background", Contrast.ratio(inactive, r.background), WASH_MIN)
        atLeast(check, name, "unfocused selection clears the hover rung",
                Contrast.ratio(inactive, washed(r.foreground, HOVER_FILL, r.background)), WASH_MIN)
        atLeast(check, name, "unfocused selection clears the focused selection",
                Contrast.ratio(inactive, washed(r.foreground, SELECTION_FILL, r.background)), WASH_MIN)
        atLeast(check, name, "unfocused cursor edge against the unfocused selection", Contrast.ratio(r.muted, inactive), EDGE_MIN)

        // The primary button carries its identity in a frame, on the card's own surface.
        atLeast(check, name, "primary frame on surface", Contrast.ratio(r.accentFrame, r.surface), MARK_MIN)
    }
    // Measured: while the walk bounded red alone, 46 colour and ground pairs with a channel at 0 or 255
    // came back under the floor, this one at 2.9996 for a requested 3.
    atLeast(check, "a channel at the extreme", "the lift toward black still delivers the floor",
            Contrast.ratio(Contrast.ensureRatio("#0099ff", "#ffffff", MARK_MIN), "#ffffff"), MARK_MIN)
    // The other direction, where the bounded channel is the one already at the extreme: measured at
    // 4.4943 for a requested 4.5, on the ground catppuccin actually ships.
    atLeast(check, "a channel at the extreme", "the lift toward white still delivers the floor",
            Contrast.ratio(Contrast.ensureRatio("#ff0000", "#1e1e2e", TEXT_MIN), "#1e1e2e"), TEXT_MIN)
    tagThemeSamples(check)
}

var HOVER_FILL = 0.08       // Style.hoverFillAlpha, the OEM default.
var SELECTION_FILL = 0.35   // Style.selectionFillAlpha, the OEM default.

// Tag swatches lift to 3:1 on both grounds with the digit fallback decided once per theme load.
function tagThemeSamples(check) {
    var latte = { red: "#d20f39", orange: "#d84e2b", yellow: "#df8e1d", green: "#40a02b",
                  blue: "#1e66f5", magenta: "#ea76cb", dark_foreground: "#9ca0b0" }
    var matte = { red: "#d35f5f", orange: "#c63d3d", yellow: "#b91c1c", green: "#ffc107",
                  blue: "#e68e0d", magenta: "#d35f5f", dark_foreground: "#555555" }
    var white = { red: "#2a2a2a", orange: "", yellow: "#4a4a4a", green: "#3a3a3a",
                  blue: "#1a1a1a", magenta: "#2e2e2e", dark_foreground: "#c0c0c0" }
    var bg = "#eff1f5", surface = "#e6e9ef"
    var t = Contrast.tagTheme(latte, bg, surface)
    check("catppuccin latte reads in colour", t.fallback, false)
    check("catppuccin latte names every tag on both grounds",
          Object.keys(t.colors).length + "|" + Object.keys(t.menuColors).length, "7|7")
    check("catppuccin latte lifts Red to 3:1 on the background",
          Contrast.ratio(t.colors.Red, bg) >= MARK_MIN, true)
    check("catppuccin latte lifts Gray to 3:1 on the surface",
          Contrast.ratio(t.menuColors.Gray, surface) >= MARK_MIN, true)
    check("matte black falls back to digits", Contrast.tagTheme(matte, "#121212", "#1a1a1a").fallback, true)
    check("white falls back to digits", Contrast.tagTheme(white, "#ffffff", "#f2f2f2").fallback, true)
    check("a missing colour falls back", Contrast.tagTheme({
        red: "#d20f39", orange: "", yellow: "#df8e1d", green: "#40a02b",
        blue: "#1e66f5", magenta: "#ea76cb", dark_foreground: "#9ca0b0" }, bg, surface).fallback, true)
}
