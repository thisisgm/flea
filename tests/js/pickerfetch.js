.import "../../ui/js/PickerEntry.js" as PickerEntry
.import "../../ui/js/PickerFetch.js" as Fetch

// The location field downloads a URL without showing the user first, so every word the footer can
// say about it, and every refusal before the request goes out, is asserted here with no window.

function remote(text) {
    return PickerEntry.classify(text, "/home/gm", "/home/gm")
}

function refusal(text, folderMode, saving) {
    return Fetch.refusal(remote(text), folderMode === true, saving === true)
}

function run(check) {
    // Before the request: only an open request for a file fetches; the rest say why not.
    check("an open request fetches", refusal("https://example.org/a.pdf"), "")
    check("a folder request refuses a URL", refusal("https://example.org/a.pdf", true), Fetch.FOLDER_ONLY)
    check("a save refuses a URL", refusal("https://example.org/a.pdf", false, true), Fetch.SAVE_LOCAL)
    check("the folder refusal comes first", refusal("https://example.org/a.pdf", true, true), Fetch.FOLDER_ONLY)
    check("a trailing slash is not a file", refusal("https://example.org/dir/"), Fetch.NOT_FILE)
    check("a slash inside the query is not a directory", refusal("https://example.org/a.pdf?x=1/"), "")
    check("a bare host with a slash is not a file", refusal("http://example.org/"), Fetch.NOT_FILE)
    check("ftp fetches too", refusal("ftp://example.org/pub/a.tar"), "")

    // The name the footer fetches by is the backend's own rule for the file on disk.
    check("the leaf is the last segment", Fetch.leafOf("https://example.org/reports/q3.pdf"), "q3.pdf")
    check("without the query", Fetch.leafOf("https://example.org/a.pdf?dl=1"), "a.pdf")
    check("without the fragment", Fetch.leafOf("https://example.org/a.pdf#page=2"), "a.pdf")
    check("percent-decoded", Fetch.leafOf("https://example.org/a%20b.pdf"), "a b.pdf")
    check("a bad escape stays as typed", Fetch.leafOf("https://example.org/a%zz.pdf"), "a%zz.pdf")
    check("a port is not a segment", Fetch.leafOf("http://127.0.0.1:8000/alpha.txt"), "alpha.txt")
    check("no path is download", Fetch.leafOf("http://example.org"), "download")
    check("a slash alone is download", Fetch.leafOf("http://example.org/"), "download")

    // The footer's line, from nothing received to a known total.
    check("no bytes yet is the name alone", Fetch.line("q3.pdf", 0, 0), "Fetching q3.pdf")
    check("bytes with no total count alone", Fetch.line("q3.pdf", 1200000, 0), "Fetching q3.pdf · 1.2 MB")
    check("bytes of a total", Fetch.line("q3.pdf", 1200000, 5000000), "Fetching q3.pdf · 1.2 MB of 5.0 MB")
    check("small files count whole", Fetch.line("a.txt", 20, 20), "Fetching a.txt · 20 B of 20 B")

    // The bar's fill: nothing while the total is unknown, clamped once it is.
    check("no total is no fill", Fetch.fraction(500, 0), 0)
    check("halfway", Fetch.fraction(2500000, 5000000), 0.5)
    check("done", Fetch.fraction(5000000, 5000000), 1)
    check("over is clamped", Fetch.fraction(6000000, 5000000), 1)
    check("nothing yet is empty", Fetch.fraction(0, 5000000), 0)

    // What a fetch that answered no file says.
    check("a cancel is one word", Fetch.failure("cancelled"), Fetch.CANCELLED)
    check("gio's line is said as is", Fetch.failure("gio: https://x.org/gone.pdf: HTTP Error: Not Found"),
          "gio: https://x.org/gone.pdf: HTTP Error: Not Found")
    check("no reason still says it failed", Fetch.failure(""), Fetch.FAILED)
}
