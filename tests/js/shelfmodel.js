.import "../../shelf/Model.js" as Shelf
.import "../../shelf/Run.js" as Run

// The shelf plugin's own pure half. It lives here while the plugin lives in this tree; it moves with
// shelf/ when it is split into its own repository, which is why it imports across rather than up.

function run(check) {
    check("nothing read yet is an empty pile rather than an undefined one",
          Shelf.empty().count + "|" + Shelf.empty().items.length + "|" + Shelf.empty().ok, "0|0|false")

    var one = Shelf.parse('{"items":[{"path":"/home/gm/Work/a.txt","bytes":1024}]}')
    check("a pile of one names its row from the path", one.items[0].name, "a.txt")
    check("and carries the bytes the writer answered", one.items[0].bytes, 1024)
    check("and counts what it holds", one.count + "|" + one.ok, "1|true")

    var folder = Shelf.parse('{"items":[{"path":"/home/gm/Work/","folder":true}]}')
  check("and a pile entry says whether it is pinned", folder.items[0].pinned, false)
    check("a folder is named by its own leaf and wears its separator", folder.items[0].name, "Work/")
    check("and a size the writer did not answer is not a zero", folder.items[0].bytes, -1)

    // The file is written by another process, so every one of these is a real state the bar can read.
    check("a file that is not there at all is the empty pile", Shelf.parse("").count, 0)
    check("a file caught half written is the empty pile", Shelf.parse('{"items":[{"pa').ok, false)
    check("and so is one that parses into something else", Shelf.parse('"a string"').ok, false)
    check("an items list that is not a list is refused too", Shelf.parse('{"items":{}}').ok, false)

    var mixed = Shelf.parse('{"items":[{"path":"/a"},{"bytes":3},{"path":""},{"path":"/b/c"}]}')
    check("an entry with no path of its own is dropped, and the rest stand",
          mixed.count + "|" + mixed.items[0].path + "|" + mixed.items[1].path, "2|/a|/b/c")

    // Main rule 4: a size nobody has answered yet is one dot, and a walk cut short keeps its prefix.
    check("a size not yet answered is a dot", Shelf.sizeText(folder.items[0]), "\u00b7")
    check("a size that is known reads as Flea's own", Shelf.sizeText({ bytes: 4600000 }), "4.6 MB")
    check("and a walk cut short keeps its own prefix", Shelf.sizeText({ bytes: 138, partial: true }), ">138 B")

  // Main rule 4's budget: one request per drawn path, never twice, and never for a path off the card.
  var pile = Shelf.parse('{"items":[{"path":"/a","bytes":12},{"path":"/b/big","folder":true},{"path":"/c"}]}')
  var drawnRows = Shelf.rows(pile, [])
  check("the first ask is the first row the writer left without a size", Shelf.nextSize(drawnRows, {}), "/b/big")
  var answered = { "/b/big": { bytes: 4096, partial: true } }
  check("an answered path is never asked again, so the ask moves on", Shelf.nextSize(drawnRows, answered), "/c")
  answered["/c"] = { bytes: -1, partial: false }
  check("a path that could not be read is answered too, so nothing loops", Shelf.nextSize(drawnRows, answered), "")
  var drawn = Shelf.rows(pile, [], answered)
  check("the writer's own bytes stand where it had them", drawn[0].bytes, 12)
  check("the answer fills the row that had none, prefix and all", Shelf.sizeText(drawn[1]), ">4.1 kB")
  check("and a path that could not be read draws the same dot a pending one does", Shelf.sizeText(drawn[2]), "\u00b7")
  check("the drawn row keeps its own name", drawn.length + "|" + drawn[1].name, "3|big/")
  var pruned = Shelf.keep(answered, Shelf.rows(Shelf.parse('{"items":[{"path":"/c"}]}'), []))
  check("a size the card is no longer drawing is not kept",
        (pruned["/b/big"] === undefined) + "|" + pruned["/c"].bytes, "true|-1")

    // ShelfEmpty rules 2, 4, 5 and 7: the tray, what it says, and the routes the empty card names.
  var shots = Shelf.parseCaptures("1757890932000 /home/gm/Pictures/screenshot-2026-09-14_19-02-11.png\n"
                                  + "1757889660000 /home/gm/Videos/screenrecording-2026-09-14_18-41-00.mp4\n"
                                  + "bad line with no time\n")
  check("a capture line is its time and its path, and a line that is neither is dropped", shots.length, 2)
  check("a recording is known by its own extension and never decoded",
        shots[0].recording + "|" + shots[1].recording, "false|true")


  // The review round on shelf/*.js: four the reviewers found, each pinned by the case that failed.
  check("a name carrying a hash or a question mark survives becoming a uri",
        Shelf.uriPath("/home/gm/track #1?.mp3"), "/home/gm/track%20%231%3F.mp3")
  check("a uri with an escape that is not one still lands, and takes the rest of the drop with it",
        Shelf.pathsFromUris("file:///home/gm/50%.png\nfile:///home/gm/two.txt\n").join("|"),
        "/home/gm/50%.png|/home/gm/two.txt")
  check("the bar's tooltip counts the pile alone, the way the card does",
        Shelf.tooltip(Shelf.parse('{"items":[{"path":"/p/a"},{"path":"/p/b","pinned":true}]}')),
        "Flea shelf is holding 1 item")
  check("a bell that cannot be read is no reading, not a count of zero",
        Shelf.ringsOf("") + "|" + Shelf.ringsOf("{oh no") + "|" + Shelf.ringsOf('{"summon":0}'),
        "-1|-1|0")
  check("neither capture kind checked asks for no captures at all",
        Shelf.wantsCaptures({ screenshots: false, recordings: false, recent: 3 }) + "|"
        + Shelf.wantsCaptures({ screenshots: true, recordings: false, recent: 3 }) + "|"
        + Shelf.wantsCaptures({ screenshots: true, recordings: true, recent: 0 }),
        "false|true|false")
  check("a hand-edited count above the largest stop falls back to the default",
        Shelf.shelfOf('{"shelf":{"recent":100000}}').recent + "|"
        + Shelf.shelfOf('{"shelf":{"recent":5}}').recent, "3|5")
  check("a listing line keeps a path's own spaces, and drops only the line ending",
        Run.lines("/home/gm/two words \r\n\n /leading\n").join("|"),
        "/home/gm/two words | /leading")

  // Main rules 9, 10 and 12: one list of three sections, and the header counts the pile alone.
  var sections = Shelf.parse('{"items":[{"path":"/p/one"},{"path":"/p/pinned","pinned":true},{"path":"/p/two"}]}')
  var shots = [{ path: "/s/screenshot-a.png", at: 3, recording: false },
               { path: "/v/screenrecording-b.mp4", at: 2, recording: true }]
  var list = Shelf.rows(sections, shots)
  check("the pile comes first, then the pins, then the captures",
        list.map(function (r) { return r.section }).join("|"),
        "pile|pile|pinned|capture|capture")
  check("a caption sits above the first row of a section and nowhere else",
        [Shelf.captionFor(list, 0), Shelf.captionFor(list, 1), Shelf.captionFor(list, 2),
         Shelf.captionFor(list, 3), Shelf.captionFor(list, 4)].join("|"),
        "||Pinned|Screenshots & recordings|")
  check("the captures caption names the kinds that are checked",
        Shelf.capturesCaption({ screenshots: true, recordings: false }) + "|"
        + Shelf.capturesCaption({ screenshots: false, recordings: true }),
        "Screenshots|Recordings")
  check("a capture row has no size of its own until one is answered",
        Shelf.sizeText(list[3]) + "|" + Shelf.sizeText(Shelf.rows(sections, shots, { "/s/screenshot-a.png": { bytes: 2048, partial: false } })[3]),
        "\u00b7|2.0 kB")
  check("the subset gesture reaches a capture row like any other",
        Shelf.actionPaths(Shelf.toggleChosen({}, "/v/screenrecording-b.mp4"), list).join("|"),
        "/v/screenrecording-b.mp4")
  check("with nothing chosen the five actions take the pile and its pins, never a capture",
        Shelf.actionPaths({}, list).join("|") + " / " + Shelf.wholeCount(list),
        "/p/one|/p/two|/p/pinned / 3")
  // Main rule 4: the thumbnail answers, path-addressed because the card has no listing to index.
  check("only the drawn rows with no answer yet are asked for",
        Shelf.thumbWanted(list, { "/p/one": "/c/one.png" }).join("|"),
        "/p/two|/p/pinned|/s/screenshot-a.png|/v/screenrecording-b.mp4")
  var answered = Shelf.thumbsFrom("/c/a.png\t/s/screenshot-a.png\nnone\t/p/one\n", {})
  check("an answer is kept by the path it names, and none means no thumbnail",
        Shelf.thumbFor(answered, list[3]) + "|[" + Shelf.thumbFor(answered, list[0]) + "]|["
        + Shelf.thumbFor(answered, list[1]) + "]",
        "/c/a.png|[]|[]")
  check("a run that answered for nothing still answers every path it was asked about",
        JSON.stringify(Shelf.thumbsNone(["/a", "/b"], { "/a": "/c/a.png" })),
        '{"/a":"/c/a.png","/b":""}')
  check("the strip's Pin is an unpin only when every chosen row is already pinned",
        Shelf.allPinned(["/p/pinned"], list) + "|" + Shelf.allPinned(["/p/one", "/p/pinned"], list)
        + "|" + Shelf.allPinned([], list),
        "true|false|false")
  check("a path that answered with nothing is asked again on the next open",
        JSON.stringify(Shelf.thumbsFound({ "/a": "/c/a.png", "/b": "" })),
        '{"/a":"/c/a.png"}')
  check("and a path already answered is not asked for again",
        Shelf.thumbWanted(list, answered).join("|"),
        "/p/two|/p/pinned|/v/screenrecording-b.mp4")
  check("a grab carries the chosen rows, or the row it started from",
        Shelf.carryPaths({}, list, 3).join("|") + " / "
        + Shelf.carryPaths(Shelf.toggleChosen({}, "/p/one"), list, 3).join("|"),
        "/s/screenshot-a.png / /p/one")
  check("a drag carrying a capture or a pinned row is a copy, whole-pile or chosen",
        [Shelf.dragMoves(["/p/one", "/p/two"], list, true),
         Shelf.dragMoves(["/p/one", "/v/screenrecording-b.mp4"], list, true),
         Shelf.dragMoves(["/p/pinned"], list, true),
         Shelf.dragMoves(Shelf.actionPaths({}, list), list, true),
         Shelf.dragMoves(["/p/one"], list, false)].join("|"),
        "true|false|false|false|false")

  // Rule 11 and directive 59: Flea's own settings, read and never written.
  check("the shelf's own settings come from Flea's file, with its defaults when they do not",
        JSON.stringify(Shelf.shelfOf("")) + "|" + JSON.stringify(Shelf.shelfDefaults()),
        JSON.stringify(Shelf.shelfDefaults()) + "|" + JSON.stringify(Shelf.shelfDefaults()))
  check("and a stored value this build cannot honour falls back to its default",
        JSON.stringify(Shelf.shelfOf('{"shelf":{"enabled":false,"bar":false,"rail":"diagonal","recent":2}}')),
        '{"enabled":false,"bar":false,"rail":"off","screenshots":true,"recordings":true,"recent":2}')
  check("an action's tooltip takes its key only while Flea's hints are on",
        Shelf.actionTip("Move", "m", false) + "|" + Shelf.actionTip("Move", "m", true),
        "Move|Move  m")
  check("key hints are off until Flea's own file says otherwise",
        Shelf.keyHintsOf("") + "|" + Shelf.keyHintsOf('{"keyHints":true}'), "false|true")
  var settings = Shelf.shelfOf('{"shelf":{"screenshots":false,"recent":1}}')
  check("a Shelf section that names one key leaves the others at their defaults",
        settings.screenshots + "|" + settings.recordings + "|" + settings.recent, "false|true|1")
  check("and a file that is not there at all is every default",
        Shelf.shelfOf("").recent + "|" + Shelf.shelfOf("").screenshots, "3|true")
  check("the kinds cross the command line as one word",
        Shelf.kindsArg({ screenshots: true, recordings: true }) + "|"
        + Shelf.kindsArg({ screenshots: false, recordings: true }), "both|recordings")

  // Summon: the bell, the cleared transient, and what the Recent piles rows say.
  check("a summon file nobody has written yet is no reading at all", Shelf.ringsOf(""), -1)
  check("and one that is half written is not a ring either", Shelf.ringsOf('{"summ'), -1)
  check("each write of the file is one more ring", Shelf.ringsOf('{"summon":7}'), 7)
  check("the cleared transient says how to get it back",
        Shelf.clearedText(4) + "|" + Shelf.clearedText(1),
        "Cleared 4 items \u00b7 z restores|Cleared 1 item \u00b7 z restores")
  check("clearing an empty shelf says nothing at all", Shelf.clearedText(0), "")

  // Keys board: z undoes, and the card reads back which of the two reversible things it undid.
  var said = Shelf.undone("move 4\n")
  check("the undo answer is what it reversed and how many", said.kind + "|" + said.count, "move|4")
  check("a verb that answered nothing is nothing undone",
        Shelf.undone("").kind + "|" + Shelf.undone("").count, "none|0")
  check("the card says the same thing about a reversed move the pane says",
        Shelf.undoneText("move", 4), "Undid the move")
  check("and names the pile when that is what came back",
        Shelf.undoneText("pile", 4) + "|" + Shelf.undoneText("pile", 1),
        "Put 4 items back|Put 1 item back")
  check("nothing to undo says so rather than claiming work",
        Shelf.undoneText("none", 0) + "|" + Shelf.undoneText("move", 0), "Nothing to undo|Nothing to undo")

  var kept = Shelf.parsePiles("1789426925000 4\n1789420000000 2\nnot a pile\n")
  check("a kept pile is its time and its count, and a line that is neither is dropped", kept.length, 2)
  var row = Shelf.pileText({ count: 4, at: 1789426925000 })
  check("a pile row says how many and when, in the one date format this project draws",
        row.slice(0, 10) + "|" + row.slice(10).length + "|" + row.charAt(14) + row.charAt(17) + row.charAt(23),
        "4 items \u00b7 |16|--:")
  check("a time that is not one leaves the stamp out rather than printing nonsense",
        Shelf.stamp("never"), "")
  check("and one item is one item", Shelf.pileText({ count: 1, at: 1789426925000 }).slice(0, 7), "1 item ")

  // EdgeRail: what a drop on the wall is offering, and what the header says while it hovers.
  var uris = Shelf.pathsFromUris("file:///home/gm/Pictures/one%20two.png\r\nfile:///tmp/a.txt\r\n")
  check("a uri list is read as paths, percent escapes and all", uris.join("|"),
        "/home/gm/Pictures/one two.png|/tmp/a.txt")
  check("a comment line and a foreign scheme are not paths",
        Shelf.pathsFromUris("# a comment\nhttps://example.com/x\nfile:///tmp/b\n").join("|"), "/tmp/b")
  check("and a drop carrying nothing is no paths at all", Shelf.pathsFromUris("").length, 0)

  // Keys: the subset gesture, and what the card says while one is being chosen.
  var four = Shelf.parse('{"items":[{"path":"/p/a"},{"path":"/p/b"},{"path":"/p/c"},{"path":"/p/d"}]}')
  var picked = Shelf.toggleChosen({}, "/p/b")
  check("v takes the cursor row, and a second v gives it back",
        Shelf.chosenCount(picked, four.items) + "|" + Shelf.chosenCount(Shelf.toggleChosen(picked, "/p/b"), four.items),
        "1|0")
  var ranged = Shelf.chooseRange(picked, four.items, 1, 3)
  check("shift-j takes everything it passes", Shelf.chosenCount(ranged, four.items), 3)
  check("ctrl-a takes all of them", Shelf.chosenCount(Shelf.chooseAll({}, four.items), four.items), 4)
  check("and a second ctrl-a gives them all back",
        Shelf.chosenCount(Shelf.chooseAll(Shelf.chooseAll({}, four.items), four.items), four.items), 0)
  // A chosen row that has left the shelf is not chosen, which is why the map is keyed by path.
  var gone = Shelf.parse('{"items":[{"path":"/p/a"}]}')
  check("a chosen row that left the pile counts for nothing", Shelf.chosenCount(ranged, gone.items), 0)
  check("every action is chosen-or-whole: the chosen ones in the pile's own order",
        Shelf.actionPaths(ranged, four.items).join("|"), "/p/b|/p/c|/p/d")
  check("and the whole pile when nothing is chosen",
        Shelf.actionPaths({}, four.items).join("|"), "/p/a|/p/b|/p/c|/p/d")

  // Actions: what an action says while it runs and when it lands.
  var run = Run.sampled(Run.idle(), '{"t":"transferstarted","id":0,"n":4,"moving":true}')
  check("a started transfer names its verb and how many", Run.runText(run), "Moving 1 of 4")
  run = Run.sampled(run, '{"t":"transferprogress","id":0,"index":1,"name":"plate-2.raw","bytes":40,"total":100,"scanned":0}')
  check("and a sample says which one, counted from one", Run.runText(run), "Moving 2 of 4")
  check("the bar is the fraction of the one in flight", Run.runFraction(run), 0.4)
  check("the footer carries the keyboard's own half", Run.runFooter(run), "Moving 2 of 4 \u00b7 esc cancels")
  check("a line that is not one of ours changes nothing", Run.runText(Run.sampled(run, "not json")), "Moving 2 of 4")
  check("and a done leaves nothing running",
        Run.sampled(run, '{"t":"transferdone","id":0,"ok":4,"failed":0,"skipped":0,"cancelled":false,"retryPaths":[]}').running,
        false)

  check("a clean move says where they went and that it can be undone",
        Run.movedText("Moved", 4, 0, 4, "drafts", ""), "Moved 4 items to drafts \u00b7 z undoes")
  check("a copy leaves the pile, so it offers no undo",
        Run.movedText("Copied", 4, 0, 4, "drafts", ""), "Copied 4 items to drafts")
  check("a partial failure is one sentence, not two lines",
        Run.movedText("Moved", 3, 1, 4, "drafts", "drafts is read-only for cover-grade.jpg"),
        "Moved 3 of 4 \u00b7 drafts is read-only for cover-grade.jpg")
  check("a run that died before its first item still says the copy did not happen",
        Run.movedText("Copied", 0, 0, 0, "drafts", "") + "|" + Run.runFailedText("Copied")
        + "|" + Run.runFailedText("Moved"),
        "|The items could not be copied.|The items could not be moved.")
  check("the other three actions say what they did",
        Run.zippedText(4) + "|" + Run.copiedPathsText(4) + "|" + Run.sentText(1, "macbookair"),
        "Zipped 4 items into one archive|Copied 4 paths|Sent 1 item to macbookair")
  check("copy to clipboard says the files went, not their paths, and groups its count",
        Run.clippedText(1) + "|" + Run.clippedText(1204) + "|" + Run.clippedText(0),
        "Copied 1 item to the clipboard|Copied 1,204 items to the clipboard|")
  check("a refused clipboard copy names its cause, without the flea: prefix",
        Run.clipFailedText("flea: this compositor does not offer clipboard access\n") + "|" + Run.clipFailedText(""),
        "The items could not be copied to the clipboard: this compositor does not offer clipboard access."
        + "|The items could not be copied to the clipboard.")
  check("the clipboard mark is the one Flea's own menus draw", Shelf.ACTION_GLYPHS.clipboard,
        "M9 2h6v4H9z M6 4H3v18h18V4h-3 M8 12h8 M8 16h5")
  check("the flyout says what is about to happen to how many",
        Run.flyoutTitle("move", 4) + "|" + Run.flyoutTitle("send", 1), "Move 4 items to|Send 1 item to")
  check("a destination is named by its leaf, because the flyout said the whole path",
        Run.destName("/home/gm/Work/drafts/"), "drafts")
  check("a listing is its non-empty lines, spaces and all", Run.lines("a\n\n b \n").join("|"), "a| b ")
  check("only the line ending goes, so a path holding a carriage return keeps it",
        Run.lines("one\r\ntw\ro\r\n").join("|"), "one|tw\ro")

    check("the tooltip says what is held, because the bar itself never draws a count",
          Shelf.tooltip(Shelf.empty()) + " / " + Shelf.tooltip(one) + " / " + Shelf.tooltip(mixed),
          "Flea shelf is empty / Flea shelf is holding 1 item / Flea shelf is holding 2 items")
    // The shelf's counts group the way the window's do, through its own helper: it cannot import
    // up to ui/js/Format.js, because it moves with shelf/ into its own repository.
    check("counts group in thousands", Shelf.grouped(1204), "1,204")
    check("clearing groups what it cleared", Shelf.clearedText(1204), "Cleared 1,204 items · z restores")
    check("and putting back groups too", Shelf.undoneText("pile", 1204), "Put 1,204 items back")
    check("a run headline groups its position and total",
          Run.runText({ running: true, verb: "Moving", index: 203, count: 1204 }), "Moving 204 of 1,204")
    check("and its landings group",
          Run.movedText("Moved", 1204, 0, 1204, "drafts", ""), "Moved 1,204 items to drafts · z undoes")
    check("a partial shelf landing groups both halves",
          Run.movedText("Moved", 1000, 204, 1204, "drafts", "disk full"), "Moved 1,000 of 1,204 · disk full")
    check("the flyout groups what is about to move", Run.flyoutTitle("move", 1204), "Move 1,204 items to")
    check("and the other actions group as well",
          Run.zippedText(1204) + "|" + Run.copiedPathsText(1204) + "|" + Run.sentText(1204, "macbookair"),
          "Zipped 1,204 items into one archive|Copied 1,204 paths|Sent 1,204 items to macbookair")
    // Issue 255: the rail strip the card's mask cuts out, one source for the panel and the probe.
    check("the panel mask cut has one pure source", typeof Shelf.railStrip, "function")
    if (typeof Shelf.railStrip === "function") {
        var cut = function (e, w, h) { var r = Shelf.railStrip(e, 2560, 1440, w, h); return r.x + "," + r.y + "," + r.width + "," + r.height }
        check("right sits at the screen edge, centred vertically", cut("right", 4, 864), "2556,288,4,864")
        check("left sits at zero, centred vertically", cut("left", 4, 864), "0,288,4,864")
        check("bottom sits at the screen floor, centred horizontally", cut("bottom", 1536, 4), "512,1436,1536,4")
        check("off is no strip at all", cut("off", 1536, 4), "0,0,0,0")
    }
}
