.import "../../ui/js/Facts.js" as Facts
.import "../../ui/js/Kinds.js" as Kinds

// Every expectation here is read off the canvas's Preview column board, label by label and in its
// own order, so a drift in either one fails rather than passing quietly.
function run(check) {
    // A fixed instant, 2026-08-31 12:00:00 UTC less a day, so "Modified" never reads the clock.
    var day = 86400
    var mtime = 1788177600 - day

    function row(icon, size, mode) {
        return { n: "x", d: false, s: size, m: mtime, p: mode === undefined ? 33188 : mode, i: icon, k: 0 }
    }
    function labels(list) {
        var out = []
        for (var i = 0; i < list.length; i++) out.push(list[i].label)
        return out.join("|")
    }
    function valueOf(list, label) {
        for (var i = 0; i < list.length; i++) if (list[i].label === label) return list[i].value
        return "(no such row)"
    }

    var folder = {n: "photos", d: true, s: 0, m: mtime, p: 16877, i: "folder"}
    check("directory preview never presents the entry's zero size as contents",
          valueOf(Facts.facts(Facts.UNSUPPORTED, folder, null, "Folder"), "Size"), "Not calculated")
    folder.s = 4096
    check("local directory preview does not present inode bytes as contents either",
          valueOf(Facts.facts(Facts.LOADING, folder, null, "Folder"), "Size"), "Not calculated")
    check("a mixed selection cannot claim a complete sum of folder contents",
          valueOf(Facts.multiFacts([folder, row("text-x-generic", 42)], 2), "Combined"), "Not calculated")

    check("an icon name classifies the state without the column re-deriving one",
          Kinds.kindState("image-x-generic") + "|" + Kinds.kindState("video-x-generic")
          + "|" + Kinds.kindState("audio-x-generic") + "|" + Kinds.kindState("text-x-script")
          + "|" + Kinds.kindState("text-x-generic") + "|" + Kinds.kindState("package-x-generic")
          + "|" + Kinds.kindState("application-x-executable"),
          "image|video|audio|code|text|archive|unsupported")

    // A .doc and a .pdf share one icon, and only one of them is something this column can draw.
    // Routing the rest of that family to text made the backend count the newlines in a binary.
    function named(name, icon) {
        var r = row(icon, 10)
        r.n = name
        return r
    }
    check("the office family is unsupported, because none of it is text",
          Kinds.kindState("x-office-document") + "|" + Kinds.kindState("x-office-spreadsheet")
          + "|" + Kinds.kindState("x-office-presentation"),
          "unsupported|unsupported|unsupported")
    check("a word processor document is not routed to the text frame",
          Facts.state(named("report.odt", "x-office-document"), 1, false, ""), "unsupported")
    check("but a pdf sharing that icon is still told apart by its extension",
          Facts.state(named("manual.pdf", "x-office-document"), 1, false, ""), "pdf")
    // The canvas's own Text tile is field-bench-notes.md, and .md carries the office icon too.
    check("and markdown reaches the text frame, which is what the canvas draws",
          Facts.state(named("field-bench-notes.md", "x-office-document"), 1, false, ""), "text")
    check("every markdown suffix, not just .md",
          Facts.state(named("a.markdown", "x-office-document"), 1, false, "") + "|"
          + Facts.state(named("b.mkd", "x-office-document"), 1, false, ""), "text|text")
    check("and a name merely containing md is not markdown",
          Facts.state(named("amd.doc", "x-office-document"), 1, false, ""), "unsupported")

    // A symlink's own mode wins over whatever its name says it is.
    check("a symlink is a symlink whatever its name looks like",
          Facts.state(row("image-x-generic", 10, 41471), 0, false, ""), "symlink")
    check("a multi-selection outranks the row under the cursor",
          Facts.state(row("image-x-generic", 10), 4, false, ""), "multi")
    check("an error outranks a kind",
          Facts.state(row("image-x-generic", 10), 1, false, "Permission denied"), "error")
    check("and a preview still arriving is loading",
          Facts.state(row("video-x-generic", 10), 1, true, ""), "loading")
    // The backend's icon for a source file is inconsistent, so code is told apart by extension.
    check("a source file is code whatever icon the backend gave it",
          ["icons.rs", "app.py", "main.go", "run.sh", "conf.toml"].map(function (n) {
              return Facts.state({ n: n, d: false, s: 1, m: 1, p: 33188, i: "text-x-generic", k: 0 }, 1, false, "", "Rust source code")
          }).join("|"),
          "code|code|code|code|code")
    check("and prose is not",
          Facts.state({ n: "notes.md", d: false, s: 1, m: 1, p: 33188, i: "text-x-generic", k: 0 }, 1, false, "", "Markdown"),
          "text")
    // An image named .css is an image; only a row the icon already called text can be promoted.
    check("an extension alone never promotes a row the icon did not call text",
          Facts.state({ n: "sprite.css", d: false, s: 1, m: 1, p: 33188, i: "image-x-generic", k: 0 }, 1, false, "", "PNG image"),
          "image")
    check("and the extension table matches only a real suffix",
          Kinds.isCode("a.rs") + "|" + Kinds.isCode("rs") + "|" + Kinds.isCode("a.rsx") + "|" + Kinds.isCode("README"),
          "true|false|false|false")

    // The backend says "I could not identify this" by making the Kind the icon name itself, which is
    // the canvas's own core.dump tile and is not text however far the icon ladder fell.
    check("a type nothing recognised is unsupported, not text",
          Facts.state(row("text-x-generic", 212000000), 1, false, "", "Data"), "unsupported")
    check("while a real text type stays text",
          Facts.state(row("text-x-generic", 18000), 1, false, "", "Plain text document"), "text")
    check("and a caller that names no kind at all still classifies by icon",
          Facts.state(row("text-x-generic", 18000), 1, false, ""), "text")
    check("no row at all is the unsupported state rather than a crash",
          Facts.state(null, 0, false, ""), "unsupported")
    check("a directory is not a preview subject",
          Kinds.isPreviewable({ d: true }) + "|" + Kinds.isPreviewable({ d: false }) + "|" + Kinds.isPreviewable(null),
          "false|true|false")

    // Preview board rule 1: eight ordinary settled kinds open with Kind, Size, Modified, five add one
    // kind fact and stop at four rows, and video, audio and code carry two.
    check("image states Kind, Size, Modified, Pixels",
          labels(Facts.facts("image", row("image-x-generic", 2100000), { w: 2560, h: 1440 }, "PNG image")),
          "Kind|Size|Modified|Pixels")
    check("and its pixels read as the canvas writes them",
          valueOf(Facts.facts("image", row("image-x-generic", 2100000), { w: 2560, h: 1440 }, "PNG image"), "Pixels"),
          "2560 × 1440")
    check("video states Kind, Size, Modified, Duration, Pixels",
          labels(Facts.facts("video", row("video-x-generic", 48000000), { w: 1920, h: 1080 }, "MP4 video", { duration: "1:12" })),
          "Kind|Size|Modified|Duration|Pixels")
    check("audio states Kind, Size, Modified, Duration, Rate",
          labels(Facts.facts("audio", row("audio-x-generic", 31000000), null, "FLAC audio", { duration: "4:05", rate: "44.1 kHz" })),
          "Kind|Size|Modified|Duration|Rate")
    check("pdf states Kind, Size, Modified, Pages",
          labels(Facts.facts("pdf", row("application-pdf", 2300000), null, "PDF document", { pages: "51" })),
          "Kind|Size|Modified|Pages")
    check("text states Kind, Size, Modified, Lines",
          labels(Facts.facts("text", row("text-x-generic", 18000), { lines: 214 }, "Markdown")),
          "Kind|Size|Modified|Lines")
    // Mode stays: columns view draws no permission column, so dropping it here removes it entirely.
    check("code states Kind, Size, Modified, Lines, Mode",
          labels(Facts.facts("code", row("text-x-script", 4200), { lines: 132 }, "Rust source")),
          "Kind|Size|Modified|Lines|Mode")
    check("archive states Kind, Size, Modified, Entries",
          labels(Facts.facts("archive", row("package-x-generic", 1200000000), null, "Zstandard tar", { entries: "214", unpacked: "3.4 GB" })),
          "Kind|Size|Modified|Entries")
    // Size above already says what the archive weighs packed, so its one row carries both counts.
    check("and the archive's one fact is the count and the unpacked total together",
          valueOf(Facts.facts("archive", row("package-x-generic", 1200000000), null, "Zstandard tar", { entries: "214", unpacked: "3.4 GB" }), "Entries"),
          "214, 3.4 GB out")
    check("an archive nothing has measured unpacked still states its count",
          valueOf(Facts.facts("archive", row("package-x-generic", 1200000000), null, "Zstandard tar", { entries: "214" }), "Entries"),
          "214")
    check("symlink states Kind, Target, Points at, Mode",
          labels(Facts.facts("symlink", row("folder", 18, 41471), { target: "/usr/share/omarchy", targetDir: true }, "Folder")),
          "Kind|Target|Points at|Mode")
    check("and says what it points at rather than repeating the path",
          valueOf(Facts.facts("symlink", row("folder", 18, 41471), { target: "/usr/share/omarchy", targetDir: true }, "Folder"), "Points at"),
          "Folder")
    check("a symlink to a file says so too",
          valueOf(Facts.facts("symlink", row("file", 18, 41471), { target: "a.txt", targetDir: false }, "x"), "Points at"),
          "File")
    check("error states Kind, Size, Mode, Owner",
          labels(Facts.facts("error", row("application-x-generic", 419, 33152), null, "Unknown", { owner: "root" })),
          "Kind|Size|Mode|Owner")
    check("loading states Kind, Size, State",
          labels(Facts.facts("loading", row("video-x-generic", 48000000), null, "MP4 video")),
          "Kind|Size|State")
    check("unsupported states Kind, Size, Modified, Mode",
          labels(Facts.facts("unsupported", row("application-x-generic", 212000000), null, "Data")),
          "Kind|Size|Modified|Mode")

    // Duration and rate come off the meta answer rather than being assembled by a caller.
    check("a video reads its duration off the meta answer",
          valueOf(Facts.facts("video", row("video-x-generic", 48000000), { w: 1920, h: 1080, durationMs: 72000 }, "MP4 video"), "Duration"),
          "1:12")
    check("an audio row reads its rate the way the canvas writes it",
          valueOf(Facts.facts("audio", row("audio-x-generic", 31000000), { durationMs: 245000, sampleRate: 44100 }, "FLAC audio"), "Rate"),
          "44.1 kHz")
    check("a whole number of kilohertz drops the decimal",
          Facts.mediaExtra({ sampleRate: 48000 }).rate, "48 kHz")
    check("a media row with nothing probed yet shows empty cells rather than zeroes",
          Facts.mediaExtra({ durationMs: 0, sampleRate: 0 }).duration + "|" + Facts.mediaExtra({ durationMs: 0, sampleRate: 0 }).rate,
          "|")
    check("and no meta at all is not a crash",
          JSON.stringify(Facts.mediaExtra(null)), "{}")

    // Entries and the unpacked total come off the archive's own index, read without extracting it.
    check("an archive states its entries and what they weigh unpacked",
          valueOf(Facts.facts("archive", row("package-x-generic", 1200000000), { entries: 214, unpacked: 3400000000 }, "Zstandard tar"), "Entries"),
          "214, 3.4 GB out")
    check("and an archive nothing has listed yet shows empty cells rather than zeroes",
          JSON.stringify(Facts.archiveExtra({ entries: 0, unpacked: 0 })), "{}")

    // The count is exact however long the listing is: the backend streams it and caps only the names
    // it sends, and the difference is the tile's own "+ N more" line.
    var big = { entries: 214, unpacked: 3400000000,
                names: [{ n: "daemon", d: true }, { n: "ui", d: true }, { n: "Cargo.toml", d: false }] }
    check("a long archive states an exact count and an exact unpacked total, never a cap",
          valueOf(Facts.facts("archive", row("package-x-generic", 1200000000), big, "Zstandard tar"), "Entries"),
          "214, 3.4 GB out")
    check("the tile lists the names the wire carried",
          Facts.archiveEntries(big).map(function (e) { return e.n }).join("|"),
          "daemon|ui|Cargo.toml")
    check("and states how many it could not list",
          String(Facts.archiveMore(big)), "211")
    check("an archive whose names all fit says no more at all",
          String(Facts.archiveMore({ entries: 3, unpacked: 10, names: [{ n: "a" }, { n: "b" }, { n: "c" }] })), "0")
    check("and one nothing has listed yet lists nothing rather than throwing",
          Facts.archiveEntries(null).length + "|" + Facts.archiveMore(null), "0|0")
    // The frame bounds the list a second time, so a tall wire answer cannot run past the tile and
    // hide the very number the tile exists to state.
    check("a frame with room for two names lists two",
          Facts.archiveEntries(big, 2).map(function (e) { return e.n }).join("|"), "daemon|ui")
    check("and counts every entry it did not name, wire and frame alike",
          String(Facts.archiveMore(big, 2)), "212")
    check("a frame with no room at all lists none and counts them all",
          Facts.archiveEntries(big, 0).length + "|" + Facts.archiveMore(big, 0), "0|214")

    // A line count the backend had to stop early is a floor, marked the way a partial size is.
    check("a complete line count is a plain number",
          Facts.lineCount({ lines: 214, partial: false }), "214")
    check("and a truncated one is marked as a floor",
          Facts.lineCount({ lines: 9000, partial: true }), "> 9000")
    check("no meta yet is an empty cell rather than a zero",
          Facts.lineCount(null), "")
    // src/backend/metareq.rs answers lines 0 for a file it could not open, the same 0 an empty file
    // answers, so without lfailed the table printed "Lines: 0" as if it had measured one.
    check("a file the backend could not open states no count rather than zero",
          Facts.lineCount({ lines: 0, partial: false, linesFailed: true }), "")
    check("and a file that really is empty still states its zero",
          Facts.lineCount({ lines: 0, partial: false, linesFailed: false }), "0")
    // HANDOFF rule 19: a label with no value is not drawn, so the row goes rather than the value.
    check("an unreadable text file draws no Lines row at all",
          labels(Facts.facts("text", row("text-x-generic", 18000),
                             { lines: 0, partial: false, linesFailed: true }, "Markdown")), "Kind|Size|Modified")
    check("and a raw image whose dimensions are unreadable draws no Pixels row",
          labels(Facts.facts("image", row("image-x-generic", 2100000), null, "Raw image")), "Kind|Size|Modified")
    check("an archive nothing has listed draws no Entries row either",
          labels(Facts.facts("archive", row("package-x-generic", 1200000), null, "Zip archive")), "Kind|Size|Modified")
    check("a video nothing has probed yet drops Duration and keeps the rest",
          labels(Facts.facts("video", row("video-x-generic", 48000000), { w: 1920, h: 1080 }, "MP4 video")),
          "Kind|Size|Modified|Pixels")
    check("pixels with nothing behind them are empty rather than 0 × 0",
          Facts.pixels(null) + "|" + Facts.pixels({ w: 0, h: 0 }), "|")

    // The multi-select summary, which is a summary and never a collage.
    var many = [row("image-x-generic", 1000), row("image-x-generic", 2000),
                row("video-x-generic", 4000), row("text-x-generic", 8000)]
    check("multi-select states Kinds, Combined, Newest, Oldest",
          labels(Facts.multiFacts(many)), "Kinds|Combined|Newest|Oldest")
    check("and counts the kinds the way the canvas phrases them",
          valueOf(Facts.multiFacts(many), "Kinds"), "2 images, 1 video, 1 text")
    check("the combined size is every selected row added up",
          valueOf(Facts.multiFacts(many), "Combined"), "15.0 kB")
    check("an empty selection states only the row it can fill, rather than throwing",
          labels(Facts.multiFacts([])), "Combined")
    check("a hole in the selection is skipped rather than counted",
          valueOf(Facts.multiFacts([null, row("image-x-generic", 1000)]), "Kinds"), "1 image")

    // A selection wider than the held window cannot be summed without a metadata sweep, which this
    // codebase refuses, so every number it does produce is a floor and has to look like one.
    check("a selection wider than the held window states floors, not an undercount",
          valueOf(Facts.multiFacts(many, 9), "Combined") + "|"
          + valueOf(Facts.multiFacts(many, 9), "Kinds"),
          "> 15.0 kB|> 2 images, > 1 video, > 1 text")
    check("and a selection entirely inside the window states plain totals",
          valueOf(Facts.multiFacts(many, 4), "Combined") + "|"
          + valueOf(Facts.multiFacts(many, 4), "Kinds"),
          "15.0 kB|2 images, 1 video, 1 text")
    check("a hole inside the window is a floor too, because a row it could not read is a row it did not count",
          valueOf(Facts.multiFacts([null, row("image-x-generic", 1000)], 2), "Combined"), "> 1.0 kB")

    // The Quick Look classifies from the same icon and name the column does, so the two can never
    // disagree about what a row is; the overlay refused every image for as long as it kept its own list.
    check("the Quick Look draws an image, an archive and a symlink to an image by the icon the backend sent",
          Kinds.quickLookKind("image-x-generic", "photo.png") + "|" + Kinds.quickLookKind("package-x-generic", "backup.tar.zst")
          + "|" + Kinds.quickLookKind("image-x-generic", "link-to-photo.png"),
          "image|archive|image")
    check("the Quick Look keeps its media, pdf, markdown and text answers",
          Kinds.quickLookKind("audio-x-generic", "tone.wav") + "|" + Kinds.quickLookKind("video-x-generic", "clip.mp4")
          + "|" + Kinds.quickLookKind("x-office-document", "manual.pdf") + "|" + Kinds.quickLookKind("x-office-document", "notes.md")
          + "|" + Kinds.quickLookKind("text-x-generic", "notes.txt"),
          "audio|video|pdf|text|text")
    check("code renders as text in the Quick Look, and an office file or an executable is declined",
          Kinds.quickLookKind("text-x-script", "script.sh") + "|" + Kinds.quickLookKind("x-office-document", "report.doc")
          + "|" + Kinds.quickLookKind("application-x-executable", "a.out"),
          "text|unsupported|unsupported")
    // /usr/share/mime/generic-icons on this box maps text/* to five different text- names, so the
    // family classifies, and a list of the names seen so far is exactly what missed every image.
    check("an icon classifies by its family, not by a list of the names seen so far",
          Kinds.kindState("text-x-generic-template") + "|" + Kinds.kindState("image-x-generic"),
          "text|image")

    // The Quick Look's one line under an archive's name, from the same meta the column's table reads.
    check("an archive's Quick Look line states the count and the unpacked size, and nothing before the index lands",
          Facts.archiveLine({ entries: 214, unpacked: 3400000000, names: [] }) + "|"
          + Facts.archiveLine({ entries: 1, unpacked: 0, names: [] }) + "|" + Facts.archiveLine(null) + "|"
          + Facts.archiveLine({ entries: 0, unpacked: 0, archiveFailed: false, names: [] }) + "|"
          + Facts.archiveLine({ entries: 0, unpacked: 0, archiveFailed: true, names: [] }),
          "214 entries \u00b7 3.4 GB unpacked|1 entry||0 entries|")
}
