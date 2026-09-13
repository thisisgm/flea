.import "../../ui/js/Thumbs.js" as Thumbs

// A held window starting at 10, four rows, the third of which no thumbnailer declares.
function heldRows() {
    return [{ n: "a", t: true }, { n: "b", t: true }, { n: "c", t: false }, { n: "d", t: true }]
}

function run(check) {
    check("off skips every thumbnail source", Thumbs.allowed({ t: true, i: "image-x-generic" }, "off"), false)
    check("images excludes video sources", Thumbs.allowed({ t: true, i: "video-x-generic" }, "images"), false)
    check("images retains image sources", Thumbs.allowed({ t: true, i: "image-x-generic" }, "images"), true)
    check("media retains eligible video sources", Thumbs.allowed({ t: true, i: "video-x-generic" }, "media"), true)
    var policyRows = [{ t: true, i: "image-x-generic" }, { t: true, i: "video-x-generic" }, { t: true, i: "video-x-generic" }]
    var pending = Thumbs.applied(Thumbs.empty(), {ask: [400, 401, 402], drop: []})
    pending = Thumbs.remember(pending, 402, "/cache/402.png", 240)
    var imagesOnly = Thumbs.plan(pending, policyRows, 400, 400, 402, "images")
    check("switching to images cancels pending visible video without cancelling images or completed work", imagesOnly.drop.join(","), "401")
    pending = Thumbs.applied(pending, imagesOnly)
    var disabled = Thumbs.plan(pending, policyRows, 400, 400, 402, "off")
    check("switching off cancels the remaining pending visible image", disabled.drop.join(","), "400")
    check("switching off never starts replacement work", disabled.ask.length, 0)
    pending = Thumbs.applied(pending, disabled)
    check("policy changes preserve completed cached thumbnails", Thumbs.fileFor(pending, 402), "/cache/402.png")
    check("reenabling media requests cancelled work without regenerating completed work",
          Thumbs.plan(pending, policyRows, 400, 400, 402, "media").ask.join(","), "400,401")
    var rows = heldRows()
    var s = Thumbs.empty()

    var work = Thumbs.plan(s, rows, 10, 10, 13)
    check("only thumbnailable rows are asked for", work.ask.join(","), "10,11,13")
    check("nothing is cancelled on a first pass", work.drop.length, 0)

    s = Thumbs.applied(s, work)
    check("a row already asked is not asked twice", Thumbs.plan(s, rows, 10, 10, 13).ask.length, 0)
    check("a row asked and still waiting reports no file", Thumbs.fileFor(s, 10), "")

    work = Thumbs.plan(s, rows, 10, 12, 13)
    check("a row the viewport left is cancelled", work.drop.join(","), "10,11")
    check("and a row still visible is not", work.ask.length, 0)

    s = Thumbs.applied(s, work)
    check("a cancelled row leaves the order too", s.order.length, 1)
    check("a cancelled row can be asked for again", Thumbs.plan(s, rows, 10, 10, 13).ask.join(","), "10,11")

    s = Thumbs.remember(s, 13, "/cache/13.png", 240)
    check("an answered row reports its file", Thumbs.fileFor(s, 13), "/cache/13.png")
    check("and is never asked for again", Thumbs.plan(s, rows, 10, 13, 13).ask.length, 0)
    check("and is never cancelled, having no job to drop", Thumbs.plan(s, rows, 10, 0, 1).drop.indexOf(13), -1)

    s = Thumbs.remember(s, 11, "", 240)
    check("a row answered with no thumbnail reports nothing", Thumbs.fileFor(s, 11), "")
    check("and is never asked again either", Thumbs.plan(s, rows, 10, 11, 11).ask.length, 0)

    check("an unknown row reports nothing", Thumbs.fileFor(Thumbs.empty(), 4), "")
    check("a row outside the held window is not asked for", Thumbs.plan(Thumbs.empty(), rows, 10, 0, 3).ask.length, 0)
    // Held at 0, so the rows above the viewport are in memory and only the lower bound keeps them out.
    check("a row above the viewport is not asked for", Thumbs.plan(Thumbs.empty(), rows, 0, 2, 3).ask.join(","), "3")

    // The cap bounds a policy bug: normal use only ever records the rows a viewport held.
    var capped = Thumbs.empty()
    for (var i = 0; i < 5; i++) {
        capped = Thumbs.remember(capped, i, "/cache/" + i + ".png", 3)
    }
    check("the cap evicts the oldest entry", Thumbs.fileFor(capped, 0), "")
    check("and keeps the newest", Thumbs.fileFor(capped, 4), "/cache/4.png")
    check("and holds exactly the cap", capped.order.length, 3)

    capped = Thumbs.remember(capped, 4, "/cache/4b.png", 3)
    check("answering one row twice does not grow the order", capped.order.length, 3)
    check("and the newer answer wins", Thumbs.fileFor(capped, 4), "/cache/4b.png")

    check("each empty state is its own", Thumbs.empty() === Thumbs.empty(), false)

    // A refusal and a row still in flight both read as no file, and the preview column has to draw
    // them differently: one is a placeholder, the other is the whole report a network mount gets.
    var r = Thumbs.empty()
    r = Thumbs.applied(r, Thumbs.plan(r, rows, 10, 10, 11))
    check("a row still waiting is not a refusal", Thumbs.refused(r, 10), false)
    check("a row never asked about is not a refusal", Thumbs.refused(r, 99), false)
    r = Thumbs.remember(r, 10, "", 8)
    check("an empty answer is a refusal", Thumbs.refused(r, 10), true)
    check("and it still reads as no file", Thumbs.fileFor(r, 10), "")
    r = Thumbs.remember(r, 11, "/cache/b.png", 8)
    check("a real answer is not a refusal", Thumbs.refused(r, 11), false)

    // The viewport is the only range a request may name, so its clamp is the no-sweep rule in arithmetic.
    var v = Thumbs.viewport(0, 37, 36, 2000)
    check("a viewport at the top starts at row zero", v.first + "," + v.last, "0,35")
    v = Thumbs.viewport(370, 37, 36, 2000)
    check("a scrolled viewport starts at the row under contentY", v.first + "," + v.last, "10,45")
    v = Thumbs.viewport(0, 37, 36, 4)
    check("a listing shorter than the screen clamps to its last row", v.first + "," + v.last, "0,3")
    v = Thumbs.viewport(18, 37, 36, 2000)
    check("a part-scrolled row counts as the first visible one", v.first, 0)
    // contentY 21 of a 37 px row leaves a 16 px sliver of row 36 on screen with visibleRows 36.
    v = Thumbs.viewport(21, 37, 36, 1000)
    check("a part-scrolled window reaches the row its bottom edge straddles", v.first + "," + v.last, "0,36")
    v = Thumbs.viewport(21, 37, 36, 30)
    check("and that straddled row is still clamped to the last row of the listing", v.last, 29)
}
