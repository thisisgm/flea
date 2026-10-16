.import "../../ui/js/Kinds.js" as Kinds
.import "../../ui/js/Format.js" as Format
.import "../../ui/js/Ops.js" as Ops

// Quick Look's font page: the facts line the board draws, with each part a font may lack, and the kind rule that routes a file to it.
function run(check) {
    function line(family, style, bytes, glyphs) { return Kinds.fontLine(family, style, bytes, glyphs, Ops.pluralWord) }
    var size = Format.size(312000)
    var dot = " \u00b7 "
    check("a whole font reads family, style, size and the grouped glyph count",
          line("Inter", "Bold", 312000, 2548), "Inter" + dot + "Bold" + dot + size + dot + "2,548 glyphs")
    check("one glyph is singular", line("Inter", "Bold", 312000, 1), "Inter" + dot + "Bold" + dot + size + dot + "1 glyph")
    check("a font with no family drops it", line("", "Bold", 312000, 2548), "Bold" + dot + size + dot + "2,548 glyphs")
    check("a font with no style drops it", line("Inter", "", 312000, 2548), "Inter" + dot + size + dot + "2,548 glyphs")
    check("a font with no glyph count drops it", line("Inter", "Bold", 312000, 0), "Inter" + dot + "Bold" + dot + size)
    check("a font with no tables states its size alone and never Unknown", line(undefined, undefined, 312000, undefined), size)
    check("a four-figure count is grouped", line("A", "B", 1000, 1204), "A" + dot + "B" + dot + Format.size(1000) + dot + "1,204 glyphs")
    check("the sample is the board's sentence at the board's three sizes",
          Kinds.SAMPLE_TEXT + "|" + Kinds.SAMPLE_SIZES.join(","), "The quick brown fox jumps over the lazy dog|22,15,12")
    check("TrueType, OpenType and collections open the font page, whatever the case",
          ["a.ttf", "A.OTF", "b.ttc", "x.tar.ttf"].map(function (n) { return Kinds.quickLookKind("font-x-generic", n) }).join("|"),
          "font|font|font|font")
    check("WOFF, WOFF2 and Type 1 open management; a bare extension remains unsupported",
          ["a.woff", "a.woff2", "a.pfb", "ttf", ".ttf"].map(function (n) { return Kinds.quickLookKind("font-x-generic", n) }).join("|"),
          "font|font|font|unsupported|font")
    check("only the upstream formats use the isolated sample", ["a.ttf", "a.otf", "a.ttc", "a.woff", "a.pfb"].map(Kinds.isSampleFont).join("|"), "true|true|true|false|false")
}
