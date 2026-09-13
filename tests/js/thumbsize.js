.import "../../ui/js/ThumbSize.js" as ThumbSize

function run(check) {
    check("six named stops, matching the schema and the settings row",
          ThumbSize.NAMES.join(","), "small,medium,large,xlarge,xxlarge,huge")
    check("pixels stay the four shipped sizes, then 192 and the 256 the backend already writes",
          ThumbSize.PIXELS.join(","), "48,64,96,128,192,256")
    check("the default is medium, which is also the schema default", ThumbSize.DEFAULT, "medium")
    check("extra large stays 128 so an existing ui.json does not jump", ThumbSize.pixels("xlarge"), 128)
    check("huge is the backend THUMB_SIZE, not an upscale", ThumbSize.pixels("huge"), 256)
    check("an unknown name falls back to medium rather than inventing a size",
          ThumbSize.parse("tiny"), "medium")
    check("and draws at medium's pixels", ThumbSize.pixels("nope"), 64)
    check("a step up from extra large is the new 192 stop", ThumbSize.step("xlarge", 1), "xxlarge")
    check("a step up from huge clamps", ThumbSize.step("huge", 1), "huge")
    check("a step down from small clamps", ThumbSize.step("small", -1), "small")
    check("reset lands on the schema default, not the floor", ThumbSize.DEFAULT, "medium")
    check("the settings caption is the pixel size, not the name",
          ThumbSize.caption("huge"), "256 px")
    check("the chord announces the size with the panel shut",
          ThumbSize.announce("xxlarge"), "Thumbnail size 192 px.")
    check("labels stay wording, never the stored token",
          ThumbSize.LABELS.join(","), "Small,Medium,Large,Extra large,XX-large,Huge")
}
