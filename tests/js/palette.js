.import "../../ui/js/Palette.js" as Palette

// Lines of the live vernier colors.toml, verbatim, so the suite parses what the box parses.
var VERNIER = "# Vernier: the surface ladder, measured and regularised.\n"
    + "\n"
    + "mode = \"dark\"\n"
    + "accent = \"#b38956\"\n"
    + "selection = \"#31363a\"   # the selection highlight; foreground on it is 7.56:1\n"
    + "dark_background    = \"#0e1112\"   # chrome: bar, sidebars\n"
    + "background         = \"#14181a\"   # canvas: terminal, app bodies\n"
    + "cyan    = \"#27a6a2\"\n"
    + "hyprland_inactive_border = \"rgb(1b1f21)\"\n"

// A theme with no chrome plane, which is the missing-dark_background case ThemeRoles.html names:
// "background is the neutral fallback when dark_background is absent".
var NO_DARK_BACKGROUND = "mode = \"dark\"\n"
    + "accent = \"#7aa2f7\"\n"
    + "selection = \"#292e42\"\n"
    + "background = \"#1a1b26\"\n"
    + "foreground = \"#a9b1d6\"\n"

// Alacritty-derived palettes carry the ANSI ring and no background ladder key at all, which is the
// one case the third rung answers; see AGENTS.md "Theme roles and sources".
var NEITHER_LADDER_KEY = "mode = \"dark\"\n"
    + "accent = \"#7aa2f7\"\n"
    + "selection = \"#292e42\"\n"
    + "color2 = \"#9ece6a\"\n"
    + "color6 = \"#449dab\"\n"

// ui/Theme.qml's applyColors picks colour.surface with this exact array, so these cases read the
// app's own ladder rather than a copy of it that could stay green over a wrong one.
var SURFACE = Palette.SURFACE_KEYS
var SURFACE_FALLBACK = "#181825"

// Lines of the stock tokyo-night colors.toml, verbatim, the shape every theme installed here has.
var TOKYO_NIGHT = "mode = \"dark\"\n"
    + "accent = \"#7aa2f7\"\n"
    + "selection = \"#292e42\"\n"
    + "background = \"#1a1b26\"\n"
    + "dark_background = \"#13141c\"\n"
    + "darker_background = \"#0e0e14\"\n"
    + "foreground = \"#a9b1d6\"\n"

// Theme.applyColors reads isPalette as "the file was there and it meant something", and only that.
function run(check) {
    check("no file at all is not a palette", Palette.isPalette(Palette.parse("")), false)
    check("an unreadable file reads as no palette", Palette.isPalette(Palette.parse(undefined)), false)
    check("comments and blank lines alone are not a palette",
          Palette.isPalette(Palette.parse("# Vernier\n\n   \n")), false)
    check("a toml with no colour in it is not a palette",
          Palette.isPalette(Palette.parse("mode = \"dark\"\nname = \"vernier\"\n")), false)

    check("the live theme is a palette", Palette.isPalette(Palette.parse(VERNIER)), true)
    // Not stricter than the truth: a theme that sets one role is still a parsed theme.
    check("one colour is already a palette",
          Palette.isPalette(Palette.parse("accent = \"#b38956\"\n")), true)
    check("a palette naming no role Flea models is still a palette",
          Palette.isPalette(Palette.parse("wallpaper_tint = \"#1b1f21\"\n")), true)

    var live = Palette.parse(VERNIER)
    check("the live fixture yields five colours", Object.keys(live).length, 5)
    check("the surface ladder is ThemeRoles.html's own three rungs, in its order",
          Palette.SURFACE_KEYS.join(","), "dark_background,background,selection")
    check("dark_background is the surface role",
          Palette.pick(live, SURFACE, SURFACE_FALLBACK), "#0e1112")
    // The live-theme guard on the insert: every theme installed here sets all three keys, and the
    // chrome plane has to keep winning for all of them.
    check("a theme that sets both ladder keys still takes the chrome one",
          Palette.pick(Palette.parse(TOKYO_NIGHT), SURFACE, SURFACE_FALLBACK), "#13141c")
    check("background is the surface role when dark_background is absent",
          Palette.pick(Palette.parse(NO_DARK_BACKGROUND), SURFACE, SURFACE_FALLBACK), "#1a1b26")
    check("selection answers only when neither ladder key is set",
          Palette.pick(Palette.parse(NEITHER_LADDER_KEY), SURFACE, SURFACE_FALLBACK), "#292e42")
    check("a body with none of the three keeps Flea's own surface",
          Palette.pick(Palette.parse("accent = \"#7aa2f7\"\n"), SURFACE, SURFACE_FALLBACK), SURFACE_FALLBACK)
    check("cyan is the symlink role", Palette.pick(live, ["cyan", "color6"], "#94e2d5"), "#27a6a2")
    // green is in the real file but not in this fixture, which is the per-role fallback pick() owns.
    check("a role the body never set keeps its fallback",
          Palette.pick(live, ["green", "color2"], "#a6e3a1"), "#a6e3a1")
    check("an rgb() value is not a hex colour and is skipped",
          live["hyprland_inactive_border"], undefined)
}
